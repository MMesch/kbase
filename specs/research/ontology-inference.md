# Ontology and Inference

Research on whether kg benefits from ontologies and inference, and which approaches fit our ecosystem.

See: [ADR-001: Stack Decision](../decisions/ADR-001-stack.md)

## Overview

kg stores notes with structured metadata (tags, links, fields). We may benefit from:
- **Ontologies**: Formal definitions of concepts and relationships
- **Inference**: Automatically deriving new facts from existing data

For example, if a note is tagged `dev/rust/async`, inference could automatically derive membership in `dev/rust` and `dev` without explicit storage.

## Evaluation Criteria

- **Value**: Does inference reduce manual work or enable new features?
- **Performance**: Inference must not slow down common operations
- **Complexity**: Balance power against implementation/maintenance cost
- **Ecosystem fit**: Must work with chosen database (ADR-002) and language (ADR-001)

## Use Cases for Inference

### 1. Tag Hierarchy (Transitive Closure)
```
Given: note tagged "dev/rust/async"
Infer: note is also member of "dev/rust" and "dev"
```

**Without inference**: Store all ancestor tags explicitly, or query recursively
**With inference**: Store only leaf tag, reasoner derives ancestors

### 2. Inverse Relationships
```
Given: noteA linksTo noteB
Infer: noteB hasBacklink noteA
```

**Without inference**: Maintain backlinks manually or query inverse
**With inference**: Define `hasBacklink` as inverse of `linksTo`

### 3. Type Inheritance
```
Given: "meeting-note" is subclass of "note"
       All notes require "title"
Infer: meeting-notes also require "title"
```

**Without inference**: Duplicate validation rules
**With inference**: Define class hierarchy, inherit constraints

### 4. Symmetric Relationships
```
Given: noteA relatedTo noteB
Infer: noteB relatedTo noteA
```

**Without inference**: Store both directions
**With inference**: Define `relatedTo` as symmetric

---

## Considered Options

### Option 1: OWL 2 (Web Ontology Language)

**Description**: W3C standard for defining ontologies with formal semantics. Multiple profiles with different expressiveness/complexity tradeoffs.

**Profiles**:
| Profile | Expressiveness | Complexity | Use Case |
|---------|---------------|------------|----------|
| OWL 2 Full | Maximum | Undecidable | Research |
| OWL 2 DL | High | Decidable | General ontologies |
| OWL 2 EL | Medium | Polynomial | Large taxonomies |
| OWL 2 QL | Medium | SQL-reducible | Query answering |
| OWL 2 RL | Medium | Rule-based | Business rules |

**Example** (tag hierarchy):
```turtle
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix kg: <http://kg.local/> .

kg:parentTag a owl:TransitiveProperty ;
    rdfs:domain kg:Tag ;
    rdfs:range kg:Tag .

kg:hasTag a owl:ObjectProperty ;
    rdfs:domain kg:Note ;
    rdfs:range kg:Tag .

# Inference rule (implicit):
# If note hasTag X, and X parentTag Y, then note hasTag Y
```

**Availability**:
| Language | Library | Profile | Maturity |
|----------|---------|---------|----------|
| Rust | horned-owl | OWL 2 (parsing) | ✅ Production |
| Rust | reasonable | OWL 2 RL | ✅ Production |
| Rust | whelk-rs | OWL 2 EL | ✅ Stable |
| Python | owlrl | OWL 2 RL | ✅ Production |
| Python | owlready2 | OWL 2 | ✅ Production |
| JS/TS | — | — | ❌ Limited |

**Pros**:
- W3C standard, well-documented
- Rich expressiveness
- Formal semantics enable verification
- Interoperable with semantic web tools

**Cons**:
- Complex, steep learning curve
- Performance overhead for reasoning
- Overkill for simple hierarchies
- Limited JS/TS support

---

### Option 2: RDFS (RDF Schema)

**Description**: Lightweight vocabulary for RDF. Simpler than OWL, covers common cases.

**Features**:
- `rdfs:subClassOf` - class hierarchy
- `rdfs:subPropertyOf` - property hierarchy
- `rdfs:domain` / `rdfs:range` - type constraints

**Example**:
```turtle
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix kg: <http://kg.local/> .

kg:DevTag rdfs:subClassOf kg:Tag .
kg:RustTag rdfs:subClassOf kg:DevTag .

# Inference: If X is RustTag, then X is also DevTag and Tag
```

**Availability**: Supported by all RDF stores (Oxigraph, rdflib, Quadstore)

**Pros**:
- Simple, easy to understand
- Widely supported
- Low overhead
- Good for class/property hierarchies

**Cons**:
- Limited expressiveness (no transitivity, inverses, etc.)
- Not sufficient for complex inference

---

### Option 3: Datalog Rules

**Description**: Express inference as recursive Datalog rules. Natural fit for CozoDB.

**Example** (tag ancestors):
```datalog
# Base case: direct parent
tag_ancestor[tag, ancestor] := *tags{tag, parent: ancestor}

# Recursive case: transitive closure
tag_ancestor[tag, ancestor] :=
    tag_ancestor[tag, mid],
    *tags{mid, parent: ancestor}

# Inferred membership: note belongs to all ancestor tags
note_in_tag[note, tag] := *note_tags{note, tag}
note_in_tag[note, ancestor] :=
    *note_tags{note, tag},
    tag_ancestor[tag, ancestor]
```

**Example** (inverse links):
```datalog
# Backlinks as inverse of links
backlink[target, source] := *links{from: source, to: target}
```

**Availability**: CozoDB (native), other Datalog engines

**Pros**:
- Native to Datalog databases
- Efficient recursive evaluation
- Same language for queries and rules
- No additional dependencies

**Cons**:
- Tied to Datalog ecosystem
- Not a standard (CozoDB-specific syntax)
- Must manually define each inference rule

---

### Option 4: SPARQL Property Paths

**Description**: Use SPARQL 1.1 property paths for transitive queries without materialized inference.

**Example**:
```sparql
# Find all ancestor tags (transitive closure at query time)
PREFIX kg: <http://kg.local/>
SELECT ?ancestor WHERE {
    kg:tag/dev/rust/async kg:parentTag* ?ancestor .
}

# Find notes in tag hierarchy
SELECT ?note WHERE {
    ?note kg:hasTag/kg:parentTag* kg:tag/dev .
}
```

**Availability**: All SPARQL 1.1 engines (Oxigraph, Quadstore, Comunica)

**Pros**:
- No separate reasoner needed
- Query-time computation (always current)
- Standard SPARQL syntax
- No materialization overhead

**Cons**:
- Computed per-query (may be slower for frequent queries)
- Limited to path patterns (no arbitrary inference)
- Can't define custom inference rules

---

### Option 5: Materialized Views / Triggers

**Description**: Pre-compute inferred facts and store them. Update via triggers on data changes.

**Example** (pseudo-code):
```
ON INSERT note_tag(note, tag):
    FOR ancestor IN get_ancestors(tag):
        INSERT note_tag_computed(note, ancestor)

ON DELETE note_tag(note, tag):
    # Recompute affected entries
```

**Availability**: Any database with trigger support

**Pros**:
- Fast reads (pre-computed)
- No runtime inference overhead
- Works with any storage backend

**Cons**:
- Write amplification
- Complexity in maintaining consistency
- Storage overhead for materialized facts
- Must implement trigger logic

---

### Option 6: No Inference (Query-Time Only)

**Description**: Don't use inference. Handle hierarchies via recursive queries.

**Example** (SPARQL):
```sparql
# Recursive CTE-style query for tag hierarchy
PREFIX kg: <http://kg.local/>
SELECT ?note WHERE {
    ?tag kg:parentTag* kg:tag/dev .
    ?note kg:hasTag ?tag .
}
```

**Pros**:
- Simplest implementation
- No additional dependencies
- No materialization overhead
- Always consistent

**Cons**:
- Recursive queries on every request
- May be slower for deep hierarchies
- No automatic inverse relationships

---

## Comparison Matrix

| Criterion | OWL 2 | RDFS | Datalog | Property Paths | Materialized | None |
|-----------|-------|------|---------|----------------|--------------|------|
| Expressiveness | ✅✅ | ⚠️ | ✅ | ⚠️ | ✅ | ❌ |
| Complexity | ❌ High | ✅ Low | ✅ Medium | ✅ Low | ⚠️ Medium | ✅ None |
| Performance | ⚠️ | ✅ | ✅ | ⚠️ Query-time | ✅ Read | ⚠️ Query-time |
| Standards | ✅ W3C | ✅ W3C | ❌ | ✅ W3C | ❌ | N/A |
| Rust support | ✅ | ✅ | ✅ CozoDB | ✅ Oxigraph | ✅ | ✅ |
| TS support | ❌ | ✅ | ✅ CozoDB | ✅ Quadstore | ✅ | ✅ |
| Python support | ✅ | ✅ | ✅ CozoDB | ✅ rdflib | ✅ | ✅ |

## Ecosystem Alignment

| Approach | Best with Database |
|----------|-------------------|
| OWL 2 | Oxigraph + external reasoner, rdflib + owlrl |
| RDFS | Any RDF store |
| Datalog rules | CozoDB |
| Property paths | Oxigraph, Quadstore (SPARQL stores) |
| Materialized | Any with triggers |
| None | Any |

## Recommendation

**For kg's use cases** (tag hierarchies, backlinks), full OWL reasoning is overkill.

**Recommended approach by database choice**:

| Database | Recommended Inference |
|----------|----------------------|
| CozoDB | Datalog rules (native, efficient) |
| Oxigraph | SPARQL property paths (simple) or RDFS (if more needed) |
| Quadstore | SPARQL property paths |
| Custom | Materialized views or none |

**Start simple**: Use property paths or Datalog rules. Add OWL/RDFS only if we need interoperability with external semantic web tools.

## Related Research

- [Languages](./languages.md)
- [Graph Databases](./graph-databases.md)
- [Validation](./validation.md)
