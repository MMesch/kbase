use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use kbase::{config, config::Config, embeddings, lsp, note, schema, vault};

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
    Similar {
        /// Note to find similar notes for (title or path)
        note: String,
        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    /// Semantic search across notes
    Search {
        /// Search query
        query: String,
        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    /// Start LSP server (for editor integration)
    Lsp,
}

fn main() -> Result<()> {
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
            let notes = store.list_notes(tag.as_deref())?;
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

            // Embed all notes (using cache)
            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let chunks = embeddings::split_into_chunks(&content, chunk_level);

                let texts: Vec<&str> = chunks.iter().map(|(_, text)| text.as_str()).collect();
                let vectors = cache.get_or_compute_batch(&texts, &mut *backend)?;

                for ((headers, text), embedding) in chunks.into_iter().zip(vectors) {
                    store.add_chunk(embeddings::Chunk {
                        note_path: n.path.to_string_lossy().to_string(),
                        header_path: headers,
                        text,
                        embedding,
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
        Commands::Search { query, limit } => {
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

            // Embed all notes (using cache)
            for n in &notes {
                let content = std::fs::read_to_string(&n.path)?;
                let chunks = embeddings::split_into_chunks(&content, chunk_level);

                let texts: Vec<&str> = chunks.iter().map(|(_, text)| text.as_str()).collect();
                let vectors = cache.get_or_compute_batch(&texts, &mut *backend)?;

                for ((headers, text), embedding) in chunks.into_iter().zip(vectors) {
                    store.add_chunk(embeddings::Chunk {
                        note_path: n.path.to_string_lossy().to_string(),
                        header_path: headers,
                        text,
                        embedding,
                    });
                }
            }

            // Embed the query (also cached)
            let query_embedding = cache.get_or_compute(&query, &mut *backend)?;

            // Find similar chunks
            let results = store.find_similar(&query_embedding, limit);

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
                let preview: String = chunk.text.chars().take(100).collect();
                println!("       {}", preview.replace('\n', " "));
                println!();
            }
        }
        Commands::Lsp => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(lsp::run_server());
        }
    }

    Ok(())
}
