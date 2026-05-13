# RFC 0001: VOIDNET Network Foundation

Status: draft  
Author: VOIDNET contributors  
Created: 2026-05-11

## Summary

Define the initial foundation for VOIDNET as a peer-to-peer runtime ecosystem above existing internet transport.

## Motivation

VOIDNET needs a small but real systems foundation before application work begins. The first architecture must prioritize correct boundaries: identity, transport, protocol, DNS, runtime, and storage.

## Proposal

Use Rust for network-critical components and Go for infrastructure tooling. The Phase 1 node uses libp2p over QUIC, ed25519 identities, a typed VOID Protocol envelope, and local-first `.void` DNS records with a path to distributed lookup.

## Component Decisions

- Transport: libp2p with QUIC.
- Runtime: Tokio async event loop.
- Protocol: typed v1 envelope and frames.
- Identity: ed25519 signing keys with deterministic peer ids.
- Storage: sled namespace store.
- DNS: local TTL cache first, DHT lookup next.

## Consequences

This keeps the project low-level and protocol-oriented. Application features depend on runtime and protocol correctness instead of building around web app conventions.

## Alternatives Rejected

- HTTP-first service APIs: too close to ordinary web backend design.
- Blockchain-based naming: unnecessary consensus weight for Phase 1.
- Browser clone UI: not aligned with a minimal native VOID runtime.
- JSON-only protocol: easy to prototype, but weak for binary efficiency and typed streaming.

## Open Work

- Canonical binary layout.
- Envelope signatures.
- x25519 session establishment.
- DHT key format.
- Record conflict policy.
- VOID UI parser implementation.

