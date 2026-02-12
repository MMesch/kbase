# Programming Languages

Research on programming language options for kg. This document evaluates languages on their own merits. The final stack decision combines this with database, validation, and inference research.

See: [ADR-001: Stack Decision](../decisions/ADR-001-stack.md)

## Evaluation Criteria

- **Single binary**: Users should install one file, no runtime dependencies
- **Fast startup**: Sub-100ms target for CLI responsiveness
- **Type safety**: Strong types to catch errors at compile time
- **Ecosystem**: CLI libraries, YAML/Markdown parsing, general tooling
- **Contributor accessibility**: Balance performance with approachability
- **Cross-platform**: Linux-first, macOS/Windows as future scope

## Considered Options

### Option 1: Rust

**Description**: Systems language with zero-cost abstractions and memory safety.

| Aspect | Details |
|--------|---------|
| Binary | Static linking, single executable |
| Size | ~10-15 MB typical |
| Startup | less than 10ms |
| Type System | Strong, algebraic data types, traits |

**Pros**:
- Excellent single-binary story
- Very fast startup and runtime
- Strong CLI ecosystem (clap, serde, tokio)
- Memory safety without GC
- Good cross-platform support

**Cons**:
- Steeper learning curve
- Longer compile times
- Smaller contributor pool than JS/Go
- No code sharing with Obsidian plugin

---

### Option 2: Go

**Description**: Simple, fast-compiling language with built-in concurrency.

| Aspect | Details |
|--------|---------|
| Binary | Static linking, single executable |
| Size | ~15-20 MB typical |
| Startup | under 20ms |
| Type System | Static, interfaces, generics (1.18+) |

**Pros**:
- Simple language, low learning barrier
- Fast compilation
- Large community
- Good cross-platform support
- Excellent for CLI tools

**Cons**:
- Less expressive type system
- No code sharing with Obsidian plugin
- Larger binaries than Rust
- Error handling can be verbose

---

### Option 3: TypeScript + Deno

**Description**: TypeScript compiled to single binary via Deno.

| Aspect | Details |
|--------|---------|
| Binary | Deno compile bundles runtime |
| Size | ~50-80 MB |
| Startup | ~50-100ms |
| Type System | Structural, gradual typing |

**Pros**:
- **Code sharing with Obsidian plugin** - same codebase
- **Browser potential** - future web UI
- Rich ecosystem (markdown, yaml, CLI libraries)
- Large contributor pool
- TypeScript's type system is solid

**Cons**:
- Larger binary (includes V8 runtime)
- Slower startup than compiled languages
- Runtime overhead
- FFI support still maturing

---

### Option 4: Haskell

**Description**: Purely functional language with strong static types.

| Aspect | Details |
|--------|---------|
| Binary | GHC compiles to native code |
| Size | ~20-30 MB |
| Startup | ~20-50ms |
| Type System | Hindley-Milner, type classes, GADTs |

**Pros**:
- Excellent for parsing (Markdown, YAML, query languages)
- Very strong type system, correctness guarantees
- Good performance when optimized
- Elegant code for data transformations

**Cons**:
- Steep learning curve
- Smaller ecosystem for CLI tooling
- Smaller contributor pool
- Longer compile times
- Lazy evaluation can surprise

---

### Option 5: Python

**Description**: Dynamic language with extensive ecosystem and rapid development.

| Aspect | Details |
|--------|---------|
| Binary | PyInstaller/PyOxidizer bundles interpreter |
| Size | ~30-80 MB |
| Startup | ~100-300ms |
| Type System | Dynamic, optional type hints (mypy) |

**Pros**:
- Very large ecosystem and community
- Rapid prototyping and iteration
- Excellent data processing libraries
- Low learning curve
- Good RDF/graph libraries (rdflib, networkx)

**Cons**:
- Slow startup (interpreter initialization)
- Large bundled binaries
- Runtime performance overhead
- Type hints optional, not enforced
- Packaging/distribution complexity

---

## Analysis Matrix

| Criterion | Rust | Go | TypeScript | Haskell | Python |
|-----------|------|-----|------------|---------|--------|
| Single binary | ✅ | ✅ | ✅ | ✅ | ⚠️ Bundled |
| Binary size | ✅ Small | ✅ Medium | ⚠️ Large | ✅ Medium | ⚠️ Large |
| Startup time | ✅ under 10ms | ✅ under 20ms | ⚠️ ~50-100ms | ✅ ~20-50ms | ❌ ~100-300ms |
| Type safety | ✅✅ | ✅ | ✅ | ✅✅ | ⚠️ Optional |
| CLI ecosystem | ✅ | ✅ | ✅ | ⚠️ | ✅ |
| Learning curve | ⚠️ Steep | ✅ Easy | ✅ Easy | ⚠️ Steep | ✅ Easy |
| Contributor pool | ⚠️ | ✅ | ✅✅ | ⚠️ | ✅✅ |
| Obsidian code share | ❌ | ❌ | ✅✅ | ❌ | ❌ |
| Browser potential | ⚠️ WASM | ❌ | ✅✅ | ❌ | ⚠️ Pyodide |

## Key Tradeoffs

### Performance vs. Code Sharing
- **Rust/Go**: Optimal CLI performance, but requires separate Obsidian plugin
- **TypeScript**: Slower startup, but same code runs in CLI, Obsidian, and browser

### Type Safety vs. Approachability
- **Rust/Haskell**: Maximum type safety, steeper learning curve
- **Go/TypeScript**: Easier onboarding, more permissive type systems

### Binary Size
- For a developer tool, 50-80MB (TypeScript) is acceptable
- For system-wide installation, 10-15MB (Rust) is preferable

## Implications by Choice

| Language | Strengths | Weaknesses |
|----------|-----------|------------|
| Rust | Optimal perf, strong types | Steep learning, separate Obsidian |
| Go | Simple, large community | Less expressive, separate Obsidian |
| TypeScript | Obsidian code share, browser | Slow startup, large binary |
| Haskell | Excellent parsing, correctness | Small ecosystem, separate Obsidian |
| Python | Rapid dev, large ecosystem | Slowest startup, distribution pain |

## Related Research

- [Graph Databases](./graph-databases.md)
- [Validation](./validation.md)
- [Ontology](./ontology.md)
- [Inference](./inference.md)
