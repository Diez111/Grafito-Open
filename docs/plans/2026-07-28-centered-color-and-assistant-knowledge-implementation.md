# Centered Color and Assistant Knowledge Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Center and unify color editing while grounding Assistant syntax in a deterministic Rust-native knowledge graph derived from registered commands.

**Architecture:** Use explicit picker target state in `grafito-app`; retain staged document replacement for color changes. Build a static process-local knowledge graph in `grafito-command::assistant_context` from `CommandSpec` and assistant execution policy, then render a bounded text projection through the existing Assistant request field and transport validation.

**Tech Stack:** Rust 2021, egui/eframe, `grafito-command` registry, `grafito-assistant-types`, existing Graphify CLI for developer indexing.

---

### Task 1: Unify the color-dialog target and presentation

**Files:**
- Modify: `crates/grafito-app/src/app.rs`
- Modify: `crates/grafito-app/src/ui.rs`
- Modify: `crates/grafito-app/src/algebra.rs`
- Modify: `crates/grafito-app/src/panels.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write failing tests**

Add contracts for a centered, constrained dialog, explicit `ObjectColor`/`RegularPolychoronFill` targets, staged fill-color edits, and absence of native inspector picker calls.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-app color_picker --lib`

**Step 3: Implement the minimal route**

Introduce an active picker struct and target enum. Add open helpers and a fill-color replacement helper. Route Algebra and Inspector swatches into the shared theme-owned centered `HsvColorPicker` dialog.

**Step 4: Run the focused tests to verify they pass**

Run: `cargo test -p grafito-app color_picker --lib`

### Task 2: Build deterministic Assistant knowledge retrieval

**Files:**
- Modify: `crates/grafito-command/src/assistant_context.rs`
- Modify: `crates/grafito-command/src/command_registry.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Test: `crates/grafito-command/src/assistant_context.rs`
- Test: `crates/grafito-command/tests/command_registry.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write failing tests**

Cover alias relevance, alternate safe forms for `ImplicitCurve` and `Surface3D`, deterministic byte-bounded rendering, and rejection of label/path/data forms as executable proposals.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-command assistant_context::tests --lib`

**Step 3: Implement the minimal graph**

Create an immutable `AssistantKnowledgeGraph` from registry nodes and existing execution-policy edges. Use all safe signatures rather than only `signatures.first()`, include aliases/categories in scoring, and retain existing catalog-string request transport.

**Step 4: Align live metadata**

Correct `ImplicitCurve` executable arity and registry signatures for public optional DomainColoring bounds and optional attractor parameters only where existing handlers already support them.

**Step 5: Run focused verification**

Run: `cargo test -p grafito-command assistant_context::tests --lib`

### Task 3: Verify the transport boundary and release artifact

**Files:**
- Modify: `docs/commands.md` through the registry projection if metadata changes.
- Modify: `docs/plans/2026-07-28-centered-color-and-assistant-knowledge-design.md` only for verified implementation changes.

**Step 1: Privacy and transport tests**

Run: `cargo test -p grafito-assistant --test remote_transport`

**Step 2: Full verification**

Run: `cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release`.

**Step 3: Maintain developer graph and package**

Run `graphify update .`, `packaging/build-deb.sh`, install the generated package, and verify `/usr/bin/grafito` matches `target/release/grafito`.
