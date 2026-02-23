use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use anyhow::{Context, Result};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store as OxiStore;

use crate::note::Note;

const KBASE_NS: &str = "http://kbase.local/";

pub struct Store {
    inner: OxiStore,
    /// Path to N-Quads cache file (if using nquads backend)
    cache_path: Option<PathBuf>,
    /// Whether the store has been modified since last save
    dirty: AtomicBool,
}

impl Store {
    /// Create an in-memory store (no persistence)
    pub fn new() -> Result<Self> {
        let inner = OxiStore::new()?;
        Ok(Self {
            inner,
            cache_path: None,
            dirty: AtomicBool::new(false),
        })
    }

    /// Create an in-memory store with N-Quads file persistence
    /// Loads existing data from the cache file if it exists
    pub fn new_with_cache(cache_path: &Path) -> Result<Self> {
        let inner = OxiStore::new()?;

        // Load existing data if cache exists
        if cache_path.exists() {
            let file = File::open(cache_path)
                .with_context(|| format!("Failed to open {}", cache_path.display()))?;
            let reader = BufReader::new(file);
            inner.bulk_loader().load_from_reader(
                RdfParser::from_format(RdfFormat::NQuads),
                reader,
            )?;
        }

        Ok(Self {
            inner,
            cache_path: Some(cache_path.to_path_buf()),
            dirty: AtomicBool::new(false),
        })
    }

    /// Open or create a persistent RocksDB store at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let inner = OxiStore::open(path)?;
        Ok(Self {
            inner,
            cache_path: None,
            dirty: AtomicBool::new(false),
        })
    }

    /// Save to N-Quads cache file (if configured)
    pub fn save(&self) -> Result<()> {
        if let Some(ref cache_path) = self.cache_path {
            if self.dirty.load(Ordering::Relaxed) {
                let file = File::create(cache_path)
                    .with_context(|| format!("Failed to create {}", cache_path.display()))?;
                self.inner.dump_to_writer(RdfFormat::NQuads, file)?;
                self.dirty.store(false, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    /// Mark the store as modified
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Check if store has unsaved changes
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    /// Get the stored mtime for a note (as unix timestamp)
    pub fn get_note_mtime(&self, path: &str) -> Result<Option<u64>> {
        // Query by path property since notes are identified by title
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?mtime WHERE {{
                ?note kb:path "{}" .
                ?note kb:mtime ?mtime .
            }}
            "#,
            path.replace('\\', "\\\\").replace('"', "\\\"")
        );

        if let QueryResults::Solutions(mut solutions) = self.inner.query(&query)? {
            if let Some(solution) = solutions.next() {
                let solution = solution?;
                if let Some(Term::Literal(mtime)) = solution.get("mtime") {
                    if let Ok(ts) = mtime.value().parse::<u64>() {
                        return Ok(Some(ts));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Get all stored note paths
    pub fn get_all_note_paths(&self) -> Result<Vec<String>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?path WHERE {{
                ?note kb:type kb:Note .
                ?note kb:path ?path .
            }}
            "#
        );

        let mut paths = Vec::new();
        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::Literal(path)) = solution.get("path") {
                    paths.push(path.value().to_string());
                }
            }
        }
        Ok(paths)
    }

    /// Get all note paths with their mtimes in a single query (for incremental updates)
    pub fn get_all_note_mtimes(&self) -> Result<std::collections::HashMap<String, u64>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?path ?mtime WHERE {{
                ?note kb:type kb:Note .
                ?note kb:path ?path .
                OPTIONAL {{ ?note kb:mtime ?mtime }}
            }}
            "#
        );

        let mut mtimes = std::collections::HashMap::new();
        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::Literal(path)) = solution.get("path") {
                    let mtime = solution.get("mtime")
                        .and_then(|t| if let Term::Literal(lit) = t { Some(lit) } else { None })
                        .and_then(|lit| lit.value().parse::<u64>().ok())
                        .unwrap_or(0);
                    mtimes.insert(path.value().to_string(), mtime);
                }
            }
        }
        Ok(mtimes)
    }

    /// Remove a note completely (for deleted files)
    pub fn remove_note(&self, path: &str) -> Result<()> {
        self.remove_note_triples(path)?;
        self.mark_dirty();
        Ok(())
    }

    /// Remove orphaned tree hierarchy edges that no notes reference
    pub fn cleanup_orphan_tags(&self) -> Result<usize> {
        // Step 1: Get all tree paths currently used by notes
        let used_paths: HashSet<String> = {
            let query = format!(
                r#"
                PREFIX kb: <{KBASE_NS}>

                SELECT DISTINCT ?tree ?parent WHERE {{
                    ?note kb:type kb:Note .
                    ?note ?pred ?parent .
                    FILTER(STRSTARTS(STR(?pred), "{KBASE_NS}child/"))
                    BIND(REPLACE(STR(?pred), "{KBASE_NS}child/", "") AS ?tree)
                }}
                "#
            );

            let mut paths = HashSet::new();
            if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
                for solution in solutions.flatten() {
                    if let (Some(Term::Literal(tree)), Some(Term::NamedNode(parent))) =
                        (solution.get("tree"), solution.get("parent"))
                    {
                        let tree_name = tree.value();
                        let parent_title = parent
                            .as_str()
                            .strip_prefix(&format!("{KBASE_NS}note/"))
                            .and_then(|s| urlencoding::decode(s).ok())
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        if !parent_title.is_empty() {
                            paths.insert(format!("{}/{}", tree_name, parent_title));
                        }
                    }
                }
            }
            paths
        };

        // Step 2: Find all tree hierarchy edges (non-note subjects)
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?child ?pred ?parent WHERE {{
                ?child ?pred ?parent .
                FILTER(STRSTARTS(STR(?pred), "{KBASE_NS}child/"))
                FILTER(!EXISTS {{ ?child kb:type kb:Note }})
            }}
            "#
        );

        let mut to_remove = Vec::new();
        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions.flatten() {
                if let (
                    Some(Term::NamedNode(child)),
                    Some(Term::NamedNode(pred)),
                    Some(Term::NamedNode(parent)),
                ) = (solution.get("child"), solution.get("pred"), solution.get("parent"))
                {
                    let tree = pred
                        .as_str()
                        .strip_prefix(&format!("{KBASE_NS}child/"))
                        .unwrap_or("");
                    let child_title = child
                        .as_str()
                        .strip_prefix(&format!("{KBASE_NS}note/"))
                        .and_then(|s| urlencoding::decode(s).ok())
                        .map(|s| s.to_string())
                        .unwrap_or_default();

                    let path = format!("{}/{}", tree, child_title);

                    // Check if this path (or any descendant) is used by notes
                    let is_used = used_paths.iter().any(|p| p.starts_with(&path));

                    if !is_used {
                        to_remove.push((child.clone(), pred.clone(), parent.clone()));
                    }
                }
            }
        }

        // Step 3: Remove orphaned edges
        let count = to_remove.len();
        for (child, pred, parent) in to_remove {
            self.inner.remove(&Quad::new(
                child,
                pred,
                parent,
                GraphNameRef::DefaultGraph,
            ))?;
        }

        if count > 0 {
            self.mark_dirty();
        }

        Ok(count)
    }

    /// Insert or update a note in the graph
    pub fn upsert_note(&self, note: &Note) -> Result<()> {
        self.upsert_note_with_mtime(note, None)
    }

    /// Insert or update a note with explicit mtime tracking
    pub fn upsert_note_with_mtime(&self, note: &Note, mtime: Option<SystemTime>) -> Result<()> {
        let path_str = note.path.to_string_lossy();
        let note_iri = self.note_iri(&note.title);

        // Remove existing triples for this note (by path lookup)
        self.remove_note_triples(&path_str)?;

        // Add note type
        self.inner.insert(&Quad::new(
            note_iri.clone(),
            self.iri("type"),
            self.iri("Note"),
            GraphNameRef::DefaultGraph,
        ))?;

        // Add title
        self.inner.insert(&Quad::new(
            note_iri.clone(),
            self.iri("title"),
            Literal::new_simple_literal(&note.title),
            GraphNameRef::DefaultGraph,
        ))?;

        // Add path
        self.inner.insert(&Quad::new(
            note_iri.clone(),
            self.iri("path"),
            Literal::new_simple_literal(note.path.to_string_lossy()),
            GraphNameRef::DefaultGraph,
        ))?;

        // Add mtime if provided
        if let Some(mtime) = mtime {
            if let Ok(duration) = mtime.duration_since(SystemTime::UNIX_EPOCH) {
                self.inner.insert(&Quad::new(
                    note_iri.clone(),
                    self.iri("mtime"),
                    Literal::new_simple_literal(duration.as_secs().to_string()),
                    GraphNameRef::DefaultGraph,
                ))?;
            }
        }

        // Add tags and tag hierarchy
        for tag in &note.tags {
            let tag_iri = self.tag_iri(tag);
            self.inner.insert(&Quad::new(
                note_iri.clone(),
                self.iri("hasTag"),
                tag_iri.clone(),
                GraphNameRef::DefaultGraph,
            ))?;

            // Ensure tag hierarchy exists
            self.ensure_tag_hierarchy(tag)?;
        }

        // Add tree edges: <note> <kb:child/{tree}> <parent_note>
        for edge in &note.tree_edges {
            // The parent is the last element in the path
            // Create edge from this note to its parent
            let parent_iri = self.note_iri(&edge.parent);
            let predicate = self.tree_predicate(&edge.tree);
            self.inner.insert(&Quad::new(
                note_iri.clone(),
                predicate,
                parent_iri,
                GraphNameRef::DefaultGraph,
            ))?;

            // Also ensure intermediate nodes exist in the hierarchy
            self.ensure_tree_hierarchy(&edge.tree, &edge.ancestors)?;
        }

        // Add other fields as literals
        for (key, value) in &note.fields {
            if let Some(s) = value.as_str() {
                self.inner.insert(&Quad::new(
                    note_iri.clone(),
                    self.iri(key),
                    Literal::new_simple_literal(s),
                    GraphNameRef::DefaultGraph,
                ))?;
            }
        }

        // Add links
        for link in &note.links {
            self.inner.insert(&Quad::new(
                note_iri.clone(),
                self.iri("linksTo"),
                Literal::new_simple_literal(&link.target),
                GraphNameRef::DefaultGraph,
            ))?;
        }

        self.mark_dirty();
        Ok(())
    }

    /// List notes, optionally filtered by tag (includes descendants)
    pub fn list_notes(&self, tag: Option<&str>, direct_only: bool) -> Result<Vec<String>> {
        let query = match tag {
            Some(tag) => {
                let tag_iri = format!("{KBASE_NS}tag/{tag}");
                if direct_only {
                    // Query notes directly tagged with this specific tag only
                    format!(
                        r#"
                        PREFIX kb: <{KBASE_NS}>

                        SELECT DISTINCT ?title ?path WHERE {{
                            ?note kb:type kb:Note .
                            ?note kb:title ?title .
                            ?note kb:path ?path .
                            ?note kb:hasTag <{tag_iri}> .
                        }}
                        ORDER BY ?title
                        "#
                    )
                } else {
                    // Query notes with this tag or any descendant tag
                    format!(
                        r#"
                        PREFIX kb: <{KBASE_NS}>

                        SELECT DISTINCT ?title ?path WHERE {{
                            ?note kb:type kb:Note .
                            ?note kb:title ?title .
                            ?note kb:path ?path .
                            ?note kb:hasTag ?tag .
                            ?tag kb:parentTag* <{tag_iri}> .
                        }}
                        ORDER BY ?title
                        "#
                    )
                }
            }
            None => {
                // Query all notes
                format!(
                    r#"
                    PREFIX kb: <{KBASE_NS}>

                    SELECT ?title ?path WHERE {{
                        ?note kb:type kb:Note .
                        ?note kb:title ?title .
                        ?note kb:path ?path .
                    }}
                    ORDER BY ?title
                    "#
                )
            }
        };

        let mut results = Vec::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let (Some(Term::Literal(title)), Some(Term::Literal(path))) =
                    (solution.get("title"), solution.get("path"))
                {
                    results.push(format!("{} ({})", title.value(), path.value()));
                }
            }
        }

        Ok(results)
    }

    /// Get all tag paths as plain strings (e.g., "domain", "domain/ai")
    pub fn get_all_tag_paths(&self) -> Result<Vec<String>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?tag WHERE {{
                ?tag kb:type kb:Tag .
            }}
            "#
        );

        let prefix = format!("{}tag/", KBASE_NS);
        let mut tags = Vec::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(tag_node)) = solution.get("tag") {
                    if let Some(tag) = tag_node.as_str().strip_prefix(&prefix) {
                        if !tag.is_empty() {
                            tags.push(tag.to_string());
                        }
                    }
                }
            }
        }

        tags.sort();
        Ok(tags)
    }

    /// List all tags as a tree structure, optionally filtered and with notes
    pub fn list_tags(&self, filter: Option<&str>, show_notes: bool) -> Result<Vec<String>> {
        // Query all tags and their parents
        let tags_query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?tag ?parent WHERE {{
                ?tag kb:type kb:Tag .
                OPTIONAL {{ ?tag kb:parentTag ?parent }}
            }}
            "#
        );

        // Collect tag -> parent relationships
        let mut tag_children: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut has_parent: std::collections::HashSet<String> = std::collections::HashSet::new();

        let prefix = format!("{}tag/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&tags_query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(tag_node)) = solution.get("tag") {
                    let tag = tag_node.as_str().strip_prefix(&prefix).unwrap_or("");
                    if tag.is_empty() {
                        continue;
                    }
                    all_tags.insert(tag.to_string());

                    if let Some(Term::NamedNode(parent_node)) = solution.get("parent") {
                        let parent = parent_node.as_str().strip_prefix(&prefix).unwrap_or("");
                        if !parent.is_empty() {
                            has_parent.insert(tag.to_string());
                            tag_children
                                .entry(parent.to_string())
                                .or_default()
                                .push(tag.to_string());
                        }
                    }
                }
            }
        }

        // Query notes per tag if showing notes
        let mut tag_notes: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        if show_notes {
            let notes_query = format!(
                r#"
                PREFIX kb: <{KBASE_NS}>

                SELECT ?tag ?title WHERE {{
                    ?note kb:type kb:Note .
                    ?note kb:title ?title .
                    ?note kb:hasTag ?tagNode .
                    BIND(REPLACE(STR(?tagNode), "^{KBASE_NS}tag/", "") AS ?tag)
                }}
                ORDER BY ?tag ?title
                "#
            );

            if let QueryResults::Solutions(solutions) = self.inner.query(&notes_query)? {
                for solution in solutions {
                    let solution = solution?;
                    if let (Some(Term::Literal(tag)), Some(Term::Literal(title))) =
                        (solution.get("tag"), solution.get("title"))
                    {
                        tag_notes
                            .entry(tag.value().to_string())
                            .or_default()
                            .push(title.value().to_string());
                    }
                }
            }
        }

        // Find roots based on filter
        let roots: Vec<String> = if let Some(filter_tag) = filter {
            if all_tags.contains(filter_tag) {
                vec![filter_tag.to_string()]
            } else {
                vec![]
            }
        } else {
            let mut r: Vec<String> = all_tags.difference(&has_parent).cloned().collect();
            r.sort();
            r
        };

        // Build tree output
        let mut output = Vec::new();
        for root in roots {
            self.format_tag_tree_with_notes(
                &root,
                "",
                true,
                true,
                &tag_children,
                &tag_notes,
                show_notes,
                &mut output,
            );
        }

        Ok(output)
    }

    #[allow(clippy::too_many_arguments)]
    fn format_tag_tree_with_notes(
        &self,
        tag: &str,
        prefix: &str,
        is_last: bool,
        is_root: bool,
        tag_children: &std::collections::HashMap<String, Vec<String>>,
        tag_notes: &std::collections::HashMap<String, Vec<String>>,
        show_notes: bool,
        output: &mut Vec<String>,
    ) {
        let connector = if is_root {
            ""
        } else if is_last {
            "└── "
        } else {
            "├── "
        };

        // Display just the last segment of the tag
        let display_name = tag.rsplit('/').next().unwrap_or(tag);
        output.push(format!("{}{}{}", prefix, connector, display_name));

        let child_tags = tag_children.get(tag);
        let notes = if show_notes { tag_notes.get(tag) } else { None };

        let has_children = child_tags.is_some_and(|c| !c.is_empty());
        let has_notes = notes.is_some_and(|n| !n.is_empty());

        if !has_children && !has_notes {
            return;
        }

        let new_prefix = if is_root {
            String::new()
        } else if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        // Collect all items (child tags and notes) to determine last item
        let mut sorted_child_tags: Vec<String> = child_tags.cloned().unwrap_or_default();
        sorted_child_tags.sort();

        let sorted_notes: Vec<String> = notes.cloned().unwrap_or_default();

        let total_items = sorted_child_tags.len() + sorted_notes.len();
        let mut item_index = 0;

        // Print child tags first
        for child in &sorted_child_tags {
            item_index += 1;
            let child_is_last = item_index == total_items;
            self.format_tag_tree_with_notes(
                child,
                &new_prefix,
                child_is_last,
                false,
                tag_children,
                tag_notes,
                show_notes,
                output,
            );
        }

        // Print notes
        for note_title in &sorted_notes {
            item_index += 1;
            let note_is_last = item_index == total_items;
            let note_connector = if note_is_last { "└── " } else { "├── " };
            output.push(format!("{}{}[{}]", new_prefix, note_connector, note_title));
        }
    }

    /// Get all tag paths as a flat list
    pub fn all_tags(&self) -> Result<Vec<String>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?tag WHERE {{
                ?tag kb:type kb:Tag .
            }}
            "#
        );

        let prefix = format!("{}tag/", KBASE_NS);
        let mut tags = Vec::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(tag_node)) = solution.get("tag") {
                    if let Some(tag) = tag_node.as_str().strip_prefix(&prefix) {
                        if !tag.is_empty() {
                            tags.push(tag.to_string());
                        }
                    }
                }
            }
        }

        tags.sort();
        Ok(tags)
    }

    /// Find notes that link to the given note
    pub fn backlinks(&self, note_ref: &str) -> Result<Vec<String>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?title ?path WHERE {{
                ?note kb:type kb:Note .
                ?note kb:title ?title .
                ?note kb:path ?path .
                ?note kb:linksTo ?link .
                FILTER(CONTAINS(LCASE(?link), LCASE("{note_ref}")))
            }}
            ORDER BY ?title
            "#
        );

        let mut results = Vec::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let (Some(Term::Literal(title)), Some(Term::Literal(path))) =
                    (solution.get("title"), solution.get("path"))
                {
                    results.push(format!("{} ({})", title.value(), path.value()));
                }
            }
        }

        Ok(results)
    }

    /// Ensure tag hierarchy exists (create parent tags)
    fn ensure_tag_hierarchy(&self, tag: &str) -> Result<()> {
        let parts: Vec<&str> = tag.split('/').collect();

        for i in 0..parts.len() {
            let current = parts[..=i].join("/");
            let current_iri = self.tag_iri(&current);

            // Mark as tag type
            self.inner.insert(&Quad::new(
                current_iri.clone(),
                self.iri("type"),
                self.iri("Tag"),
                GraphNameRef::DefaultGraph,
            ))?;

            // Add parent relationship
            if i > 0 {
                let parent = parts[..i].join("/");
                let parent_iri = self.tag_iri(&parent);

                self.inner.insert(&Quad::new(
                    current_iri,
                    self.iri("parentTag"),
                    parent_iri,
                    GraphNameRef::DefaultGraph,
                ))?;
            }
        }

        Ok(())
    }

    /// Remove all triples for a note
    fn remove_note_triples(&self, path: &str) -> Result<()> {
        // Find note IRI by path property
        let note_iri = match self.find_note_iri_by_path(path)? {
            Some(iri) => iri,
            None => return Ok(()), // Note doesn't exist, nothing to remove
        };

        // Find and remove all triples where note is subject
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?p ?o WHERE {{
                <{}> ?p ?o .
            }}
            "#,
            note_iri.as_str()
        );

        let mut to_remove = Vec::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let (Some(p), Some(o)) = (solution.get("p"), solution.get("o")) {
                    to_remove.push((p.clone(), o.clone()));
                }
            }
        }

        for (p, o) in to_remove {
            if let (Term::NamedNode(p), o) = (p, o) {
                self.inner.remove(&Quad::new(
                    note_iri.clone(),
                    p,
                    o,
                    GraphNameRef::DefaultGraph,
                ))?;
            }
        }

        Ok(())
    }

    /// Create IRI for a note by title (unified scheme for notes and tree nodes)
    fn note_iri(&self, title: &str) -> NamedNode {
        let encoded = urlencoding::encode(title);
        NamedNode::new_unchecked(format!("{}note/{}", KBASE_NS, encoded))
    }

    /// Find note IRI by path property (for lookups when we only have path)
    fn find_note_iri_by_path(&self, path: &str) -> Result<Option<NamedNode>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?note WHERE {{
                ?note kb:path "{}" .
            }}
            "#,
            path.replace('\\', "\\\\").replace('"', "\\\"")
        );

        if let QueryResults::Solutions(mut solutions) = self.inner.query(&query)? {
            if let Some(solution) = solutions.next() {
                let solution = solution?;
                if let Some(Term::NamedNode(node)) = solution.get("note") {
                    return Ok(Some(node.clone()));
                }
            }
        }
        Ok(None)
    }

    fn tag_iri(&self, tag: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}tag/{}", KBASE_NS, tag))
    }

    fn iri(&self, local: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}{}", KBASE_NS, local))
    }

    /// Create predicate for tree edge: kb:child/{tree_name}
    fn tree_predicate(&self, tree: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}child/{}", KBASE_NS, tree))
    }

    /// Ensure all nodes in a tree hierarchy exist
    fn ensure_tree_hierarchy(&self, tree: &str, ancestors: &[String]) -> Result<()> {
        let predicate = self.tree_predicate(tree);

        // Create edges between consecutive ancestors
        // e.g., for [domain, ai, llms]: domain<-ai, ai<-llms
        for i in 1..ancestors.len() {
            let child_iri = self.note_iri(&ancestors[i]);
            let parent_iri = self.note_iri(&ancestors[i - 1]);

            self.inner.insert(&Quad::new(
                child_iri,
                predicate.clone(),
                parent_iri,
                GraphNameRef::DefaultGraph,
            ))?;
        }

        Ok(())
    }

    /// Get children of a note in a specific tree
    pub fn tree_children(&self, tree: &str, parent_title: &str) -> Result<Vec<String>> {
        let parent_iri = self.note_iri(parent_title);
        let predicate = format!("{}child/{}", KBASE_NS, tree);

        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?child WHERE {{
                ?child <{predicate}> <{}> .
            }}
            "#,
            parent_iri.as_str()
        );

        let mut children = Vec::new();
        let prefix = format!("{}note/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(child)) = solution.get("child") {
                    if let Some(encoded) = child.as_str().strip_prefix(&prefix) {
                        if let Ok(decoded) = urlencoding::decode(encoded) {
                            children.push(decoded.to_string());
                        }
                    }
                }
            }
        }

        Ok(children)
    }

    /// Get all descendants of a note in a specific tree (recursive)
    pub fn tree_descendants(&self, tree: &str, parent_title: &str) -> Result<Vec<String>> {
        let parent_iri = self.note_iri(parent_title);
        let predicate = format!("{}child/{}", KBASE_NS, tree);

        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?descendant WHERE {{
                ?descendant <{predicate}>+ <{}> .
            }}
            "#,
            parent_iri.as_str()
        );

        let mut descendants = Vec::new();
        let prefix = format!("{}note/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(desc)) = solution.get("descendant") {
                    if let Some(encoded) = desc.as_str().strip_prefix(&prefix) {
                        if let Ok(decoded) = urlencoding::decode(encoded) {
                            descendants.push(decoded.to_string());
                        }
                    }
                }
            }
        }

        Ok(descendants)
    }

    /// Get all tree roots (nodes with no parent in that tree)
    pub fn tree_roots(&self, tree: &str) -> Result<Vec<String>> {
        let predicate = format!("{}child/{}", KBASE_NS, tree);

        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT DISTINCT ?root WHERE {{
                ?child <{predicate}> ?root .
                FILTER NOT EXISTS {{ ?root <{predicate}> ?parent }}
            }}
            "#
        );

        let mut roots = Vec::new();
        let prefix = format!("{}note/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(root)) = solution.get("root") {
                    if let Some(encoded) = root.as_str().strip_prefix(&prefix) {
                        if let Ok(decoded) = urlencoding::decode(encoded) {
                            roots.push(decoded.to_string());
                        }
                    }
                }
            }
        }

        Ok(roots)
    }

    /// Get all tree paths for autocompletion
    /// Returns paths like "type", "type/concept", "domain/ai"
    pub fn all_tree_paths(&self) -> Result<Vec<String>> {
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT DISTINCT ?predicate ?child ?parent WHERE {{
                ?child ?predicate ?parent .
                FILTER(STRSTARTS(STR(?predicate), "{KBASE_NS}child/"))
            }}
            "#
        );

        let mut paths = std::collections::HashSet::new();
        let note_prefix = format!("{}note/", KBASE_NS);
        let pred_prefix = format!("{}child/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let (
                    Some(Term::NamedNode(pred)),
                    Some(Term::NamedNode(child)),
                    Some(Term::NamedNode(parent)),
                ) = (
                    solution.get("predicate"),
                    solution.get("child"),
                    solution.get("parent"),
                ) {
                    if let (Some(tree), Some(child_enc), Some(parent_enc)) = (
                        pred.as_str().strip_prefix(&pred_prefix),
                        child.as_str().strip_prefix(&note_prefix),
                        parent.as_str().strip_prefix(&note_prefix),
                    ) {
                        let child_decoded = urlencoding::decode(child_enc)
                            .map(|s| s.to_string())
                            .unwrap_or_default();
                        let parent_decoded = urlencoding::decode(parent_enc)
                            .map(|s| s.to_string())
                            .unwrap_or_default();

                        // Add the tree name itself
                        paths.insert(tree.to_string());

                        // Add parent if different from tree name (avoid type/type)
                        if parent_decoded != tree {
                            paths.insert(format!("{}/{}", tree, parent_decoded));
                        }

                        // Add child path
                        if parent_decoded == tree {
                            // Parent is tree root, so just tree/child
                            paths.insert(format!("{}/{}", tree, child_decoded));
                        } else {
                            // Full path: tree/parent/child
                            paths.insert(format!("{}/{}/{}", tree, parent_decoded, child_decoded));
                        }
                    }
                }
            }
        }

        let mut result: Vec<String> = paths.into_iter().collect();
        result.sort();
        Ok(result)
    }

    /// Validate graph constraints using SPARQL queries
    /// Returns violations as (note_title, note_path, constraint_message)
    pub fn validate_constraints(
        &self,
        constraints: &[crate::schema::Constraint],
    ) -> Result<Vec<(String, String, String)>> {
        let mut violations = Vec::new();

        for constraint in constraints {
            // The query should SELECT ?note ?title ?path for violating notes
            // We wrap user query to ensure it returns what we need
            let wrapped_query = format!(
                r#"
                PREFIX kb: <{KBASE_NS}>

                SELECT ?title ?path WHERE {{
                    ?note kb:type kb:Note .
                    ?note kb:title ?title .
                    ?note kb:path ?path .
                    {}
                }}
                "#,
                constraint.query
            );

            if let QueryResults::Solutions(solutions) = self.inner.query(&wrapped_query)? {
                for solution in solutions {
                    let solution = solution?;
                    if let (Some(Term::Literal(title)), Some(Term::Literal(path))) =
                        (solution.get("title"), solution.get("path"))
                    {
                        violations.push((
                            title.value().to_string(),
                            path.value().to_string(),
                            constraint.message.clone(),
                        ));
                    }
                }
            }
        }

        Ok(violations)
    }

    /// Run an arbitrary SPARQL SELECT query
    /// Returns results as Vec of Vec<(var_name, value)>
    pub fn query(&self, sparql: &str) -> Result<Vec<Vec<(String, String)>>> {
        // Auto-add PREFIX if not present
        let query = if sparql.to_uppercase().contains("PREFIX") {
            sparql.to_string()
        } else {
            format!("PREFIX kb: <{}>\n{}", KBASE_NS, sparql)
        };

        let mut results = Vec::new();

        match self.inner.query(&query)? {
            QueryResults::Solutions(solutions) => {
                let vars: Vec<String> = solutions.variables().iter().map(|v| v.as_str().to_string()).collect();
                for solution in solutions.flatten() {
                    let mut row = Vec::new();
                    for var in &vars {
                        let value = solution.get(var.as_str())
                            .map(|term| match term {
                                Term::NamedNode(n) => n.as_str().to_string(),
                                Term::Literal(l) => l.value().to_string(),
                                Term::BlankNode(b) => format!("_:{}", b.as_str()),
                                Term::Triple(_) => "<triple>".to_string(),
                            })
                            .unwrap_or_default();
                        row.push((var.clone(), value));
                    }
                    results.push(row);
                }
            }
            QueryResults::Boolean(b) => {
                results.push(vec![("result".to_string(), b.to_string())]);
            }
            QueryResults::Graph(_) => {
                anyhow::bail!("CONSTRUCT/DESCRIBE queries not supported, use SELECT");
            }
        }

        Ok(results)
    }

    /// Get the SPARQL schema documentation
    pub fn schema_doc() -> &'static str {
        r#"SPARQL Schema (prefix kb: <http://kbase.local/>):

ENTITIES:
  Notes:  kb:note/{encoded_title}  (type: kb:Note)
  Tags:   kb:tag/{tag_path}        (type: kb:Tag)

NOTE PROPERTIES:
  kb:title    - Note title (literal)
  kb:path     - File path (literal)
  kb:mtime    - Modification time as Unix timestamp (literal)
  kb:linksTo  - Link target title (literal, one per link)
  kb:hasTag   - Tag IRI (links to kb:tag/*)

TAG PROPERTIES:
  kb:parentTag - Parent tag IRI (for hierarchical tags like domain/ai)

TREE EDGES:
  kb:child/{tree_name} - Links child note to parent note in a tree

EXAMPLE QUERIES:
  # All notes with their tags
  SELECT ?title ?tag WHERE {
    ?n kb:type kb:Note ; kb:title ?title ; kb:hasTag ?t .
    BIND(REPLACE(STR(?t), "^.*tag/", "") AS ?tag)
  }

  # Notes linking to "Concept"
  SELECT ?title WHERE {
    ?n kb:type kb:Note ; kb:title ?title ; kb:linksTo "Concept" .
  }

  # Tag hierarchy
  SELECT ?child ?parent WHERE {
    ?c kb:type kb:Tag ; kb:parentTag ?p .
    BIND(REPLACE(STR(?c), "^.*tag/", "") AS ?child)
    BIND(REPLACE(STR(?p), "^.*tag/", "") AS ?parent)
  }
"#
    }

    /// Export the graph as DOT format (Graphviz)
    pub fn export_dot(&self) -> Result<String> {
        let mut dot = String::from("digraph vault {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box];\n\n");

        // Get all notes
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?title ?path WHERE {{
                ?note kb:type kb:Note .
                ?note kb:title ?title .
                ?note kb:path ?path .
            }}
            "#
        );

        let mut notes: Vec<(String, String)> = Vec::new();
        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions.flatten() {
                if let (Some(Term::Literal(title)), Some(Term::Literal(path))) =
                    (solution.get("title"), solution.get("path"))
                {
                    notes.push((title.value().to_string(), path.value().to_string()));
                }
            }
        }

        // Add note nodes
        dot.push_str("  // Notes\n");
        for (title, _path) in &notes {
            let escaped = title.replace('"', "\\\"");
            let id = Self::dot_id(title);
            dot.push_str(&format!("  {} [label=\"{}\"];\n", id, escaped));
        }

        // Get links between notes
        dot.push_str("\n  // Links\n");
        let link_query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?from_title ?target WHERE {{
                ?note kb:type kb:Note .
                ?note kb:title ?from_title .
                ?note kb:linksTo ?target .
            }}
            "#
        );

        if let QueryResults::Solutions(solutions) = self.inner.query(&link_query)? {
            for solution in solutions.flatten() {
                if let (Some(Term::Literal(from)), Some(Term::Literal(target))) =
                    (solution.get("from_title"), solution.get("target"))
                {
                    let from_id = Self::dot_id(from.value());
                    let to_id = Self::dot_id(target.value());
                    dot.push_str(&format!("  {} -> {} [style=dashed, color=gray];\n", from_id, to_id));
                }
            }
        }

        // Get tree edges
        dot.push_str("\n  // Tree edges\n");
        let tree_query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?child ?pred ?parent WHERE {{
                ?child ?pred ?parent .
                FILTER(STRSTARTS(STR(?pred), "{KBASE_NS}child/"))
            }}
            "#
        );

        let note_prefix = format!("{}note/", KBASE_NS);
        let pred_prefix = format!("{}child/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&tree_query)? {
            for solution in solutions.flatten() {
                if let (
                    Some(Term::NamedNode(child)),
                    Some(Term::NamedNode(pred)),
                    Some(Term::NamedNode(parent)),
                ) = (solution.get("child"), solution.get("pred"), solution.get("parent"))
                {
                    if let (Some(child_enc), Some(parent_enc), Some(tree)) = (
                        child.as_str().strip_prefix(&note_prefix),
                        parent.as_str().strip_prefix(&note_prefix),
                        pred.as_str().strip_prefix(&pred_prefix),
                    ) {
                        let child_title = urlencoding::decode(child_enc).unwrap_or_default();
                        let parent_title = urlencoding::decode(parent_enc).unwrap_or_default();
                        let child_id = Self::dot_id(&child_title);
                        let parent_id = Self::dot_id(&parent_title);
                        dot.push_str(&format!(
                            "  {} -> {} [label=\"{}\", color=blue];\n",
                            child_id, parent_id, tree
                        ));
                    }
                }
            }
        }

        dot.push_str("}\n");
        Ok(dot)
    }

    /// Export the graph as GraphML format
    pub fn export_graphml(&self) -> Result<String> {
        let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>
<graphml xmlns="http://graphml.graphdrawing.org/xmlns">
  <key id="title" for="node" attr.name="title" attr.type="string"/>
  <key id="path" for="node" attr.name="path" attr.type="string"/>
  <key id="type" for="edge" attr.name="type" attr.type="string"/>
  <key id="tree" for="edge" attr.name="tree" attr.type="string"/>
  <graph id="vault" edgedefault="directed">
"#);

        // Get all notes
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?title ?path WHERE {{
                ?note kb:type kb:Note .
                ?note kb:title ?title .
                ?note kb:path ?path .
            }}
            "#
        );

        let mut node_id = 0;
        let mut title_to_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions.flatten() {
                if let (Some(Term::Literal(title)), Some(Term::Literal(path))) =
                    (solution.get("title"), solution.get("path"))
                {
                    let t = title.value().to_string();
                    let p = path.value().to_string();
                    title_to_id.insert(t.clone(), node_id);
                    xml.push_str(&format!(
                        "    <node id=\"n{}\">\n      <data key=\"title\">{}</data>\n      <data key=\"path\">{}</data>\n    </node>\n",
                        node_id,
                        Self::xml_escape(&t),
                        Self::xml_escape(&p)
                    ));
                    node_id += 1;
                }
            }
        }

        // Get links
        let mut edge_id = 0;
        let link_query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?from_title ?target WHERE {{
                ?note kb:type kb:Note .
                ?note kb:title ?from_title .
                ?note kb:linksTo ?target .
            }}
            "#
        );

        if let QueryResults::Solutions(solutions) = self.inner.query(&link_query)? {
            for solution in solutions.flatten() {
                if let (Some(Term::Literal(from)), Some(Term::Literal(target))) =
                    (solution.get("from_title"), solution.get("target"))
                {
                    let from_title = from.value();
                    let to_title = target.value();
                    if let (Some(&from_id), Some(&to_id)) =
                        (title_to_id.get(from_title), title_to_id.get(to_title))
                    {
                        xml.push_str(&format!(
                            "    <edge id=\"e{}\" source=\"n{}\" target=\"n{}\">\n      <data key=\"type\">link</data>\n    </edge>\n",
                            edge_id, from_id, to_id
                        ));
                        edge_id += 1;
                    }
                }
            }
        }

        // Get tree edges
        let tree_query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?child ?pred ?parent WHERE {{
                ?child ?pred ?parent .
                FILTER(STRSTARTS(STR(?pred), "{KBASE_NS}child/"))
            }}
            "#
        );

        let note_prefix = format!("{}note/", KBASE_NS);
        let pred_prefix = format!("{}child/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&tree_query)? {
            for solution in solutions.flatten() {
                if let (
                    Some(Term::NamedNode(child)),
                    Some(Term::NamedNode(pred)),
                    Some(Term::NamedNode(parent)),
                ) = (solution.get("child"), solution.get("pred"), solution.get("parent"))
                {
                    if let (Some(child_enc), Some(parent_enc), Some(tree)) = (
                        child.as_str().strip_prefix(&note_prefix),
                        parent.as_str().strip_prefix(&note_prefix),
                        pred.as_str().strip_prefix(&pred_prefix),
                    ) {
                        let child_title = urlencoding::decode(child_enc).unwrap_or_default().to_string();
                        let parent_title = urlencoding::decode(parent_enc).unwrap_or_default().to_string();

                        // Ensure nodes exist for tree nodes that aren't notes
                        let child_id = *title_to_id.entry(child_title.clone()).or_insert_with(|| {
                            let id = node_id;
                            xml.push_str(&format!(
                                "    <node id=\"n{}\">\n      <data key=\"title\">{}</data>\n    </node>\n",
                                id,
                                Self::xml_escape(&child_title)
                            ));
                            node_id += 1;
                            id
                        });
                        let parent_id = *title_to_id.entry(parent_title.clone()).or_insert_with(|| {
                            let id = node_id;
                            xml.push_str(&format!(
                                "    <node id=\"n{}\">\n      <data key=\"title\">{}</data>\n    </node>\n",
                                id,
                                Self::xml_escape(&parent_title)
                            ));
                            node_id += 1;
                            id
                        });

                        xml.push_str(&format!(
                            "    <edge id=\"e{}\" source=\"n{}\" target=\"n{}\">\n      <data key=\"type\">tree</data>\n      <data key=\"tree\">{}</data>\n    </edge>\n",
                            edge_id, child_id, parent_id, Self::xml_escape(tree)
                        ));
                        edge_id += 1;
                    }
                }
            }
        }

        xml.push_str("  </graph>\n</graphml>\n");
        Ok(xml)
    }

    /// Create a valid DOT node ID from a title
    fn dot_id(title: &str) -> String {
        let clean: String = title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        format!("n_{}", clean)
    }

    /// Escape special XML characters
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
}
