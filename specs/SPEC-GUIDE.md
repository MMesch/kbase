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
└── decisions/             # Architecture Decision Records (ADRs)
    └── ADR-*.md
```

## Writing Effective Specifications

### Principles

1. **Behavior over implementation**: Describe *what*, not *how*
2. **Domain language**: Use business/user terminology, not code jargon
3. **Deterministic**: Unambiguous enough that two developers would build the same thing
4. **Testable**: Every requirement maps to a verifiable outcome

### Specification Template

Each feature spec should include:

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

For interfaces and APIs:

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

## Workflow

1. **Discover** → Write/refine specs through discussion
2. **Review** → Human approval before implementation
3. **Implement** → Generate/write code from specs
4. **Validate** → Tests verify spec compliance
5. **Iterate** → Specs evolve, code follows
