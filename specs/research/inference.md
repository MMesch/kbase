---
title: Inference Strategies
tags:
  - spec/research
status: complete
---
# Inference Strategies

Research on inference approaches for deriving new knowledge from existing data in kbase.

See: [Decision 001: Stack Decision](../decisions/decision-001-stack.md)

## Overview

Inference automatically derives new facts from existing data, reducing manual work and enabling advanced features. For kbase, inference can:

- **Reduce redundancy**: Store only leaf tags, infer ancestor membership
- **Maintain consistency**: Automatically update backlinks when links change
- **Enforce constraints**: Validate data based on type hierarchies
- **Enable smart queries**: Find related notes through inferred relationships

## Evaluation Criteria

- **Value**: Does inference solve real problems or just add complexity?
- **Performance**: Must not significantly slow down common operations
- **Implementation complexity**: Balance power against development effort
- **Ecosystem fit**: Compatibility with chosen ontology and database stack
- **Predictability**: Users should understand what gets inferred

## Use Cases for Inference in kbase

### 1. Tag Hierarchy (Transitive Closure)
```
Given: note tagged "dev/rust/async"
Infer: note is also member of "dev/rust" and "dev"
```

**Without inference**: Store all ancestor tags explicitly or use recursive queries
**With inference**: Store only leaf tag, reasoner derives ancestors automatically

### 2. Inverse Relationships
```
Given: noteA linksTo noteB
Infer: noteB hasBacklink noteA
```

**Without inference**: Maintain backlinks manually or query inverse relationships
**With inference**: Define `hasBacklink` as inverse of `linksTo`, automatically maintained

### 3. Symmetric Relationships
```
Given: noteA relatedTo noteB
Infer: noteB relatedTo noteA
```

**Without inference**: Store both directions explicitly
**With inference**: Define `relatedTo` as symmetric, single assertion suffices

### 4. Property Chains
```
Given: noteA hasAuthor personX
      personX hasAffiliation orgY
Infer: noteA hasAffiliation orgY
```

**Without inference**: Manual property traversal or complex queries
**With inference**: Automatic property chain reasoning

## Inference Approaches

### Option 1: OWL 2 Reasoning

**Description**: Use OWL 2 reasoning profiles for automatic inference.

**Profiles relevant to kbase**:
- **OWL 2 RL**: Rule-based, good for business rules and hierarchies
- **OWL 2 EL**: Optimized for large taxonomies (tag hierarchies)
- **OWL 2 QL**: Query-oriented, good for database-backed systems

**Rust Ecosystem**:
- `reasonable`: OWL 2 RL reasoner
- `whelk-rs`: OWL 2 EL reasoner
- `horned-owl`: OWL 2 parsing (can integrate with external reasoners)

**Pros**:
- Standardized approach with well-defined semantics
- Rich expressiveness for complex inference patterns
- Interoperability with semantic web tools
- Formal correctness guarantees

**Cons**:
- Complex implementation and integration
- Potential performance overhead
- Steep learning curve
- May be overkill for kbase's needs

### Option 2: RDFS Entailment

**Description**: Basic inference using RDF Schema semantics.

**Supported inferences**:
- Subclass propagation (`rdfs:subClassOf`)
- Subproperty propagation (`rdfs:subPropertyOf`)
- Domain and range inference
- Simple class membership

**Pros**:
- Native support in RDF stores (Oxigraph, Quadstore)
- Simple to implement and understand
- Good performance characteristics
- Sufficient for basic hierarchies

**Cons**:
- Limited to basic schema-level inference
- No support for complex rules
- May need supplementation for advanced use cases

### Option 3: Datalog Rules

**Description**: Express inference as recursive Datalog rules (natural fit for CozoDB).

**Example** (tag hierarchy):
```datalog
# Tag hierarchy inference
tag_ancestor(?tag, ?ancestor) :- 
    tag_parent(?tag, ?ancestor).

tag_ancestor(?tag, ?ancestor) :- 
    tag_parent(?tag, ?intermediate),
    tag_ancestor(?intermediate, ?ancestor).

# Note inherits ancestor tags through its tags
note_has_tag(?note, ?tag) :- 
    note_tag(?note, ?tag).

note_has_tag(?note, ?ancestor) :- 
    note_tag(?note, ?tag),
    tag_ancestor(?tag, ?ancestor).
```

**Pros**:
- Natural fit for Datalog-based databases (CozoDB)
- Explicit and understandable rule definitions
- Good performance for recursive patterns
- Flexible and extensible

**Cons**:
- Must manually define each inference rule
- No standardization (kbase-specific rules)
- Requires Datalog expertise

### Option 4: SPARQL Property Paths

**Description**: Use SPARQL 1.1 property paths for transitive queries without materialized inference.

**Example** (find all notes under "dev" tag):
```sparql
SELECT ?note WHERE {
  ?note kg:hasTag/kg:subTag* kg:dev .
}
```

**Pros**:
- No materialized inference needed
- Standard SPARQL 1.1 feature
- Good performance in optimized stores
- Flexible query patterns

**Cons**:
- Inference only at query time (not materialized)
- Limited to path patterns (no arbitrary inference)
- Complex queries for advanced inference

### Option 5: Materialized Inference

**Description**: Pre-compute inferences and store as explicit facts.

**Approach**:
1. Run inference engine on data changes
2. Store inferred facts alongside explicit data
3. Query both explicit and inferred facts uniformly

**Pros**:
- Fast query performance (inferences pre-computed)
- Simple query patterns
- Works with any database

**Cons**:
- Storage overhead for inferred facts
- Must handle inference updates on data changes
- Potential consistency issues

### Option 6: No Inference

**Description**: Handle hierarchies and relationships through explicit queries.

**Approach**:
- Use recursive queries for hierarchies
- Maintain inverse relationships manually
- Validate constraints through explicit checks

**Pros**:
- Simplest implementation
- No inference overhead
- Predictable behavior
- Easy to debug

**Cons**:
- More manual work for users
- Complex queries for common patterns
- Potential data inconsistency

## Recommendation

**Start with SPARQL Property Paths + Limited Materialization**:

1. **Primary approach**: Use SPARQL 1.1 property paths for transitive queries (tag hierarchies, etc.)
2. **Supplementary**: Materialize simple inferences (backlinks) for performance
3. **Future extension**: Add Datalog rules if using CozoDB, or OWL RL if complex inference needed

**Implementation phases**:
- **Phase 1**: SPARQL property paths for basic transitive queries
- **Phase 2**: Materialize backlinks and simple inverses
- **Phase 3**: Add Datalog rules if using CozoDB
- **Phase 4**: Consider OWL RL only if specific complex inference needs arise

**Decision factors**:
- If using **Oxigraph/Quadstore**: SPARQL property paths + RDFS entailment
- If using **CozoDB**: Datalog rules for inference
- If needing **complex inference**: OWL 2 RL with reasonable crate

## Performance Considerations

- **Query-time inference**: Slower queries but no storage overhead
- **Materialized inference**: Faster queries but storage and update overhead
- **Hybrid approach**: Materialize common inferences, compute rare ones on-demand

## References

- [OWL 2 Web Ontology Language](https://www.w3.org/TR/owl2-overview/)
- [SPARQL 1.1 Property Paths](https://www.w3.org/TR/sparql11-query/#propertyPaths)
- [Datalog Educational Resources](https://www.learndatalogtoday.org/)
- [RDF Semantics](https://www.w3.org/TR/rdf11-mt/)