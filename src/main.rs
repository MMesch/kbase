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
