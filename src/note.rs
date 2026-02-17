use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::config::LinkSyntax;

/// A link in a note with position information
#[derive(Debug, Clone)]
pub struct Link {
    /// The link target (title for wiki links, path for markdown links)
    pub target: String,
    /// Line number (0-indexed)
    pub line: u32,
    /// Start column (0-indexed, byte offset)
    pub start_col: u32,
    /// End column (0-indexed, byte offset)
    pub end_col: u32,
}

/// Parsed note with frontmatter
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    pub fields: HashMap<String, serde_yaml::Value>,
    pub links: Vec<Link>,
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

    let (frontmatter, body, body_start_line) = split_frontmatter(&content)?;

    let fm: Frontmatter = serde_yaml::from_str(&frontmatter)
        .with_context(|| format!("Failed to parse frontmatter in {}", path.display()))?;

    let title = fm.title.unwrap_or_else(|| {
        // Fallback: extract title from filename
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    });

    let links = extract_links(&body, link_syntax, body_start_line);

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
/// Returns (frontmatter, body, body_start_line)
fn split_frontmatter(content: &str) -> Result<(String, String, u32)> {
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

    // Count lines before body starts (opening ---, frontmatter content, closing ---)
    let frontmatter_section = &content[..content.len() - body.len()];
    let body_start_line = frontmatter_section.matches('\n').count() as u32;

    Ok((frontmatter, body, body_start_line))
}

// Regex for [[link]] or [[link|alias]]
static WIKI_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());

// Regex for [text](path) - captures the path part
static MD_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());

/// Extract links from markdown body based on configured syntax
/// `body_start_line` is the line number where the body starts (after frontmatter)
fn extract_links(body: &str, syntax: LinkSyntax, body_start_line: u32) -> Vec<Link> {
    let mut links = Vec::new();

    // Build a line offset map: byte offset -> (line, line_start_byte)
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(body.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    let byte_to_line_col = |byte_offset: usize| -> (u32, u32) {
        let line_idx = line_starts.partition_point(|&start| start <= byte_offset) - 1;
        let col = byte_offset - line_starts[line_idx];
        (body_start_line + line_idx as u32, col as u32)
    };

    if syntax == LinkSyntax::Wiki || syntax == LinkSyntax::Both {
        for cap in WIKI_LINK_RE.captures_iter(body) {
            let link = cap[1].trim();
            if !link.is_empty() {
                let m = cap.get(0).unwrap();
                let (line, start_col) = byte_to_line_col(m.start());
                let (_, end_col) = byte_to_line_col(m.end());
                links.push(Link {
                    target: link.to_string(),
                    line,
                    start_col,
                    end_col,
                });
            }
        }
    }

    if syntax == LinkSyntax::Markdown || syntax == LinkSyntax::Both {
        for cap in MD_LINK_RE.captures_iter(body) {
            let path = cap[2].trim();
            // Skip external links and anchors
            if !path.starts_with("http") && !path.starts_with('#') {
                let m = cap.get(0).unwrap();
                let (line, start_col) = byte_to_line_col(m.start());
                let (_, end_col) = byte_to_line_col(m.end());
                links.push(Link {
                    target: path.to_string(),
                    line,
                    start_col,
                    end_col,
                });
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
        let links = extract_links(body, LinkSyntax::Wiki, 0);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["Note A", "Note B"]);
    }

    #[test]
    fn extract_wiki_links_with_alias() {
        let body = "See [[Target|display text]]";
        let links = extract_links(body, LinkSyntax::Wiki, 0);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["Target"]);
    }

    #[test]
    fn extract_markdown_links() {
        let body = "See [note](path/to/note.md) and [other](other.md)";
        let links = extract_links(body, LinkSyntax::Markdown, 0);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["path/to/note.md", "other.md"]);
    }

    #[test]
    fn extract_markdown_links_skips_external() {
        let body = "See [google](https://google.com) and [note](note.md)";
        let links = extract_links(body, LinkSyntax::Markdown, 0);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["note.md"]);
    }

    #[test]
    fn extract_both_syntaxes() {
        let body = "Wiki [[Note A]] and markdown [text](note.md)";
        let links = extract_links(body, LinkSyntax::Both, 0);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, vec!["Note A", "note.md"]);
    }

    #[test]
    fn extract_links_with_positions() {
        let body = "Line 0 [[Note A]]\nLine 1 [[Note B]]";
        let links = extract_links(body, LinkSyntax::Wiki, 5); // body starts at line 5
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Note A");
        assert_eq!(links[0].line, 5);
        assert_eq!(links[0].start_col, 7);
        assert_eq!(links[1].target, "Note B");
        assert_eq!(links[1].line, 6);
    }

    #[test]
    fn split_frontmatter_basic() {
        let content = "---\ntitle: Test\n---\nBody here";
        let (fm, body, line) = split_frontmatter(content).unwrap();
        assert_eq!(fm, "title: Test");
        assert_eq!(body, "\nBody here");
        assert_eq!(line, 2);  // 0: ---, 1: title, 2: ---, body starts after
    }

    #[test]
    fn split_frontmatter_missing() {
        let content = "No frontmatter here";
        assert!(split_frontmatter(content).is_err());
    }
}
