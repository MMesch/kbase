//! Configuration types for kbase vaults.
//!
//! Maps to `.kbase/config.yaml` in the vault root.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkSyntax {
    Wiki,     // [[target]]
    Markdown, // [text](path)
    #[default]
    Both, // recognize both
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EmbeddingsConfig {
    #[serde(default)]
    pub backend: EmbeddingBackend,
    #[serde(default)]
    pub chunk_level: String, // "none", "#", "##", "###", "paragraph"
    /// Include note title and parent headers in chunk text for better context (default: true)
    #[serde(default = "default_include_context")]
    pub include_context: bool,
}

fn default_include_context() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingBackend {
    #[default]
    Onnx,   // Local ONNX model (no server needed)
    Ollama, // Ollama server
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StoreBackend {
    #[default]
    #[serde(alias = "ntriples")]
    Nquads, // In-memory + N-Quads file (fast startup, ~5ms)
    Rocksdb,  // Oxigraph persistent store (slow startup, ~400ms)
    Fresh,    // In-memory only, rebuild from files each time
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeSyntax {
    Tags,  // tags: [domain/ai]
    Trees, // trees: { domain: domain/ai }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TreesConfig {
    /// Which syntaxes to recognize (default: both)
    #[serde(default = "TreesConfig::default_syntax")]
    pub syntax: Vec<TreeSyntax>,
    /// Warn about orphan notes in tree paths (default: true)
    #[serde(default = "TreesConfig::default_warn_orphans")]
    pub warn_orphans: bool,
}

impl Default for TreesConfig {
    fn default() -> Self {
        Self {
            syntax: Self::default_syntax(),
            warn_orphans: true,
        }
    }
}

impl TreesConfig {
    fn default_syntax() -> Vec<TreeSyntax> {
        vec![TreeSyntax::Tags, TreeSyntax::Trees]
    }

    fn default_warn_orphans() -> bool {
        true
    }

    pub fn supports_tags(&self) -> bool {
        self.syntax.contains(&TreeSyntax::Tags)
    }

    pub fn supports_trees(&self) -> bool {
        self.syntax.contains(&TreeSyntax::Trees)
    }
}

/// How markdown link paths are resolved for completion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LinkBase {
    Relative, // Relative to current file (standard markdown)
    #[default]
    Vault, // Relative to vault root (.kbase directory)
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub link_syntax: LinkSyntax,
    #[serde(default)]
    pub link_base: LinkBase,
    /// Subdirectory containing notes (relative to vault root).
    /// Only files under this directory are scanned for notes.
    /// E.g. `notes_dir: knowledge` scans `<vault_root>/knowledge/**/*.md`
    #[serde(default)]
    pub notes_dir: Option<String>,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub trees: TreesConfig,
    #[serde(default)]
    pub store: StoreBackend,
}

impl Config {
    /// Directory to scan for notes. Returns `vault_path/notes_dir` if set, otherwise `vault_path`.
    pub fn notes_path(&self, vault_path: &Path) -> std::path::PathBuf {
        match &self.notes_dir {
            Some(dir) => vault_path.join(dir),
            None => vault_path.to_path_buf(),
        }
    }

    /// Load config from .kbase/config.yaml, or return defaults
    pub fn load(vault_path: &Path) -> Result<Self> {
        let config_path = vault_path.join(".kbase").join("config.yaml");

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;

        Ok(config)
    }
}
