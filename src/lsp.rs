use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::config::Config;
use crate::note::{self, Link, Note};
use crate::vault;

/// LSP backend for kbase
pub struct KbaseLanguageServer {
    client: Client,
    /// Cached notes indexed by file path
    notes: RwLock<HashMap<PathBuf, Note>>,
    /// Vault root path
    vault_path: RwLock<Option<PathBuf>>,
}

impl KbaseLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            notes: RwLock::new(HashMap::new()),
            vault_path: RwLock::new(None),
        }
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
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "kbase LSP initialized")
            .await;
        self.refresh_notes().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, _params: DidOpenTextDocumentParams) {
        self.refresh_notes().await;
    }

    async fn did_save(&self, _params: DidSaveTextDocumentParams) {
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
                let before_cursor = &line[..position.character as usize];

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
}

/// Run the LSP server
pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(KbaseLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
