# Headless Assistant Harness Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Provide an offline, headless request-plan-preview-apply harness with privacy-preserving staging receipts and explicit local replay.

**Architecture:** `grafito-command::assistant_plan` owns sequential atomic staging because it already owns the allowlist and `OperationBatch`. `grafito-assistant-types` defines serializable receipts containing only versions, counts, and commitments. `grafito-assistant::harness` composes local resolution with staging and replay; the desktop app becomes a thin adapter for the same APIs.

**Tech Stack:** Rust 2021, `OperationBatch`/`ChangeSet`, serde JSON canonicalization, SHA-256, existing local assistant resolver.

---

### Task 1: Define receipt contracts

**Files:**
- Modify: `crates/grafito-assistant-types/src/lib.rs`
- Modify: `crates/grafito-assistant-types/Cargo.toml` only if required by type tests

**Step 1: Write failing receipt tests**

Cover JSON round-trip and validate that serialized receipts contain no source plan, question, expression, label, attachment, or document content.

**Step 2: Implement minimal receipt types**

Add a versioned receipt, base/staged state, delta counters, SHA-256 algorithm marker, and bounded validation. Store only hashes, revisions, schema versions, and counts.

**Step 3: Run focused tests**

Run: `cargo test -p grafito-assistant-types`

### Task 2: Make assistant plans stage sequentially

**Files:**
- Modify: `crates/grafito-command/src/assistant_plan.rs`
- Modify: `crates/grafito-command/Cargo.toml`

**Step 1: Write failing staging tests**

Cover a `SetVariable` followed by a graph using that variable, a failed stage preserving the source document, and no-op plan rejection.

**Step 2: Implement staged plans and evidence**

Stage every operation in order against a detached document through `OperationBatch`; preserve the `ChangeSet`, preview, and receipt. Apply only a successfully staged plan after rechecking its base. Build canonical SHA-256 commitments in memory and add non-mutating receipt replay.

**Step 3: Run focused tests**

Run: `cargo test -p grafito-command assistant_plan`

### Task 3: Extract the headless local harness

**Files:**
- Create: `crates/grafito-assistant/src/harness.rs`
- Modify: `crates/grafito-assistant/src/lib.rs`
- Modify: `crates/grafito-assistant/Cargo.toml`
- Create: `crates/grafito-assistant/tests/headless_harness.rs`

**Step 1: Write failing end-to-end tests**

Cover local request to staged graph, explicit apply with one revision, refusal of remote requests, replay success, and rejection of tampered plan/base/delta/evidence without mutation.

**Step 2: Implement the harness**

Expose `request`, `stage`, `apply`, and `replay`. Restrict `request` to `PrivacyMode::LocalOnly` and require an exact local document context. Do not expose providers, keyring, egui, or transport from this API.

**Step 3: Run focused tests**

Run: `cargo test -p grafito-assistant --test headless_harness`

### Task 4: Route desktop local flow through the harness

**Files:**
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `docs/spec/assistant.md`
- Modify: `docs/Plans.md`

**Step 1: Update the local adapter**

Use harness request/staging for local preview and harness apply for commit while retaining existing UI state, undo adaptation, and remote paths.

**Step 2: Document the receipt boundary**

Document that receipts are local opt-in evidence and never contain raw prompt, plan, document, images, paths, credentials, provider data, or diagnostics.

**Step 3: Run workspace verification**

Run: `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release`. Run scoped rustfmt because the workspace fmt gate has a known unrelated difference in `grafito-core/src/validation.rs`.
