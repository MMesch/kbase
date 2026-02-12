# ADR-001: Technology Stack

**Status**: Evaluating
**Date**: 2026-02-12
**Deciders**: TBD

## Context

This ADR selects a coherent technology stack for kg by crossing our research on:
- [Languages](../research/languages.md)
- [Graph Databases](../research/graph-databases.md)
- [Validation](../research/validation.md)
- [Ontology](../research/ontology.md)
- [Inference](../research/inference.md)

## Decision Drivers

- **Viability**: Language must have mature database bindings
- **Validation**: Stack must support chosen validation strategy
- **Inference**: Stack must support needed inference capabilities
- **Performance**: Meet CLI responsiveness requirements
- **Maintainability**: Reasonable complexity and contributor accessibility

---

## Viable Stacks by Language

### Rust

| Database | Query | Validation | Inference | Maturity |
|----------|-------|------------|-----------|----------|
| **Oxigraph** | SPARQL 1.1 | SHACL (rudof), SPARQL ASK | Property paths, RDFS | ✅ Production |
| **CozoDB** | Datalog | Datalog constraints | Datalog rules | ✅ Production |
| **CQLite** | Cypher | Custom only | Custom only | ⚠️ Pre-release |

**Stack options**:
1. **Rust + Oxigraph**: SPARQL ecosystem, SHACL validation, semantic web interop
2. **Rust + CozoDB**: Datalog power, built-in algorithms, native recursion

```rust
// Example: Rust + Oxigraph
use oxigraph::store::Store;
use oxigraph::sparql::QueryResults;

let store = Store::open("kg.db")?;
let results = store.query("SELECT ?note WHERE { ?note kg:hasTag kg:dev }")?;

// Example: Rust + CozoDB
use cozo::DbInstance;

let db = DbInstance::new("rocksdb", "kg.db", "")?;
let results = db.run_script("?[note] := *notes{id: note, tag: 'dev'}", Default::default())?;
```

---

### TypeScript / JavaScript

| Database | Query | Validation | Inference | Maturity |
|----------|-------|------------|-----------|----------|
| **Quadstore** | SPARQL 1.1 | SHACL (rdf-validate-shacl) | Property paths | ✅ Production |
| **CozoDB** | Datalog | Datalog constraints | Datalog rules | ✅ Production (WASM) |
| **LevelGraph** | Custom API | Custom only | Custom only | ✅ Stable |

**Stack options**:
1. **TypeScript + Quadstore**: SPARQL, browser-ready, Obsidian-compatible
2. **TypeScript + CozoDB (WASM)**: Datalog power in JS runtime

```typescript
// Example: TypeScript + Quadstore
import { Quadstore } from 'quadstore';
import { Engine } from '@comunica/query-sparql';

const store = new Quadstore({ backend: levelDB });
const engine = new Engine();
const results = await engine.queryBindings(
  `SELECT ?note WHERE { ?note kg:hasTag kg:dev }`,
  { sources: [store] }
);

// Example: TypeScript + CozoDB
import { CozoDb } from 'cozo-node';

const db = new CozoDb();
const results = await db.run("?[note] := *notes{id: note, tag: 'dev'}");
```

---

### Python

| Database | Query | Validation | Inference | Maturity |
|----------|-------|------------|-----------|----------|
| **rdflib** | SPARQL 1.1 | SHACL (pyshacl), OWL (owlrl) | RDFS, OWL 2 RL | ✅ Production |
| **Oxigraph** | SPARQL 1.1 | SHACL (pyshacl) | Property paths | ✅ Production |
| **CozoDB** | Datalog | Datalog constraints | Datalog rules | ✅ Production |

**Stack options**:
1. **Python + rdflib**: Full semantic web stack, rich inference
2. **Python + Oxigraph**: Fast SPARQL with Python ergonomics
3. **Python + CozoDB**: Datalog power with Python bindings

```python
# Example: Python + rdflib
from rdflib import Graph
g = Graph()
g.parse("notes.ttl")
results = g.query("SELECT ?note WHERE { ?note kg:hasTag kg:dev }")

# Example: Python + CozoDB
from cozo import Client
db = Client()
results = db.run("?[note] := *notes{id: note, tag: 'dev'}")
```

---

### Go

| Database | Query | Validation | Inference | Maturity |
|----------|-------|------------|-----------|----------|
| **Cayley** | Gizmo | Custom only | Limited | ⚠️ Maintenance |
| **Custom** | Custom | Custom only | Custom only | N/A |

**Assessment**: Go lacks mature embedded graph database options. Not recommended for kg.

---

### Haskell

| Database | Query | Validation | Inference | Maturity |
|----------|-------|------------|-----------|----------|
| **rdf4h** | In-memory | Custom only | Custom only | ⚠️ Limited |

**Assessment**: Haskell lacks embedded persistent graph databases. Not recommended for kg.

---

## Stack Comparison Matrix

| Stack | Performance | Validation | Inference | Obsidian | Maturity | Complexity |
|-------|-------------|------------|-----------|----------|----------|------------|
| Rust + Oxigraph | ✅✅ | ✅ SHACL | ✅ SPARQL/RDFS | ❌ | ✅ | Medium |
| Rust + CozoDB | ✅✅ | ✅ Datalog | ✅ Datalog | ❌ | ✅ | Medium |
| TS + Quadstore | ⚠️ | ✅ SHACL | ✅ SPARQL | ✅✅ | ✅ | Medium |
| TS + CozoDB | ⚠️ | ✅ Datalog | ✅ Datalog | ✅ | ✅ | Medium |
| Python + rdflib | ⚠️ | ✅✅ SHACL+OWL | ✅✅ OWL | ❌ | ✅ | Low |
| Python + CozoDB | ⚠️ | ✅ Datalog | ✅ Datalog | ❌ | ✅ | Low |

---

## Recommended Stacks

### Primary Recommendation: Rust + CozoDB

**Rationale**:
- Optimal CLI performance (startup, queries)
- Datalog is ideal for recursive graph queries
- Built-in full-text search and algorithms
- Multiple storage backends (SQLite for simplicity, RocksDB for scale)
- Validation via Datalog constraints

**Tradeoffs**:
- No SPARQL/semantic web interop
- Separate Obsidian plugin needed
- Steeper learning curve (Rust + Datalog)

---

### Alternative: Rust + Oxigraph

**Rationale**:
- W3C SPARQL standard
- SHACL validation support (via rudof)
- Semantic web interoperability
- Property paths for recursive queries

**Tradeoffs**:
- More verbose than Datalog
- No built-in algorithms (must implement)
- RDF complexity (URIs, namespaces)

---

### Alternative: TypeScript + Quadstore

**Rationale**:
- Code sharing with Obsidian plugin
- Browser-ready (future web UI)
- Large contributor pool
- SPARQL standard

**Tradeoffs**:
- Slower startup (~50-100ms)
- Larger binary (~50-80MB)
- LevelDB less battle-tested than RocksDB

---

### Alternative: Python + rdflib

**Rationale**:
- Rapid prototyping
- Richest inference options (OWL, RDFS)
- Best SHACL ecosystem (pyshacl)
- Large community

**Tradeoffs**:
- Slowest startup (~100-300ms)
- Distribution complexity
- Not suitable for snappy CLI feel

---

## Evaluation Plan

Build minimal prototypes for top candidates:

1. **Rust + CozoDB**
   - Implement: init, new, backlinks, tag query
   - Test: Datalog constraint validation
   - Benchmark: startup, query performance

2. **Rust + Oxigraph**
   - Same features
   - Test: SHACL validation (rudof)
   - Compare: query expressiveness

3. **TypeScript + Quadstore** (if Obsidian priority increases)
   - Same features
   - Test: Deno compile binary
   - Measure: startup overhead

## Decision

**TBD** - Awaiting prototype results.

## Consequences

### If Rust + CozoDB
- Maximum performance
- Datalog learning curve
- Built-in algorithms available
- Separate Obsidian implementation

### If Rust + Oxigraph
- SPARQL/RDF ecosystem
- Semantic web interoperability
- SHACL validation standard
- Separate Obsidian implementation

### If TypeScript + Quadstore
- Obsidian code sharing
- Larger/slower binary
- SPARQL standard
- Browser-ready

### If Python + rdflib
- Best for prototyping
- Rich inference ecosystem
- Slow CLI startup
- Distribution challenges

## Related

**Research:**
- [Languages](../research/languages.md)
- [Graph Databases](../research/graph-databases.md)
- [Validation](../research/validation.md)
- [Ontology](../research/ontology.md)
- [Inference](../research/inference.md)

**Specs:**
- [PRODUCT.md](../PRODUCT.md): Product requirements
- [note-format.md](../contracts/note-format.md): Note schema
