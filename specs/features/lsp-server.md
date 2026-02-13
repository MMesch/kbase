---
title: LSP Server
tags:
  - spec/feature
status: complete
---
# Feature: LSP Server

## Purpose

Provide a Language Server Protocol (LSP) implementation for kbase vaults. Enables editor integration for autocompletion, validation diagnostics, hover information, and navigation across notes.

## LSP Capabilities

### 1. Autocompletion (`textDocument/completion`)

Fast completion for links, tags, and fields while editing notes.

#### Link Completion

**Trigger**: `[[` followed by any characters

**Completes**:
- Note titles
- Note UUIDs
- Note aliases (if defined in frontmatter)

**Example**:
```
[[learn     →  [[Learning Rust]]
                [[Learn Go Basics]]
```

**Ranking**:
1. Exact prefix match on title
2. Recent notes (recently modified/viewed)
3. Frequently linked notes
4. Fuzzy match on title

#### Tag Completion

**Trigger**: In frontmatter `tags:` field or after `- ` under tags

**Completes**:
- Existing tags in the vault
- Parent tags (hierarchy-aware)

**Example**:
```yaml
tags:
  - dev/ru    →  dev/rust
                  dev/rust/async
```

**Ranking**:
1. Exact prefix match
2. Frequency (most used tags first)
3. Hierarchical siblings

#### Field Completion

**Trigger**: In frontmatter for field names and values

**Field names**: Schema-defined fields, fields used in other notes
**Field values**: Previous values, schema-defined enums

**Example**:
```yaml
status: dra    →  draft
                   archived
```

---

### 2. Diagnostics (`textDocument/publishDiagnostics`)

Real-time validation of note metadata against SHACL shapes.

#### Schema Violations

**Severity**: Error

```yaml
---
priority: "high"    # Error: expected integer, got string
created: 2026-13-45 # Error: invalid date format
---
```

**Diagnostic**:
```json
{
  "range": {"start": {"line": 2, "character": 10}, "end": {"line": 2, "character": 16}},
  "severity": 1,
  "message": "Expected xsd:integer, got string \"high\"",
  "source": "kbase"
}
```

#### Missing Required Fields

**Severity**: Error

```yaml
---
author: "Alice"
# missing required 'title' field
---
```

#### Invalid Links

**Severity**: Warning

```markdown
See [[nonexistent-note]] for details.
```

**Diagnostic**: "Link target 'nonexistent-note' not found in vault"

#### Unknown Tags

**Severity**: Hint

```yaml
tags:
  - dev/rust
  - typo/tga    # Hint: tag 'typo/tga' doesn't exist, did you mean 'dev/tag'?
```

#### Type Constraints

From SHACL shapes:
- `sh:datatype` - Type checking (string, integer, date, etc.)
- `sh:minCount` / `sh:maxCount` - Cardinality
- `sh:minInclusive` / `sh:maxInclusive` - Value ranges
- `sh:pattern` - Regex validation
- `sh:in` - Enum values

---

### 3. Hover (`textDocument/hover`)

Show information when hovering over elements.

#### Link Hover

Hovering over `[[Learning Rust]]` shows:
```
**Learning Rust**
UUID: a1b2c3
Tags: dev/rust, learning
Created: 2026-01-15
Modified: 2026-02-10

First 200 characters of note content...
```

#### Tag Hover

Hovering over a tag shows:
```
**dev/rust**
42 notes
Children: dev/rust/async, dev/rust/macros
Parent: dev
```

#### Field Hover

Hovering over a schema-defined field shows:
```
**priority** (from schema)
Type: integer
Range: 1-5
Description: Task priority level
```

---

### 4. Go to Definition (`textDocument/definition`)

Navigate to linked notes or tag definitions.

- `[[Note Title]]` → Opens the linked note
- `#tag/path` → Opens tag index or first note with tag

---

### 5. Find References (`textDocument/references`)

Find all notes that reference a given note or tag.

- On a note: Find all backlinks
- On a tag: Find all notes with that tag

---

### 6. Document Symbols (`textDocument/documentSymbol`)

Outline of note structure:
- Frontmatter fields
- Headings (h1-h6)
- Links
- Tags

---

## Performance Requirements

| Operation | Target | Max |
|-----------|--------|-----|
| Completion response | 10ms | 50ms |
| Diagnostics (single file) | 20ms | 100ms |
| Hover | 5ms | 20ms |
| Go to definition | 5ms | 20ms |
| Server startup | 100ms | 500ms |

---

## Integration

### Server Lifecycle

```bash
# Start LSP server (stdio transport)
kg lsp

# Start with TCP transport (for debugging)
kg lsp --tcp --port 9257
```

### Neovim Configuration

```lua
-- nvim-lspconfig
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

configs.kbase = {
  default_config = {
    cmd = {'kg', 'lsp'},
    filetypes = {'markdown'},
    root_dir = lspconfig.util.root_pattern('.kbase'),
    settings = {},
  },
}

lspconfig.kbase.setup({
  on_attach = function(client, bufnr)
    -- Enable completion
    vim.bo[bufnr].omnifunc = 'v:lua.vim.lsp.omnifunc'
  end,
})
```

### VS Code Extension

Future: Package as VS Code extension with bundled LSP server.

---

## Behavior

### Scenario: Validate on open

**Given** a note with invalid frontmatter
**When** the user opens the note in their editor
**Then** kg publishes diagnostics for all validation errors
**And** the editor displays inline error markers

### Scenario: Validate on change

**Given** an open note
**When** the user modifies frontmatter
**Then** kg re-validates within 100ms
**And** publishes updated diagnostics

### Scenario: Complete link with ranking

**Given** notes "Learning Rust" (viewed today), "Rust Basics" (not viewed recently)
**When** the user types `[[rust`
**Then** "Learning Rust" appears first (recency boost)

### Scenario: Diagnose broken link

**Given** a note containing `[[deleted-note]]`
**When** the note is validated
**Then** kg publishes a warning diagnostic
**And** the message suggests similar note titles

### Scenario: Schema-aware field completion

**Given** a SHACL shape defining `status` with values `[draft, review, published]`
**When** the user types `status: ` in frontmatter
**Then** kg offers `draft`, `review`, `published` as completions

---

## Index Structure

Maintain in-memory indexes for fast operations:

```
notes_index:
  by_title: prefix_tree(title) -> [note_id]
  by_uuid: map(uuid) -> note
  by_alias: prefix_tree(alias) -> note_id
  recent: LRU([note_id], max=100)

tags_index:
  by_path: prefix_tree(tag_path) -> {count, last_used}
  hierarchy: map(parent -> [children])
  by_note: map(note_id -> [tags])

links_index:
  outgoing: map(note_id -> [target_ids])
  incoming: map(note_id -> [source_ids])  # backlinks

fields_index:
  names: prefix_tree(name) -> {count, in_schema}
  values: map(field -> prefix_tree(value) -> count)

schema:
  shapes: parsed SHACL shapes for validation
```

Indexes update incrementally on `textDocument/didSave`.

---

## Constraints

- MUST implement LSP 3.17 specification
- MUST use stdio transport by default (TCP optional)
- MUST handle concurrent requests from multiple files
- MUST gracefully handle malformed frontmatter (partial parse)
- MUST NOT block on slow validation (timeout and return partial)
- Diagnostics MUST clear when errors are fixed

## Out of Scope

- Markdown formatting/linting (use markdownlint)
- Spell checking (use ltex-ls or similar)
- Code block language server delegation
- Semantic/AI-powered features (future)
