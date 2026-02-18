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
description: Validate the knowledge base against its schema
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
