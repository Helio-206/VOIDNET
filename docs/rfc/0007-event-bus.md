# RFC-0007 Event Bus

Status: draft  
Objective: define typed event architecture across VOIDNET layers  
Scope: event vocabulary, propagation classes, observability, causality, and distributed traces

## Objective

The Event Bus defines how VOIDNET observes and reacts to state transitions across transport, identity, DNS, runtime, and distributed state. It is a local and selectively distributed event discipline, not a global broker.

## Scope

Included:

- Event names and payload direction.
- Local-only and distributed event classes.
- Causality metadata.
- Correlation metadata.
- Propagation boundaries.
- Observability and tracing.

Excluded:

- Arbitrary application message broadcast.
- Unbounded network flooding.
- Centralized logging service.

## Protocol Assumptions

- Events are typed.
- Events are emitted at layer boundaries.
- Critical distributed events may require signatures.
- Propagation class controls network spread.
- Event traces are useful for security and recovery.

## Future Considerations

- Event schema registry.
- Signed distributed events.
- Replay-safe event persistence.
- Trace export format.
- Runtime permission audit integration.

