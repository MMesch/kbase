---
title: Product Specification
tags:
  - spec/product
status: complete
---
# Product Specification: kbase

## Vision

**kbase** (Knowledge Base) is a CLI-first knowledge base management tool that stores notes as Markdown files with typed frontmatter, backed by an embedded graph database.

The CLI is the core engine; future clients (Vim plugin, TUI, Obsidian integration) can be built on top of it.

## Target Users

Developers and technical users comfortable with:
- Command-line interfaces
- Configuration files
- Text-based workflows

## Core Concepts

### Vault

A vault is a directory containing:
- Markdown note files (flat structure recommended, subfolders supported)
- `.kbase/` directory with configuration, schema, database, and virtual views (optional)
- Virtual folder views generated dynamically by tag, field, etc.

Users can have **multiple vaults** for different knowledge domains.

**Vault discovery**: The CLI automatically detects vaults by looking for a `.kbase/` directory in the current directory or parent directories. Users can also specify a vault explicitly using the `--vault` argument.

**Vault structure:**
```
my-vault/
├── .kbase/
│   ├── config.toml          # Vault configuration
│   ├── schema.yaml          # Frontmatter schema definition
│   ├── graph.db             # Embedded graph database
│   ├── embeddings/          # Cached embeddings for similarity search
│   └── views/               # Virtual folder views (symlinks)
│       └── tags/
│           └── dev/
│               └── rust/
│                   └── note-a1b2c3.md -> ../../../note-a1b2c3.md
├── my-note-a1b2c3.md         # Notes live flat in vault root
├── another-note-d4e5f6.md
└── project-plan-x7y8z9.md
```

**Note**: The `.kbase/` directory is optional. Users can work with a standard Obsidian vault without the `.kbase/` directory, though advanced features like graph queries and virtual views will be unavailable.

**Note**: Physical files are flat (UUID ensures uniqueness). Organization is via **virtual folder views** that create symlink hierarchies based on tags or other fields. A note can appear in multiple virtual folders. Subfolders are also supported for Obsidian compatibility and structured organization.

### Note

A note is a Markdown file with:
- **Filename**: `{slugified-title}-{short-uuid}.md` (e.g., `my-note-a1b2c3.md`)
- **Typed frontmatter**: YAML header validated against vault schema
- **Body**: Standard Markdown content
- **Links**: Both `[[WikiLinks]]` and `[standard](markdown/links.md)` supported

### Graph

**All metadata is a graph.** Every frontmatter field becomes an edge:

```
Note (subject) --[field name]--> Value (object)
```

Values can be:
- **Literals**: strings, numbers, dates (`title`, `created`)
- **Nodes**: entities that connect multiple notes (`author:alice`, `tag:dev/rust`)

This enables:
- Query by any field, not just links
- Relationships between notes via shared nodes (e.g., notes with same author)
- Unified model for links, tags, and custom fields
- Graph validation (SHACL-like constraints)
- Inference (OWL-like reasoning)

This model maps to all three candidate databases:
- **Datalog**: Relations/tuples
- **SPARQL**: RDF triples
- **Cypher**: Property graph nodes/edges

See `specs/features/metadata-graph.md` for details.

Query language options under evaluation (see ADR-002):
- **Datalog** (CozoDB) - recursive queries, built-in algorithms
- **SPARQL** (Oxigraph) - semantic web standard
- **Cypher** (CQLite) - Neo4j-style pattern matching

**Database synchronization**: The Markdown files are the source of truth. The graph database is kept in sync with the structured elements of the vault for fast queries. The database is regenerated on demand and can be cached for performance.

### Tags

Hierarchical tags using slash notation:
- `tags: [dev/rust, dev/go, reading/books]`
- Parent tags are implied: `dev/rust` implies membership in `dev`
- Stored in frontmatter, indexed in graph

**Tags are nodes in the graph**: Tags are treated as first-class nodes in the knowledge graph, not just metadata. This enables:
- Tag hierarchies as tree structures through the graph
- Easier traversal and search
- Unified representation with other entities

**Tags are trees in the graph** with `parentTag` edges:
```
tag:dev/rust --parentTag--> tag:dev
```

**Descendant search by default**: `kbase list dev` returns notes tagged `dev`, `dev/rust`, `dev/go`, etc. Use `--exact` for exact match only.

**Filter syntax**: `kbase list dev -f "status==draft author==Alice"` combines tag with field filters.

These trees can be **realized as virtual folder views** for browsing notes by tag hierarchy.

## Compatibility

**Obsidian vault format**: kg vaults should be readable/writable by Obsidian:
- Standard Markdown files with YAML frontmatter
- Flat file structure (Obsidian can add its own folder organization)
- WikiLinks and standard links both supported
- Obsidian-compatible default frontmatter fields
- `.kbase/` directory is ignored by Obsidian (hidden folder)

## Functional Requirements

### Core Operations

| Command | Description |
|---------|-------------|
| `kbase init [path]` | Initialize a new vault |
| `kbase new <title>` | Create a new note with default frontmatter |
| `kbase edit <ref>` | Open note in editor (prompt if $EDITOR unset) |
| `kbase show <ref>` | Display note content |
| `kbase list [tag] [-f "filters"] [-s "text"]` | List notes by tag, filters, and/or full-text search |
| `kbase rm <ref>` | Delete note (warn if has backlinks, confirm) |

**Note references (`<ref>`)**: Accept file path or short UUID with shell completion.

**List examples**:
```bash
kbase list dev                         # by tag (+ descendants)
kbase list dev -f "status==draft"      # tag + field filter
kbase list -s "rust async"             # full-text search
kbase list dev -f "author==alice" -s "tutorial"  # combined
```

All modifying operations (`kbase new`, `kbase edit`, `kbase rm`) automatically update virtual folder views.

### Graph Operations

| Command | Description |
|---------|-------------|
| `kbase links <ref>` | Show outgoing links from note |
| `kbase backlinks <ref>` | Show incoming links to note |
| `kbase query <query>` | Execute raw graph query (advanced, syntax per ADR-002) |
| `kbase export [--format dot\|graphml]` | Export graph structure |
| `kbase similar <ref> [--top N]` | Find similar notes using embeddings |
| `kbase embed <ref>` | Generate/update embeddings for a note |
| `kbase validate` | Run graph validation (SHACL-like constraints) |
| `kbase infer` | Run inference (OWL-like reasoning) |

**Note**: Links are created by editing note content (WikiLinks or Markdown links). The graph is derived from note content, not managed separately.

**Embeddings**: Vector representations of notes are stored in `.kbase/embeddings/` and cached based on content hash for efficient similarity search.

### Organization Operations

| Command | Description |
|---------|-------------|
| `kbase view tags` | Show notes as virtual folder tree by tag hierarchy |
| `kbase view <field>` | Show notes grouped by any field (status, author, etc.) |
| `kbase view create` | Create a custom view definition |
| `kbase view sync` | Regenerate virtual folder symlinks |
| `kbase reorg <rule>` | Physically reorganize files by rule |
| `kbase tags` | List all tags with counts |
| `kbase tags tree` | Show tag hierarchy as tree |

Virtual folder views create symlink trees in `.kbase/views/` for file manager browsing.

### Vault Management

| Command | Description |
|---------|-------------|
| `kbase vault status` | Show current vault info and stats |
| `kbase sync` | Force re-sync graph from files |
| `kbase validate` | Validate all notes against schema |

### Schema Operations

| Command | Description |
|---------|-------------|
| `kbase schema show` | Display frontmatter schema |
| `kbase schema validate` | Validate all notes against schema |
| `kbase schema migrate` | Update notes when schema changes |

### Output Formatting

All list/query commands support multiple output formats:
- Default: Human-readable, pretty-printed
- `--json`: JSON output for scripting
- `--csv`: CSV output for spreadsheets
- `--template <tpl>`: Custom template formatting

### Shell Completion

- `kg completion <shell>` generates completion script
- Completions are path-aware (check filesystem)
- ID-based completion via `.kbase/by-id/` virtual folder

## Non-Functional Requirements

### Platform
- **Linux only** (initial release)

### Dependencies
- **Minimal**: Single static binary preferred
- No runtime dependencies (no Node, Python, etc.)
- Embedded database (no external services)

### Performance
- Fast startup (sub-100ms target)
- Efficient for vaults with 10,000+ notes
- File change detection on every command (no daemon)

### Verbosity
- Default: Quiet (only requested output)
- `-v` / `--verbose`: Informative messages
- Configurable default in `.kbase/config.toml`

## Configuration

All configuration lives in `.kbase/config.toml` within each vault.

```toml
[vault]
name = "My Knowledge Base"

[notes]
# Behavior when linking to non-existent notes
broken_links = "warn"  # "allow" | "warn" | "block"

[output]
default_format = "human"  # "human" | "json" | "csv"
verbose = false

[editor]
# Fallback if $EDITOR unset (or prompt user)
fallback = "prompt"  # "prompt" | "vim" | "nano" | etc.
```

## Schema Definition

Schema lives in `.kbase/schema.yaml`:

```yaml
# Required fields for all notes
required:
  - title
  - created
  - modified

# Field definitions
fields:
  title:
    type: string
    description: Note title

  created:
    type: datetime
    auto: true  # Auto-set on creation

  modified:
    type: datetime
    auto: true  # Auto-update on save

  tags:
    type: list
    items: string
    description: Hierarchical tags (slash notation)

  aliases:
    type: list
    items: string
    description: Alternative titles for linking (Obsidian compatible)

  # User-defined fields...
  status:
    type: enum
    values: [draft, review, published]
    default: draft
```

## Technical Decisions

### Language/Stack

**Status**: Pending - See `decisions/ADR-001-language.md`

### Graph Database

**Status**: Evaluating - See `decisions/ADR-002-graph-database.md`

Candidates (all Rust, embedded):
- **CozoDB** - Datalog queries, built-in algorithms, multiple backends
- **Oxigraph** - SPARQL 1.1, RDF/semantic web standard
- **CQLite** - Cypher subset (pre-release, monitoring)

### File Watching

Each command checks for file changes before executing (no background daemon).
Approach: Compare file mtimes against last-known state in graph DB.

## Open Questions

- [ ] SPARQL endpoint exposure for external tools?
- [ ] Note aliases - how do they interact with WikiLinks?
- [ ] View persistence format and query syntax
- [ ] Physical reorg rules syntax

## Out of Scope (v1)

- Windows/macOS support
- GUI application
- Cloud sync
- Real-time collaboration
- Mobile apps
- Full template system (just default frontmatter)
- Background daemon/file watching service

## Future Roadmap

1. **v1**: Core CLI with SPARQL graph operations
2. **v2**: Plugin system architecture
3. **v3**: TUI mode
4. **v4**: Vim plugin
5. **v5**: Cross-platform support
