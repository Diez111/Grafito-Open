# Grafito Foundation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Convert the audit findings into a safe, test-driven delivery sequence that first establishes mathematical and resource-safety guarantees.

**Architecture:** Phase 0 makes targeted corrections behind current public APIs. Phase 1 introduces typed contracts and transactions behind adapters. Product expansion depends on the contracts rather than multiplying parser, renderer and UI paths.

**Tech Stack:** Rust 2021, serde, egui/eframe, wgpu, nalgebra, Criterion, cargo test.

---

The executable task backlog, dependencies, statuses and DoDs are the tables in `docs/Plans.md`. This document records the operating constraints:

1. Reserve `commands.rs`, `ast.rs`, `object.rs` and app/UI files to one worker each until the existing local work is integrated.
2. Start independent work only in clean files: `statistics.rs`, `validation.rs`, dedicated test files and new documentation.
3. Every P0 correction starts with a failing regression test and ends with the smallest behavior-preserving implementation.
4. A command error is atomic: no objects, variables, constraints, style changes or cache-invalidating document mutations may escape.
5. Error handling replaces fabricated values. Optional arguments may default only when omitted, never when malformed.
6. Run `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release` after each integrated batch.

## Initial execution order

1. Resolve ownership of the existing dirty working tree.
2. Execute `0.2`, split into mathematical, resource and command-atomicity red tests.
3. Execute `0.3`, `0.4` and `0.5` sequentially where they touch shared source files.
4. Integrate through `0.6` before beginning Phase 1.

## Explicit implementation boundaries

- The P0 transaction wrapper may clone `Document` as a transitional correctness measure; replace snapshots with operations only in task `1.3`.
- `Script` becomes atomic in Phase 0; a future best-effort mode must be explicit and separately named.
- The existing hyperbolic safety edits in `ast.rs` and `expr.rs` are retained and extended; they must not be overwritten.
- The existing curriculum expansion in `commands.rs` is preserved while validating its input and outcome semantics.
