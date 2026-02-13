---
title: Note CRUD Operations
tags:
  - spec/feature
status: complete
---
# Feature: Note CRUD Operations

## Purpose

Create, read, update, and delete notes in a kg vault.

## Preconditions

- Current directory is inside a kg vault (or vault path specified)
- Vault is initialized

## Behavior

### Create Note

#### Scenario: Create note with title

**Given** the user is in a kg vault
**When** the user runs `kg new "My First Note"`
**Then** kg creates file `my-first-note-{uuid}.md` in current directory
**And** the file contains default frontmatter:
  ```yaml
  ---
  title: "My First Note"
  created: {timestamp}
  modified: {timestamp}
  tags: []
  aliases: []
  ---
  ```
**And** kg adds the note to the graph database
**And** kg creates symlink `.kg/by-id/{uuid}` → `my-first-note-{uuid}.md`
**And** outputs the path to the created file

#### Scenario: Create note in subdirectory

**Given** the user is in a kg vault
**When** the user runs `kg new "Project Plan" --dir projects`
**Then** kg creates `projects/` if it doesn't exist
**And** creates `projects/project-plan-{uuid}.md`
**And** symlink points correctly `../../projects/project-plan-{uuid}.md`

#### Scenario: Create note with tags

**Given** the user is in a kg vault
**When** the user runs `kg new "Rust Notes" --tags dev/rust,learning`
**Then** the frontmatter contains:
  ```yaml
  tags:
    - dev/rust
    - learning
  ```
**And** tags are indexed in the graph with hierarchy

#### Scenario: Create note and open editor

**Given** the user is in a kg vault
**And** $EDITOR is set to "vim"
**When** the user runs `kg new "Quick Note" --edit`
**Then** kg creates the note
**And** opens vim with the new file

#### Scenario: Duplicate title handling

**Given** a note "My Note" already exists
**When** the user runs `kg new "My Note"`
**Then** kg creates `my-note-{different-uuid}.md`
**And** both notes coexist (UUIDs differentiate them)

---

### Read Note

#### Scenario: Show note by path

**Given** a note exists at `notes/my-note-a1b2c3.md`
**When** the user runs `kg show notes/my-note-a1b2c3.md`
**Then** kg outputs the full note content (frontmatter + body)

#### Scenario: Show note by UUID

**Given** a note exists with UUID `a1b2c3`
**When** the user runs `kg show a1b2c3`
**Then** kg resolves via `.kg/by-id/a1b2c3` symlink
**And** outputs the note content

#### Scenario: Show note metadata only

**Given** a note exists
**When** the user runs `kg show a1b2c3 --meta`
**Then** kg outputs only the frontmatter as YAML

#### Scenario: Show note as JSON

**Given** a note exists
**When** the user runs `kg show a1b2c3 --json`
**Then** kg outputs:
  ```json
  {
    "id": "a1b2c3",
    "path": "notes/my-note-a1b2c3.md",
    "title": "My Note",
    "created": "2026-02-11T10:30:00Z",
    "modified": "2026-02-11T10:30:00Z",
    "tags": ["dev/rust"],
    "body": "Note content..."
  }
  ```

#### Scenario: Note not found

**Given** no note with UUID `xyz123` exists
**When** the user runs `kg show xyz123`
**Then** kg outputs error "Note not found: xyz123"
**And** exits with non-zero status

---

### Update Note

#### Scenario: Edit note in editor

**Given** a note exists at `a1b2c3`
**And** $EDITOR is set
**When** the user runs `kg edit a1b2c3`
**Then** kg opens the note in $EDITOR
**And** after editor closes, kg updates `modified` timestamp
**And** kg re-indexes the note in the graph

#### Scenario: Editor not set

**Given** $EDITOR is not set
**And** config has `editor.fallback = "prompt"`
**When** the user runs `kg edit a1b2c3`
**Then** kg prompts: "No $EDITOR set. Choose editor: [1] vim [2] nano [3] other"
**And** opens chosen editor

#### Scenario: Detect external changes

**Given** a note was modified outside kg (e.g., in Obsidian)
**When** the user runs any kg command
**Then** kg detects the mtime change
**And** re-parses and re-indexes the note
**And** continues with the command

#### Scenario: Schema validation on edit

**Given** a note exists
**And** schema requires `status` field with enum values
**When** the user saves the note with `status: invalid`
**Then** kg outputs warning "Schema validation failed: status must be one of [draft, review, published]"
**And** the file is saved (warning only, not blocked)

---

### Delete Note

#### Scenario: Delete note with no backlinks

**Given** a note `a1b2c3` exists with no incoming links
**When** the user runs `kg rm a1b2c3`
**Then** kg deletes the file
**And** removes the symlink from `.kg/by-id/`
**And** removes the note from the graph
**And** outputs "Deleted: my-note-a1b2c3.md"

#### Scenario: Delete note with backlinks

**Given** a note `a1b2c3` exists
**And** notes `x1` and `x2` link to it
**When** the user runs `kg rm a1b2c3`
**Then** kg outputs:
  ```
  Warning: 2 notes link to this note:
    - other-note-x1.md
    - another-note-x2.md
  Delete anyway? [y/N]
  ```
**And** waits for confirmation
**If** user confirms
**Then** kg deletes the note (leaving broken links)

#### Scenario: Force delete

**Given** a note has backlinks
**When** the user runs `kg rm a1b2c3 --force`
**Then** kg deletes without confirmation

#### Scenario: Delete non-existent note

**Given** no note `xyz123` exists
**When** the user runs `kg rm xyz123`
**Then** kg outputs error "Note not found: xyz123"
**And** exits with non-zero status

---

### Move/Rename Note

#### Scenario: Move note to different directory

**Given** a note exists at `notes/my-note-a1b2c3.md`
**When** the user runs `kg mv a1b2c3 archive/`
**Then** kg moves the file to `archive/my-note-a1b2c3.md`
**And** updates the symlink in `.kg/by-id/`
**And** updates the path in the graph
**And** updates all links pointing to this note

#### Scenario: Rename note

**Given** a note `my-note-a1b2c3.md` exists
**When** the user runs `kg mv a1b2c3 --title "New Title"`
**Then** kg renames to `new-title-a1b2c3.md`
**And** updates frontmatter `title: "New Title"`
**And** updates links that referenced by old title

## Constraints

- UUID must be unique and immutable
- `modified` timestamp must update on any content change
- All changes must be reflected in graph database
- Symlinks must stay in sync with files

## Error Handling

| Error | Behavior |
|-------|----------|
| Not in vault | "Not a kg vault. Run `kg init` first." |
| Write permission denied | "Permission denied: {path}" |
| Invalid frontmatter | "Failed to parse frontmatter: {details}" |
| Schema validation | Warning (not blocking by default) |
