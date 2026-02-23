use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::config::{Config, StoreBackend};
use crate::note;
use crate::store::Store;

const KBASE_DIR: &str = ".kbase";
const GRAPH_DB: &str = "graph.db";
const GRAPH_NQ: &str = "graph.nq";

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

/// Load vault using the configured store backend
pub fn load(vault_path: &Path) -> Result<Store> {
    let config = Config::load(vault_path)?;
    load_with_backend(vault_path, config.store)
}

/// Load vault with explicit backend choice
pub fn load_with_backend(vault_path: &Path, backend: StoreBackend) -> Result<Store> {
    match backend {
        StoreBackend::Fresh => load_fresh(vault_path),
        StoreBackend::Nquads => {
            let (store, _) = load_nquads(vault_path)?;
            Ok(store)
        }
        StoreBackend::Rocksdb => {
            let (store, _) = load_rocksdb(vault_path)?;
            Ok(store)
        }
    }
}

/// Load all notes into a fresh in-memory store (no caching)
pub fn load_fresh(vault_path: &Path) -> Result<Store> {
    let notes = load_notes(vault_path)?;
    let store = Store::new()?;

    for parsed in notes {
        store.upsert_note(&parsed)?;
    }

    Ok(store)
}

/// Load using N-Quads cache (fast startup, ~5ms)
/// Returns the store and the number of notes updated
pub fn load_nquads(vault_path: &Path) -> Result<(Store, usize)> {
    let cache_path = vault_path.join(KBASE_DIR).join(GRAPH_NQ);
    let store = Store::new_with_cache(&cache_path)?;
    let config = Config::load(vault_path)?;

    let updated = sync_store_with_files(&store, vault_path, &config)?;

    // Save if anything changed
    if updated > 0 {
        store.save()?;
    }

    Ok((store, updated))
}

/// Load using RocksDB persistent store (slow startup, ~400ms)
/// Returns the store and the number of notes updated
pub fn load_rocksdb(vault_path: &Path) -> Result<(Store, usize)> {
    let db_path = vault_path.join(KBASE_DIR).join(GRAPH_DB);
    let store = Store::open(&db_path)?;
    let config = Config::load(vault_path)?;

    let updated = sync_store_with_files(&store, vault_path, &config)?;

    Ok((store, updated))
}

/// Sync store with filesystem - update changed notes, remove deleted ones
fn sync_store_with_files(store: &Store, vault_path: &Path, config: &Config) -> Result<usize> {
    // Get all stored note paths and mtimes in a single query
    let stored_mtimes = store.get_all_note_mtimes()?;

    // Scan filesystem for current notes
    let mut current_paths: HashSet<String> = HashSet::new();
    let mut updated = 0;

    for entry in WalkDir::new(vault_path)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
    {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().is_some_and(|ext| ext == "md") {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        current_paths.insert(path_str.clone());

        // Get file mtime
        let metadata = fs::metadata(path)?;
        let file_mtime = metadata.modified().ok();
        let file_mtime_secs = file_mtime
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Check if we need to update this note (compare with cached mtime)
        let stored_mtime = stored_mtimes.get(&path_str).copied().unwrap_or(0);
        let needs_update = file_mtime_secs > stored_mtime || stored_mtime == 0;

        if needs_update {
            if let Ok(parsed) = note::parse(path, config.link_syntax) {
                store.upsert_note_with_mtime(&parsed, file_mtime)?;
                updated += 1;
            }
        }
    }

    // Remove notes that no longer exist
    for stored_path in stored_mtimes.keys() {
        if !current_paths.contains(stored_path) {
            store.remove_note(stored_path)?;
            updated += 1;
        }
    }

    // Clean up orphaned tree hierarchy edges
    if updated > 0 {
        store.cleanup_orphan_tags()?;
    }

    Ok(updated)
}

/// Load vault using the configured backend, returning the store and update count
pub fn load_persistent(vault_path: &Path) -> Result<(Store, usize)> {
    let config = Config::load(vault_path)?;
    match config.store {
        StoreBackend::Nquads => load_nquads(vault_path),
        StoreBackend::Rocksdb => load_rocksdb(vault_path),
        StoreBackend::Fresh => {
            let store = load_fresh(vault_path)?;
            Ok((store, 0))
        }
    }
}

/// Update a single note in the persistent store
pub fn update_note(store: &Store, path: &Path, link_syntax: crate::config::LinkSyntax) -> Result<()> {
    if path.exists() {
        let metadata = fs::metadata(path)?;
        let mtime = metadata.modified().ok();
        let parsed = note::parse(path, link_syntax)?;
        store.upsert_note_with_mtime(&parsed, mtime)?;
    } else {
        // File was deleted
        store.remove_note(&path.to_string_lossy())?;
    }

    // Clean up any orphan tags that may result from tag changes
    store.cleanup_orphan_tags()?;

    Ok(())
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
