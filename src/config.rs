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

/// Configuration for new note creation
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct NewNoteConfig {
    /// Default tags added to new notes
    #[serde(default)]
    pub tags: Vec<String>,
    /// Additional default frontmatter fields (key: value)
    #[serde(default)]
    pub fields: std::collections::HashMap<String, String>,
    /// Infer tag from current working directory (relative to notes_dir)
    #[serde(default = "default_true")]
    pub infer_tag_from_cwd: bool,
}

fn default_true() -> bool {
    true
}

fn default_organize_root() -> String {
    "/".to_string()
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
    /// Root for folder organization. "/" uses full tag path, "domain" strips that prefix.
    /// E.g., organize_root: domain → tag domain/ai → folder ai/
    #[serde(default = "default_organize_root")]
    pub organize_root: String,
    /// Configuration for `kbase new` command
    #[serde(default)]
    pub new_note: NewNoteConfig,
}

impl Config {
    /// Directory to scan for notes. Returns `vault_path/notes_dir` if set, otherwise `vault_path`.
    pub fn notes_path(&self, vault_path: &Path) -> std::path::PathBuf {
        match &self.notes_dir {
            Some(dir) => vault_path.join(dir),
            None => vault_path.to_path_buf(),
        }
    }

    /// Get the folder path for a tag based on organize_root.
    /// Returns None if the tag doesn't match the organize_root.
    /// - organize_root: "/" → "domain/ai" returns "domain/ai"
    /// - organize_root: "domain" → "domain/ai" returns "ai"
    /// - organize_root: "domain" → "type/ref" returns None
    pub fn folder_for_tag(&self, tag: &str) -> Option<String> {
        if self.organize_root == "/" {
            if tag.contains('/') {
                Some(tag.to_string())
            } else {
                None // Single-segment tags don't create folders
            }
        } else {
            let prefix = format!("{}/", self.organize_root);
            tag.strip_prefix(&prefix).map(|s| s.to_string())
        }
    }

    /// Find the first tag that matches organize_root and return its folder path.
    pub fn folder_for_tags(&self, tags: &[String]) -> Option<String> {
        tags.iter().find_map(|t| self.folder_for_tag(t))
    }

    /// Convert a folder path back to a tag based on organize_root.
    /// - organize_root: "/" → "ai/ml" returns "ai/ml"
    /// - organize_root: "domain" → "ai/ml" returns "domain/ai/ml"
    pub fn tag_for_folder(&self, folder: &str) -> String {
        if self.organize_root == "/" {
            folder.to_string()
        } else {
            format!("{}/{}", self.organize_root, folder)
        }
    }

    /// Load config from .kbase/config.yaml
    pub fn load(vault_path: &Path) -> Result<Self> {
        let config_path = vault_path.join(".kbase").join("config.yaml");

        if !config_path.exists() {
            anyhow::bail!(
                "No config found at {}. Run `kbase init` to create a vault.",
                config_path.display()
            );
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read {}", config_path.display()))?;

        let config: Config = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse {}", config_path.display()))?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_root(root: &str) -> Config {
        Config {
            organize_root: root.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn folder_for_tag_with_root_slash() {
        let cfg = config_with_root("/");
        // With "/" root, hierarchical tags return full path
        assert_eq!(cfg.folder_for_tag("domain/ai"), Some("domain/ai".to_string()));
        assert_eq!(cfg.folder_for_tag("domain/ai/ml"), Some("domain/ai/ml".to_string()));
        // Single-segment tags don't create folders
        assert_eq!(cfg.folder_for_tag("flat"), None);
    }

    #[test]
    fn folder_for_tag_with_specific_root() {
        let cfg = config_with_root("domain");
        // With "domain" root, strips the prefix
        assert_eq!(cfg.folder_for_tag("domain/ai"), Some("ai".to_string()));
        assert_eq!(cfg.folder_for_tag("domain/ai/ml"), Some("ai/ml".to_string()));
        // Tags not matching root return None
        assert_eq!(cfg.folder_for_tag("type/reference"), None);
        assert_eq!(cfg.folder_for_tag("domain"), None); // exact match, no subfolder
    }

    #[test]
    fn folder_for_tags_finds_first_match() {
        let cfg = config_with_root("domain");
        let tags = vec![
            "type/reference".to_string(),
            "domain/ai".to_string(),
            "domain/web".to_string(),
        ];
        // Should find first matching tag (domain/ai)
        assert_eq!(cfg.folder_for_tags(&tags), Some("ai".to_string()));

        // No matching tags
        let tags2 = vec!["type/reference".to_string(), "flat".to_string()];
        assert_eq!(cfg.folder_for_tags(&tags2), None);
    }

    #[test]
    fn tag_for_folder_with_root_slash() {
        let cfg = config_with_root("/");
        assert_eq!(cfg.tag_for_folder("ai/ml"), "ai/ml");
        assert_eq!(cfg.tag_for_folder("web"), "web");
    }

    #[test]
    fn tag_for_folder_with_specific_root() {
        let cfg = config_with_root("domain");
        assert_eq!(cfg.tag_for_folder("ai"), "domain/ai");
        assert_eq!(cfg.tag_for_folder("ai/ml"), "domain/ai/ml");
    }

    #[test]
    fn roundtrip_folder_tag() {
        let cfg = config_with_root("domain");
        let tag = "domain/ai/ml";
        let folder = cfg.folder_for_tag(tag).unwrap();
        let back = cfg.tag_for_folder(&folder);
        assert_eq!(back, tag);
    }
}
