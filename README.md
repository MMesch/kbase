# kbase - Knowledge Base CLI

A CLI-first knowledge base management tool with graph-powered relationships.

## What is kbase?

**kbase** stores your notes as Markdown files with typed frontmatter, backed by an embedded graph database. It's designed for developers who want:

- **CLI-first workflow**: Script, automate, and integrate with your tools
- **Graph relationships**: First-class support for links, backlinks, and queries
- **Obsidian compatibility**: Use kg for CLI power, Obsidian for visual editing
- **Multiple vaults**: Separate knowledge bases for different domains

## Development Approach

This project uses **Spec-Driven Development (SDD)**: specifications are the source of truth, implementation follows.

### Development Environment

Use Nix flake for reproducible development:

```bash
# Enter development shell
nix develop

# Or with direenv (recommended)
echo "use flake" > .envrc
direnv allow
```

The environment includes:
- Rust toolchain (rustc, cargo, rust-analyzer)
- TypeScript/Deno
- Benchmarking tools (hyperfine)
- Core utilities

## Project Structure

```
├── specs/                 # Source of truth
│   ├── PRODUCT.md         # Product specification
│   ├── features/          # Feature specifications
│   ├── contracts/         # Interface contracts
│   └── decisions/         # Architecture Decision Records
├── AGENTS.md              # AI assistant instructions
├── docs/                  # Documentation
└── src/                   # Implementation
```

## Current Phase

**Specification**: Defining requirements and making architectural decisions.

### Key Decisions Pending

- [ ] Language choice (Rust vs Go) - `ADR-001`
- [ ] Embedded graph database - `ADR-002`

## Quick Reference

See `specs/PRODUCT.md` for full specification.

### Planned Commands

```bash
# Vault management
kbase init                    # Initialize vault
kbase vault list              # List vaults

# Note operations
kbase new "Note title"        # Create note
kbase edit note-name          # Edit in $EDITOR
kbase list                    # List notes
kbase search "query"          # Full-text search

# Graph operations
kbase links note-name         # Show outgoing links
kbase backlinks note-name     # Show incoming links
kbase query "..."             # Graph query
kbase similar note-name       # Find similar notes
kbase embed note-name         # Generate embeddings
```

## License

TBD
