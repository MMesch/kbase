use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Install Claude Code skills to the current working directory
pub fn install(_vault_path: &Path) -> Result<Vec<String>> {
    let skills_dir = std::env::current_dir()?.join(".claude").join("skills");
    let mut installed = Vec::new();

    // Create each skill
    for (name, content) in SKILLS {
        let skill_dir = skills_dir.join(name);
        fs::create_dir_all(&skill_dir)
            .with_context(|| format!("Failed to create {}", skill_dir.display()))?;

        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, content)
            .with_context(|| format!("Failed to write {}", skill_file.display()))?;

        installed.push(name.to_string());
    }

    Ok(installed)
}

const SKILLS: &[(&str, &str)] = &[
    ("kb-search", KB_SEARCH),
    ("kb-similar", KB_SIMILAR),
    ("kb-backlinks", KB_BACKLINKS),
    ("kb-validate", KB_VALIDATE),
    ("kb-new", KB_NEW),
    ("kb-overview", KB_OVERVIEW),
    ("kb-query", KB_QUERY),
    ("kb-tags", KB_TAGS),
    ("kb-list", KB_LIST),
    ("kb-retag", KB_RETAG),
    ("kb-organize", KB_ORGANIZE),
];

const KB_SEARCH: &str = r#"---
name: kb-search
description: Search the knowledge base for relevant information about a topic
---

# Knowledge Base Search

Search the knowledge base for: $ARGUMENTS

## Results

!`kbase search "$ARGUMENTS" --limit 10`

## Instructions

Use the search results above to inform your response. Reference specific notes when relevant.
If no results are found, acknowledge this and suggest alternative search terms.
"#;

const KB_SIMILAR: &str = r#"---
name: kb-similar
description: Find notes similar to a given note in the knowledge base
---

# Find Similar Notes

Finding notes similar to: $ARGUMENTS

## Similar Notes

!`kbase similar "$ARGUMENTS" --limit 10`

## Instructions

Use these related notes to provide additional context or connections.
Highlight relevant relationships between the notes.
"#;

const KB_BACKLINKS: &str = r#"---
name: kb-backlinks
description: Find notes that reference a given topic
---

# Backlinks

Finding notes that link to: $ARGUMENTS

## References

!`kbase backlinks "$ARGUMENTS"`

## Instructions

These notes reference the topic. Use them to understand:
- How this concept relates to others
- What depends on this topic
- The broader context
"#;

const KB_VALIDATE: &str = r#"---
name: kb-validate
description: Validate the knowledge base against its schema and check for broken links
---

# Knowledge Base Validation

!`kbase validate`

## Instructions

Review any validation errors above. For each error:
1. Explain what the error means
2. Suggest how to fix it
3. Offer to help fix the issue if requested
"#;

const KB_NEW: &str = r#"---
name: kb-new
description: Create a new note in the knowledge base
---

# Create New Note

Create a new note with title: $ARGUMENTS

## Instructions

1. First, create the note:
   ```bash
   kbase new "$ARGUMENTS"
   ```

2. Then help the user fill in the content based on:
   - The current conversation context
   - Related notes (search for similar topics)
   - The vault's schema requirements

3. Suggest appropriate:
   - Tags based on existing tag hierarchy
   - Links to related notes
   - Required fields from the schema
"#;

const KB_OVERVIEW: &str = r#"---
name: kb-overview
description: Get an overview of the knowledge base structure
---

# Knowledge Base Overview

!`kbase overview --limit 10`

## Instructions

Use this overview to understand:
- **Tags**: The taxonomy and categorization of knowledge
- **Most referenced notes** (high in-degree): Key concepts that many notes link to
- **Most referencing notes** (high out-degree): Index or MOC (Map of Content) notes
- **Orphan notes**: Notes that may need better integration

Suggest ways to improve organization if requested.
"#;

const KB_QUERY: &str = r#"---
name: kb-query
description: Run SPARQL queries on the knowledge graph
---

# Knowledge Graph Query

Query: $ARGUMENTS

## Schema Reference

!`kbase query schema`

## Query Results

!`kbase query "$ARGUMENTS"`

## Instructions

Help the user understand the query results. If the query failed, suggest corrections based on the schema.

Common queries:
- Notes with specific tags: `SELECT ?title WHERE { ?n kb:title ?title ; kb:hasTag <kb:tag/mytag> }`
- All links from a note: `SELECT ?target WHERE { ?n kb:title "Note" ; kb:linksTo ?target }`
- Tag hierarchy: `SELECT ?tag ?parent WHERE { ?t kb:type kb:Tag ; kb:parentTag ?p . BIND(...) }`
"#;

const KB_TAGS: &str = r#"---
name: kb-tags
description: Show the tag hierarchy in the knowledge base
---

# Tag Hierarchy

!`kbase tags --notes`

## Instructions

Use the tag tree to understand how knowledge is organized.
Suggest improvements to the taxonomy if requested.
"#;

const KB_LIST: &str = r#"---
name: kb-list
description: List notes in the knowledge base, optionally filtered by tag
---

# Notes

!`kbase list $ARGUMENTS`

## Instructions

Show the list of notes. If $ARGUMENTS contains a tag filter (e.g. `--tag recipe`), explain the filtering.
"#;

const KB_RETAG: &str = r#"---
name: kb-retag
description: Rename tags by replacing a prefix across all notes
---

# Retag Notes

Renaming tag prefix: $ARGUMENTS

## Preview

!`kbase retag $ARGUMENTS --dry-run`

## Instructions

Show the user what would change. If they confirm, run without --dry-run:
```bash
kbase retag $ARGUMENTS
```

Examples:
- `kbase retag tech/ai type` — renames tech/ai -> type, tech/ai/ml -> type/ml
- `kbase retag old new` — renames old -> new, old/sub -> new/sub
"#;

const KB_ORGANIZE: &str = r#"---
name: kb-organize
description: Organize notes into directories based on tag hierarchy
---

# Organize Notes

## Preview

!`kbase organize $ARGUMENTS --dry-run`

## Instructions

Show the user the planned file moves. If they confirm, run without --dry-run:
```bash
kbase organize $ARGUMENTS
```

Options:
- `--tree domain` — only organize by tags under the "domain" prefix
- `--flat` — move all notes to vault root instead of creating directories
- `--symlinks` — create symlinks for notes with multiple tag paths
"#;
