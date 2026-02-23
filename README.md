# kbase - Knowledge Base CLI

A CLI-first knowledge base management tool with graph-powered relationships.

## What is kbase?

**kbase** stores your notes as Markdown files with typed frontmatter, backed by an embedded graph database. It's designed for developers who want:

- **CLI-first workflow**: Script, automate, and integrate with your tools
- **Graph relationships**: First-class support for links, backlinks, and queries
- **Obsidian compatibility**: Use kbase for CLI power, Obsidian for visual editing
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
- Rust toolchain (rustc, cargo, clippy, rustfmt, rust-analyzer)
- TypeScript/Deno
- Core utilities

### Development Commands

```bash
cargo build              # Build the project
cargo test               # Run all tests (unit + integration)
cargo clippy             # Run linter
cargo fmt                # Format code
cargo fmt --check        # Check formatting without changes
```

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

## Commands

```bash
# Vault management
kbase init [path]             # Initialize vault
kbase new "Note title"        # Create note (uses schema template)

# List and query
kbase list                    # List all notes
kbase list --tag recipe       # Filter by tag
kbase tags                    # Show tag tree
kbase tags --notes            # Show tag tree with notes
kbase backlinks note-name     # Show incoming links
kbase overview                # Vault summary: tags, key notes, link structure

# Graph queries (SPARQL)
kbase query "SELECT ..."      # Run SPARQL query on knowledge graph
kbase query schema            # Show SPARQL schema documentation
kbase query --file q.sparql   # Run query from file

# Link conversion
kbase convert markdown        # Convert [[wiki]] to [title](slug.md)
kbase convert wiki            # Convert [text](slug.md) to [[Title]]
kbase convert normalize       # Rewrite markdown link paths using configured link_base
kbase convert markdown --dry-run  # Preview changes

# Tag management
kbase retag old/prefix new    # Rename tags: old/prefix/x -> new/x
kbase retag old/prefix new --dry-run  # Preview tag changes
kbase organize --tree domain  # Move notes into directories matching tag hierarchy
kbase organize --dry-run      # Preview file moves
kbase clean-tags              # Remove orphan tags from the graph

# Validation
kbase validate                # Validate against schema and detect broken links

# Semantic search (requires ONNX model)
kbase search "query"          # Semantic search across notes
kbase similar note-name       # Find similar notes

# Editor integration
kbase lsp                     # Start LSP server
kbase install-skills          # Install AI assistant skills

# Export
kbase export --format dot     # Export graph to DOT (Graphviz)
kbase export --format graphml # Export graph to GraphML
```

## Configuration

Settings live in `.kbase/config.yaml`:

```yaml
# Link syntax to recognize (wiki, markdown, or both; default: both)
link_syntax: both

# How link paths are computed for completion/normalize (vault or relative; default: vault)
# vault: paths from vault root with leading / (e.g. /knowledge/note.md)
# relative: paths relative to current file (e.g. ../other/note.md)
link_base: vault

# Restrict note scanning to a subdirectory (default: entire vault)
notes_dir: knowledge

# Store backend (nquads, rocksdb, or fresh; default: nquads)
store: nquads

# Embeddings configuration
embeddings:
  backend: onnx
  chunk_level: "##"
  include_context: true
```

## Semantic Search Setup

kbase uses local ONNX models for embeddings - no external API needed.

### Download the model

```bash
mkdir -p ~/.cache/kbase/models
curl -L -o ~/.cache/kbase/models/model.onnx \
  https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
curl -L -o ~/.cache/kbase/models/tokenizer.json \
  https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json
```

Models are searched in: `./models`, `<vault>/models`, `$XDG_CACHE_HOME/kbase/models`.

### Configure chunking (optional)

In `.kbase/config.yaml`:

```yaml
embeddings:
  backend: onnx           # or "ollama" for Ollama server
  chunk_level: "##"       # Split on H2 headers (options: none, #, ##, ###, paragraph)
  include_context: true   # Include note title + parent headers in chunks (default: true)
```

With `include_context: true`, each chunk includes the note title and parent headers for better semantic understanding. For example, a chunk under "## Ingredients" in "Recipe.md" would include:
```
# Recipe

## Ingredients

- flour
- sugar
```

Embeddings are cached in `.kbase/embeddings.redb` - only changed content is re-embedded.

## LSP Server

kbase includes an LSP server for editor integration with features like:

- **Go to definition**: Follow `[[wiki links]]` or `[markdown](links.md)` to target notes
  - On tags: jumps to the tag note (e.g., `domain/ai` → opens "ai" note)
  - Prompts to create note if it doesn't exist
- **Find references**: Show all backlinks to the current note
- **Hover**: Preview note content when hovering over links
- **Completion**: Note titles on `[[` (wiki) and `[` (markdown), tag paths in frontmatter
- **Diagnostics**: Schema validation errors shown inline

### Neovim + CoC Setup

Add to your `coc-settings.json`:

```json
{
  "languageserver": {
    "kbase": {
      "command": "kbase",
      "args": ["lsp"],
      "filetypes": ["markdown"],
      "rootPatterns": [".kbase"]
    }
  }
}
```

## Neovim Integrations

kbase includes a Neovim plugin for enhanced editing with Telescope pickers and Neo-tree integration.

### Installation

**lazy.nvim:**
```lua
{
  "your-user/kbase",
  dir = "nvim",  -- Use the nvim/ subdirectory
  dependencies = {
    "nvim-telescope/telescope.nvim",
    "nvim-neo-tree/neo-tree.nvim",  -- optional
  },
  config = function()
    require("kbase").setup({
      telescope_prefix = "<leader>k",  -- Keymaps: <leader>ks, <leader>kb, etc.
      neotree_keymap = "<leader>kT",   -- Optional: keymap for tag tree
    })
  end,
}
```

**Nix (Home Manager):**
```nix
programs.neovim.plugins = [
  {
    plugin = pkgs.vimUtils.buildVimPlugin {
      pname = "kbase-nvim";
      version = "0.1.0";
      src = "${kbase}/nvim";
    };
    config = ''
      lua require("kbase").setup()
    '';
  }
];
```

**Manual:**
```lua
vim.opt.runtimepath:append("/path/to/kbase/nvim")
require("kbase").setup()
```

### Features

**Telescope Pickers** (`<leader>k` prefix by default):
- `<leader>ks` - Semantic search across notes
- `<leader>kb` - Backlinks to current note
- `<leader>kn` - Browse all notes
- `<leader>kt` - Browse tags

**Neo-tree Tag Browser:**
- `:KbaseTags` or configured keymap
- Hierarchical tag tree with lazy-loaded children
- Notes shown under their direct tags (no duplicates)
- Press Enter to expand tags or open notes

## AI Assistant Skills

kbase can generate skills for AI coding assistants (like Claude Code):

```bash
kbase install-skills
```

This creates `.claude/skills/` with:

- `/kb-search` - Semantic search with results as context
- `/kb-similar` - Find related notes
- `/kb-backlinks` - Show notes that reference a topic
- `/kb-validate` - Run schema validation and check for broken links
- `/kb-new` - Create new notes with suggestions
- `/kb-overview` - Get vault summary (tags, key notes, structure)
- `/kb-query` - Run SPARQL queries on the knowledge graph
- `/kb-tags` - Show tag hierarchy
- `/kb-list` - List notes (with optional tag filter)
- `/kb-retag` - Rename tags by replacing a prefix
- `/kb-organize` - Organize notes into directories by tag

Use these skills to query your knowledge base from any project directory.

## License

TBD
