# Release Integrity Remediation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the release-review correctness blockers before publishing v1.2.20-beta.

**Architecture:** Keep spreadsheet ownership authoritative in `Document`, so commands, assistant plans, animations, and persistence share one policy. Enforce coordinate-cell ownership at the central point-mutation API and reconcile persisted points from their spreadsheet source. Mirror CPU invalid-`clamp` semantics in every scalar WGSL interpreter.

**Tech Stack:** Rust 2021, cargo tests, wgpu/WGSL.

---

### Task 1: Spreadsheet ownership and animation

**Files:**
- Modify: `crates/grafito-core/src/document.rs`
- Test: `crates/grafito-core/tests/document_integration.rs`
- Test: `crates/grafito-core/tests/persistence_properties.rs`
- Test: `crates/grafito-command/tests/command_transactions.rs`
- Test: `crates/grafito-assistant/tests/headless_harness.rs`

1. Add failing regressions for invalid-formula label reservation and formula recomputation during animation.
2. Reserve every non-empty spreadsheet cell label, independent of resolution.
3. Reject scalar writes and animation configuration for reserved labels; exclude stale legacy metadata from animation advancement.
4. Route animation updates through spreadsheet recomputation before bound-geometry propagation.
5. Run focused core, command, and assistant tests.

### Task 2: Coordinate-cell point ownership

**Files:**
- Modify: `crates/grafito-core/src/document.rs`
- Modify: `crates/grafito-core/src/persistence.rs`
- Test: `crates/grafito-core/tests/document_integration.rs`
- Test: `crates/grafito-command/tests/command_transactions.rs`

1. Add failing direct-move, `SetValue`, and deserialize-reconciliation regressions.
2. Reject generic movement of a spreadsheet-coordinate-owned point from `try_update_point_and_re_evaluate`.
3. Reconcile every coordinate-cell owner from spreadsheet source during deserialization.
4. Run focused core and command tests.

### Task 3: CPU/GPU clamp parity

**Files:**
- Modify: `crates/grafito-render/src/{function,implicit,parametric,vector,fill}_compute.wgsl`
- Test: `crates/grafito-render/tests/gpu_compute.rs`

1. Add required-Vulkan parity coverage for inverted clamp bounds.
2. Return NaN in each WGSL bytecode interpreter when bounds are non-finite or reversed.
3. Run required Vulkan GPU regressions, then workspace verification.
