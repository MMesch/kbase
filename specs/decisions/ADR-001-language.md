# ADR-001: Programming Language

**Status**: Accepted
**Date**: 2026-02-11
**Deciders**: TBD

## Context

We need to choose a programming language for kg. The choice affects:
- Distribution model (single binary vs. runtime dependency)
- Startup performance
- SPARQL/RDF library availability
- Code sharing with future Obsidian plugin
- Developer experience and contribution barrier

## Decision Drivers

- **SPARQL support**: Must have embedded SPARQL query engine
- **Minimal dependencies**: Users should install a single binary
- **Fast startup**: Sub-100ms target for CLI responsiveness
- **Obsidian plugin potential**: Code sharing with browser/Electron
- **Linux-first**: Primary platform, cross-platform is future scope

## Considered Options

### Option 1: Rust + Oxigraph

**Description**: Systems language with native RDF/SPARQL support.

**SPARQL**: [Oxigraph](https://github.com/oxigraph/oxigraph) - native SPARQL 1.1, embedded, RocksDB-backed.

**Pros**:
- Excellent single-binary story (static linking)
- Very fast startup and runtime
- Oxigraph is purpose-built for this use case
- Strong type system
- Good CLI ecosystem (clap, serde)

**Cons**:
- No code sharing with Obsidian plugin (different runtime)
- Steeper learning curve
- Longer compile times
- Smaller contributor pool

**Binary size**: ~10-15 MB
**Startup**: <10ms

---

### Option 2: Go

**Description**: Simple, fast-compiling language.

**SPARQL**: Fragmented ecosystem. Options:
- [Cayley](https://github.com/cayleygraph/cayley) - graph DB, partial SPARQL
- Build on SQLite with custom SPARQL subset
- FFI to Oxigraph (complexity)

**Pros**:
- Simple language, low barrier
- Fast compilation
- Single binary
- Large community

**Cons**:
- **Weak SPARQL ecosystem** - major concern
- No code sharing with Obsidian
- Larger binaries than Rust
- Less expressive types

**Binary size**: ~15-20 MB
**Startup**: <20ms

---

### Option 3: TypeScript + Deno

**Description**: TypeScript compiled to single binary via Deno.

**SPARQL**: [Quadstore](https://github.com/quadstorejs/quadstore) + [Comunica](https://comunica.dev/) - LevelDB-backed, full SPARQL support, works in Node/Deno/browsers.

**Pros**:
- **Code sharing with Obsidian plugin** - huge advantage
- **Runs in browser** - future web UI, Obsidian integration
- Quadstore provides embedded SPARQL with persistence
- Rich ecosystem (markdown, yaml, CLI libraries)
- Deno compile produces single binary
- TypeScript's type system is solid
- Large contributor pool

**Cons**:
- Larger binary (~50-80 MB with Deno runtime)
- Slower startup than Rust/Go (~50-100ms)
- Deno compile FFI support is recent (2.3+)
- LevelDB less battle-tested than SQLite/RocksDB

**Binary size**: ~50-80 MB
**Startup**: ~50-100ms

---

### Option 4: Haskell

**Description**: Functional language with strong types.

**SPARQL**:
- [rdf4h](https://hackage.haskell.org/package/rdf4h) - RDF processing, in-memory graphs
- [hsparql](https://github.com/robstewart57/hsparql) - SPARQL DSL for querying external servers
- **No embedded SPARQL engine** - would need to build one

**Pros**:
- Excellent for parsing (Markdown, YAML, SPARQL syntax)
- Strong type system, correctness guarantees
- Compiles to single binary (GHC)
- Good performance when optimized

**Cons**:
- **No embedded SPARQL execution** - significant gap
- Would need to implement SPARQL eval or use external store
- Smaller ecosystem for CLI tooling
- Steeper learning curve
- Smaller contributor pool
- Longer compile times

**Binary size**: ~20-30 MB
**Startup**: ~20-50ms

---

## Analysis Matrix

| Criterion | Rust | Go | TypeScript | Haskell |
|-----------|------|-----|------------|---------|
| Embedded SPARQL | ✅ Oxigraph | ⚠️ Weak | ✅ Quadstore | ❌ None |
| Single binary | ✅ | ✅ | ✅ Deno | ✅ |
| Binary size | ✅ Small | ✅ Medium | ⚠️ Large | ✅ Medium |
| Startup time | ✅ <10ms | ✅ <20ms | ⚠️ ~50-100ms | ✅ ~20-50ms |
| Obsidian code share | ❌ | ❌ | ✅✅ | ❌ |
| Browser potential | ❌ | ❌ | ✅✅ | ❌ |
| Ecosystem | ✅ | ✅ | ✅ | ⚠️ |
| Contributor pool | ⚠️ | ✅ | ✅ | ⚠️ |

## Recommendation

**TypeScript + Deno** is surprisingly compelling because:

1. **Obsidian synergy**: Same codebase could power:
   - CLI tool (`deno compile`)
   - Obsidian plugin (runs in Electron)
   - Future web UI (runs in browser)

2. **SPARQL solved**: Quadstore + Comunica provides full SPARQL 1.1 with LevelDB persistence, working across all JS runtimes.

3. **Acceptable tradeoffs**:
   - Larger binary (~60MB vs ~15MB) is fine for a dev tool
   - Startup ~50-100ms is acceptable (still feels instant)

**Rust + Oxigraph** remains excellent if:
- Code sharing with Obsidian isn't prioritized
- Minimal binary size is critical
- Maximum performance is required

**Haskell** is not recommended due to the SPARQL gap.

**Go** is not recommended due to weak RDF/SPARQL ecosystem.

## Evaluation Plan

1. **Prototype A**: TypeScript + Deno + Quadstore
   - Implement `kg init`, `kg new`, `kg query`
   - Test Deno compile binary size and startup
   - Verify SPARQL queries work as expected

2. **Prototype B**: Rust + Oxigraph
   - Same commands
   - Compare DX and performance

3. **Decide** based on:
   - SPARQL query correctness
   - Startup time measurements
   - Code complexity comparison
   - Obsidian plugin feasibility (TypeScript only)

## Decision

**Rust**

Rationale:
- Focus on terminal-first CLI tool
- Maximum performance and minimal binary size
- Multiple graph database options available (see ADR-002)
- Obsidian integration can come later as separate implementation

## Consequences

### If TypeScript + Deno
- Larger binary, acceptable startup
- Can share code with Obsidian plugin
- Can potentially run in browser
- Good contributor accessibility

### If Rust + Oxigraph
- Optimal performance and size
- Separate Obsidian plugin implementation
- Steeper contribution barrier

## Related

- ADR-002: Graph Database (Quadstore vs Oxigraph)
- specs/PRODUCT.md: Performance requirements

## Sources

- [Quadstore](https://github.com/quadstorejs/quadstore) - LevelDB-backed RDF store with SPARQL
- [Deno Compile](https://docs.deno.com/runtime/reference/cli/compile/) - Single binary compilation
- [Oxigraph](https://github.com/oxigraph/oxigraph) - Rust SPARQL engine
- [rdf4h](https://hackage.haskell.org/package/rdf4h) - Haskell RDF library
- [hsparql](https://github.com/robstewart57/hsparql) - Haskell SPARQL DSL
