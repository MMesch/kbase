use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use kbase::{note, vault};

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
    Tags,
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
        Commands::Tags => {
            let vault_path = get_vault()?;
            let store = vault::load(&vault_path)?;
            let tree = store.list_tags()?;
            for line in tree {
                println!("{}", line);
            }
        }
    }

    Ok(())
}
