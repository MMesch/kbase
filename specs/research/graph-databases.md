# Graph Databases

Research on embedded graph database options for kg. Evaluates query paradigms, capabilities, and database candidates.

See: [ADR-001: Stack Decision](../decisions/ADR-001-stack.md)

## Requirements

- **Embedded**: No external process or service
- **Persistent**: Data survives restarts
- **Performance**: Handle 10,000+ notes, sub-second queries
- **Query power**: Backlinks, recursive tag queries, path traversal
- **Maturity**: Stable enough for production use
- **Single binary**: Compiles/bundles into kg

## Query Paradigms

### SPARQL (RDF Triple Stores)
W3C standard for querying RDF data. Subject-predicate-object triples.

```sparql
# Find notes linking to target
PREFIX kg: <http://kg.local/>
SELECT ?note ?title WHERE {
  ?note kg:linksTo kg:note/abc123 .
  ?note kg:title ?title .
}
```

**Strengths**: Standards compliance, semantic web interop, property paths for recursion
**Weaknesses**: Verbose syntax, URI/namespace overhead

### Datalog
Logic programming language for deductive databases. Native recursion.

```datalog
# Find notes linking to target
?[note, title] :=
  *links{from: note, to: "abc123"},
  *notes{id: note, title}
```

**Strengths**: Natural recursive queries, declarative, built-in algorithms
**Weaknesses**: Learning curve, less familiar than SQL

### Cypher
Pattern matching language popularized by Neo4j. Property graphs.

```cypher
// Find notes linking to target
MATCH (note)-[:LINKS_TO]->(target {id: "abc123"})
RETURN note.id, note.title
```

**Strengths**: Intuitive syntax, visual pattern matching, widely known
**Weaknesses**: No embedded production-ready option yet

### SQL + Graph Extensions
Traditional SQL with recursive CTEs or graph extensions.

```sql
-- Find notes linking to target
SELECT n.id, n.title
FROM notes n
JOIN links l ON l.from_id = n.id
WHERE l.to_id = 'abc123';

-- Recursive tag hierarchy (CTE)
WITH RECURSIVE ancestors AS (
  SELECT tag, parent FROM tags WHERE tag = 'dev/rust'
  UNION ALL
  SELECT t.tag, t.parent FROM tags t
  JOIN ancestors a ON t.tag = a.parent
)
SELECT * FROM ancestors;
```

**Strengths**: Familiar syntax, mature tooling
**Weaknesses**: Recursion via CTEs is awkward, not graph-native

---

## Database Candidates

| Database | Query Language | Storage | Maturity | Validation | Languages |
|----------|---------------|---------|----------|------------|-----------|
| **Oxigraph** | SPARQL 1.1 | RocksDB | ✅ Production | SHACL, SPARQL ASK | Rust, Python |
| **CozoDB** | Datalog | RocksDB/SQLite/Memory | ✅ Production | Datalog constraints | Rust, Python, WASM |
| **Quadstore** | SPARQL 1.1 | LevelDB | ✅ Production | SHACL, SPARQL ASK | JS/TS |
| **rdflib** | SPARQL 1.1 | Multiple | ✅ Production | SHACL, OWL | Python |
| **CQLite** | Cypher (subset) | Custom | ⚠️ Pre-release | Custom | Rust |
| **Cayley** | Gizmo | Multiple | ⚠️ Maintenance | Custom | Go |

For detailed language × database combinations, see **ADR-005: Stack Decision**.

---

## Detailed Evaluation

### Oxigraph (Rust, SPARQL)

```sparql
# Backlinks query
PREFIX kg: <http://kg.local/>
SELECT ?source ?title WHERE {
  ?source kg:linksTo <http://kg.local/note/a1b2c3> .
  ?source kg:title ?title .
}

# Recursive tag descendants (property paths)
PREFIX kg: <http://kg.local/>
SELECT DISTINCT ?note ?title WHERE {
  ?tag kg:parentTag* <http://kg.local/tag/dev> .
  ?note kg:hasTag ?tag .
  ?note kg:title ?title .
}
```

**Pros**: Full SPARQL 1.1, active development, RDF interop
**Cons**: RDF complexity (URIs, namespaces), verbose

---

### CozoDB (Rust/WASM, Datalog)

```datalog
# Backlinks query
?[source, title] :=
  *links{from: source, to: "a1b2c3"},
  *notes{id: source, title}

# Recursive tag hierarchy
ancestor[tag, parent] := *tags{tag, parent}
ancestor[tag, anc] := ancestor[tag, mid], *tags{mid, parent: anc}

?[note, title] :=
  *note_tags{note, tag},
  ancestor[tag, "dev"],
  *notes{id: note, title}
```

**Pros**: Native recursion, built-in PageRank/shortest-path, FTS, multi-backend
**Cons**: Datalog learning curve, custom syntax

---

### Quadstore + Comunica (TypeScript, SPARQL)

```typescript
import { Quadstore } from 'quadstore';
import { Engine } from '@comunica/query-sparql';

const store = new Quadstore({ backend: levelDB });
const engine = new Engine();

const result = await engine.queryBindings(`
  PREFIX kg: <http://kg.local/>
  SELECT ?source ?title WHERE {
    ?source kg:linksTo <http://kg.local/note/a1b2c3> .
    ?source kg:title ?title .
  }
`, { sources: [store] });
```

**Pros**: Full SPARQL, browser-compatible, Obsidian-ready
**Cons**: LevelDB less battle-tested, JS overhead

---

### CQLite (Rust, Cypher) - Future

```cypher
MATCH (source)-[:LINKS_TO]->(target {id: "a1b2c3"})
RETURN source.id, source.title
```

**Pros**: Intuitive syntax, property graph model
**Cons**: Pre-release, not production-ready

---

## Comparison Matrix

| Criterion | Oxigraph | CozoDB | Quadstore | CQLite | Cayley |
|-----------|----------|--------|-----------|--------|--------|
| Languages | Rust, Python | Rust, WASM, Python | JS/TS | Rust | Go |
| Query Lang | SPARQL | Datalog | SPARQL | Cypher | Gizmo |
| Maturity | ✅ | ✅ | ✅ | ⚠️ | ⚠️ |
| Recursive queries | ✅ | ✅✅ | ✅ | ✅ | ⚠️ |
| Built-in algorithms | ❌ | ✅ | ❌ | ❌ | ⚠️ |
| Full-text search | ❌ | ✅ | ❌ | ❌ | ❌ |
| Browser support | ❌ | ✅ WASM | ✅ | ❌ | ❌ |
| Standards | ✅ W3C | ❌ | ✅ W3C | ⚠️ | ❌ |
| Validation | SHACL, SPARQL | Datalog rules | SHACL, SPARQL | Custom | Custom |

## Evaluation Plan

For each viable language × database combination:
1. Implement minimal prototype (init, create note, backlinks, tag query)
2. Benchmark with test dataset (100, 1000, 10000 notes)
3. Evaluate query expressiveness and DX
4. Document findings

## Leaning Toward

**CozoDB** appears strongest across languages because:
- Native recursion (ideal for tag hierarchies)
- Built-in algorithms reduce custom code
- Works in Rust (native) and TypeScript (WASM)
- Production-ready with multiple storage backends

**Oxigraph/Quadstore** if we need:
- W3C SPARQL compliance
- Semantic web interoperability
- External tool integration

## Related Research

- [Languages](./languages.md)
- [Validation](./validation.md)
- [Ontology](./ontology.md)
- [Inference](./inference.md)
