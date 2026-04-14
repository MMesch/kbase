use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing_subscriber::EnvFilter;

use kbase::{config, config::Config, config::StoreBackend, embeddings, lsp, note, schema, skills, store, vault};

/// Parse key=value pairs for --field option
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let pos = s.find('=')
        .ok_or_else(|| format!("invalid field format '{}', expected key=value", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[derive(Parser)]
#[command(name = "kbase")]
#[command(about = "Knowledge graph CLI for markdown notes")]
struct Cli {
    /// Path to vault (defaults to searching up from current directory)
    #[arg(short, long, global = true)]
    vault: Option<PathBuf>,

    /// Store backend: nquads (fast, default), rocksdb (slow), fresh (no cache)
    #[arg(long, global = true, value_enum)]
    store: Option<StoreBackendArg>,

    /// Show timing information for operations
    #[arg(long, global = true)]
    time: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, ValueEnum)]
enum StoreBackendArg {
    /// N-Quads file cache (fast startup, ~5ms)
    Nquads,
    /// RocksDB persistent store (slow startup, ~400ms)
    Rocksdb,
    /// Fresh in-memory, rebuild from files each time
    Fresh,
}

impl From<StoreBackendArg> for StoreBackend {
    fn from(arg: StoreBackendArg) -> Self {
        match arg {
            StoreBackendArg::Nquads => StoreBackend::Nquads,
            StoreBackendArg::Rocksdb => StoreBackend::Rocksdb,
            StoreBackendArg::Fresh => StoreBackend::Fresh,
        }
    }
}

/// Timer helper for benchmarking
struct Timer {
    enabled: bool,
    start: Instant,
    last: Instant,
}

impl Timer {
    fn new(enabled: bool) -> Self {
        let now = Instant::now();
        Self { enabled, start: now, last: now }
    }

    fn lap(&mut self, label: &str) {
        if self.enabled {
            let now = Instant::now();
            eprintln!("[{:>8.2?}] {}", now.duration_since(self.last), label);
            self.last = now;
        }
    }

    fn total(&self) {
        if self.enabled {
            eprintln!("[{:>8.2?}] total", Instant::now().duration_since(self.start));
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new vault
    Init {
        /// Path to initialize (defaults to current directory)
        path: Option<PathBuf>,
    },
    /// Create a new note
    New {
        /// Note title
        title: String,
        /// Tags to add to the new note (can be repeated: -t foo -t bar)
        #[arg(short, long = "tag")]
        tags: Vec<String>,
        /// Frontmatter fields as key=value (can be repeated: -f status=draft -f author=me)
        #[arg(short, long = "field", value_parser = parse_key_value)]
        fields: Vec<(String, String)>,
        /// Open the note in $EDITOR after creation
        #[arg(short, long)]
        edit: bool,
    },
    /// List notes
    List {
        /// Filter by tag (includes descendants)
        #[arg(long)]
        tag: Option<String>,
    },
    /// Show backlinks to a note
    Backlinks {
        /// Note identifier (title)
        note: String,
    },
    /// Show tag hierarchy as a tree
    Tags {
        /// Filter to specific tag subtree
        tag: Option<String>,
        /// Show notes under each tag
        #[arg(short, long)]
        notes: bool,
    },
    /// Validate notes against schema
    Validate {
        /// Check that file locations match tags (based on organize_root)
        #[arg(long)]
        structure: bool,
    },
    /// Find similar notes using embeddings
    ///
    /// Requires ONNX model files (model.onnx, tokenizer.json) in one of:
    /// ./models, <vault>/models, or $XDG_CACHE_HOME/kbase/models
    Similar {
        /// Note to find similar notes for (title or path)
        note: String,
        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    /// Semantic search across notes
    ///
    /// Requires ONNX model files (model.onnx, tokenizer.json) in one of:
    /// ./models, <vault>/models, or $XDG_CACHE_HOME/kbase/models
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
        /// Minimum similarity score threshold (0.0-1.0)
        #[arg(short, long, default_value = "0.0")]
        threshold: f32,
        /// Preview length in characters (0 for full chunk)
        #[arg(short, long, default_value = "1000")]
        preview: usize,
        /// Output format (text, json, quickfix)
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Start LSP server (for editor integration)
    Lsp,
    /// Install Claude Code skills for this vault
    InstallSkills,
    /// Export vault graph to DOT or GraphML format
    Export {
        /// Output format: dot or graphml
        #[arg(short, long, default_value = "dot")]
        format: String,
        /// Output file (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Filter to only show links of this frontmatter field type (e.g., depends_on)
        #[arg(long)]
        link_type: Option<String>,
    },
    /// Show vault overview (tags, link structure, key notes)
    Overview {
        /// Number of top notes to show for each metric
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    /// Organize notes into directories based on tag tree
    ///
    /// Moves notes into a directory structure matching their tag hierarchy.
    /// Fails if conflicts are detected (user must resolve manually).
    Organize {
        /// Tag tree prefix to organize by (e.g., "domain" for domain/*)
        #[arg(long)]
        tree: Option<String>,
        /// Keep flat structure (no directories, just move to vault root)
        #[arg(long)]
        flat: bool,
        /// Create symlinks for notes with multiple tag paths
        #[arg(long)]
        symlinks: bool,
        /// Preview changes without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Rename tags by replacing a prefix across all notes
    ///
    /// Examples:
    ///   kbase retag tech/ai type       # tech/ai -> type, tech/ai/ml -> type/ml
    ///   kbase retag old/path new/path   # old/path/x -> new/path/x
    Retag {
        /// Old tag prefix to replace
        from: String,
        /// New tag prefix
        to: String,
        /// Move files to match new tag structure (based on organize_root)
        #[arg(long, short = 'm')]
        r#move: bool,
        /// Preview changes without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Move/rename a note, updating links and folder location
    Move {
        /// Current note (title or path)
        from: String,
        /// New title
        to: String,
        /// Preview changes without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Clean orphan tags (tags in graph with no notes)
    CleanTags {
        /// Preview without deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// Convert links between wiki and markdown syntax, or normalize link paths
    ///
    /// Wiki:      [[Note Title]] or [[Note Title|alias]]
    /// Markdown:  [Note Title](note-title.md) or [alias](note-title.md)
    /// Normalize: rewrite markdown link paths using the configured link_base
    Convert {
        /// Target syntax: "wiki", "markdown", or "normalize"
        to: String,
        /// Dry run - show changes without modifying files
        #[arg(long)]
        dry_run: bool,
    },
    /// Run a SPARQL query on the knowledge graph
    ///
    /// Use "schema" as query to see full schema documentation.
    ///
    /// Schema (prefix kb: auto-added):
    ///   Notes:  kb:note/{title} with kb:type kb:Note
    ///   Props:  kb:title, kb:path, kb:linksTo, kb:hasTag, kb:mtime
    ///   Tags:   kb:tag/{name} with kb:type kb:Tag, kb:parentTag
    ///   Trees:  kb:child/{tree} predicate links child -> parent
    ///
    /// Multiline queries:
    ///
    ///   bash/zsh (single quotes):
    ///     kbase query 'SELECT ?title WHERE {
    ///       ?n kb:type kb:Note ; kb:title ?title .
    ///     }'
    ///
    ///   bash/zsh (heredoc):
    ///     kbase query "$(cat <<'EOF'
    ///     SELECT ?title WHERE { ?n kb:title ?title }
    ///     EOF
    ///     )"
    ///
    ///   nushell:
    ///     kbase query 'SELECT ?title WHERE {
    ///       ?n kb:type kb:Note ; kb:title ?title .
    ///     }'
    ///
    ///   fish:
    ///     kbase query 'SELECT ?title WHERE {
    ///       ?n kb:type kb:Note ; kb:title ?title .
    ///     }'
    ///
    ///   PowerShell:
    ///     kbase query @'
    ///     SELECT ?title WHERE { ?n kb:title ?title }
    ///     '@
    ///
    ///   Or use --file to read from a file:
    ///     kbase query --file query.sparql
    Query {
        /// SPARQL SELECT query (kb: prefix auto-added), or "schema" for docs
        #[arg(conflicts_with = "file")]
        sparql: Option<String>,
        /// Read query from file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    // Initialize tracing (controlled by RUST_LOG env var, e.g. RUST_LOG=debug)
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let mut timer = Timer::new(cli.time);

    let get_vault = || -> Result<PathBuf> {
        match &cli.vault {
            Some(p) => Ok(p.clone()),
            None => vault::find_vault_root(),
        }
    };

    // Load store with configured or overridden backend
    let load_store = |vault_path: &PathBuf, timer: &mut Timer| -> Result<store::Store> {
        let backend = match cli.store {
            Some(arg) => arg.into(),
            None => Config::load(vault_path)?.store,
        };
        timer.lap(&format!("loading {:?} store", backend));
        let store = vault::load_with_backend(vault_path, backend)?;
        timer.lap("store ready");
        Ok(store)
    };

    // Default to Overview if no command provided
    let command = cli.command.unwrap_or(Commands::Overview { limit: 5 });

    match command {
        Commands::Init { path } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            vault::init(&path)?;
            println!("Initialized kbase vault in {}", path.display());
        }
        Commands::New { title, tags, fields, edit } => {
            let vault_path = get_vault()?;
            let config = Config::load(&vault_path)?;
            let notes_path = config.notes_path(&vault_path);

            // Start with config default tags, then CLI tags
            let mut all_tags: Vec<String> = config.new_note.tags.clone();
            all_tags.extend(tags.iter().cloned());

            // Infer tag from current working directory
            if config.new_note.infer_tag_from_cwd {
                if let Ok(cwd) = std::env::current_dir() {
                    if let Ok(rel) = cwd.strip_prefix(&notes_path) {
                        let folder_path = rel.to_string_lossy().replace('\\', "/");
                        if !folder_path.is_empty() {
                            let inferred_tag = config.tag_for_folder(&folder_path);
                            if !all_tags.contains(&inferred_tag) {
                                all_tags.push(inferred_tag);
                            }
                        }
                    }
                }
            }

            // Merge config default fields with CLI fields (CLI overrides)
            let mut all_fields = config.new_note.fields.clone();
            for (k, v) in fields {
                all_fields.insert(k, v);
            }

            // Determine folder from tags based on organize_root
            let folder = config.folder_for_tags(&all_tags);

            let note_path = note::create(&notes_path, &title, &all_tags, &all_fields, folder.as_deref())?;
            println!("Created {}", note_path.display());

            if edit {
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                std::process::Command::new(&editor)
                    .arg(&note_path)
                    .status()
                    .with_context(|| format!("Failed to open {} with {}", note_path.display(), editor))?;
            }
        }
        Commands::List { tag } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;
            let notes = store.list_notes(tag.as_deref(), false)?;
            timer.lap("list_notes");
            for note in notes {
                println!("{}", note);
            }
        }
        Commands::Backlinks { note } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;
            let backlinks = store.backlinks(&note)?;
            timer.lap("backlinks");
            for link in backlinks {
                println!("{}", link);
            }
        }
        Commands::Tags { tag, notes } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;
            let tree = store.list_tags(tag.as_deref(), notes)?;
            timer.lap("list_tags");
            for line in tree {
                println!("{}", line);
            }
        }
        Commands::Validate { structure } => {
            let vault_path = get_vault()?;
            let config = Config::load(&vault_path)?;
            let notes_path = config.notes_path(&vault_path);
            let schema = schema::Schema::load(&vault_path)?;
            timer.lap("load schema");
            let notes = vault::load_notes(&vault_path)?;
            timer.lap("load notes");

            let mut total_violations = 0;

            // Schema validation (per-note)
            for parsed_note in &notes {
                let violations = schema.validate(parsed_note);
                for v in &violations {
                    println!("{}:{}: {}", v.note_path, v.field, v.message);
                    total_violations += 1;
                }
            }
            timer.lap("schema validation");

            // Broken link detection
            let titles: std::collections::HashSet<String> = notes.iter()
                .map(|n| n.title.to_lowercase())
                .collect();
            for n in &notes {
                for link in &n.links {
                    let found = match config.link_syntax {
                        config::LinkSyntax::Wiki => {
                            titles.contains(&link.target.to_lowercase())
                        }
                        config::LinkSyntax::Markdown | config::LinkSyntax::Both => {
                            // For markdown links, try resolving the path
                            if link.target.ends_with(".md") {
                                note::resolve_link_target(&link.target, &n.path, &notes, Some(&vault_path)).is_some()
                            } else {
                                // Wiki-style target in Both mode
                                titles.contains(&link.target.to_lowercase())
                            }
                        }
                    };
                    if !found {
                        println!("{}:{}: broken link to '{}'", n.path.display(), link.line + 1, link.target);
                        total_violations += 1;
                    }
                }
            }
            timer.lap("broken links");

            // Structure validation: check file locations match tags
            if structure {
                for n in &notes {
                    if let Some(expected_folder) = config.folder_for_tags(&n.tags) {
                        let actual_folder = n.path.parent()
                            .and_then(|p| p.strip_prefix(&notes_path).ok())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();

                        let expected_normalized = expected_folder.replace('\\', "/");
                        let actual_normalized = actual_folder.replace('\\', "/");

                        if expected_normalized != actual_normalized {
                            println!(
                                "{}:structure: expected folder '{}' but found '{}'",
                                n.path.display(),
                                expected_normalized,
                                actual_normalized
                            );
                            total_violations += 1;
                        }
                    }
                }
                timer.lap("structure validation");
            }

            // Graph constraints (SPARQL)
            if !schema.constraints.is_empty() {
                let store = load_store(&vault_path, &mut timer)?;
                let graph_violations = store.validate_constraints(&schema.constraints)?;
                timer.lap("graph constraints");
                for (_title, path, message) in &graph_violations {
                    println!("{}:constraint: {}", path, message);
                    total_violations += 1;
                }
            }

            if total_violations == 0 {
                println!("All {} notes valid", notes.len());
            } else {
                println!("\n{} violation(s) in {} notes", total_violations, notes.len());
                std::process::exit(1);
            }
        }
        Commands::Similar { note, limit } => {
            let vault_path = get_vault()?;
            let config = Config::load(&vault_path)?;
            let notes = vault::load_notes(&vault_path)?;

            // Find the target note
            let target = notes
                .iter()
                .find(|n| n.title.to_lowercase().contains(&note.to_lowercase())
                    || n.path.to_string_lossy().contains(&note))
                .ok_or_else(|| anyhow::anyhow!("Note not found: {}", note))?;

            println!("Finding notes similar to: {}", target.title);
            println!();

            // Build embeddings with cache
            let chunk_level = embeddings::ChunkLevel::from_str(&config.embeddings.chunk_level);
            let cache_path = vault_path.join(".kbase").join("embeddings.redb");
            let cache = embeddings::EmbeddingCache::open(&cache_path)?;
            let mut store = embeddings::EmbeddingStore::new();

            // Create backend based on config
            let mut backend: Box<dyn embeddings::EmbeddingBackend> = match config.embeddings.backend {
                config::EmbeddingBackend::Onnx => {
                    let model_dir = embeddings::OnnxBackend::find_model_dir(&vault_path)?;
                    Box::new(embeddings::OnnxBackend::new(&model_dir)?)
                }
                config::EmbeddingBackend::Ollama => {
                    Box::new(embeddings::OllamaBackend::new(None)?)
                }
            };

            let include_context = config.embeddings.include_context;

            // Embed all notes (using cache)
            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let chunks = embeddings::split_into_chunks(&content, chunk_level, Some(&n.title), include_context);

                let texts: Vec<&str> = chunks.iter().map(|(_, text, _)| text.as_str()).collect();
                let vectors = cache.get_or_compute_batch(&texts, &mut *backend)?;

                for ((headers, text, line), embedding) in chunks.into_iter().zip(vectors) {
                    store.add_chunk(embeddings::Chunk {
                        note_path: n.path.to_string_lossy().to_string(),
                        header_path: headers,
                        text,
                        embedding,
                        line,
                    });
                }
            }

            // Get embedding for target note
            let target_chunks = store.chunks_for_note(&target.path.to_string_lossy());
            if target_chunks.is_empty() {
                anyhow::bail!("No chunks found for target note");
            }

            // Average the embeddings of target chunks
            let dim = backend.dimension();
            let mut avg_embedding = vec![0.0f32; dim];
            for chunk in &target_chunks {
                for (i, v) in chunk.embedding.iter().enumerate() {
                    avg_embedding[i] += v;
                }
            }
            let count = target_chunks.len() as f32;
            for v in &mut avg_embedding {
                *v /= count;
            }

            // Find similar (excluding the target note)
            let similar = store.find_similar_excluding(
                &avg_embedding,
                &target.path.to_string_lossy(),
                limit,
            );

            for (chunk, score) in similar {
                let header = if chunk.header_path.is_empty() {
                    String::new()
                } else {
                    format!(" > {}", chunk.header_path.join(" > "))
                };
                println!("{:.3}  {}{}", score, chunk.note_path, header);
            }
        }
        Commands::Search { query, limit, threshold, preview, format } => {
            let vault_path = get_vault()?;
            let config = Config::load(&vault_path)?;
            let notes = vault::load_notes(&vault_path)?;

            // Build embeddings with cache
            let chunk_level = embeddings::ChunkLevel::from_str(&config.embeddings.chunk_level);
            let cache_path = vault_path.join(".kbase").join("embeddings.redb");
            let cache = embeddings::EmbeddingCache::open(&cache_path)?;
            let mut store = embeddings::EmbeddingStore::new();

            // Create backend based on config
            let mut backend: Box<dyn embeddings::EmbeddingBackend> = match config.embeddings.backend {
                config::EmbeddingBackend::Onnx => {
                    let model_dir = embeddings::OnnxBackend::find_model_dir(&vault_path)?;
                    Box::new(embeddings::OnnxBackend::new(&model_dir)?)
                }
                config::EmbeddingBackend::Ollama => {
                    Box::new(embeddings::OllamaBackend::new(None)?)
                }
            };

            let include_context = config.embeddings.include_context;

            // Embed all notes (using cache)
            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let chunks = embeddings::split_into_chunks(&content, chunk_level, Some(&n.title), include_context);

                let texts: Vec<&str> = chunks.iter().map(|(_, text, _)| text.as_str()).collect();
                let vectors = cache.get_or_compute_batch(&texts, &mut *backend)?;

                for ((headers, text, line), embedding) in chunks.into_iter().zip(vectors) {
                    store.add_chunk(embeddings::Chunk {
                        note_path: n.path.to_string_lossy().to_string(),
                        header_path: headers,
                        text,
                        embedding,
                        line,
                    });
                }
            }

            // Embed the query (also cached)
            let query_embedding = cache.get_or_compute(&query, &mut *backend)?;

            // Find similar chunks and filter by threshold
            let results: Vec<_> = store.find_similar(&query_embedding, limit)
                .into_iter()
                .filter(|(_, score)| *score >= threshold)
                .collect();

            // Helper to truncate text for preview
            let truncate = |text: &str| -> String {
                if preview == 0 {
                    text.to_string()
                } else {
                    text.chars().take(preview).collect()
                }
            };

            match format.as_str() {
                "json" => {
                    let json_results: Vec<serde_json::Value> = results
                        .iter()
                        .map(|(chunk, score)| {
                            serde_json::json!({
                                "path": chunk.note_path,
                                "score": score,
                                "section": chunk.header_path.join(" > "),
                                "line": chunk.line,
                                "preview": truncate(&chunk.text)
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string(&json_results)?);
                }
                "quickfix" => {
                    // Format: file:line:col:text (vim quickfix format)
                    for (chunk, score) in &results {
                        let section = if chunk.header_path.is_empty() {
                            String::new()
                        } else {
                            format!(" > {}", chunk.header_path.join(" > "))
                        };
                        let text = truncate(&chunk.text).replace('\n', " ");
                        println!("{}:{}:1:[{:.2}]{} {}", chunk.note_path, chunk.line, score, section, text);
                    }
                }
                _ => {
                    println!("Results for: {}", query);
                    println!();

                    for (chunk, score) in results {
                        let header = if chunk.header_path.is_empty() {
                            String::new()
                        } else {
                            format!(" > {}", chunk.header_path.join(" > "))
                        };
                        println!("{:.3}  {}{}", score, chunk.note_path, header);
                        // Show preview of text
                        let text = truncate(&chunk.text).replace('\n', " ");
                        println!("       {}", text);
                        println!();
                    }
                }
            }
        }
        Commands::Lsp => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(lsp::run_server());
        }
        Commands::InstallSkills => {
            let vault_path = get_vault()?;
            let installed = skills::install(&vault_path)?;
            for skill in &installed {
                println!("Installed /{}", skill);
            }
            let cwd = std::env::current_dir()?;
            println!("\nSkills installed to {}/.claude/skills/", cwd.display());
        }
        Commands::Export { format, output, link_type } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;

            let content = match format.as_str() {
                "dot" => store.export_dot(link_type.as_deref())?,
                "graphml" => store.export_graphml()?,
                _ => anyhow::bail!("Unknown format: {}. Use 'dot' or 'graphml'", format),
            };
            timer.lap("export");

            match output {
                Some(path) => {
                    std::fs::write(&path, &content)?;
                    eprintln!("Exported to {}", path.display());
                }
                None => print!("{}", content),
            }
        }
        Commands::Overview { limit } => {
            let vault_path = get_vault()?;
            let notes = vault::load_notes(&vault_path)?;
            timer.lap("load notes");
            let store = load_store(&vault_path, &mut timer)?;

            // Count notes
            println!("## Vault Overview\n");
            println!("**Notes:** {}\n", notes.len());

            // Tag summary
            let tags = store.list_tags(None, false)?;
            println!("**Tags:** {} unique tags\n", tags.len());
            if !tags.is_empty() {
                println!("### Top-level tags\n");
                for tag in tags.iter().filter(|t| !t.contains('/')) {
                    println!("- {}", tag);
                }
                println!();
            }

            // Build link degree maps
            // in_degree: how many notes link TO this note (backlinks)
            // out_degree: how many notes this note links TO
            let mut in_degree: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut out_degree: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            for note in &notes {
                out_degree.insert(note.title.clone(), note.links.len());
                for link in &note.links {
                    *in_degree.entry(link.target.clone()).or_insert(0) += 1;
                }
            }

            // Top by in-degree (most backlinked = important concepts)
            println!("### Most referenced (in-degree = backlinks)\n");
            println!("_Notes that many other notes link to - likely key concepts_\n");
            let mut by_in: Vec<_> = in_degree.iter().collect();
            by_in.sort_by(|a, b| b.1.cmp(a.1));
            for (title, count) in by_in.iter().take(limit) {
                println!("- **{}** ({} backlinks)", title, count);
            }
            println!();

            // Top by out-degree (most outlinks = index/MOC notes)
            println!("### Most referencing (out-degree = outlinks)\n");
            println!("_Notes that link to many others - likely index or MOC notes_\n");
            let mut by_out: Vec<_> = out_degree.iter().collect();
            by_out.sort_by(|a, b| b.1.cmp(a.1));
            for (title, count) in by_out.iter().take(limit) {
                println!("- **{}** ({} outlinks)", title, count);
            }

            // Orphans (no links in or out)
            let orphans: Vec<_> = notes.iter()
                .filter(|n| n.links.is_empty() && !in_degree.contains_key(&n.title))
                .collect();
            if !orphans.is_empty() {
                println!("\n### Orphan notes (no links)\n");
                for note in orphans.iter().take(limit) {
                    println!("- {}", note.title);
                }
                if orphans.len() > limit {
                    println!("- ... and {} more", orphans.len() - limit);
                }
            }
        }
        Commands::Organize { tree, flat, symlinks, dry_run } => {
            let vault_path = get_vault()?;
            let cfg = config::Config::load(&vault_path)?;
            let notes_base = cfg.notes_path(&vault_path);
            let notes = vault::load_notes(&vault_path)?;

            // Use CLI --tree or fall back to config organize_root
            let tree_filter = tree.as_deref().unwrap_or(&cfg.organize_root);

            // Build a map of note paths to their target locations
            let mut moves: Vec<(PathBuf, PathBuf, String)> = Vec::new(); // (from, to, title)
            let mut conflicts: Vec<String> = Vec::new();
            let mut multi_path: Vec<(String, Vec<String>)> = Vec::new(); // (title, [tags])

            for n in &notes {
                // Get folder paths for matching tags
                let matching_folders: Vec<(String, &String)> = if tree_filter == "/" {
                    // All hierarchical tags
                    n.tags.iter()
                        .filter_map(|t| cfg.folder_for_tag(t).map(|f| (f, t)))
                        .collect()
                } else {
                    // Only tags matching the tree filter
                    n.tags.iter()
                        .filter(|t| t.starts_with(tree_filter) && t.len() > tree_filter.len())
                        .filter_map(|t| cfg.folder_for_tag(t).map(|f| (f, t)))
                        .collect()
                };

                if matching_folders.is_empty() {
                    continue; // Note doesn't match tree filter
                }

                // Check for multi-path within the filtered tree
                if matching_folders.len() > 1 {
                    multi_path.push((
                        n.title.clone(),
                        matching_folders.iter().map(|(_, t)| t.to_string()).collect(),
                    ));
                }

                // Use first matching folder
                let (primary_folder, _primary_tag) = &matching_folders[0];

                let target_path = if flat {
                    // Flat: keep in notes root
                    let filename = n.path.file_name().unwrap();
                    notes_base.join(filename)
                } else {
                    // Tree: create directory structure from folder path
                    let folder_path: PathBuf = primary_folder.split('/').collect();
                    let filename = n.path.file_name().unwrap();
                    notes_base.join(folder_path).join(filename)
                };

                if target_path != n.path {
                    moves.push((n.path.clone(), target_path, n.title.clone()));
                }
            }

            // Check for filename conflicts (multiple notes targeting same path)
            let mut target_counts: std::collections::HashMap<PathBuf, Vec<String>> =
                std::collections::HashMap::new();
            for (_, target, title) in &moves {
                target_counts.entry(target.clone()).or_default().push(title.clone());
            }
            for (target, titles) in &target_counts {
                if titles.len() > 1 {
                    conflicts.push(format!(
                        "Multiple notes target {}: {}",
                        target.display(),
                        titles.join(", ")
                    ));
                }
            }

            // Report multi-path notes
            if !multi_path.is_empty() {
                println!("## Multi-path notes\n");
                println!("These notes have multiple tags in the selected tree:");
                for (title, tags) in &multi_path {
                    println!("  {} -> {} (using first)", title, tags.join(", "));
                }
                if !symlinks {
                    println!("\nUse --symlinks to create links at secondary locations.\n");
                }
            }

            // Report conflicts and abort
            if !conflicts.is_empty() {
                println!("## Conflicts detected\n");
                for c in &conflicts {
                    println!("  {}", c);
                }
                println!("\nResolve conflicts before organizing (rename notes or adjust tags).");
                anyhow::bail!("Cannot organize: {} conflicts found", conflicts.len());
            }

            // Report or execute moves
            if moves.is_empty() {
                println!("No files to move.");
            } else {
                println!("## {}\n", if dry_run { "Would move" } else { "Moving" });

                for (from, to, _title) in &moves {
                    let from_rel = from.strip_prefix(&vault_path).unwrap_or(from);
                    let to_rel = to.strip_prefix(&vault_path).unwrap_or(to);
                    println!("  {} -> {}", from_rel.display(), to_rel.display());

                    if !dry_run {
                        // Create target directory
                        if let Some(parent) = to.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        // Move file
                        std::fs::rename(from, to)?;
                    }
                }

                // Create symlinks for multi-path notes if requested
                if symlinks && !multi_path.is_empty() {
                    println!("\n## {}\n", if dry_run { "Would create symlinks" } else { "Creating symlinks" });
                    for (title, tags) in &multi_path {
                        // Find the note that was moved
                        if let Some(n) = notes.iter().find(|n| &n.title == title) {
                            let primary_tag = &tags[0];
                            for secondary_tag in &tags[1..] {
                                let symlink_dir: PathBuf = secondary_tag.split('/').collect();
                                let symlink_path = notes_base.join(symlink_dir).join(n.path.file_name().unwrap());

                                let primary_dir: PathBuf = primary_tag.split('/').collect();
                                let target = notes_base.join(primary_dir).join(n.path.file_name().unwrap());

                                println!("  {} -> {}", symlink_path.display(), target.display());

                                if !dry_run {
                                    if let Some(parent) = symlink_path.parent() {
                                        std::fs::create_dir_all(parent)?;
                                    }
                                    #[cfg(unix)]
                                    std::os::unix::fs::symlink(&target, &symlink_path)?;
                                    #[cfg(windows)]
                                    std::os::windows::fs::symlink_file(&target, &symlink_path)?;
                                }
                            }
                        }
                    }
                }

                // Update markdown links in all notes to reflect new paths
                let link_updates = update_links_after_moves(&vault_path, &moves, dry_run)?;
                if link_updates > 0 {
                    println!("\n{} link(s) {}", link_updates, if dry_run { "would be updated" } else { "updated" });
                }

                println!("\n{} files {}", moves.len(), if dry_run { "would be moved" } else { "moved" });
            }
        }
        Commands::Retag { from, to, r#move, dry_run } => {
            let vault_path = get_vault()?;
            let cfg = Config::load(&vault_path)?;
            let notes_path = cfg.notes_path(&vault_path);
            let notes = vault::load_notes(&vault_path)?;

            let mut total_changes = 0;
            let mut moves: Vec<(PathBuf, PathBuf, String)> = Vec::new();

            for n in &notes {
                // Only process notes that have a matching tag
                if !n.tags.iter().any(|t| t == &from || t.starts_with(&format!("{}/", from))) {
                    continue;
                }

                let content = std::fs::read_to_string(&n.path)?;
                let new_content = retag_frontmatter(&content, &from, &to);

                if content != new_content {
                    total_changes += 1;
                    if dry_run {
                        println!("Would modify: {}", n.path.display());
                        for (i, (old, new)) in content.lines().zip(new_content.lines()).enumerate() {
                            if old != new {
                                println!("  L{}: {} -> {}", i + 1, old.trim(), new.trim());
                            }
                        }
                    } else {
                        std::fs::write(&n.path, &new_content)?;
                        println!("Modified: {}", n.path.display());
                    }

                    // Calculate new file location if --move is set
                    if r#move {
                        // Compute new tags after retag
                        let new_tags: Vec<String> = n.tags.iter().map(|t| {
                            if t == &from {
                                to.clone()
                            } else if let Some(rest) = t.strip_prefix(&format!("{}/", from)) {
                                format!("{}/{}", to, rest)
                            } else {
                                t.clone()
                            }
                        }).collect();

                        // Get folder for new tags
                        if let Some(new_folder) = cfg.folder_for_tags(&new_tags) {
                            let filename = n.path.file_name().unwrap();
                            let new_path = notes_path.join(&new_folder).join(filename);
                            if new_path != n.path {
                                moves.push((n.path.clone(), new_path, n.title.clone()));
                            }
                        }
                    }
                }
            }

            // Execute moves if --move is set
            if r#move && !moves.is_empty() {
                println!("\n## {}\n", if dry_run { "Would move" } else { "Moving" });
                for (from_path, to_path, _title) in &moves {
                    let from_rel = from_path.strip_prefix(&vault_path).unwrap_or(from_path);
                    let to_rel = to_path.strip_prefix(&vault_path).unwrap_or(to_path);
                    println!("  {} -> {}", from_rel.display(), to_rel.display());

                    if !dry_run {
                        if let Some(parent) = to_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::rename(from_path, to_path)?;
                    }
                }

                // Update links after moves
                let link_updates = update_links_after_moves(&vault_path, &moves, dry_run)?;
                if link_updates > 0 {
                    println!("\n{} link(s) {}", link_updates, if dry_run { "would be updated" } else { "updated" });
                }

                println!("\n{} files {}", moves.len(), if dry_run { "would be moved" } else { "moved" });
            }

            if dry_run {
                println!("\n{} files would be modified", total_changes);
            } else {
                println!("\n{} files modified", total_changes);
            }
        }
        Commands::Move { from, to, dry_run } => {
            let vault_path = get_vault()?;
            let cfg = Config::load(&vault_path)?;
            let notes_path = cfg.notes_path(&vault_path);
            let notes = vault::load_notes(&vault_path)?;

            // Find the source note
            let source = notes.iter()
                .find(|n| n.title.to_lowercase() == from.to_lowercase()
                    || n.path.to_string_lossy().contains(&from))
                .ok_or_else(|| anyhow::anyhow!("Note not found: {}", from))?;

            // Strip .md if provided
            let new_title = to.strip_suffix(".md").unwrap_or(&to);
            let new_filename = format!("{}.md", note::slugify(new_title));

            // Compute new path based on tags and organize_root
            let new_folder = cfg.folder_for_tags(&source.tags);
            let new_path = match new_folder {
                Some(folder) => notes_path.join(&folder).join(&new_filename),
                None => notes_path.join(&new_filename),
            };

            if new_path.exists() && new_path != source.path {
                anyhow::bail!("Target already exists: {}", new_path.display());
            }

            // Update title in frontmatter
            let content = std::fs::read_to_string(&source.path)?;
            let new_content = update_frontmatter_title(&content, new_title);

            let from_rel = source.path.strip_prefix(&vault_path).unwrap_or(&source.path);
            let to_rel = new_path.strip_prefix(&vault_path).unwrap_or(&new_path);

            if dry_run {
                println!("Would rename: {} -> {}", from_rel.display(), to_rel.display());
                if content != new_content {
                    println!("Would update title in frontmatter");
                }
            } else {
                // Write updated content
                std::fs::write(&source.path, &new_content)?;

                // Move file if path changed
                if new_path != source.path {
                    if let Some(parent) = new_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(&source.path, &new_path)?;
                }

                println!("Renamed: {} -> {}", from_rel.display(), to_rel.display());
            }

            // Update links in other notes
            if new_path != source.path {
                let moves = vec![(source.path.clone(), new_path.clone(), source.title.clone())];
                let link_updates = update_links_after_moves(&vault_path, &moves, dry_run)?;
                if link_updates > 0 {
                    println!("{} link(s) {}", link_updates, if dry_run { "would be updated" } else { "updated" });
                }
            }
        }
        Commands::CleanTags { dry_run } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;

            // Get all tag paths from the graph
            let all_tags = store.get_all_tag_paths()?;

            // Get tags actually used by notes (from disk)
            let notes = vault::load_notes(&vault_path)?;
            let mut used_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
            for n in &notes {
                for tag in &n.tags {
                    // Add the tag and all its ancestors, slugifying each segment
                    // to match how they are stored in the graph.
                    let parts: Vec<String> = tag.split('/').map(note::slugify).collect();
                    for i in 1..=parts.len() {
                        used_tags.insert(parts[..i].join("/"));
                    }
                }
            }

            // Find orphan tags
            let orphan_tags: Vec<String> = all_tags
                .iter()
                .filter(|t| !used_tags.contains(t.as_str()))
                .cloned()
                .collect();

            if orphan_tags.is_empty() {
                println!("No orphan tags found.");
            } else {
                println!("## Orphan tags{}\n", if dry_run { " (dry run)" } else { "" });
                for tag in &orphan_tags {
                    println!("  {}", tag);
                }

                if !dry_run {
                    let removed = store.remove_orphan_tag_nodes(&orphan_tags)?;
                    store.save()?;
                    println!("\nRemoved {} orphan tags.", removed);
                } else {
                    println!("\n{} orphan tags would be removed.", orphan_tags.len());
                }
            }
        }
        Commands::Convert { to, dry_run } => {
            let vault_path = get_vault()?;
            let notes = vault::load_notes(&vault_path)?;
            let cfg = config::Config::load(&vault_path)?;

            enum ConvertMode { ToMarkdown, ToWiki, Normalize }
            let mode = match to.to_lowercase().as_str() {
                "markdown" | "md" => ConvertMode::ToMarkdown,
                "wiki" => ConvertMode::ToWiki,
                "normalize" | "norm" => ConvertMode::Normalize,
                _ => anyhow::bail!("Unknown target: {}. Use 'wiki', 'markdown', or 'normalize'", to),
            };

            let mut total_changes = 0;

            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let new_content = match &mode {
                    ConvertMode::ToMarkdown => convert_wiki_to_markdown(&content, &notes),
                    ConvertMode::ToWiki => convert_markdown_to_wiki(&content),
                    ConvertMode::Normalize => normalize_markdown_links(&content, &n.path, &notes, &vault_path, cfg.link_base),
                };

                if content != new_content {
                    total_changes += 1;
                    if dry_run {
                        println!("Would modify: {}", n.path.display());
                        for (i, (old, new)) in content.lines().zip(new_content.lines()).enumerate() {
                            if old != new {
                                println!("  L{}: {} -> {}", i + 1, old.trim(), new.trim());
                            }
                        }
                    } else {
                        std::fs::write(&n.path, &new_content)?;
                        println!("Modified: {}", n.path.display());
                    }
                }
            }

            if dry_run {
                println!("\n{} files would be modified", total_changes);
            } else {
                println!("\n{} files modified", total_changes);
            }
        }
        Commands::Query { sparql, file } => {
            let vault_path = get_vault()?;
            let store = load_store(&vault_path, &mut timer)?;

            // Get query from file or argument
            let query = match (sparql, file) {
                (Some(q), None) => q,
                (None, Some(path)) => std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?,
                (None, None) => anyhow::bail!("Provide a query or use --file"),
                (Some(_), Some(_)) => unreachable!(), // clap handles this
            };

            // Handle special "schema" query to show documentation
            if query.trim().to_lowercase() == "schema" {
                print!("{}", store::Store::schema_doc());
                return Ok(());
            }

            let results = store.query(&query)?;
            timer.lap("query");

            if results.is_empty() {
                println!("No results");
            } else {
                // Print header from first row
                if let Some(first) = results.first() {
                    let headers: Vec<_> = first.iter().map(|(k, _)| k.as_str()).collect();
                    println!("{}", headers.join("\t"));
                    println!("{}", headers.iter().map(|h| "-".repeat(h.len())).collect::<Vec<_>>().join("\t"));
                }

                // Print rows
                for row in &results {
                    let values: Vec<_> = row.iter().map(|(_, v)| v.as_str()).collect();
                    println!("{}", values.join("\t"));
                }

                eprintln!("\n({} results)", results.len());
            }
        }
    }

    timer.total();
    Ok(())
}

/// Convert wiki links [[Title]] to markdown [Title](slug.md)
fn convert_wiki_to_markdown(content: &str, notes: &[note::Note]) -> String {
    use regex::Regex;

    // Build title -> slug map
    let title_to_slug: std::collections::HashMap<String, String> = notes
        .iter()
        .map(|n| {
            let slug = n.path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&n.title)
                .to_string();
            (n.title.to_lowercase(), slug)
        })
        .collect();

    // Match [[Title]] or [[Title|Alias]]
    let re = Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").unwrap();

    re.replace_all(content, |caps: &regex::Captures| {
        let title = &caps[1];
        let alias = caps.get(2).map(|m| m.as_str());
        let display = alias.unwrap_or(title);

        // Find slug for this title, or generate from title if note doesn't exist
        let slug = title_to_slug
            .get(&title.to_lowercase())
            .cloned()
            .unwrap_or_else(|| note::slugify(title));

        format!("[{}]({}.md)", display, slug)
    })
    .to_string()
}

/// Normalize markdown link paths using the configured link_base
fn normalize_markdown_links(
    content: &str,
    current_file: &Path,
    notes: &[note::Note],
    vault_path: &Path,
    link_base: config::LinkBase,
) -> String {
    use regex::Regex;

    let re = Regex::new(r"\[([^\]]+)\]\(([^)]+\.md)\)").unwrap();

    re.replace_all(content, |caps: &regex::Captures| {
        let text = &caps[1];
        let target = &caps[2];

        // Skip external links
        if target.starts_with("http") {
            return caps[0].to_string();
        }

        // Try to resolve the link target to a known note
        if let Some(resolved_note) = note::resolve_link_target(target, current_file, notes, Some(vault_path)) {
            let new_path = note::link_path(&resolved_note.path, current_file, link_base, Some(vault_path));
            format!("[{}]({})", text, new_path)
        } else {
            // Can't resolve — leave as-is
            caps[0].to_string()
        }
    })
    .to_string()
}

/// Convert markdown links [Text](slug.md) to wiki [[Title]]
/// Update the title field in frontmatter
fn update_frontmatter_title(content: &str, new_title: &str) -> String {
    use regex::Regex;

    let re = Regex::new(r#"(?m)^title:\s*["']?.*["']?\s*$"#).unwrap();
    re.replace(content, format!("title: \"{}\"", new_title)).to_string()
}

fn convert_markdown_to_wiki(content: &str) -> String {
    use regex::Regex;

    // Match [Text](path.md) - only .md files, not http links
    let re = Regex::new(r"\[([^\]]+)\]\(([^)]+\.md)\)").unwrap();

    re.replace_all(content, |caps: &regex::Captures| {
        let text = &caps[1];
        let path = &caps[2];

        // Extract title from path (remove .md, convert dashes to spaces, title case)
        let slug = path.trim_end_matches(".md");
        let title = slug
            .split('-')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        // If display text differs from title, use alias syntax
        if text != title {
            format!("[[{}|{}]]", title, text)
        } else {
            format!("[[{}]]", title)
        }
    })
    .to_string()
}

/// Replace a tag prefix in frontmatter, preserving formatting.
/// Handles block-style `tags:\n  - foo` and flow-style `tags: [foo, bar]`.
fn retag_frontmatter(content: &str, from: &str, to: &str) -> String {
    // Find frontmatter boundaries
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    let offset = content.len() - trimmed.len();
    let rest = &trimmed[3..];
    let Some(end) = rest.find("\n---") else {
        return content.to_string();
    };

    let fm_start = offset + 3;
    let fm_end = fm_start + end;
    let frontmatter = &content[fm_start..fm_end];

    let from_slash = format!("{}/", from);
    let mut new_fm = String::with_capacity(frontmatter.len());
    let mut in_tags = false;

    for line in frontmatter.lines() {
        let trimmed_line = line.trim();

        // Detect start of tags field
        if trimmed_line.starts_with("tags:") {
            in_tags = true;

            // Flow style: tags: [foo, bar]
            if let Some(bracket_start) = line.find('[') {
                if let Some(bracket_end) = line.find(']') {
                    let items = &line[bracket_start + 1..bracket_end];
                    let new_items: Vec<String> = items
                        .split(',')
                        .map(|item| {
                            let t = item.trim();
                            if t == from {
                                to.to_string()
                            } else if let Some(rest) = t.strip_prefix(&from_slash) {
                                format!("{}/{}", to, rest)
                            } else {
                                t.to_string()
                            }
                        })
                        .collect();
                    new_fm.push_str(&line[..bracket_start + 1]);
                    new_fm.push_str(&new_items.join(", "));
                    new_fm.push_str(&line[bracket_end..]);
                    new_fm.push('\n');
                    continue;
                }
            }
            new_fm.push_str(line);
            new_fm.push('\n');
            continue;
        }

        // Block style tag item: "  - value"
        if in_tags && trimmed_line.starts_with("- ") {
            let dash_pos = line.find("- ").unwrap();
            let tag = trimmed_line[2..].trim();
            let new_tag = if tag == from {
                to.to_string()
            } else if let Some(rest) = tag.strip_prefix(&from_slash) {
                format!("{}/{}", to, rest)
            } else {
                tag.to_string()
            };
            new_fm.push_str(&line[..dash_pos]);
            new_fm.push_str("- ");
            new_fm.push_str(&new_tag);
            new_fm.push('\n');
            continue;
        }

        // Any other field ends the tags section
        if in_tags && !trimmed_line.is_empty() && !trimmed_line.starts_with('#') {
            in_tags = false;
        }

        new_fm.push_str(line);
        new_fm.push('\n');
    }

    // Remove trailing newline (we'll reconstruct with the original boundaries)
    if new_fm.ends_with('\n') {
        new_fm.pop();
    }

    let mut result = String::with_capacity(content.len());
    result.push_str(&content[..fm_start]);
    result.push_str(&new_fm);
    result.push_str(&content[fm_end..]);
    result
}

/// Update markdown links in all notes after files have been moved.
/// Returns the number of links updated.
fn update_links_after_moves(
    vault_path: &Path,
    moves: &[(PathBuf, PathBuf, String)],
    dry_run: bool,
) -> Result<usize> {
    use regex::Regex;
    use walkdir::WalkDir;

    if moves.is_empty() {
        return Ok(0);
    }

    let config = Config::load(vault_path)?;
    let notes_path = config.notes_path(vault_path);

    // Build a map from old path to new path (relative to notes_path)
    let mut path_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (from, to, _) in moves {
        if let (Ok(from_rel), Ok(to_rel)) = (from.strip_prefix(&notes_path), to.strip_prefix(&notes_path)) {
            path_map.insert(
                from_rel.to_string_lossy().to_string(),
                to_rel.to_string_lossy().to_string(),
            );
        }
    }

    let link_re = Regex::new(r"\[([^\]]+)\]\(([^)]+\.md)\)").unwrap();
    let mut total_updates = 0;

    // Scan all markdown files (use new paths for moved files)
    for entry in WalkDir::new(&notes_path)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_str().is_some_and(|s| s.starts_with('.')))
    {
        let entry = entry?;
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }

        let content = std::fs::read_to_string(path)?;
        let current_dir = path.parent().unwrap_or(path);

        let mut modified = false;
        let new_content = link_re.replace_all(&content, |caps: &regex::Captures| {
            let text = &caps[1];
            let target = &caps[2];

            // Skip external links
            if target.starts_with("http") {
                return caps[0].to_string();
            }

            // Resolve the link target relative to current file
            let resolved = current_dir.join(target);
            let resolved_rel = resolved.strip_prefix(&notes_path)
                .ok()
                .map(|p| p.to_string_lossy().to_string());

            // Check if this path was moved
            if let Some(rel) = resolved_rel {
                // Normalize path separators
                let rel_normalized = rel.replace('\\', "/");
                if let Some(new_rel) = path_map.get(&rel_normalized) {
                    modified = true;
                    // Compute new relative path from current file to new location
                    let new_abs = notes_path.join(new_rel);
                    let new_target = note::relative_path(current_dir, &new_abs);
                    return format!("[{}]({})", text, new_target.to_string_lossy());
                }
            }

            caps[0].to_string()
        });

        if modified && content != new_content.as_ref() {
            total_updates += 1;
            if !dry_run {
                std::fs::write(path, new_content.as_ref())?;
            }
        }
    }

    Ok(total_updates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_wiki_to_markdown_basic() {
        let content = "See [[My Note]] for details.";
        let notes = vec![];  // Empty - will generate slug from title
        let result = convert_wiki_to_markdown(content, &notes);
        assert_eq!(result, "See [My Note](my-note.md) for details.");
    }

    #[test]
    fn convert_wiki_to_markdown_with_alias() {
        let content = "See [[My Note|this note]] for details.";
        let notes = vec![];
        let result = convert_wiki_to_markdown(content, &notes);
        assert_eq!(result, "See [this note](my-note.md) for details.");
    }

    #[test]
    fn convert_markdown_to_wiki_basic() {
        let content = "See [My Note](my-note.md) for details.";
        let result = convert_markdown_to_wiki(content);
        assert_eq!(result, "See [[My Note]] for details.");
    }

    #[test]
    fn convert_markdown_to_wiki_with_alias() {
        let content = "See [this note](my-note.md) for details.";
        let result = convert_markdown_to_wiki(content);
        assert_eq!(result, "See [[My Note|this note]] for details.");
    }

    #[test]
    fn convert_preserves_non_links() {
        let content = "Regular text with no links.";
        let notes = vec![];
        assert_eq!(convert_wiki_to_markdown(content, &notes), content);
        assert_eq!(convert_markdown_to_wiki(content), content);
    }

    #[test]
    fn convert_ignores_http_links() {
        let content = "See [website](https://example.com) for details.";
        let result = convert_markdown_to_wiki(content);
        assert_eq!(result, content);  // Should not convert http links
    }

    #[test]
    fn retag_block_style() {
        let content = "---\ntitle: Test\ntags:\n  - tech/ai\n  - tech/ai/ml\n  - other\n---\nBody text\n";
        let result = retag_frontmatter(content, "tech/ai", "type");
        assert_eq!(result, "---\ntitle: Test\ntags:\n  - type\n  - type/ml\n  - other\n---\nBody text\n");
    }

    #[test]
    fn retag_flow_style() {
        let content = "---\ntitle: Test\ntags: [tech/ai, tech/ai/ml, other]\n---\nBody\n";
        let result = retag_frontmatter(content, "tech/ai", "type");
        assert_eq!(result, "---\ntitle: Test\ntags: [type, type/ml, other]\n---\nBody\n");
    }

    #[test]
    fn retag_no_match() {
        let content = "---\ntitle: Test\ntags:\n  - other\n---\nBody\n";
        let result = retag_frontmatter(content, "tech/ai", "type");
        assert_eq!(result, content);
    }

    #[test]
    fn retag_preserves_body() {
        let content = "---\ntags:\n  - tech/ai\n---\nBody with tech/ai text\n";
        let result = retag_frontmatter(content, "tech/ai", "type");
        assert!(result.contains("- type\n"));
        assert!(result.contains("Body with tech/ai text"));
    }
}

