use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

use kbase::{config, config::Config, embeddings, lsp, note, schema, skills, store, vault};

#[derive(Parser)]
#[command(name = "kbase")]
#[command(about = "Knowledge graph CLI for markdown notes")]
struct Cli {
    /// Path to vault (defaults to searching up from current directory)
    #[arg(short, long, global = true)]
    vault: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
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
    Validate,
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
    /// Clean orphan tags (tags in graph with no notes)
    CleanTags {
        /// Preview without deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// Convert links between wiki and markdown syntax
    ///
    /// Wiki:     [[Note Title]] or [[Note Title|alias]]
    /// Markdown: [Note Title](note-title.md) or [alias](note-title.md)
    Convert {
        /// Target syntax: "wiki" or "markdown"
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

    let get_vault = || -> Result<PathBuf> {
        match &cli.vault {
            Some(p) => Ok(p.clone()),
            None => vault::find_vault_root(),
        }
    };

    match cli.command {
        Commands::Init { path } => {
            let path = path.unwrap_or_else(|| PathBuf::from("."));
            vault::init(&path)?;
            println!("Initialized kbase vault in {}", path.display());
        }
        Commands::New { title } => {
            let vault_path = get_vault()?;
            let note_path = note::create(&vault_path, &title)?;
            println!("Created {}", note_path.display());
        }
        Commands::List { tag } => {
            let vault_path = get_vault()?;
            let store = vault::load(&vault_path)?;
            let notes = store.list_notes(tag.as_deref(), false)?;
            for note in notes {
                println!("{}", note);
            }
        }
        Commands::Backlinks { note } => {
            let vault_path = get_vault()?;
            let store = vault::load(&vault_path)?;
            let backlinks = store.backlinks(&note)?;
            for link in backlinks {
                println!("{}", link);
            }
        }
        Commands::Tags { tag, notes } => {
            let vault_path = get_vault()?;
            let store = vault::load(&vault_path)?;
            let tree = store.list_tags(tag.as_deref(), notes)?;
            for line in tree {
                println!("{}", line);
            }
        }
        Commands::Validate => {
            let vault_path = get_vault()?;
            let schema = schema::Schema::load(&vault_path)?;
            let notes = vault::load_notes(&vault_path)?;

            let mut total_violations = 0;

            // Schema validation (per-note)
            for parsed_note in &notes {
                let violations = schema.validate(parsed_note);
                for v in &violations {
                    println!("{}:{}: {}", v.note_path, v.field, v.message);
                    total_violations += 1;
                }
            }

            // Graph constraints (SPARQL)
            if !schema.constraints.is_empty() {
                let store = vault::load(&vault_path)?;
                let graph_violations = store.validate_constraints(&schema.constraints)?;
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
        Commands::Export { format, output } => {
            let vault_path = get_vault()?;
            let store = vault::load(&vault_path)?;

            let content = match format.as_str() {
                "dot" => store.export_dot()?,
                "graphml" => store.export_graphml()?,
                _ => anyhow::bail!("Unknown format: {}. Use 'dot' or 'graphml'", format),
            };

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
            let store = vault::load(&vault_path)?;

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
            let notes = vault::load_notes(&vault_path)?;

            // Build a map of note paths to their target locations
            let mut moves: Vec<(PathBuf, PathBuf, String)> = Vec::new(); // (from, to, title)
            let mut conflicts: Vec<String> = Vec::new();
            let mut multi_path: Vec<(String, Vec<String>)> = Vec::new(); // (title, [tags])

            for n in &notes {
                // Get tags matching the tree filter
                let matching_tags: Vec<&String> = if let Some(ref tree_prefix) = tree {
                    n.tags.iter().filter(|t| t.starts_with(tree_prefix)).collect()
                } else {
                    n.tags.iter().collect()
                };

                if matching_tags.is_empty() {
                    continue; // Note doesn't match tree filter
                }

                // Check for multi-path within the filtered tree
                if matching_tags.len() > 1 {
                    multi_path.push((
                        n.title.clone(),
                        matching_tags.iter().map(|t| t.to_string()).collect(),
                    ));
                }

                // Use first matching tag as primary path
                let primary_tag = matching_tags[0];

                let target_path = if flat {
                    // Flat: keep in vault root
                    let filename = n.path.file_name().unwrap();
                    vault_path.join(filename)
                } else {
                    // Tree: create directory structure from tag
                    let tag_path: PathBuf = primary_tag.split('/').collect();
                    let filename = n.path.file_name().unwrap();
                    vault_path.join(tag_path).join(filename)
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
                                let symlink_path = vault_path.join(symlink_dir).join(n.path.file_name().unwrap());

                                let primary_dir: PathBuf = primary_tag.split('/').collect();
                                let target = vault_path.join(primary_dir).join(n.path.file_name().unwrap());

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

                // TODO: Update links in all notes to reflect new paths

                println!("\n{} files {}", moves.len(), if dry_run { "would be moved" } else { "moved" });
            }
        }
        Commands::CleanTags { dry_run } => {
            let vault_path = get_vault()?;
            let notes = vault::load_notes(&vault_path)?;
            let store = vault::load(&vault_path)?;

            // Get all tag paths from the graph (raw paths, not formatted tree)
            let all_tags = store.get_all_tag_paths()?;

            // Get tags actually used by notes
            let mut used_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
            for n in &notes {
                for tag in &n.tags {
                    // Add the tag and all its ancestors
                    let parts: Vec<&str> = tag.split('/').collect();
                    for i in 1..=parts.len() {
                        used_tags.insert(parts[..i].join("/"));
                    }
                }
            }

            // Find orphan tags
            let orphan_tags: Vec<_> = all_tags
                .iter()
                .filter(|t| !used_tags.contains(*t))
                .collect();

            if orphan_tags.is_empty() {
                println!("No orphan tags found.");
            } else {
                println!("## Orphan tags{}\n", if dry_run { " (dry run)" } else { "" });
                for tag in &orphan_tags {
                    println!("  {}", tag);
                }

                if !dry_run {
                    // Rebuild the graph without orphan tags (by reloading from notes)
                    println!("\nRebuilding graph...");
                    // The simplest way is to delete and recreate the graph store
                    let graph_path = vault_path.join(".kbase").join("graph.db");
                    if graph_path.exists() {
                        std::fs::remove_dir_all(&graph_path)?;
                    }
                    vault::load(&vault_path)?; // This rebuilds the graph
                    println!("Removed {} orphan tags.", orphan_tags.len());
                } else {
                    println!("\n{} orphan tags would be removed.", orphan_tags.len());
                }
            }
        }
        Commands::Convert { to, dry_run } => {
            let vault_path = get_vault()?;
            let notes = vault::load_notes(&vault_path)?;

            let to_markdown = match to.to_lowercase().as_str() {
                "markdown" | "md" => true,
                "wiki" => false,
                _ => anyhow::bail!("Unknown target syntax: {}. Use 'wiki' or 'markdown'", to),
            };

            let mut total_changes = 0;

            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let new_content = if to_markdown {
                    convert_wiki_to_markdown(&content, &notes)
                } else {
                    convert_markdown_to_wiki(&content)
                };

                if content != new_content {
                    total_changes += 1;
                    if dry_run {
                        println!("Would modify: {}", n.path.display());
                        // Show diff-like output
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
            let store = vault::load(&vault_path)?;

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

/// Convert markdown links [Text](slug.md) to wiki [[Title]]
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
}

