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

    /// Insert or update a note in the graph
    pub fn upsert_note(&self, note: &Note) -> Result<()> {
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
                Literal::new_simple_literal(link),
                GraphNameRef::DefaultGraph,
            ))?;
        }

        Ok(())
    }

    /// List notes, optionally filtered by tag (includes descendants)
    pub fn list_notes(&self, tag: Option<&str>) -> Result<Vec<String>> {
        let query = match tag {
            Some(tag) => {
                let tag_iri = format!("{KBASE_NS}tag/{tag}");
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

    /// List all tags as a tree structure
    pub fn list_tags(&self) -> Result<Vec<String>> {
        // Query all tags and their parents
        let query = format!(
            r#"
            PREFIX kb: <{KBASE_NS}>

            SELECT ?tag ?parent WHERE {{
                ?tag kb:type kb:Tag .
                OPTIONAL {{ ?tag kb:parentTag ?parent }}
            }}
            "#
        );

        // Collect tag -> parent relationships
        let mut children: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut has_parent: std::collections::HashSet<String> = std::collections::HashSet::new();

        let prefix = format!("{}tag/", KBASE_NS);

        if let QueryResults::Solutions(solutions) = self.inner.query(&query)? {
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
                            children
                                .entry(parent.to_string())
                                .or_default()
                                .push(tag.to_string());
                        }
                    }
                }
            }
        }

        // Find root tags (no parent)
        let mut roots: Vec<String> = all_tags.difference(&has_parent).cloned().collect();
        roots.sort();

        // Build tree output
        let mut output = Vec::new();
        for root in roots {
            self.format_tag_tree(&root, "", true, true, &children, &mut output);
        }

        Ok(output)
    }

    fn format_tag_tree(
        &self,
        tag: &str,
        prefix: &str,
        is_last: bool,
        is_root: bool,
        children: &std::collections::HashMap<String, Vec<String>>,
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

        if let Some(child_tags) = children.get(tag) {
            let mut sorted_children = child_tags.clone();
            sorted_children.sort();

            let new_prefix = if is_root {
                String::new()
            } else if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };

            for (i, child) in sorted_children.iter().enumerate() {
                let child_is_last = i == sorted_children.len() - 1;
                self.format_tag_tree(child, &new_prefix, child_is_last, false, children, output);
            }
        }
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
}
