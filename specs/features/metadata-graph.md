# Feature: Metadata as Graph

## Purpose

All note metadata forms a unified graph where notes are subjects, field names are predicates, and field values are objects. This enables powerful queries across any metadata, not just explicit links.

## Core Concept

Every frontmatter field becomes a graph edge:

```
Note (subject) --[field name]--> Value (object)
```

**Example note**:
```yaml
---
title: "Learning Rust"
author: "Alice"
status: draft
tags:
  - dev/rust
  - learning
related:
  - project-abc
---
```

**Becomes graph triples**:
```
note:a1b2c3 --title--> "Learning Rust"
note:a1b2c3 --author--> "Alice"
note:a1b2c3 --status--> "draft"
note:a1b2c3 --hasTag--> tag:dev/rust
note:a1b2c3 --hasTag--> tag:learning
note:a1b2c3 --related--> note:project-abc
```

## Behavior

### Scenario: Query by any field

**Given** notes have various frontmatter fields
**When** the user queries for notes where `author = "Alice"`
**Then** kg traverses the graph: `?note --author--> "Alice"`
**And** returns matching notes

**Datalog**:
```datalog
?[note, title] :=
  *triples{subject: note, predicate: "author", object: "Alice"},
  *triples{subject: note, predicate: "title", object: title}
```

**SPARQL**:
```sparql
SELECT ?note ?title WHERE {
  ?note kg:author "Alice" .
  ?note kg:title ?title .
}
```

### Scenario: Query relationships between field values

**Given** notes have `author` and `status` fields
**When** the user queries "all authors with published notes"
**Then** kg can traverse: `?note --author--> ?author, ?note --status--> "published"`

### Scenario: Field values as nodes

**Given** a field value appears in multiple notes
**When** the user queries "all notes by author Alice"
**Then** `author:Alice` acts as a node connecting those notes

This enables queries like "find notes that share the same author as note X".

---

## Tag Hierarchies as Trees

Tags form tree structures within the graph:

```
tag:dev
├── tag:dev/rust
│   └── tag:dev/rust/async
└── tag:dev/go
```

**Graph representation**:
```
tag:dev/rust --parentTag--> tag:dev
tag:dev/rust/async --parentTag--> tag:dev/rust
tag:dev/go --parentTag--> tag:dev
```

### Scenario: Recursive tag queries (descendant search)

**This is a core query pattern.** Searching for `/tree1` returns notes touching:
- `tree1` (exact match)
- `tree1/child1` (child)
- `tree1/child1/child11` (grandchild)
- All descendants of `tree1`

**Given** tag hierarchy:
```
tree1
├── child1
│   ├── child11
│   └── child12
└── child2
```

**And** notes:
- Note A tagged `tree1`
- Note B tagged `tree1/child1`
- Note C tagged `tree1/child1/child11`
- Note D tagged `tree1/child2`
- Note E tagged `other`

**When** the user runs `kg list --tag tree1`
**Then** kg returns Notes A, B, C, D (all under `tree1`)
**And** excludes Note E

**Implementation** - find all descendants, then find notes with those tags:

**Datalog** (native recursion):
```datalog
# Define descendant relationship (tag is descendant of ancestor)
descendant[tag, ancestor] := *tags{tag, parent: ancestor}
descendant[tag, ancestor] := *tags{tag, parent: mid}, descendant[mid, ancestor]

# Find notes with tree1 OR any descendant of tree1
?[note, title] :=
  *note_tags{note, tag},
  (tag = "tree1" or descendant[tag, "tree1"]),
  *notes{id: note, title}
```

**SPARQL** (property paths):
```sparql
PREFIX kg: <http://kg.local/>

SELECT ?note ?title WHERE {
  ?note kg:hasTag ?tag .
  ?tag kg:parentTag* kg:tag/tree1 .  # zero or more parentTag hops
  ?note kg:title ?title .
}
```

**Cypher** (variable-length relationships):
```cypher
MATCH (n:Note)-[:HAS_TAG]->(t:Tag)-[:PARENT_TAG*0..]->(ancestor:Tag {name: "tree1"})
RETURN DISTINCT n.id, n.title
```

### Scenario: Tag ancestry (what tags does a note have?)

**Given** a note is tagged `dev/rust/async`
**When** the user asks "what tags does this note have?"
**Then** kg returns:
- `dev/rust/async` (explicit)
- `dev/rust` (ancestor, implied)
- `dev` (ancestor, implied)

---

## Virtual Folder Views

Tag trees can be **realized as virtual folder structures** for browsing.

### Scenario: View notes by tag hierarchy

**Given** notes exist with various tags
**When** the user runs `kg view tags`
**Then** kg displays:
```
dev/
├── rust/
│   ├── learning-rust-a1b2c3.md
│   ├── async/
│   │   └── tokio-guide-d4e5f6.md
│   └── ...
└── go/
    └── go-basics-x7y8z9.md
reading/
└── books/
    └── rust-book-notes-m3n4o5.md
```

### Scenario: Virtual folder symlinks

**Given** the user enables virtual folders
**When** kg syncs the vault
**Then** kg creates `.kg/views/tags/` with symlinks:
```
.kg/views/tags/
├── dev/
│   ├── rust/
│   │   ├── learning-rust-a1b2c3.md -> ../../../../notes/learning-rust-a1b2c3.md
│   │   └── async/
│   │       └── tokio-guide-d4e5f6.md -> ../../../../../notes/tokio-guide-d4e5f6.md
```

This allows file managers and editors to browse by tag.

### Scenario: Notes in multiple tag folders

**Given** a note has tags `dev/rust` and `learning`
**Then** the note appears in both virtual paths:
- `.kg/views/tags/dev/rust/note.md`
- `.kg/views/tags/learning/note.md`

### Scenario: View by any field

**Given** notes have a `status` field
**When** the user runs `kg view status`
**Then** kg displays:
```
draft/
├── note-1.md
├── note-2.md
review/
└── note-3.md
published/
└── note-4.md
```

### Scenario: Create custom view

**Given** the user wants a view by author
**When** the user runs `kg view create --name by-author --field author`
**Then** kg creates a view definition
**And** `kg view by-author` shows notes grouped by author

---

## Data Model

### Triples Table

All metadata stored as subject-predicate-object triples:

| subject | predicate | object | object_type |
|---------|-----------|--------|-------------|
| note:a1b2c3 | title | "Learning Rust" | literal |
| note:a1b2c3 | author | "Alice" | literal |
| note:a1b2c3 | hasTag | tag:dev/rust | node |
| note:a1b2c3 | linksTo | note:d4e5f6 | node |
| tag:dev/rust | parentTag | tag:dev | node |

### Node Types

- `note:{uuid}` - A note
- `tag:{path}` - A tag (e.g., `tag:dev/rust`)
- Literals - String, number, date, boolean values

### Special Predicates

| Predicate | Meaning |
|-----------|---------|
| `title` | Note title |
| `created` | Creation timestamp |
| `modified` | Last modified timestamp |
| `hasTag` | Note has this tag |
| `parentTag` | Tag's parent in hierarchy |
| `linksTo` | Explicit link to another note |

User-defined fields become custom predicates.

---

## Query Syntax

Tags are the primary organizer (positional argument). Field filters use `--filter` flag.

### Basic Syntax

```bash
kg list [tag] [--filter "conditions"] [--exact]
```

### Tag Queries

```bash
kg list dev              # notes with dev or any descendant tag
kg list dev/rust         # notes with dev/rust or descendants
kg list dev --exact      # only notes tagged exactly "dev"
kg list                  # all notes (no tag filter)
```

### Field Filters

Use `--filter` (or `-f`) with conditions:

```bash
kg list dev --filter "status==draft"
kg list dev -f "status==draft author==Alice"
kg list -f "created>=2026-01-01"           # filter only, no tag
```

**Operators**:

| Operator | Meaning | Example |
|----------|---------|---------|
| `==` | Equals | `status==draft` |
| `!=` | Not equals | `author!=Bob` |
| `>=` | Greater/equal | `created>=2026-01-01` |
| `<=` | Less/equal | `modified<=2026-06-01` |
| `>` | Greater than | `priority>3` |
| `<` | Less than | `priority<5` |
| `~=` | Contains | `title~=rust` |

Multiple conditions are space-separated (AND logic):

```bash
kg list dev -f "status==published author==Alice created>=2026-01-01"
```

### Tag Helpers

```bash
kg tags ancestors dev/rust/async   # show: dev/rust/async → dev/rust → dev
kg tags descendants dev            # show all tags under dev
```

---

## Constraints

- Every note MUST be a subject in the graph
- Tags MUST form valid trees (no cycles)
- Field names (predicates) MUST be valid identifiers
- Deleting a note removes all its triples
- Renaming a tag updates all references

## Open Questions

- [ ] Should non-tag fields also support hierarchies? (e.g., `author/team/alice`)
- [ ] Index strategy for literal values vs node references
- [ ] How to handle field value types in the graph (dates, numbers)?
