# Specification Guide

This project follows **Spec-Driven Development (SDD)**: specifications are the source of truth, code is derived from them.

## Reading Order and Structure

Start with [PRODUCT.md](PRODUCT.md) for the overall vision, then explore by category:

### Contracts
Data schemas and interface definitions:
- [note-format.md](contracts/note-format.md) - Note file format specification
- [plugin-api.md](contracts/plugin-api.md) - Extension points (future)

### Features
Feature specifications (one per feature):
- [vault-init.md](features/vault-init.md) - Vault initialization
- [note-crud.md](features/note-crud.md) - Create, read, update, delete notes
- [search-query.md](features/search-query.md) - Search and query capabilities
- [metadata-graph.md](features/metadata-graph.md) - Graph operations and queries
- [lsp-server.md](features/lsp-server.md) - Editor integration (completion, validation, navigation)

### Research
Exploration and analysis (not decisions):
- [ontology.md](research/ontology.md) - Ontology modeling approaches
- [inference.md](research/inference.md) - Inference strategies
- [validation.md](research/validation.md) - Validation approaches
- [languages.md](research/languages.md) - Language options analysis
- [graph-databases.md](research/graph-databases.md) - Database options comparison

### Decisions
- [decision-001-stack.md](decisions/decision-001-stack.md) - Technology stack decision

---

## Folder Purposes

**Contracts** (`specs/contracts/`) define interfaces and data schemas:
- File formats, API contracts, plugin interfaces
- Precise, versioned specifications
- Focus on structure and validation rules

**Features** (`specs/features/`) describe user-facing functionality:
- One file per feature
- Behavior-focused with scenarios (Given/When/Then)
- What the system does, not how it's implemented

**Research** (`specs/research/`) contains exploration and analysis:
- Evaluate options without committing to a choice
- Compare tradeoffs, list pros/cons
- Living documents that evolve as we learn more

**Decisions** (`specs/decisions/`) capture actual choices:
- Record a decision that has been made
- Numbered sequentially (decision-001, decision-002, ...)
- Reference research docs for supporting analysis
- Immutable once accepted (supersede rather than edit)

**Example flow:**
1. Create `research/databases.md` to explore database options
2. Create `research/languages.md` to explore language options
3. When ready to decide, create `decision-001-stack.md` that references both

---

## Writing Effective Specifications

### Principles

1. **Behavior over implementation**: Describe *what*, not *how*
2. **Domain language**: Use business/user terminology, not code jargon
3. **Deterministic**: Unambiguous enough that two developers would build the same thing
4. **Testable**: Every requirement maps to a verifiable outcome

### Feature Specification Template

```markdown
# Feature: [Name]

## Purpose
Why this feature exists. What problem it solves.

## Preconditions
What must be true before this feature can operate.

## Behavior

### Scenario: [Name]
**Given** [initial context]
**When** [action occurs]
**Then** [expected outcome]

## Constraints
- Performance requirements
- Security boundaries
- Compatibility requirements

## Out of Scope
Explicitly state what this feature does NOT do.
```

### Contract Template

```markdown
# Contract: [Name]

## Purpose
What this contract defines and why.

## Interface

### Input
- Field definitions with types and constraints
- Validation rules

### Output
- Return structure
- Error conditions

## Invariants
Rules that must always hold true.

## Versioning
How this contract evolves over time.
```

### Decision Template

```markdown
# Decision NNN: [Title]

**Status**: [Proposed | Accepted | Deprecated | Superseded by decision-XXX]
**Date**: YYYY-MM-DD

## Context
What is the issue that we're seeing that is motivating this decision?

## Decision
We will use **[chosen option]** because [rationale].

## Consequences

### Positive
- [Benefit 1]
- [Benefit 2]

### Negative
- [Tradeoff 1]
- [Tradeoff 2]

## Related
- Links to research docs, other decisions, or specs
```

---

## Workflow

1. **Discover** → Write/refine specs through discussion
2. **Research** → Explore options in `research/` docs
3. **Decide** → Record choices in `decisions/`
4. **Implement** → Generate/write code from specs
5. **Validate** → Tests verify spec compliance
6. **Iterate** → Specs evolve, code follows
