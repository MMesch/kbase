# Feature: Search and Query

## Purpose

Find notes using full-text search, frontmatter filters, and SPARQL graph queries.

## Preconditions

- In a kg vault
- Graph database is synced with files

## Behavior

### Full-Text Search

#### Scenario: Basic search

**Given** notes exist with various content
**When** the user runs `kg search "rust programming"`
**Then** kg searches note titles and body content
**And** outputs matching notes ranked by relevance:
  ```
  notes/rust-guide-a1b2c3.md (score: 0.95)
  notes/learning-log-d4e5f6.md (score: 0.72)
  ```

#### Scenario: Search with no results

**Given** no notes contain "xyznonexistent"
**When** the user runs `kg search "xyznonexistent"`
**Then** kg outputs "No notes found matching: xyznonexistent"

#### Scenario: Search output formats

**Given** matching notes exist
**When** the user runs `kg search "rust" --json`
**Then** kg outputs:
  ```json
  {
    "query": "rust",
    "results": [
      {"id": "a1b2c3", "path": "...", "title": "...", "score": 0.95, "snippet": "...rust..."}
    ]
  }
  ```

---

### List with Filters

#### Scenario: List all notes

**Given** 5 notes exist in the vault
**When** the user runs `kg list`
**Then** kg outputs all notes:
  ```
  a1b2c3  notes/first-note-a1b2c3.md      "First Note"
  d4e5f6  notes/second-note-d4e5f6.md     "Second Note"
  ...
  ```

#### Scenario: List with tag filter

**Given** notes exist with various tags
**When** the user runs `kg list --tag dev/rust`
**Then** kg outputs only notes tagged `dev/rust` or children (e.g., `dev/rust/async`)

#### Scenario: List with frontmatter filter

**Given** notes have `status` field
**When** the user runs `kg list --where "status=draft"`
**Then** kg outputs only notes where `status` equals `draft`

#### Scenario: List recently modified

**Given** notes exist with various modification times
**When** the user runs `kg list --recent 7d`
**Then** kg outputs notes modified in the last 7 days
**And** sorts by modification time (newest first)

#### Scenario: List with multiple filters

**Given** notes exist
**When** the user runs `kg list --tag dev --where "status=published" --recent 30d`
**Then** kg applies all filters (AND logic)

---

### Graph Queries

The query language depends on the chosen database (see ADR-002). Examples below show all three candidates.

#### Scenario: Basic graph query

**Given** notes exist in the graph
**When** the user runs `kg query '<query>'`
**Then** kg executes the query and outputs results

**Datalog (CozoDB)**:
```
kg query '?[title] := *notes{title}'
```

**SPARQL (Oxigraph)**:
```
kg query 'SELECT ?title WHERE { ?note <kg:title> ?title }'
```

**Cypher (CQLite)**:
```
kg query 'MATCH (n:Note) RETURN n.title'
```

#### Scenario: Find backlinks via query

**Given** notes link to each other
**When** the user queries for notes linking to `a1b2c3`
**Then** kg outputs source notes

**Datalog**:
```datalog
?[source, title] :=
  *links{from: source, to: "a1b2c3"},
  *notes{id: source, title}
```

**SPARQL**:
```sparql
PREFIX kg: <http://kg.local/>
SELECT ?source ?title WHERE {
  ?source kg:linksTo kg:note/a1b2c3 .
  ?source kg:title ?title .
}
```

**Cypher**:
```cypher
MATCH (source)-[:LINKS_TO]->(target {id: "a1b2c3"})
RETURN source.id, source.title
```

#### Scenario: Recursive tag hierarchy query

**Given** notes have hierarchical tags (`dev/rust` implies `dev`)
**When** the user queries for all notes under tag `dev`
**Then** kg returns notes tagged `dev`, `dev/rust`, `dev/go`, etc.

**Datalog** (native recursion):
```datalog
ancestor[tag, anc] := *tags{tag, parent: anc}
ancestor[tag, anc] := ancestor[tag, mid], *tags{mid, parent: anc}

?[note, title] :=
  *note_tags{note, tag},
  ancestor[tag, "dev"],
  *notes{id: note, title}
```

**SPARQL** (property paths):
```sparql
SELECT ?note ?title WHERE {
  ?note kg:hasTag/kg:parentTag* kg:tag/dev .
  ?note kg:title ?title .
}
```

**Cypher** (variable-length paths):
```cypher
MATCH (n:Note)-[:HAS_TAG]->(t:Tag)-[:PARENT*0..]->(p:Tag {name: "dev"})
RETURN n.id, n.title
```

#### Scenario: Query with JSON output

**Given** a valid query
**When** the user runs `kg query '<query>' --json`
**Then** kg outputs results as JSON array

#### Scenario: Invalid query

**Given** an invalid query
**When** the user runs `kg query '<invalid>'`
**Then** kg outputs "Query parse error: {details}"
**And** exits with non-zero status

#### Scenario: Query from file

**Given** a file `query.txt` exists
**When** the user runs `kg query -f query.txt`
**Then** kg reads and executes the query from the file

---

### Shortcuts for Common Queries

#### Scenario: Links command

**Given** note `a1b2c3` links to `d4e5f6` and `g7h8i9`
**When** the user runs `kg links a1b2c3`
**Then** kg outputs:
  ```
  Outgoing links from "My Note" (a1b2c3):
    → d4e5f6  "Other Note"
    → g7h8i9  "Another Note"
  ```

#### Scenario: Backlinks command

**Given** notes `x1` and `x2` link to `a1b2c3`
**When** the user runs `kg backlinks a1b2c3`
**Then** kg outputs:
  ```
  Incoming links to "My Note" (a1b2c3):
    ← x1  "First Linker"
    ← x2  "Second Linker"
  ```

#### Scenario: Tags tree

**Given** tags exist: `dev`, `dev/rust`, `dev/go`, `reading`
**When** the user runs `kg tags tree`
**Then** kg outputs:
  ```
  dev
  ├── rust (3 notes)
  └── go (1 note)
  reading (5 notes)
  ```

## Constraints

- Full-text search should complete in <1s for 10k notes
- SPARQL queries timeout after 30s (configurable)
- All queries check for file changes first (sync if needed)

## Output Formats

All commands support:
- `--json`: JSON output
- `--csv`: CSV output (where applicable)
- `--template <tpl>`: Custom Go/Rust template

## Error Handling

| Error | Behavior |
|-------|----------|
| Invalid filter syntax | "Invalid filter: {details}" |
| SPARQL timeout | "Query timed out after 30s" |
| Unknown field in --where | "Unknown field: {name}. Available: {list}" |
