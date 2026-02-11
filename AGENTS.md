# AI Coding Agent Instructions

This project follows **Spec-Driven Development**. Read this file before generating any code.

## Project: kg (Knowledge Graph CLI)

A CLI-first knowledge base management tool with:
- Markdown notes with typed frontmatter
- Embedded graph database with SPARQL support
- Obsidian vault compatibility
- Multiple vault support

## Core Principles

1. **Specs are truth**: Never implement behavior not covered by a spec
2. **Ask, don't assume**: If a spec is ambiguous, clarify before implementing
3. **Trace to specs**: Every function/module should reference its governing spec
4. **Test from specs**: Generate tests from Given/When/Then scenarios

## Project Structure

```
kg/
├── AGENTS.md                          # This file
├── README.md                          # Project overview
├── specs/
│   ├── PRODUCT.md                     # Product specification
│   ├── SPEC-GUIDE.md                  # How to write specs
│   ├── features/
│   │   ├── vault-init.md              # kg init command
│   │   ├── note-crud.md               # Create/read/update/delete notes
│   │   └── search-query.md            # Search and SPARQL queries
│   ├── contracts/
│   │   ├── note-format.md             # Note file format specification
│   │   └── plugin-api.md              # Plugin system (deferred to v2)
│   └── decisions/
│       ├── ADR-000-template.md        # ADR template
│       ├── ADR-001-language.md        # Language choice (pending)
│       └── ADR-002-graph-database.md  # Database choice (pending)
├── docs/                              # User documentation
└── src/                               # Implementation
```

## Key Design Decisions

| Decision | Status | Notes |
|----------|--------|-------|
| Language | **Accepted** | Rust |
| Database | Evaluating | CozoDB, Oxigraph, or CQLite (see ADR-002) |
| Plugin system | Deferred | v2 scope |

## Domain Terminology

| Term | Meaning |
|------|---------|
| Vault | Directory with notes, `.kg/` config, and graph DB |
| Note | Markdown file: `{slug}-{uuid}.md` with typed frontmatter |
| Link | Connection from one note to another (WikiLink or Markdown) |
| Backlink | Reverse lookup - notes that link TO a given note |
| Frontmatter | YAML metadata block at top of note |
| Schema | Type definitions in `.kg/schema.yaml` |
| UUID | 6-char alphanumeric identifier, immutable per note |

## Technical Constraints

- **Single binary**: No runtime dependencies
- **Linux only**: Initial target platform
- **Obsidian compatible**: Standard Markdown + YAML frontmatter
- **Fast startup**: Target sub-100ms
- **Graph queries**: Datalog, SPARQL, or Cypher (TBD per ADR-002)
- **No daemon**: Each command checks for file changes

## Spec Reading Order

1. `specs/PRODUCT.md` - Overall product vision
2. `specs/contracts/note-format.md` - Note file structure
3. `specs/features/*.md` - Individual feature behaviors
4. `specs/decisions/ADR-*.md` - Architectural decisions

## Workflow Rules

### Before Writing Code

1. Read the relevant spec(s) completely
2. Identify all Given/When/Then scenarios
3. If no spec exists, propose one first

### When Writing Code

1. Add spec reference in comments:
   ```rust
   // Implements: specs/features/vault-init.md#initialize-vault-in-current-directory
   ```
2. Implement scenarios as tests
3. Respect all stated constraints

### When Specs Are Incomplete

1. Document the gap
2. Propose spec additions
3. Wait for approval

## Testing Strategy

- **Unit tests**: One per Given/When/Then scenario
- **Integration tests**: Cross-feature workflows
- **Property tests**: Schema validation, link resolution

## Current Status

- [x] Product spec complete
- [x] Note format contract defined
- [x] Feature specs: vault-init, note-crud, search-query
- [x] ADR-001: Language decided (Rust)
- [ ] ADR-002: Database evaluating (CozoDB vs Oxigraph vs CQLite)
- [ ] Implementation: Not started
