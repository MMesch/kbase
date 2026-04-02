use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use crate::config::{self, LinkSyntax};
use crate::schema::Schema;

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

/// A tree relationship parsed from tags or trees field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEdge {
    /// Tree name (first segment of path)
    pub tree: String,
    /// Parent note title (last segment of path)
    pub parent: String,
    /// Full path (e.g., "domain/ai/llms")
    pub path: String,
    /// All ancestors in order (e.g., ["domain", "ai", "llms"])
    pub ancestors: Vec<String>,
}

impl TreeEdge {
    /// Parse a tag path like "domain/ai/llms" into a TreeEdge
    pub fn parse(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return None;
        }

        let tree = parts[0].to_string();
        let ancestors: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        let parent = if parts.len() > 1 {
            parts[parts.len() - 1].to_string()
        } else {
            tree.clone() // Root node, parent is the tree itself
        };

        Some(Self {
            tree,
            parent,
            path: path.to_string(),
            ancestors,
        })
    }
}

/// A typed link extracted from a frontmatter field (e.g., depends_on: ["[[target]]"])
#[derive(Debug, Clone)]
pub struct TypedLink {
    /// The frontmatter field name (e.g., "depends_on")
    pub field: String,
    /// The link target (note title from [[target]])
    pub target: String,
}

/// Parsed note with frontmatter
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub title: String,
    pub tags: Vec<String>,
    /// Tree relationships parsed from tags/trees fields
    pub tree_edges: Vec<TreeEdge>,
    pub fields: HashMap<String, serde_yaml::Value>,
    pub links: Vec<Link>,
    /// Typed links extracted from frontmatter array fields (e.g., depends_on: ["[[X]]"])
    pub typed_links: Vec<TypedLink>,
}

/// Frontmatter structure
#[derive(Debug, Deserialize, Serialize)]
struct Frontmatter {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    /// Trees field: { tree_name: "tree/path" }
    #[serde(default)]
    trees: HashMap<String, String>,
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

    // Parse tree edges from both tags and trees field
    let tree_edges = extract_tree_edges(&fm.tags, &fm.trees);

    // Parse typed links from other frontmatter fields
    let typed_links = extract_typed_links(&fm.other);

    Ok(Note {
        path: path.to_path_buf(),
        title,
        tags: fm.tags,
        tree_edges,
        fields: fm.other,
        links,
        typed_links,
    })
}

/// Extract tree edges from tags array and trees field
fn extract_tree_edges(
    tags: &[String],
    trees: &HashMap<String, String>,
) -> Vec<TreeEdge> {
    let mut edges = Vec::new();

    // Parse from tags: [domain/ai, type/reference]
    for tag in tags {
        if tag.contains('/') {
            if let Some(edge) = TreeEdge::parse(tag) {
                edges.push(edge);
            }
        }
    }

    // Parse from trees: { domain: domain/ai }
    for (_tree_name, path) in trees {
        if let Some(edge) = TreeEdge::parse(path) {
            edges.push(edge);
        }
    }

    edges
}

// Regex to match [[wiki links]] in frontmatter values
static FRONTMATTER_WIKI_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());

/// Extract typed links from frontmatter fields.
/// Looks for array fields containing [[wiki links]], e.g.:
///   depends_on:
///     - "[[jupyter-chat]]"
///     - "[[jupyter-ai-router]]"
fn extract_typed_links(fields: &HashMap<String, serde_yaml::Value>) -> Vec<TypedLink> {
    // Fields to skip (handled elsewhere)
    const SKIP_FIELDS: &[&str] = &["title", "tags", "trees"];

    let mut typed_links = Vec::new();

    for (field, value) in fields {
        if SKIP_FIELDS.contains(&field.as_str()) {
            continue;
        }

        match value {
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    if let Some(s) = item.as_str() {
                        for cap in FRONTMATTER_WIKI_LINK_RE.captures_iter(s) {
                            let target = cap[1].trim().to_string();
                            if !target.is_empty() {
                                typed_links.push(TypedLink {
                                    field: field.clone(),
                                    target,
                                });
                            }
                        }
                    }
                }
            }
            serde_yaml::Value::String(s) => {
                for cap in FRONTMATTER_WIKI_LINK_RE.captures_iter(s) {
                    let target = cap[1].trim().to_string();
                    if !target.is_empty() {
                        typed_links.push(TypedLink {
                            field: field.clone(),
                            target,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    typed_links
}

/// Create a new note with the given title, tags, and extra fields
pub fn create(
    vault_path: &Path,
    title: &str,
    tags: &[String],
    extra_fields: &HashMap<String, String>,
) -> Result<PathBuf> {
    let (subdir, leaf_title) = split_title_path(title);
    let filename = slugify(&leaf_title);
    let note_dir = match subdir {
        Some(dir) => vault_path.join(dir),
        None => vault_path.to_path_buf(),
    };
    let note_path = note_dir.join(format!("{}.md", filename));

    if note_path.exists() {
        bail!("Note already exists: {}", note_path.display());
    }

    fs::create_dir_all(&note_dir)
        .with_context(|| format!("Failed to create directory {}", note_dir.display()))?;

    // Build tags YAML
    let tags_yaml = if tags.is_empty() {
        "[]".to_string()
    } else {
        format!("[{}]", tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", "))
    };

    // Build extra fields YAML
    let extra_yaml = extra_fields
        .iter()
        .map(|(k, v)| format!("{}: \"{}\"", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    let content = if extra_yaml.is_empty() {
        format!(
            r#"---
title: "{}"
tags: {}
---
# {}
"#,
            leaf_title, tags_yaml, leaf_title
        )
    } else {
        format!(
            r#"---
title: "{}"
tags: {}
{}
---
# {}
"#,
            leaf_title, tags_yaml, extra_yaml, leaf_title
        )
    };

    fs::write(&note_path, content)
        .with_context(|| format!("Failed to write {}", note_path.display()))?;

    Ok(note_path)
}

/// Create a new note with schema-based frontmatter template
pub fn create_with_schema(
    vault_path: &Path,
    title: &str,
    tags: &[String],
    extra_fields: &HashMap<String, String>,
) -> Result<PathBuf> {
    let (subdir, leaf_title) = split_title_path(title);
    let filename = slugify(&leaf_title);
    let note_dir = match subdir {
        Some(dir) => vault_path.join(dir),
        None => vault_path.to_path_buf(),
    };
    let note_path = note_dir.join(format!("{}.md", filename));

    if note_path.exists() {
        bail!("Note already exists: {}", note_path.display());
    }

    fs::create_dir_all(&note_dir)
        .with_context(|| format!("Failed to create directory {}", note_dir.display()))?;

    let schema = Schema::load(vault_path)?;
    let mut frontmatter = schema.generate_frontmatter(&leaf_title);

    // Override tags if provided
    if !tags.is_empty() {
        let tags_line = format!("tags: [{}]", tags.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(", "));
        // Replace the tags line in frontmatter
        let lines: Vec<&str> = frontmatter.lines().collect();
        let new_lines: Vec<String> = lines.iter().map(|line| {
            if line.starts_with("tags:") {
                tags_line.clone()
            } else {
                line.to_string()
            }
        }).collect();
        frontmatter = new_lines.join("\n");
    }

    // Add extra fields
    for (k, v) in extra_fields {
        frontmatter.push_str(&format!("\n{}: \"{}\"", k, v));
    }

    let content = format!(
        "---\n{}\n---\n\n# {}\n",
        frontmatter, leaf_title
    );

    fs::write(&note_path, content)
        .with_context(|| format!("Failed to write {}", note_path.display()))?;

    Ok(note_path)
}

/// Split a title that may contain path separators into (subdir, leaf_title).
/// "/subfolder/My Note" -> (Some("subfolder"), "My Note")
/// "subfolder/My Note"  -> (Some("subfolder"), "My Note")
/// "My Note"            -> (None, "My Note")
fn split_title_path(title: &str) -> (Option<&str>, String) {
    let title = title.trim_start_matches('/');
    if let Some(pos) = title.rfind('/') {
        let dir = &title[..pos];
        let leaf = &title[pos + 1..];
        (Some(dir), leaf.to_string())
    } else {
        (None, title.to_string())
    }
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

/// Compute a relative path from `from_dir` to `to_file`
pub fn relative_path(from_dir: &Path, to_file: &Path) -> PathBuf {
    let from: Vec<Component> = from_dir.components().collect();
    let to: Vec<Component> = to_file.components().collect();
    let common = from.iter().zip(to.iter()).take_while(|(a, b)| a == b).count();

    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for comp in &to[common..] {
        result.push(comp);
    }
    result
}

/// Compute the link path for a note based on the configured link_base
pub fn link_path(
    note_path: &Path,
    current_file: &Path,
    link_base: config::LinkBase,
    vault_root: Option<&Path>,
) -> String {
    match link_base {
        config::LinkBase::Relative => {
            let current_dir = current_file.parent().unwrap_or(current_file);
            relative_path(current_dir, note_path).to_string_lossy().to_string()
        }
        config::LinkBase::Vault => {
            if let Some(root) = vault_root {
                format!("/{}", note_path.strip_prefix(root)
                    .unwrap_or(note_path)
                    .to_string_lossy())
            } else {
                format!("/{}", note_path.to_string_lossy())
            }
        }
    }
}

/// Resolve a markdown link target to a note.
/// Tries resolving relative to the current file first, then relative to vault root.
pub fn resolve_link_target<'a>(
    target: &str,
    current_file: &Path,
    notes: &'a [Note],
    vault_root: Option<&Path>,
) -> Option<&'a Note> {
    // Try relative to current file
    if let Some(current_dir) = current_file.parent() {
        if let Ok(resolved) = current_dir.join(target).canonicalize() {
            if let Some(note) = notes.iter().find(|n| n.path.canonicalize().ok().as_ref() == Some(&resolved)) {
                return Some(note);
            }
        }
    }
    // Try relative to vault root (strip leading / if present)
    if let Some(root) = vault_root {
        let stripped = target.strip_prefix('/').unwrap_or(target);
        if let Ok(resolved) = root.join(stripped).canonicalize() {
            if let Some(note) = notes.iter().find(|n| n.path.canonicalize().ok().as_ref() == Some(&resolved)) {
                return Some(note);
            }
        }
    }
    None
}

/// Convert title to filename-safe slug
pub fn slugify(s: &str) -> String {
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

    #[test]
    fn tree_edge_parse_simple() {
        let edge = TreeEdge::parse("domain/ai").unwrap();
        assert_eq!(edge.tree, "domain");
        assert_eq!(edge.parent, "ai");
        assert_eq!(edge.ancestors, vec!["domain", "ai"]);
    }

    #[test]
    fn tree_edge_parse_deep() {
        let edge = TreeEdge::parse("domain/ai/llms/transformers").unwrap();
        assert_eq!(edge.tree, "domain");
        assert_eq!(edge.parent, "transformers");
        assert_eq!(edge.ancestors, vec!["domain", "ai", "llms", "transformers"]);
    }

    #[test]
    fn tree_edge_parse_root() {
        let edge = TreeEdge::parse("domain").unwrap();
        assert_eq!(edge.tree, "domain");
        assert_eq!(edge.parent, "domain");
        assert_eq!(edge.ancestors, vec!["domain"]);
    }

    #[test]
    fn tree_edge_parse_empty() {
        assert!(TreeEdge::parse("").is_none());
    }

    #[test]
    fn extract_tree_edges_from_tags() {
        let tags = vec!["domain/ai".to_string(), "flat-tag".to_string(), "type/reference".to_string()];
        let trees = HashMap::new();
        let edges = extract_tree_edges(&tags, &trees);
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].tree, "domain");
        assert_eq!(edges[1].tree, "type");
    }

    #[test]
    fn extract_tree_edges_from_trees_field() {
        let tags = vec![];
        let mut trees = HashMap::new();
        trees.insert("domain".to_string(), "domain/ai/llms".to_string());
        let edges = extract_tree_edges(&tags, &trees);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].tree, "domain");
        assert_eq!(edges[0].parent, "llms");
    }

    #[test]
    fn relative_path_same_dir() {
        let from = Path::new("/vault/notes");
        let to = Path::new("/vault/notes/foo.md");
        assert_eq!(relative_path(from, to), PathBuf::from("foo.md"));
    }

    #[test]
    fn relative_path_sibling_dir() {
        let from = Path::new("/vault/notes/a");
        let to = Path::new("/vault/notes/b/foo.md");
        assert_eq!(relative_path(from, to), PathBuf::from("../b/foo.md"));
    }

    #[test]
    fn relative_path_parent_dir() {
        let from = Path::new("/vault/notes/a/b");
        let to = Path::new("/vault/notes/foo.md");
        assert_eq!(relative_path(from, to), PathBuf::from("../../foo.md"));
    }

    #[test]
    fn relative_path_child_dir() {
        let from = Path::new("/vault");
        let to = Path::new("/vault/sub/deep/foo.md");
        assert_eq!(relative_path(from, to), PathBuf::from("sub/deep/foo.md"));
    }

    #[test]
    fn link_path_relative_mode() {
        let note = Path::new("/vault/sub/note.md");
        let current = Path::new("/vault/other/current.md");
        let result = link_path(note, current, config::LinkBase::Relative, Some(Path::new("/vault")));
        assert_eq!(result, "../sub/note.md");
    }

    #[test]
    fn link_path_vault_mode() {
        let note = Path::new("/vault/sub/note.md");
        let current = Path::new("/vault/other/current.md");
        let result = link_path(note, current, config::LinkBase::Vault, Some(Path::new("/vault")));
        assert_eq!(result, "/sub/note.md");
    }

}
