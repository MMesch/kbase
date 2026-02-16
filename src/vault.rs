use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::Config;
use crate::note;
use crate::store::Store;

const KBASE_DIR: &str = ".kbase";

/// Initialize a new vault at the given path
pub fn init(path: &Path) -> Result<()> {
    let kbase_path = path.join(KBASE_DIR);

    if kbase_path.exists() {
        bail!("Vault already initialized at {}", path.display());
    }

    fs::create_dir_all(&kbase_path)
        .with_context(|| format!("Failed to create {}", kbase_path.display()))?;

    Ok(())
}

/// Find the vault root by searching up from current directory
pub fn find_vault_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        if current.join(KBASE_DIR).exists() {
            return Ok(current);
        }

        if !current.pop() {
            bail!("Not in a kbase vault (no .kbase directory found)");
        }
    }
}

/// Load all notes into an in-memory store
pub fn load(vault_path: &Path) -> Result<Store> {
    let notes = load_notes(vault_path)?;
    let store = Store::new()?;

    for parsed in notes {
        store.upsert_note(&parsed)?;
    }

    Ok(store)
}

/// Load all notes as parsed Note structs
pub fn load_notes(vault_path: &Path) -> Result<Vec<note::Note>> {
    let config = Config::load(vault_path)?;
    let mut notes = Vec::new();

    for entry in WalkDir::new(vault_path)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
    {
        let entry = entry?;
        let path = entry.path();

        if path.extension().is_some_and(|ext| ext == "md")
            && let Ok(parsed) = note::parse(path, config.link_syntax)
        {
            notes.push(parsed);
        }
    }

    Ok(notes)
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .is_some_and(|s| s.starts_with('.'))
}
