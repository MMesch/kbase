# Feature: Vault Initialization

## Purpose

Initialize a new kg vault in a directory, creating the necessary structure and configuration files.

## Preconditions

- Target directory exists (or will be created)
- User has write permissions

## Behavior

### Scenario: Initialize vault in current directory

**Given** the current directory is empty or contains only non-kg files
**When** the user runs `kg init`
**Then** kg creates:
  - `.kg/` directory
  - `.kg/config.toml` with default configuration
  - `.kg/schema.yaml` with default schema
  - `.kg/graph.db` (empty database)
  - `.kg/by-id/` directory (empty)
**And** outputs "Initialized kg vault in /path/to/dir"

### Scenario: Initialize vault in specified path

**Given** the path `/home/user/notes` does not exist
**When** the user runs `kg init /home/user/notes`
**Then** kg creates the directory `/home/user/notes`
**And** creates the vault structure inside it
**And** outputs "Initialized kg vault in /home/user/notes"

### Scenario: Initialize in existing vault

**Given** the current directory contains `.kg/`
**When** the user runs `kg init`
**Then** kg outputs error "Already a kg vault (found .kg/)"
**And** exits with non-zero status
**And** makes no changes

### Scenario: Initialize in directory with existing notes

**Given** the current directory contains markdown files
**When** the user runs `kg init`
**Then** kg creates the vault structure
**And** outputs "Initialized kg vault in /path/to/dir"
**And** outputs "Found N existing markdown files. Run `kg sync` to import them."

### Scenario: Initialize with custom name

**Given** the current directory is empty
**When** the user runs `kg init --name "My Knowledge Base"`
**Then** kg creates the vault structure
**And** `.kg/config.toml` contains `name = "My Knowledge Base"`

## Default Configuration

`.kg/config.toml`:
```toml
[vault]
name = "Untitled Vault"

[notes]
broken_links = "warn"

[output]
default_format = "human"
verbose = false

[editor]
fallback = "prompt"
```

## Default Schema

`.kg/schema.yaml`:
```yaml
required:
  - title
  - created
  - modified

fields:
  title:
    type: string
    description: Note title

  created:
    type: datetime
    auto: true

  modified:
    type: datetime
    auto: true

  tags:
    type: list
    items: string
    default: []

  aliases:
    type: list
    items: string
    default: []
```

## Constraints

- Must not overwrite existing `.kg/` directory
- Must validate write permissions before creating files
- Must create valid TOML and YAML files

## Out of Scope

- Importing existing notes (that's `kg sync`)
- Interactive setup wizard
- Cloning from template vaults
