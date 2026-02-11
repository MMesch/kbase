# ADR-002: Graph Database

**Status**: Evaluating
**Date**: 2026-02-11
**Deciders**: TBD

## Context

kg needs an embedded graph database to store notes as nodes, links as edges, and support powerful queries for backlinks, tag hierarchies, and graph traversal.

We've decided on Rust (ADR-001), which gives us three strong candidates with different query languages.

## Decision Drivers

- **Embedded**: No external process or service
- **Persistent**: Data survives restarts
- **Performance**: Handle 10,000+ notes, sub-second queries
- **Query power**: Backlinks, recursive tag queries, path traversal
- **Maturity**: Stable enough for production use
- **Single binary**: Compiles into kg

## Candidates

### Option 1: Oxigraph (SPARQL)

**Query Language**: SPARQL 1.1

**Description**: RDF triple store with full SPARQL support.

| Aspect | Details |
|--------|---------|
| Storage | RocksDB |
| Maturity | Production-ready |
| Data Model | RDF triples (subject-predicate-object) |
| Crate | [oxigraph](https://crates.io/crates/oxigraph) |

**Pros**:
- Full SPARQL 1.1 (Query, Update, Federated)
- Standard semantic web format
- Well-documented, active development
- Good for interoperability with other RDF tools

**Cons**:
- RDF model adds complexity (URIs, namespaces)
- SPARQL syntax can be verbose
- Overkill if we don't need semantic web interop

**Example query** (backlinks):
```sparql
PREFIX kg: <http://kg.local/>
SELECT ?source ?title WHERE {
  ?source kg:linksTo <http://kg.local/note/a1b2c3> .
  ?source kg:title ?title .
}
```

---

### Option 2: CozoDB (Datalog)

**Query Language**: Datalog (CozoScript)

**Description**: Relational-graph database optimized for recursive queries.

| Aspect | Details |
|--------|---------|
| Storage | RocksDB, SQLite, or in-memory |
| Maturity | Production-ready |
| Data Model | Relations (tables) with graph semantics |
| Crate | [cozo](https://crates.io/crates/cozo) |

**Pros**:
- Datalog is natural for recursive graph queries
- Built-in graph algorithms (PageRank, shortest path, etc.)
- Vector search and full-text search included
- Multiple storage backends
- Runs everywhere (WASM, mobile, embedded)
- Excellent performance (~100K QPS)

**Cons**:
- Datalog has a learning curve
- Less familiar than SQL/Cypher
- Younger project than Oxigraph

**Example query** (backlinks):
```datalog
?[source, title] :=
  *links{from: source, to: "a1b2c3"},
  *notes{id: source, title}
```

**Example** (recursive tag hierarchy):
```datalog
ancestor[tag, parent] := *tags{tag, parent}
ancestor[tag, ancestor] := ancestor[tag, mid], *tags{mid, parent: ancestor}

?[note, title] :=
  *note_tags{note, tag},
  ancestor[tag, "dev"],
  *notes{id: note, title}
```

---

### Option 3: CQLite (Cypher)

**Query Language**: Cypher (subset)

**Description**: Embedded property graph with Neo4j-style queries.

| Aspect | Details |
|--------|---------|
| Storage | Custom file format |
| Maturity | **Pre-release** |
| Data Model | Property graph (nodes + relationships) |
| Crate | [cqlite](https://crates.io/crates/cqlite) |

**Pros**:
- Cypher is widely known (Neo4j popularity)
- Intuitive pattern matching syntax
- Property graph model is natural fit

**Cons**:
- **Pre-release**: Not production-ready yet
- Subset of Cypher only
- File format not stabilized
- Less active than alternatives

**Example query** (backlinks):
```cypher
MATCH (source)-[:LINKS_TO]->(target {id: "a1b2c3"})
RETURN source.id, source.title
```

---

## Comparison Matrix

| Criterion | Oxigraph | CozoDB | CQLite |
|-----------|----------|--------|--------|
| Query Language | SPARQL | Datalog | Cypher |
| Maturity | ✅ Production | ✅ Production | ⚠️ Pre-release |
| Recursive queries | ✅ | ✅✅ Native | ✅ |
| Built-in algorithms | ❌ | ✅ PageRank, etc. | ❌ |
| Full-text search | ❌ Manual | ✅ Built-in | ❌ |
| Learning curve | Medium | Medium-High | Low |
| Storage options | RocksDB | Multiple | Custom |
| Standards compliance | ✅ W3C SPARQL | ❌ Custom | ⚠️ Subset |

## Evaluation Plan

1. **Prototype with CozoDB**
   - Implement note/link storage
   - Test backlink queries
   - Test recursive tag hierarchy
   - Benchmark with 10k notes

2. **Prototype with Oxigraph**
   - Same tests
   - Compare query complexity

3. **Monitor CQLite**
   - Track progress toward stable release
   - Evaluate when 1.0 ships

## Leaning Toward

**CozoDB** appears strongest because:
- Datalog is ideal for recursive graph queries (tags, paths)
- Built-in algorithms and full-text search reduce custom code
- Multiple storage backends (SQLite for simplicity, RocksDB for performance)
- Production-ready today

**Oxigraph** if we need:
- RDF/semantic web interoperability
- Standard SPARQL for external tool integration

**CQLite** if:
- It reaches stable release
- Cypher familiarity outweighs maturity concerns

## Decision

**TBD** - Awaiting prototype results.

## Related

- ADR-001: Language Choice (Rust - Accepted)
- specs/PRODUCT.md: Query requirements
- specs/features/search-query.md: Query use cases
