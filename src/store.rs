use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use oxigraph::model::*;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store as OxiStore;

use crate::note::Note;

const KBASE_NS: &str = "http://kbase.local/";

pub struct Store {
    inner: OxiStore,
}

impl Store {
    /// Create an in-memory store
    pub fn new() -> Result<Self> {
        let inner = OxiStore::new()?;
        Ok(Self { inner })
    }

    /// Open or create a persistent store at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let inner = OxiStore::open(path)?;
        Ok(Self { inner })
    }

    /// Get the stored mtime for a note (as unix timestamp)
    pub fn get_note_mtime(&self, path: &str) -> Result<Option<u64>> {
        let note_iri = self.note_iri(path);
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>
            SELECT ?mtime WHERE {{
                <{}> kb:mtime ?mtime .
            }}
            "#,
            note_iri.as_str()
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

    /// Remove a note completely (for deleted files)
    pub fn remove_note(&self, path: &str) -> Result<()> {
        self.remove_note_triples(path)
    }

    /// Insert or update a note in the graph
    pub fn upsert_note(&self, note: &Note) -> Result<()> {
        self.upsert_note_with_mtime(note, None)
    }

    /// Insert or update a note with explicit mtime tracking
    pub fn upsert_note_with_mtime(&self, note: &Note, mtime: Option<SystemTime>) -> Result<()> {
        let path_str = note.path.to_string_lossy();
        let note_iri = self.note_iri(&path_str);

        // Remove existing triples for this note
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
            let parent_iri = self.note_iri_by_title(&edge.parent);
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
        let note_iri = self.note_iri(path);

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

    fn note_iri(&self, path: &str) -> NamedNode {
        // Encode path for valid IRI (replace spaces, special chars)
        let encoded = path.replace(' ', "%20").replace('#', "%23");
        NamedNode::new_unchecked(format!("{}note/{}", KBASE_NS, encoded))
    }

    fn tag_iri(&self, tag: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}tag/{}", KBASE_NS, tag))
    }

    fn iri(&self, local: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}{}", KBASE_NS, local))
    }

    /// Create IRI for a note by title (for tree edges)
    fn note_iri_by_title(&self, title: &str) -> NamedNode {
        let encoded = title.replace(' ', "%20").replace('#', "%23");
        NamedNode::new_unchecked(format!("{}note/title/{}", KBASE_NS, encoded))
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
            let child_iri = self.note_iri_by_title(&ancestors[i]);
            let parent_iri = self.note_iri_by_title(&ancestors[i - 1]);

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
        let parent_iri = self.note_iri_by_title(parent_title);
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
        let prefix = format!("{}note/title/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(child)) = solution.get("child") {
                    if let Some(title) = child.as_str().strip_prefix(&prefix) {
                        let decoded = title.replace("%20", " ").replace("%23", "#");
                        children.push(decoded);
                    }
                }
            }
        }

        Ok(children)
    }

    /// Get all descendants of a note in a specific tree (recursive)
    pub fn tree_descendants(&self, tree: &str, parent_title: &str) -> Result<Vec<String>> {
        let parent_iri = self.note_iri_by_title(parent_title);
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
        let prefix = format!("{}note/title/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(desc)) = solution.get("descendant") {
                    if let Some(title) = desc.as_str().strip_prefix(&prefix) {
                        let decoded = title.replace("%20", " ").replace("%23", "#");
                        descendants.push(decoded);
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
        let prefix = format!("{}note/title/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
            for solution in solutions {
                let solution = solution?;
                if let Some(Term::NamedNode(root)) = solution.get("root") {
                    if let Some(title) = root.as_str().strip_prefix(&prefix) {
                        let decoded = title.replace("%20", " ").replace("%23", "#");
                        roots.push(decoded);
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
        let note_prefix = format!("{}note/title/", KBASE_NS);
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
                    if let (Some(tree), Some(child_title), Some(parent_title)) = (
                        pred.as_str().strip_prefix(&pred_prefix),
                        child.as_str().strip_prefix(&note_prefix),
                        parent.as_str().strip_prefix(&note_prefix),
                    ) {
                        let child_decoded = child_title.replace("%20", " ");
                        let parent_decoded = parent_title.replace("%20", " ");

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
}
