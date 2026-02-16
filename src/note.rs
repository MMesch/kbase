use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::config::LinkSyntax;

/// Parsed note with frontmatter
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub fields: HashMap<String, serde_yaml::Value>,
    pub links: Vec<String>,
}

/// Frontmatter structure
#[derive(Debug, Deserialize, Serialize)]
struct Frontmatter {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(flatten)]
    other: HashMap<String, serde_yaml::Value>,
}

/// Parse a markdown file into a Note
pub fn parse(path: &Path, link_syntax: LinkSyntax) -> Result<Note> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let (frontmatter, body) = split_frontmatter(&content)?;

    let fm: Frontmatter = serde_yaml::from_str(&frontmatter)
        .with_context(|| format!("Failed to parse frontmatter in {}", path.display()))?;

    let title = fm.title.unwrap_or_else(|| {
        // Fallback: extract title from filename
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    let links = extract_links(&body, link_syntax);

    Ok(Note {
        path: path.to_path_buf(),
        title,
        tags: fm.tags,
        fields: fm.other,
        links,
    })
}

/// Create a new note with the given title
pub fn create(vault_path: &Path, title: &str) -> Result<PathBuf> {
    let filename = slugify(title);
    let note_path = vault_path.join(format!("{}.md", filename));

    if note_path.exists() {
        bail!("Note already exists: {}", note_path.display());
    }

    let content = format!(
        r#"---
title: "{}"
tags: []
---
# {}
"#,
        title, title
    );

    fs::write(&note_path, content)
        .with_context(|| format!("Failed to write {}", note_path.display()))?;

    Ok(note_path)
}

/// Split content into frontmatter and body
fn split_frontmatter(content: &str) -> Result<(String, String)> {
    let content = content.trim_start();

    if !content.starts_with("---") {
        bail!("No frontmatter found (must start with ---)");
    }

    let rest = &content[3..];
    let end = rest
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("Frontmatter not closed (missing ---)"))?;

    let frontmatter = rest[..end].trim().to_string();
    let body = rest[end + 4..].to_string();

    Ok((frontmatter, body))
}

// Regex for [[link]] or [[link|alias]]
static WIKI_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());

// Regex for [text](path) - captures the path part
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());

/// Extract links from markdown body based on configured syntax
fn extract_links(body: &str, syntax: LinkSyntax) -> Vec<String> {
    let mut links = Vec::new();

    if syntax == LinkSyntax::Wiki || syntax == LinkSyntax::Both {
        for cap in WIKI_LINK_RE.captures_iter(body) {
            let link = cap[1].trim();
            if !link.is_empty() {
                links.push(link.to_string());
            }
        }
    }

    if syntax == LinkSyntax::Markdown || syntax == LinkSyntax::Both {
        for cap in MD_LINK_RE.captures_iter(body) {
            let path = cap[2].trim();
            // Skip external links and anchors
            if !path.starts_with("http") && !path.starts_with('#') {
                links.push(path.to_string());
            }
        }
    }

    links
}

/// Convert title to filename-safe slug
fn slugify(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("My Note!"), "my-note");
        assert_eq!(slugify("test"), "test");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("What's this?"), "what-s-this");
        assert_eq!(slugify("foo--bar"), "foo-bar");
        assert_eq!(slugify("  spaces  "), "spaces");
    }

    #[test]
    fn extract_wiki_links() {
        let body = "See [[Note A]] and [[Note B]]";
        let links = extract_links(body, LinkSyntax::Wiki);
        assert_eq!(links, vec!["Note A", "Note B"]);
    }

    #[test]
    fn extract_wiki_links_with_alias() {
        let body = "See [[Target|display text]]";
        let links = extract_links(body, LinkSyntax::Wiki);
        assert_eq!(links, vec!["Target"]);
    }

    #[test]
    fn extract_markdown_links() {
        let body = "See [note](path/to/note.md) and [other](other.md)";
        let links = extract_links(body, LinkSyntax::Markdown);
        assert_eq!(links, vec!["path/to/note.md", "other.md"]);
    }

    #[test]
    fn extract_markdown_links_skips_external() {
        let body = "See [google](https://google.com) and [note](note.md)";
        let links = extract_links(body, LinkSyntax::Markdown);
        assert_eq!(links, vec!["note.md"]);
    }

    #[test]
    fn extract_both_syntaxes() {
        let body = "Wiki [[Note A]] and markdown [text](note.md)";
        let links = extract_links(body, LinkSyntax::Both);
        assert_eq!(links, vec!["Note A", "note.md"]);
    }

    #[test]
    fn split_frontmatter_basic() {
        let content = "---\ntitle: Test\n---\nBody here";
        let (fm, body) = split_frontmatter(content).unwrap();
        assert_eq!(fm, "title: Test");
        assert_eq!(body, "\nBody here");
    }

    #[test]
    fn split_frontmatter_missing() {
        let content = "No frontmatter here";
        assert!(split_frontmatter(content).is_err());
    }
}
