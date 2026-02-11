# Contract: Note Format

**Status**: Draft

## Purpose

Defines the structure and constraints of a kg note file, ensuring Obsidian compatibility while supporting kg's typed schema system.

## File Naming

**Pattern**: `{slug}-{uuid}.md`

- **slug**: Slugified version of title
  - Lowercase
  - Spaces → hyphens
  - Remove special characters except hyphens
  - Max 50 characters (truncate if longer)
- **uuid**: 6-character alphanumeric identifier
  - Lowercase letters and numbers
  - Generated on note creation
  - Immutable (survives renames)

**Examples**:
```
my-first-note-a1b2c3.md
getting-started-with-rust-x7y8z9.md
meeting-notes-2026-02-11-k4m5n6.md
```

## File Structure

```markdown
---
title: "Note Title"
created: 2026-02-11T10:30:00Z
modified: 2026-02-11T14:22:00Z
tags:
  - dev/rust
  - projects/kg
aliases:
  - "Alternative Title"
# ... additional schema-defined fields
---

Note body content in Markdown.

Links to other notes: [[Other Note]] or [descriptive text](path/to/note.md)

## Headings

Regular markdown content...
```

## Frontmatter Specification

### Required Fields (Obsidian-compatible)

| Field | Type | Description | Auto-managed |
|-------|------|-------------|--------------|
| `title` | string | Note title | No |
| `created` | datetime | ISO 8601 timestamp | Yes (on create) |
| `modified` | datetime | ISO 8601 timestamp | Yes (on save) |

### Standard Optional Fields

| Field | Type | Description |
|-------|------|-------------|
| `tags` | list[string] | Hierarchical tags (slash notation) |
| `aliases` | list[string] | Alternative titles for WikiLink resolution |

### User-Defined Fields

Additional fields defined in `.kg/schema.yaml` are validated on note creation and edit.

## Link Formats

### WikiLinks

```markdown
[[Note Title]]
[[Note Title|Display Text]]
[[folder/Note Title]]
[[Note Title#Heading]]
[[Note Title#Heading|Display Text]]
```

**Resolution order**:
1. Exact title match
2. Alias match
3. Filename (without UUID) match
4. Path match

### Standard Markdown Links

```markdown
[Display Text](relative/path/to/note-uuid.md)
[Display Text](../other-folder/note-uuid.md)
[Display Text](note-uuid.md#heading)
```

### Metadata Storage in Graph

**All frontmatter becomes graph triples** (subject-predicate-object):

```
note:a1b2c3 --title--> "My Note Title"
note:a1b2c3 --created--> "2026-02-11T10:30:00Z"
note:a1b2c3 --author--> "Alice"
note:a1b2c3 --hasTag--> tag:dev/rust
note:a1b2c3 --linksTo--> note:d4e5f6
```

This unified model enables queries across any field, not just explicit links.

See `specs/features/metadata-graph.md` for full details.

## Tags

### Format
- Slash-separated hierarchy: `parent/child/grandchild`
- Lowercase recommended (but not enforced)
- No spaces (use hyphens)

### Hierarchy Semantics
A note tagged `dev/rust` is implicitly a member of:
- `dev/rust`
- `dev`

### Tags as Trees in the Graph

Tags form tree structures via `parentTag` edges:

```
tag:dev
├── tag:dev/rust      (parentTag -> tag:dev)
│   └── tag:dev/rust/async  (parentTag -> tag:dev/rust)
└── tag:dev/go        (parentTag -> tag:dev)
```

These trees can be realized as **virtual folder views**:
```
.kg/views/tags/
├── dev/
│   ├── rust/
│   │   ├── note-1.md -> symlink
│   │   └── async/
│   │       └── note-2.md -> symlink
│   └── go/
│       └── note-3.md -> symlink
```

See `specs/features/metadata-graph.md` for details.

## Datetime Format

All datetime fields use ISO 8601 format:
- Full: `2026-02-11T10:30:00Z`
- With timezone: `2026-02-11T10:30:00-05:00`
- Date only (where allowed): `2026-02-11`

## Invariants

1. Every note MUST have a valid YAML frontmatter block
2. Every note MUST have `title`, `created`, `modified` fields
3. The UUID portion of filename MUST be unique within the vault
4. The UUID MUST NOT change after note creation
5. `modified` timestamp MUST update when note content changes
6. All fields MUST validate against the vault schema
7. Links MUST be resolvable or flagged per `broken_links` config

## Error Conditions

| Condition | Behavior |
|-----------|----------|
| Missing frontmatter | Error on parse, suggest fix |
| Missing required field | Error on validation |
| Invalid field type | Error on validation, show expected type |
| Duplicate UUID | Error on creation (regenerate) |
| Unresolvable link | Per config: allow, warn, or block |

## Compatibility Notes

### Obsidian Compatibility

- Frontmatter format matches Obsidian expectations
- WikiLinks work in both tools
- `aliases` field enables Obsidian alias resolution
- Tags in frontmatter (Obsidian also supports inline #tags)

### Round-trip Safety

When Obsidian edits a kg note:
- kg will detect `modified` timestamp change
- kg will re-parse and re-validate
- Unknown fields added by Obsidian are preserved (not in schema = passthrough)
