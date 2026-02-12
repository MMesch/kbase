# Ontology Modeling

Research on ontology approaches for kbase knowledge representation and organization.

See: [ADR-001: Stack Decision](../decisions/ADR-001-stack.md)

## Overview

Ontologies provide formal definitions of concepts, relationships, and constraints in kbase. They enable:
- **Structured organization**: Hierarchical classification of notes and concepts
- **Semantic relationships**: Explicit definition of how entities relate
- **Interoperability**: Standardized representations for data exchange
- **Constraint definition**: Rules for data validation and integrity

## Evaluation Criteria

- **Expressiveness**: Ability to model kbase's domain (notes, tags, links, fields)
- **Complexity**: Balance between power and implementation difficulty
- **Ecosystem fit**: Compatibility with chosen database and language stack
- **Performance**: Impact on common operations (note creation, querying)
- **Standards compliance**: Use of established W3C standards when beneficial

## Use Cases for Ontologies in kbase

### 1. Concept Hierarchies
```
Note → MeetingNote → ProjectMeetingNote
     → ResearchNote → LiteratureReview
```

### 2. Tag Taxonomies
```
dev → rust → async
    → go → concurrency
    → web → frontend
```

### 3. Relationship Types
```
linksTo (Note → Note)
hasAuthor (Note → Person)
hasTag (Note → Tag)
```

### 4. Constraint Inheritance
```
All Notes require: title, created, modified
MeetingNotes additionally require: attendees, date
ResearchNotes additionally require: sources, keywords
```

## Ontology Approaches

### Option 1: OWL 2 (Web Ontology Language)

**Description**: W3C standard for defining ontologies with rich expressiveness.

**Profiles** (trade-offs between expressiveness and computability):
- **OWL 2 Full**: Maximum expressiveness, undecidable
- **OWL 2 DL**: High expressiveness, decidable, general ontologies
- **OWL 2 EL**: Medium expressiveness, polynomial complexity, large taxonomies
- **OWL 2 QL**: Medium expressiveness, SQL-reducible, query answering
- **OWL 2 RL**: Medium expressiveness, rule-based, business rules

**Rust Ecosystem**:
- `horned-owl`: OWL 2 parsing and manipulation
- `reasonable`: OWL 2 RL reasoning engine
- `whelk-rs`: OWL 2 EL reasoner

**Pros**:
- Industry standard with extensive tooling
- Rich expressiveness for complex domains
- Interoperability with semantic web tools
- Formal semantics and well-defined reasoning

**Cons**:
- Steep learning curve
- Complex implementation
- Potential performance overhead
- May be overkill for kbase's current needs

### Option 2: RDFS (RDF Schema)

**Description**: Lightweight ontology language for RDF, subset of OWL.

**Key Features**:
- Class and property hierarchies (`rdfs:subClassOf`, `rdfs:subPropertyOf`)
- Domain and range constraints
- Simple typing system

**Pros**:
- Simpler than OWL, easier to implement
- Good balance of expressiveness and complexity
- Native support in RDF stores (Oxigraph, Quadstore)
- Sufficient for basic hierarchies and constraints

**Cons**:
- Limited expressiveness compared to OWL
- No advanced reasoning capabilities
- May need extension for complex constraints

### Option 3: Custom Schema Language

**Description**: kbase-specific schema definition language.

**Example**:
```yaml
# .kbase/schema.yaml
classes:
  Note:
    fields:
      title: { type: string, required: true }
      created: { type: datetime, auto: true }
    subclasses: [MeetingNote, ResearchNote]
  
  MeetingNote:
    fields:
      attendees: { type: list, items: string }
      date: { type: date }
  
relations:
  linksTo: { domain: Note, range: Note, inverse: hasBacklink }
  hasTag: { domain: Note, range: Tag }
```

**Pros**:
- Tailored to kbase's specific needs
- Simple and intuitive for users
- No external dependencies
- Easy to extend and modify

**Cons**:
- No standardization
- Limited interoperability
- Need to implement validation and reasoning from scratch

## Recommendation

**Start with RDFS**: Provides sufficient expressiveness for kbase's current needs (tag hierarchies, basic constraints) with good ecosystem support. Can be extended with OWL later if needed.

**Implementation approach**:
1. Use RDFS for basic class hierarchies and property definitions
2. Store ontology in `.kbase/ontology.ttl` (Turtle format)
3. Leverage native RDFS support in chosen RDF store
4. Add OWL constructs incrementally only when specific needs arise

**Future extension path**:
- Add OWL 2 RL for rule-based reasoning if needed
- Consider SHACL for advanced constraints (covered in validation research)
- Explore custom extensions for kbase-specific features

## References

- [OWL 2 Web Ontology Language](https://www.w3.org/TR/owl2-overview/)
- [RDF Schema 1.1](https://www.w3.org/TR/rdf-schema/)
- [Rust RDF Crates](https://crates.io/keywords/rdf)