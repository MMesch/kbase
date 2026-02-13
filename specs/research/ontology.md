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

## Why Use Ontologies Instead of Simpler Approaches?

Ontologies provide significant advantages over simple tag hierarchies or basic validation:

### 1. Semantic Richness vs. Simple Tags

**Simple Tag Hierarchy:**
```yaml
tags: [dev/rust, dev/go]
```
- Just strings with slash notation
- No formal meaning or relationships
- Hard to validate or reason about

**Ontology Approach:**
```turtle
dev:rust rdfs:subClassOf dev: .
dev:go rdfs:subClassOf dev: .
```
- **Formal class hierarchy**: This Turtle syntax (RDF format) formally defines that `dev:rust` is a subclass of `dev:`
- **Machine-readable relationships**: Computers understand this isn't just a string, but a semantic relationship
- **Automatic reasoning**: If a note is tagged `dev/rust`, the system automatically knows it's also `dev`
- **Example**: Querying for all `dev` notes will automatically include `dev/rust` and `dev/go` notes
- **Contrast with simple tags**: Without ontologies, you'd need manual string parsing or recursive queries to achieve this

### 2. Constraint Inheritance vs. Duplicate Validation

**Term Definitions:**
- **Constraint Inheritance**: Rules defined for parent classes automatically apply to child classes
- **Duplicate Validation**: Having to repeat the same validation rules for multiple similar types

**Simple Validation (Duplicate Rules):**
```yaml
# schema.yaml
Note:
  required: [title, created, modified]
MeetingNote:
  required: [title, created, modified, attendees, date]  # Duplication!
```

**Ontology Approach (Inheritance):**
```turtle
kbase:MeetingNote rdfs:subClassOf kbase:Note .
kbase:Note kbase:requires kbase:title, kbase:created, kbase:modified .
kbase:MeetingNote kbase:requires kbase:attendees, kbase:date .
```
- **Constraints inherited automatically**: `MeetingNote` gets all `Note` requirements plus its own
- **No duplication**: Define requirements once for parent class
- **Easier maintenance**: Change parent class constraints, all children update automatically

**Composition vs Inheritance:**
- **Inheritance (shown)**: Child classes extend parent classes (`MeetingNote` is-a `Note`)
- **Composition**: Classes combine multiple components (like typeclasses)
- **Ontologies support both**: Can use inheritance for hierarchies and composition for modular design

### 3. Relationship Typing vs. Ad-Hoc Links

**Simple Links:**
```markdown
[[note-a]] links to [[note-b]]  # What kind of link?
```

**Ontology Approach:**
```turtle
kbase:linksTo a owl:ObjectProperty ;
    rdfs:domain kbase:Note ;
    rdfs:range kbase:Note ;
    rdfs:subPropertyOf kbase:relatedTo .

kbase:hasBacklink owl:inverseOf kbase:linksTo .
```
- Explicit relationship types
- Domain/range constraints
- Automatic inverse relationships

### 4. Interoperability and Standards

**Simple Approach:**
- Custom formats
- Hard to integrate with other tools
- Vendor lock-in

**Ontology Approach:**
- W3C standards (RDF, RDFS, OWL)
- Works with semantic web tools
- Data portability

### 5. Advanced Query Capabilities

**With Ontologies You Can:**
- Find all notes related through any relationship type
- Query by semantic meaning, not just string matching
- Use reasoning to find implicit connections
- Validate complex constraints automatically

**Simple Approaches Require:**
- Manual query construction for each case
- No semantic understanding
- Explicit storage of all relationships

## Comparison: Ontologies vs. Simple Approaches

| Feature | Simple Tags/Validation | Ontology Approach |
|---------|------------------------|-------------------|
| Hierarchy | Manual string parsing | Formal class hierarchy |
| Validation | Duplicate rules | Inherited constraints |
| Relationships | Ad-hoc links | Typed relationships |
| Inference | Manual queries | Automatic reasoning |
| Standards | Custom formats | W3C standards |
| Maintenance | High (duplication) | Low (inheritance) |
| Query Power | Basic | Advanced semantic queries |
| Tool Integration | Limited | Semantic web ecosystem |
| Data Portability | Low | High |

## Use Cases for Ontologies in kbase

### 1. Concept Hierarchies
```
Note → MeetingNote → ProjectMeetingNote
     → ResearchNote → LiteratureReview
```

**Advantage over simple tags:** Formal class hierarchy enables automatic constraint inheritance and type checking.

**Detailed Example:**
```turtle
# Define base Note requirements
kbase:Note kbase:requires kbase:title, kbase:created, kbase:modified .

# MeetingNote inherits Note requirements AND adds its own
kbase:MeetingNote rdfs:subClassOf kbase:Note .
kbase:MeetingNote kbase:requires kbase:attendees, kbase:date .

# Result: MeetingNotes automatically require:
# title, created, modified (inherited from Note)
# attendees, date (specific to MeetingNote)
```

**Without ontologies**: You'd need to list all 5 requirements for every note type, leading to duplication and maintenance issues.

### 2. Tag Taxonomies
```
dev → rust → async
    → go → concurrency
    → web → frontend
```

**Advantage over simple tags:** Machine-readable hierarchy enables automatic inference (e.g., dev/rust implies dev).

**Graph Query vs Ontology Inference:**

**With Simple Graph Queries:**
```sparql
# Manual recursive query needed
SELECT ?note WHERE {
  ?note kg:hasTag kg:dev/rust .
  # Would need UNION with dev/go, dev/web, etc.
}
```

**With Ontology Inference:**
```sparql
# Automatic - system knows dev/rust implies dev
SELECT ?note WHERE {
  ?note kg:hasTag kg:dev .
  # Automatically includes dev/rust, dev/go, etc.
}
```

**Key Difference**: Ontologies handle the hierarchy logic automatically, while simple queries require manual specification of all cases.

### 3. Relationship Types
```
linksTo (Note → Note)
hasAuthor (Note → Person)
hasTag (Note → Tag)
```

**Advantage over ad-hoc links:** Typed relationships with domain/range constraints prevent invalid connections.

### 4. Constraint Inheritance
```
All Notes require: title, created, modified
MeetingNotes additionally require: attendees, date
ResearchNotes additionally require: sources, keywords
```

**Advantage over duplicate validation:** Single definition of constraints that apply to all subclasses automatically.

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
