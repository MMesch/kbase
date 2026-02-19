use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use tokio::sync::mpsc;
use tracing::warn;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::config::{self, Config};
use crate::embeddings::{self, EmbeddingBackend, EmbeddingCache, EmbeddingStore};
use crate::note::{self, Link, Note};
use crate::store::Store;
use crate::vault;

/// LSP backend for kbase.
///
/// Fields use `RwLock` for interior mutability because `LanguageServer` trait methods
/// take `&self` (not `&mut self`) to allow concurrent request handling. `RwLock` permits
/// multiple simultaneous readers OR one exclusive writer, so handlers like hover and
/// completion can read `notes` concurrently while `refresh_notes` has exclusive write access.
pub struct KbaseLanguageServer {
    client: Client,
    /// Cached notes indexed by file path
    notes: RwLock<HashMap<PathBuf, Note>>,
    /// Vault root path
    vault_path: RwLock<Option<PathBuf>>,
    /// Embedding store for semantic search
    embedding_store: RwLock<Option<EmbeddingStore>>,
    /// Embedding cache (persisted)
    embedding_cache: RwLock<Option<EmbeddingCache>>,
    /// Embedding backend
    embedding_backend: RwLock<Option<Box<dyn EmbeddingBackend + Send + Sync>>>,
    /// Persistent graph store
    graph_store: RwLock<Option<Arc<Store>>>,
    /// Channel to receive file change events
    file_change_rx: RwLock<Option<mpsc::UnboundedReceiver<PathBuf>>>,
    /// Sender for file changes (kept to allow cloning for watcher)
    file_change_tx: mpsc::UnboundedSender<PathBuf>,
}

impl KbaseLanguageServer {
    /// Creates a new server instance with empty/uninitialized state.
    ///
    /// Fields like `vault_path` and `graph_store` start as `None` because the actual
    /// initialization happens later in the LSP lifecycle: `initialize()` receives the
    /// workspace root from the editor, and `initialized()` loads the graph store and
    /// starts the file watcher.
    pub fn new(client: Client) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            client,
            notes: RwLock::new(HashMap::new()),
            vault_path: RwLock::new(None),
            embedding_store: RwLock::new(None),
            embedding_cache: RwLock::new(None),
            embedding_backend: RwLock::new(None),
            graph_store: RwLock::new(None),
            file_change_rx: RwLock::new(Some(rx)),
            file_change_tx: tx,
        }
    }

    /// Initialize persistent store and file watcher
    fn init_persistent_store(&self) -> anyhow::Result<usize> {
        let vault_path = self.vault_path.read().unwrap().clone();
        let vault_path = vault_path.ok_or_else(|| anyhow::anyhow!("No vault path"))?;

        // Load persistent store with incremental updates
        let (store, updated) = vault::load_persistent(&vault_path)?;
        let store = Arc::new(store);

        *self.graph_store.write().unwrap() = Some(store);

        Ok(updated)
    }

    /// Start file watcher for the vault
    fn start_file_watcher(&self) -> anyhow::Result<()> {
        let vault_path = self.vault_path.read().unwrap().clone();
        let vault_path = vault_path.ok_or_else(|| anyhow::anyhow!("No vault path"))?;

        let tx = self.file_change_tx.clone();

        // Create debounced watcher (500ms debounce)
        let mut debouncer = new_debouncer(
            Duration::from_millis(500),
            move |res: std::result::Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                match res {
                    Ok(events) => {
                        for event in events {
                            if event.kind == DebouncedEventKind::Any {
                                // Only process markdown files
                                if event.path.extension().is_some_and(|ext| ext == "md") {
                                    let _ = tx.send(event.path);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("File watcher error: {}", e);
                    }
                }
            },
        )?;

        // Watch the vault directory recursively
        debouncer
            .watcher()
            .watch(&vault_path, notify::RecursiveMode::Recursive)?;

        // Keep the watcher alive by leaking it (it will live for the duration of the LSP)
        // This is intentional - the watcher needs to stay alive
        std::mem::forget(debouncer);

        Ok(())
    }

    /// Process pending file changes
    async fn process_file_changes(&self) {
        let vault_path = self.vault_path.read().unwrap().clone();
        let Some(vault_path) = vault_path else {
            return;
        };

        let config = match Config::load(&vault_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        // Take the receiver temporarily
        let mut rx = match self.file_change_rx.write().unwrap().take() {
            Some(rx) => rx,
            None => return,
        };

        // Process all pending changes
        let mut changed_paths = Vec::new();
        while let Ok(path) = rx.try_recv() {
            changed_paths.push(path);
        }

        // Put the receiver back
        *self.file_change_rx.write().unwrap() = Some(rx);

        if changed_paths.is_empty() {
            return;
        }

        // Update graph store
        let store = self.graph_store.read().unwrap().clone();
        if let Some(store) = store {
            for path in &changed_paths {
                if let Err(e) = vault::update_note(&store, path, config.link_syntax) {
                    self.client
                        .log_message(MessageType::WARNING, format!("Failed to update {}: {}", path.display(), e))
                        .await;
                }
            }
            self.client
                .log_message(MessageType::INFO, format!("Updated {} notes from file changes", changed_paths.len()))
                .await;
        }

        // Also refresh the notes cache for LSP features
        self.refresh_notes().await;
    }

    /// Get the graph store, initializing if needed
    fn get_or_init_store(&self) -> anyhow::Result<Arc<Store>> {
        // Check if already initialized
        if let Some(store) = self.graph_store.read().unwrap().clone() {
            return Ok(store);
        }

        // Initialize
        self.init_persistent_store()?;

        self.graph_store
            .read()
            .unwrap()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Failed to initialize store"))
    }

    /// Initialize embeddings (called lazily on first search)
    fn init_embeddings(&self) -> anyhow::Result<()> {
        let vault_path = self.vault_path.read().unwrap().clone();
        let vault_path = vault_path.ok_or_else(|| anyhow::anyhow!("No vault path"))?;

        // Already initialized?
        if self.embedding_backend.read().unwrap().is_some() {
            return Ok(());
        }

        let config = Config::load(&vault_path)?;
        let cache_path = vault_path.join(".kbase").join("embeddings.redb");
        let cache = EmbeddingCache::open(&cache_path)?;

        let backend: Box<dyn EmbeddingBackend + Send + Sync> = match config.embeddings.backend {
            config::EmbeddingBackend::Onnx => {
                let model_dir = embeddings::OnnxBackend::find_model_dir(&vault_path)?;
                Box::new(embeddings::OnnxBackend::new(&model_dir)?)
            }
            config::EmbeddingBackend::Ollama => {
                Box::new(embeddings::OllamaBackend::new(None)?)
            }
        };

        *self.embedding_cache.write().unwrap() = Some(cache);
        *self.embedding_backend.write().unwrap() = Some(backend);

        Ok(())
    }

    /// Build/refresh embedding store from notes
    fn refresh_embeddings(&self) -> anyhow::Result<()> {
        self.init_embeddings()?;

        let vault_path = self.vault_path.read().unwrap().clone();
        let vault_path = vault_path.ok_or_else(|| anyhow::anyhow!("No vault path"))?;
        let config = Config::load(&vault_path)?;
        let chunk_level = embeddings::ChunkLevel::from_str(&config.embeddings.chunk_level);

        let notes = self.notes.read().unwrap();
        let mut store = EmbeddingStore::new();

        let mut cache = self.embedding_cache.write().unwrap();
        let mut backend = self.embedding_backend.write().unwrap();

        let cache = cache.as_mut().ok_or_else(|| anyhow::anyhow!("No cache"))?;
        let backend = backend.as_mut().ok_or_else(|| anyhow::anyhow!("No backend"))?;

        for note in notes.values() {
            let content = match std::fs::read_to_string(&note.path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let chunks = embeddings::split_into_chunks(&content, chunk_level);

            let texts: Vec<&str> = chunks.iter().map(|(_, text, _)| text.as_str()).collect();
            let vectors = cache.get_or_compute_batch(&texts, backend.as_mut())?;

            for ((headers, text, line), embedding) in chunks.into_iter().zip(vectors) {
                store.add_chunk(embeddings::Chunk {
                    note_path: note.path.to_string_lossy().to_string(),
                    header_path: headers,
                    text,
                    embedding,
                    line,
                });
            }
        }

        *self.embedding_store.write().unwrap() = Some(store);
        Ok(())
    }

    /// Refresh the notes cache
    async fn refresh_notes(&self) {
        let vault_path = self.vault_path.read().unwrap().clone();
        if let Some(path) = vault_path {
            if let Ok(notes) = vault::load_notes(&path) {
                let count = {
                    let mut cache = self.notes.write().unwrap();
                    cache.clear();
                    for note in notes {
                        // Canonicalize path for consistent lookups
                        let canonical = note.path.canonicalize().unwrap_or(note.path.clone());
                        cache.insert(canonical, note);
                    }
                    cache.len()
                };
                self.client
                    .log_message(MessageType::INFO, format!("Loaded {} notes", count))
                    .await;
            }
        }
    }

    /// Find note by title (fuzzy match)
    fn find_note_by_title(&self, title: &str) -> Option<Note> {
        let notes = self.notes.read().unwrap();
        let title_lower = title.to_lowercase();
        notes
            .values()
            .find(|n| n.title.to_lowercase() == title_lower)
            .cloned()
    }

    /// Find all notes that link to a given note (by title or path)
    /// Returns Vec of (Note, Vec<Link>) where the links are the ones pointing to the target
    fn find_backlinks(&self, target_title: &str, target_path: &std::path::Path) -> Vec<(Note, Vec<Link>)> {
        let notes = self.notes.read().unwrap();
        let target_title_lower = target_title.to_lowercase();

        // Get the file name without extension for path matching
        let target_filename = target_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        notes
            .values()
            .filter_map(|n| {
                let matching_links: Vec<Link> = n.links.iter().filter(|link| {
                    let link_lower = link.target.to_lowercase();
                    // Match by title (wiki links)
                    if link_lower == target_title_lower {
                        return true;
                    }
                    // Match by path (markdown links) - check if link ends with the filename
                    if link.target.ends_with(".md") {
                        let link_path = std::path::Path::new(&link.target);
                        if let Some(link_stem) = link_path.file_stem().and_then(|s| s.to_str()) {
                            if link_stem.eq_ignore_ascii_case(target_filename) {
                                return true;
                            }
                        }
                    }
                    false
                }).cloned().collect();

                if matching_links.is_empty() {
                    None
                } else {
                    Some((n.clone(), matching_links))
                }
            })
            .collect()
    }

    /// Extract wiki link at position (returns the link target if cursor is on a [[link]])
    fn get_link_at_position(&self, content: &str, position: Position) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        let line = lines.get(position.line as usize)?;
        let col = position.character as usize;

        // Find [[ before cursor and ]] after cursor
        let before = &line[..col.min(line.len())];
        let after = &line[col.min(line.len())..];

        let start = before.rfind("[[")?;
        let end = after.find("]]")?;

        // Extract link target
        let link_content = &line[start + 2..col + end];

        // Handle [[target|alias]] format
        let target = link_content.split('|').next()?;
        Some(target.to_string())
    }

    /// Extract tag path at cursor position in frontmatter
    fn get_tag_at_position(&self, content: &str, position: Position) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        let col = position.character as usize;

        // Check if we're in frontmatter
        let mut in_frontmatter = false;
        let mut frontmatter_end = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == 0 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter && line.trim() == "---" {
                frontmatter_end = i;
                break;
            }
        }

        if line_idx == 0 || line_idx >= frontmatter_end {
            return None;
        }

        let line = lines.get(line_idx)?;
        let trimmed = line.trim();

        // Pattern 1: "  - domain/ai" (list item)
        if trimmed.starts_with("- ") {
            let value = &trimmed[2..];
            if value.contains('/') && col >= line.find("- ").unwrap_or(0) + 2 {
                return Some(value.trim().to_string());
            }
        }

        // Pattern 2: "tags: [domain/ai, ...]" (inline array)
        if trimmed.starts_with("tags:") && trimmed.contains('[') {
            if let Some(bracket_start) = line.find('[') {
                if let Some(bracket_end) = line.find(']') {
                    if col > bracket_start && col < bracket_end {
                        let array_content = &line[bracket_start + 1..bracket_end];
                        let tags: Vec<&str> = array_content.split(',').map(|s| s.trim()).collect();

                        let mut current_pos = bracket_start + 1;
                        for tag in tags {
                            let tag_start = current_pos + line[current_pos..].find(tag).unwrap_or(0);
                            let tag_end = tag_start + tag.len();
                            if col >= tag_start && col <= tag_end && tag.contains('/') {
                                return Some(tag.to_string());
                            }
                            current_pos = tag_end + 1;
                        }
                    }
                }
            }
        }

        // Pattern 3: "  domain: domain/ai" (trees field)
        if trimmed.contains(':') && !trimmed.starts_with("tags:") && !trimmed.starts_with("trees:") {
            if let Some(colon_pos) = trimmed.find(':') {
                let value = trimmed[colon_pos + 1..].trim();
                if value.contains('/') {
                    return Some(value.to_string());
                }
            }
        }

        None
    }

    /// Check if cursor is in a tags/trees context and return the prefix being typed
    /// Returns (prefix, prefix_start_col) for tag/tree completion context
    fn get_tag_completion_prefix(&self, content: &str, position: Position) -> Option<(String, u32)> {
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;
        let col = position.character as usize;

        // Check if we're in frontmatter
        let mut in_frontmatter = false;
        let mut frontmatter_end = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == 0 && line.trim() == "---" {
                in_frontmatter = true;
                continue;
            }
            if in_frontmatter && line.trim() == "---" {
                frontmatter_end = i;
                break;
            }
        }

        if line_idx == 0 || line_idx >= frontmatter_end {
            return None;
        }

        let line = lines.get(line_idx)?;
        let trimmed = line.trim();
        let indent = line.len() - trimmed.len();

        // Pattern 1: "  - prefix" (list item in tags array)
        if trimmed.starts_with("- ") {
            for i in (0..line_idx).rev() {
                let prev = lines[i].trim();
                if prev == "tags:" || prev.starts_with("tags:") {
                    // Prefix starts after "- " in the trimmed line
                    let prefix_start = indent + 2;
                    let prefix = &line[prefix_start..col.min(line.len())];
                    return Some((prefix.to_string(), prefix_start as u32));
                }
                if !prev.starts_with("- ") && !prev.is_empty() {
                    break;
                }
            }
        }

        // Pattern 2: "tags: [prefix" or inside array
        if trimmed.starts_with("tags:") && trimmed.contains('[') {
            let before_cursor = &line[..col.min(line.len())];
            if let Some(bracket_pos) = before_cursor.rfind(|c| c == '[' || c == ',') {
                // Find where actual content starts (skip whitespace after [ or ,)
                let after_bracket = &before_cursor[bracket_pos + 1..];
                let space_count = after_bracket.len() - after_bracket.trim_start().len();
                let prefix_start = bracket_pos + 1 + space_count;
                let prefix = &before_cursor[prefix_start..];
                return Some((prefix.to_string(), prefix_start as u32));
            }
        }

        // Pattern 3: "trees:" block - "  domain: domain/ai"
        if trimmed.contains(':') && !trimmed.starts_with("tags:") && !trimmed.starts_with("trees:") {
            for i in (0..line_idx).rev() {
                let prev = lines[i].trim();
                if prev == "trees:" || prev.starts_with("trees:") {
                    if let Some(colon_pos) = trimmed.find(':') {
                        // Prefix starts after ":" in the trimmed line
                        let after_colon = &trimmed[colon_pos + 1..];
                        let space_count = after_colon.len() - after_colon.trim_start().len();
                        let prefix_start = indent + colon_pos + 1 + space_count;
                        let prefix = &line[prefix_start..col.min(line.len())];
                        return Some((prefix.to_string(), prefix_start as u32));
                    }
                }
                if !prev.contains(':') || prev.starts_with("tags:") {
                    break;
                }
            }
        }

        None
    }

    /// Generate completion items for tree paths
    /// `prefix` is what the user has typed so far
    /// `position` is the cursor position
    /// `prefix_start_col` is where the prefix starts in the line
    fn complete_tree_paths(
        &self,
        prefix: &str,
        position: Position,
        prefix_start_col: u32,
    ) -> CompletionResponse {
        let store = self.graph_store.read().unwrap();

        let mut items: Vec<CompletionItem> = Vec::new();

        // Range to replace: from prefix start to cursor position
        let replace_range = Range {
            start: Position {
                line: position.line,
                character: prefix_start_col,
            },
            end: position,
        };

        // Get existing tree paths from store
        if let Some(store) = store.as_ref() {
            if let Ok(paths) = store.all_tree_paths() {
                for path in paths {
                    if prefix.is_empty() || path.starts_with(prefix) {
                        items.push(CompletionItem {
                            label: path.clone(),
                            kind: Some(CompletionItemKind::FOLDER),
                            detail: Some("tree path".to_string()),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                                range: replace_range,
                                new_text: path,
                            })),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // Also suggest note titles as potential tree nodes
        let notes = self.notes.read().unwrap();
        for note in notes.values() {
            if !prefix.is_empty() && prefix.contains('/') {
                let tree = prefix.split('/').next().unwrap_or("");
                let path = format!("{}/{}", tree, note.title);
                if path.starts_with(prefix) && !items.iter().any(|i| i.label == path) {
                    items.push(CompletionItem {
                        label: path.clone(),
                        kind: Some(CompletionItemKind::FILE),
                        detail: Some(note.path.to_string_lossy().to_string()),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                            range: replace_range,
                            new_text: path,
                        })),
                        ..Default::default()
                    });
                }
            }
        }

        CompletionResponse::Array(items)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for KbaseLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Get workspace root
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                *self.vault_path.write().unwrap() = Some(path);
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                // Go to definition - follow [[links]]
                definition_provider: Some(OneOf::Left(true)),
                // Find references - backlinks
                references_provider: Some(OneOf::Left(true)),
                // Hover - show note preview
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Completion - suggest note titles
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["[".to_string()]),
                    ..Default::default()
                }),
                // Workspace symbol search (semantic search via kbase search)
                workspace_symbol_provider: Some(OneOf::Left(true)),
                // Custom commands
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "kbase.search".to_string(),
                        "kbase.backlinks".to_string(),
                        "kbase.notes".to_string(),
                        "kbase.tags".to_string(),
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "kbase LSP initialized")
            .await;

        // Initialize persistent store
        match self.init_persistent_store() {
            Ok(updated) => {
                self.client
                    .log_message(MessageType::INFO, format!("Loaded graph store ({} notes updated)", updated))
                    .await;
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::WARNING, format!("Failed to init persistent store: {}", e))
                    .await;
            }
        }

        // Start file watcher
        match self.start_file_watcher() {
            Ok(()) => {
                self.client
                    .log_message(MessageType::INFO, "File watcher started")
                    .await;
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::WARNING, format!("Failed to start file watcher: {}", e))
                    .await;
            }
        }

        self.refresh_notes().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, _params: DidOpenTextDocumentParams) {
        self.refresh_notes().await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        // Update the specific file in the graph store
        if let Ok(path) = params.text_document.uri.to_file_path() {
            let vault_path = self.vault_path.read().unwrap().clone();
            let store = self.graph_store.read().unwrap().clone();

            if let (Some(vault_path), Some(store)) = (vault_path, store) {
                if let Ok(config) = Config::load(&vault_path) {
                    match vault::update_note(&store, &path, config.link_syntax) {
                        Err(e) => {
                            self.client
                                .log_message(MessageType::WARNING, format!("Failed to update note: {}", e))
                                .await;
                        }
                        Ok(()) => {
                            self.client
                                .log_message(MessageType::INFO, format!("Updated: {}", path.display()))
                                .await;
                        }
                    }
                }
            }
        }

        // Also process any pending file watcher events
        self.process_file_changes().await;

        self.refresh_notes().await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Read file content
        let path = uri.to_file_path().ok().and_then(|p| p.canonicalize().ok());
        let content = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());

        if let Some(content) = content {
            // Check for tag path (e.g., domain/ai) in frontmatter
            if let Some(tag_path) = self.get_tag_at_position(&content, position) {
                // Get all notes with this tag or any descendant tag
                let store = self.graph_store.read().unwrap();
                if let Some(store) = store.as_ref() {
                    if let Ok(note_titles) = store.list_notes(Some(&tag_path), false) {
                        let locations: Vec<Location> = note_titles
                            .iter()
                            .filter_map(|title| {
                                // title format is "Title (path)" - extract path
                                let path_start = title.rfind('(')?;
                                let path_end = title.rfind(')')?;
                                let path_str = &title[path_start + 1..path_end];
                                let note_path = std::path::PathBuf::from(path_str);
                                let uri = Url::from_file_path(&note_path).ok()?;
                                Some(Location {
                                    uri,
                                    range: Range::default(),
                                })
                            })
                            .collect();

                        if !locations.is_empty() {
                            return Ok(Some(GotoDefinitionResponse::Array(locations)));
                        }
                    }
                }

                // Fallback: navigate to the parent note (last segment)
                if let Some(parent) = tag_path.split('/').last() {
                    if let Some(note) = self.find_note_by_title(parent) {
                        let target_uri = Url::from_file_path(&note.path).ok();
                        if let Some(target_uri) = target_uri {
                            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                uri: target_uri,
                                range: Range::default(),
                            })));
                        }
                    }
                }
            }

            // Check for wiki link
            if let Some(link_target) = self.get_link_at_position(&content, position) {
                if let Some(note) = self.find_note_by_title(&link_target) {
                    let target_uri = Url::from_file_path(&note.path).ok();
                    if let Some(target_uri) = target_uri {
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: target_uri,
                            range: Range::default(),
                        })));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                self.client.log_message(MessageType::ERROR, "Failed to get file path from URI").await;
                return Ok(None);
            }
        };

        // Get link syntax from config (default to WikiLink)
        let vault_path = self.vault_path.read().unwrap().clone();
        let link_syntax = vault_path
            .as_ref()
            .and_then(|vp| Config::load(vp).ok())
            .map(|c| c.link_syntax)
            .unwrap_or(crate::config::LinkSyntax::Wiki);

        // Parse the current file directly to get its title
        let current_note = match note::parse(&path, link_syntax) {
            Ok(n) => n,
            Err(e) => {
                self.client.log_message(MessageType::ERROR, format!("Failed to parse note: {}", e)).await;
                return Ok(None);
            }
        };

        // Find all notes that link to this note
        let backlinks = self.find_backlinks(&current_note.title, &path);

        self.client.log_message(MessageType::INFO, format!("Found {} backlinks to '{}'", backlinks.len(), current_note.title)).await;

        let locations: Vec<Location> = backlinks
            .iter()
            .flat_map(|(note, links)| {
                let uri = Url::from_file_path(&note.path).ok();
                links.iter().filter_map(move |link| {
                    uri.clone().map(|uri| Location {
                        uri,
                        range: Range {
                            start: Position {
                                line: link.line,
                                character: link.start_col,
                            },
                            end: Position {
                                line: link.line,
                                character: link.end_col,
                            },
                        },
                    })
                })
            })
            .collect();

        if !locations.is_empty() {
            return Ok(Some(locations));
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let path = uri.to_file_path().ok().and_then(|p| p.canonicalize().ok());
        let content = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());

        if let Some(content) = content {
            if let Some(link_target) = self.get_link_at_position(&content, position) {
                if let Some(note) = self.find_note_by_title(&link_target) {
                    // Read first 500 chars of note content as preview
                    if let Ok(note_content) = std::fs::read_to_string(&note.path) {
                        let preview: String = note_content.chars().take(500).collect();
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: format!("## {}\n\n{}", note.title, preview),
                            }),
                            range: None,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let path = uri.to_file_path().ok().and_then(|p| p.canonicalize().ok());
        let content = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());

        if let Some(content) = content {
            let lines: Vec<&str> = content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let before_cursor = &line[..(position.character as usize).min(line.len())];

                // Check if we're in tags/trees context (frontmatter)
                if let Some((tag_prefix, prefix_start)) = self.get_tag_completion_prefix(&content, position) {
                    return Ok(Some(self.complete_tree_paths(&tag_prefix, position, prefix_start)));
                }

                // Check if we're inside [[
                if before_cursor.ends_with("[[") || before_cursor.contains("[[") {
                    let notes = self.notes.read().unwrap();
                    let items: Vec<CompletionItem> = notes
                        .values()
                        .map(|note| CompletionItem {
                            label: note.title.clone(),
                            kind: Some(CompletionItemKind::FILE),
                            detail: Some(note.path.to_string_lossy().to_string()),
                            ..Default::default()
                        })
                        .collect();

                    return Ok(Some(CompletionResponse::Array(items)));
                }
            }
        }

        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.trim();
        if query.is_empty() {
            return Ok(None);
        }

        // Do all the embedding work synchronously, collect errors
        let search_result: anyhow::Result<Vec<(embeddings::Chunk, f32)>> = (|| {
            // Initialize embeddings if needed
            self.refresh_embeddings()?;

            // Compute query embedding
            let query_embedding = {
                let mut cache = self.embedding_cache.write().unwrap();
                let mut backend = self.embedding_backend.write().unwrap();

                match (cache.as_mut(), backend.as_mut()) {
                    (Some(c), Some(b)) => c.get_or_compute(query, b.as_mut())?,
                    _ => anyhow::bail!("Embeddings not initialized"),
                }
            };

            // Search and clone results
            let store = self.embedding_store.read().unwrap();
            match store.as_ref() {
                Some(s) => Ok(s.find_similar(&query_embedding, 20)
                    .into_iter()
                    .map(|(chunk, score)| (chunk.clone(), score))
                    .collect()),
                None => anyhow::bail!("No embedding store"),
            }
        })();

        let results = match search_result {
            Ok(r) => r,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Search failed: {}", e))
                    .await;
                return Ok(None);
            }
        };

        #[allow(deprecated)]
        let symbols: Vec<SymbolInformation> = results
            .iter()
            .filter_map(|(chunk, score)| {
                let uri = Url::from_file_path(&chunk.note_path).ok()?;
                let name = if chunk.header_path.is_empty() {
                    std::path::Path::new(&chunk.note_path)
                        .file_stem()?
                        .to_str()?
                        .to_string()
                } else {
                    format!(
                        "{} > {}",
                        std::path::Path::new(&chunk.note_path).file_stem()?.to_str()?,
                        chunk.header_path.join(" > ")
                    )
                };

                Some(SymbolInformation {
                    name: format!("[{:.2}] {}", score, name),
                    kind: SymbolKind::FILE,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri,
                        range: Range::default(),
                    },
                    container_name: None,
                })
            })
            .collect();

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(symbols))
        }
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "kbase.search" => self.cmd_search(&params).await,
            "kbase.backlinks" => self.cmd_backlinks(&params).await,
            "kbase.notes" => self.cmd_notes(&params).await,
            "kbase.tags" => self.cmd_tags(&params).await,
            _ => Ok(None),
        }
    }
}

// Command implementations
impl KbaseLanguageServer {
    async fn cmd_search(&self, params: &ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        let query = params.arguments
            .get(0)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() {
            return Ok(None);
        }

        // Do semantic search
        let search_result: anyhow::Result<Vec<(embeddings::Chunk, f32)>> = (|| {
            self.refresh_embeddings()?;

            let query_embedding = {
                let mut cache = self.embedding_cache.write().unwrap();
                let mut backend = self.embedding_backend.write().unwrap();

                match (cache.as_mut(), backend.as_mut()) {
                    (Some(c), Some(b)) => c.get_or_compute(query, b.as_mut())?,
                    _ => anyhow::bail!("Embeddings not initialized"),
                }
            };

            let store = self.embedding_store.read().unwrap();
            match store.as_ref() {
                Some(s) => Ok(s.find_similar(&query_embedding, 20)
                    .into_iter()
                    .map(|(chunk, score)| (chunk.clone(), score))
                    .collect()),
                None => anyhow::bail!("No embedding store"),
            }
        })();

        let results = match search_result {
            Ok(r) => r,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Search failed: {}", e))
                    .await;
                return Ok(None);
            }
        };

        // Return as array with note info
        let locations: Vec<serde_json::Value> = results
            .iter()
            .filter_map(|(chunk, score)| {
                let uri = Url::from_file_path(&chunk.note_path).ok()?;
                let section = if chunk.header_path.is_empty() {
                    String::new()
                } else {
                    chunk.header_path.join(" > ")
                };
                Some(serde_json::json!({
                    "uri": uri.to_string(),
                    "path": chunk.note_path,
                    "line": chunk.line,
                    "score": score,
                    "section": section,
                    "preview": chunk.text.chars().take(100).collect::<String>()
                }))
            })
            .collect();

        Ok(Some(serde_json::json!(locations)))
    }

    async fn cmd_backlinks(&self, params: &ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        let note_ref = params.arguments
            .get(0)
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if note_ref.is_empty() {
            return Ok(None);
        }

        // Use persistent store
        let store = match self.get_or_init_store() {
            Ok(s) => s,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to get store: {}", e))
                    .await;
                return Ok(None);
            }
        };

        let backlinks = match store.backlinks(note_ref) {
            Ok(b) => b,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to get backlinks: {}", e))
                    .await;
                return Ok(None);
            }
        };

        // Parse backlink format "title (path)" into structured data
        let results: Vec<serde_json::Value> = backlinks
            .iter()
            .filter_map(|bl| {
                // Format is "title (path)"
                let paren_pos = bl.rfind(" (")?;
                let title = &bl[..paren_pos];
                let path = bl[paren_pos + 2..].trim_end_matches(')');
                let uri = Url::from_file_path(path).ok()?;
                Some(serde_json::json!({
                    "title": title,
                    "path": path,
                    "uri": uri.to_string()
                }))
            })
            .collect();

        Ok(Some(serde_json::json!(results)))
    }

    async fn cmd_notes(&self, params: &ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        let tag_filter = params.arguments
            .get(0)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        // Second argument: direct_only (default false = include descendants)
        let direct_only = params.arguments
            .get(1)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Use persistent store
        let store = match self.get_or_init_store() {
            Ok(s) => s,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to get store: {}", e))
                    .await;
                return Ok(None);
            }
        };

        let notes_list = match store.list_notes(tag_filter, direct_only) {
            Ok(n) => n,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to list notes: {}", e))
                    .await;
                return Ok(None);
            }
        };

        // Parse note format "title (path)" into structured data
        let results: Vec<serde_json::Value> = notes_list
            .iter()
            .filter_map(|n| {
                let paren_pos = n.rfind(" (")?;
                let title = &n[..paren_pos];
                let path = n[paren_pos + 2..].trim_end_matches(')');
                let uri = Url::from_file_path(path).ok()?;
                Some(serde_json::json!({
                    "title": title,
                    "path": path,
                    "uri": uri.to_string()
                }))
            })
            .collect();

        Ok(Some(serde_json::json!(results)))
    }

    async fn cmd_tags(&self, params: &ExecuteCommandParams) -> Result<Option<serde_json::Value>> {
        let filter = params.arguments
            .get(0)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let show_notes = params.arguments
            .get(1)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Use persistent store
        let store = match self.get_or_init_store() {
            Ok(s) => s,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to get store: {}", e))
                    .await;
                return Ok(None);
            }
        };

        let tree = match store.list_tags(filter, show_notes) {
            Ok(t) => t,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to list tags: {}", e))
                    .await;
                return Ok(None);
            }
        };

        // Get flat list of all tag paths
        let flat_tags = match store.all_tags() {
            Ok(t) => t,
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to get tags: {}", e))
                    .await;
                return Ok(None);
            }
        };

        Ok(Some(serde_json::json!({
            "tree": tree,
            "tags": flat_tags
        })))
    }
}

/// Run the LSP server
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(KbaseLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
