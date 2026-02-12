# Validation Strategies

Research on validation approaches for kg note frontmatter and graph constraints.

See: [ADR-001: Stack Decision](../decisions/ADR-001-stack.md)

## Requirements

kg notes have structured frontmatter (YAML) that must conform to a schema:
- Enforce required fields (title, created, modified)
- Validate field types (string, datetime, list)
- Check constraints (tag hierarchy rules, link integrity)
- Provide actionable error messages

## Evaluation Criteria

- **Correctness**: Catch invalid data before it enters the graph
- **Error quality**: Clear, actionable messages for users
- **Performance**: Validation should not slow down note operations
- **Extensibility**: Users can add custom fields and rules
- **Standards**: Prefer W3C/industry standards when beneficial
- **Portability**: Schema definitions should be human-readable

## Considered Options

### Option 1: SHACL (Shapes Constraint Language)

**Description**: W3C standard for validating RDF graphs using "shapes" that define constraints.

**Availability**:
- Rust: `rudof` crate (SHACL + ShEx), `shacl_validation` crate
- TypeScript: `rdf-validate-shacl` package
- Go: Limited support
- Haskell: No native implementation

**Example** (note shape):
```turtle
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix kg: <http://kg.local/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

kg:NoteShape a sh:NodeShape ;
    sh:targetClass kg:Note ;
    sh:property [
        sh:path kg:title ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:message "Note must have exactly one title" ;
    ] ;
    sh:property [
        sh:path kg:created ;
        sh:datatype xsd:dateTime ;
        sh:minCount 1 ;
        sh:message "Note must have a created timestamp" ;
    ] .
```

**Pros**:
- W3C standard, well-documented
- Expressive constraint language
- Generates detailed validation reports
- Portable schema definitions (Turtle format)
- Good for RDF/SPARQL ecosystems (Oxigraph, Quadstore)

**Cons**:
- Tied to RDF data model
- Additional dependency
- May be overkill for simple schemas
- Performance overhead for complex shapes

---

### Option 2: Datalog Constraints

**Description**: Use Datalog rules to express and check constraints. Natural fit for CozoDB.

**Availability**:
- CozoDB: Native constraint support via rules
- Other Datalog engines: Similar patterns

**Example** (validation rules):
```datalog
# Check: Every note must have a title
violation[note, "missing title"] :=
    *notes{id: note},
    not *note_fields{note, field: "title"}

# Check: Tags must form a tree (no cycles)
ancestor[tag, parent] := *tags{tag, parent}
ancestor[tag, anc] := ancestor[tag, mid], *tags{mid, parent: anc}

violation[tag, "cyclic tag hierarchy"] :=
    ancestor[tag, tag]

# Run validation
?[note, error] := violation[note, error]
```

**Pros**:
- Native to Datalog databases (CozoDB)
- Recursive rules natural for graph constraints
- No additional dependency
- Same language for queries and validation

**Cons**:
- Tied to Datalog ecosystem
- Less familiar syntax
- No standard format (CozoDB-specific)
- Error messages require custom formatting

---

### Option 4: SPARQL ASK Queries

**Description**: Use SPARQL ASK queries to check for constraint violations.

**Availability**:
- Any SPARQL engine (Oxigraph, Quadstore, Comunica)

**Example**:
```sparql
# Check: Note has title
ASK WHERE {
    <http://kg.local/note/abc123> a kg:Note .
    FILTER NOT EXISTS {
        <http://kg.local/note/abc123> kg:title ?title .
    }
}
# Returns true if violation exists

# Check: No cyclic tags
ASK WHERE {
    ?tag kg:parentTag+ ?tag .
}
# Returns true if cycle exists
```

**Pros**:
- Uses existing SPARQL engine
- No additional dependencies
- Flexible and expressive

**Cons**:
- Awkward for complex validation logic
- Each constraint = separate query
- Error aggregation requires custom code
- Not a standard validation framework

---

### Option 5: JSON Schema / Custom Schema

**Description**: Define schema in a custom format (YAML/JSON) and validate with native code.

**Availability**:
- All languages have JSON Schema validators
- Custom schema = custom implementation

**Example** (schema.yaml):
```yaml
required:
  - title
  - created
  - modified

fields:
  title:
    type: string
    minLength: 1
    maxLength: 200
  created:
    type: datetime
    immutable: true
  modified:
    type: datetime
  tags:
    type: list
    items:
      type: string
      pattern: "^[a-z0-9]+(/[a-z0-9]+)*$"
  status:
    type: enum
    values: [draft, review, published]
    default: draft
```

**Pros**:
- Human-readable schema format
- No external dependencies
- Full control over validation logic
- Fast (native code)
- Language-agnostic schema definition

**Cons**:
- Custom implementation required
- Need to define constraint language
- No standard error format
- Limited expressiveness for graph constraints

---

### Option 6: Hybrid Approach

**Description**: Combine approaches for different validation layers.

**Layers**:
1. **Schema validation** (fast, native): Required fields, types, formats
2. **Graph constraints** (query-based): Link integrity, tag hierarchy
3. **Optional SHACL** (standard): For interoperability/export

**Example architecture**:
```
Note input
    │
    ▼
┌──────────────────┐
│ 1. Schema Check  │  ← Native code, JSON Schema-style
│    (fast path)   │     Validates: types, required, format
└────────┬─────────┘
         │ valid
         ▼
┌──────────────────┐
│ 2. Graph Insert  │  ← Database insert
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│ 3. Constraints   │  ← Datalog rules or SPARQL ASK
│    (optional)    │     Validates: links, hierarchy
└────────┬─────────┘
         │ valid
         ▼
┌──────────────────┐
│ 4. SHACL Export  │  ← Optional, for interop
│    (on demand)   │
└──────────────────┘
```

**Pros**:
- Fast path for common cases
- Full power for complex constraints
- Standards compliance when needed
- Separation of concerns

**Cons**:
- More complex architecture
- Multiple validation systems to maintain
- Schema defined in multiple places

---

## Comparison Matrix

| Criterion | SHACL | OWL | Datalog | SPARQL ASK | Custom | Hybrid |
|-----------|-------|-----|---------|------------|--------|--------|
| Standards | ✅ W3C | ✅ W3C | ❌ | ⚠️ | ❌ | ⚠️ |
| Performance | ⚠️ | ❌ | ✅ | ✅ | ✅✅ | ✅ |
| Error quality | ✅ | ⚠️ | ⚠️ | ❌ | ✅ | ✅ |
| Expressiveness | ✅ | ✅✅ | ✅ | ⚠️ | ⚠️ | ✅ |
| Dependencies | ⚠️ | ❌ | ✅ | ✅ | ✅✅ | ⚠️ |
| Learning curve | Medium | High | Medium | Low | Low | Medium |
| RDF fit | ✅✅ | ✅✅ | ⚠️ | ✅ | ❌ | ✅ |
| Datalog fit | ❌ | ❌ | ✅✅ | ❌ | ⚠️ | ✅ |

## Ecosystem Alignment

| Validation | Best with Database |
|------------|-------------------|
| SHACL | Oxigraph, Quadstore (RDF stores) |
| Datalog | CozoDB |
| SPARQL ASK | Any SPARQL engine |
| Custom | Any |
| Hybrid | Any |

## Evaluation Plan

1. **Prototype Custom Schema validation**
   - Implement fast-path validation in native code
   - Define schema format (YAML)
   - Measure validation time per note

2. **Prototype SHACL validation** (if using RDF store)
   - Define shapes for notes and tags
   - Test with `rudof` or `shacl_validation`
   - Compare error message quality

3. **Prototype Datalog constraints** (if using CozoDB)
   - Define validation rules
   - Test recursive constraint checking
   - Measure query performance

4. **Compare approaches**
   - Validation time (1000 notes)
   - Error message quality
   - Schema maintainability
   - Dependency cost

## Leaning Toward

**Hybrid approach** with:
- **Custom schema** for fast validation (types, required fields)
- **Database-native constraints** for graph rules (Datalog or SPARQL)
- **SHACL shapes as documentation** (machine-readable schema spec)

This gives us:
- Speed for the common case
- Power for complex constraints
- Standards compliance for interoperability

## Related Research

- [Languages](./languages.md)
- [Graph Databases](./graph-databases.md)
- [Ontology](./ontology.md)
- [Inference](./inference.md)

## References

- [SHACL W3C Specification](https://www.w3.org/TR/shacl/)
- [Rudof - Rust RDF Shapes](https://github.com/rudof-project/rudof)
- [CozoDB Constraints](https://docs.cozodb.org/)
- [JSON Schema](https://json-schema.org/)
