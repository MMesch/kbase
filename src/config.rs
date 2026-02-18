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
    #[default]
    Wiki, // [[target]]
    Markdown, // [text](path)
    Both,     // recognize both
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EmbeddingsConfig {
    #[serde(default)]
    pub backend: EmbeddingBackend,
    #[serde(default)]
    pub chunk_level: String, // "none", "#", "##", "###", "paragraph"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingBackend {
    #[default]
    Onnx,   // Local ONNX model (no server needed)
    Ollama, // Ollama server
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub link_syntax: LinkSyntax,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
}

impl Config {
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
