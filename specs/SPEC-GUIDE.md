# Specification Guide

This project follows **Spec-Driven Development (SDD)**: specifications are the source of truth, code is derived from them.

## Directory Structure

```
specs/
├── SPEC-GUIDE.md          # This file - how to write specs
├── PRODUCT.md             # High-level product vision and requirements
├── features/              # Feature specifications (one per feature)
│   └── *.md
├── contracts/             # Interface contracts, data schemas, plugin APIs
│   └── *.md
├── research/              # Exploration and analysis (NOT decisions)
│   └── *.md
└── decisions/             # Architecture Decision Records (ADRs)
    └── ADR-NNN-*.md
```

## Research vs Decisions

**Research documents** (`specs/research/`) contain exploration and analysis:
- Evaluate options without committing to a choice
- Compare tradeoffs, list pros/cons
- Living documents that evolve as we learn more
- NOT numbered, can be freely edited

**Decision records** (`specs/decisions/`) capture actual choices:
- Record a decision that has been made (or is being made)
- Numbered sequentially (ADR-001, ADR-002, ...)
- Reference research docs for supporting analysis
- Immutable once accepted (supersede rather than edit)

**Example flow:**
1. Create `research/databases.md` to explore database options
2. Create `research/languages.md` to explore language options
3. When ready to decide, create `ADR-001-stack.md` that references both

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

### ADR (Architecture Decision Record) Template

```markdown
# ADR-NNN: [Title]

**Status**: [Proposed | Accepted | Deprecated | Superseded by ADR-XXX]
**Date**: YYYY-MM-DD
**Deciders**: [List of people involved]

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
- Links to research docs, other ADRs, or specs
```

---

## Workflow

1. **Discover** → Write/refine specs through discussion
2. **Research** → Explore options in `research/` docs
3. **Decide** → Record choices in `decisions/` ADRs
4. **Implement** → Generate/write code from specs
5. **Validate** → Tests verify spec compliance
6. **Iterate** → Specs evolve, code follows
