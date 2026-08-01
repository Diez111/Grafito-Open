# Assistant Flat Transcript and Proposal Recovery Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the chat-bubble transcript with a flat, equal-scale scientific conversation and make locally rejected remote graph proposals recoverable through one explicit correction.

**Architecture:** Keep the existing bounded conversation, verified-proposal allowlist, detached preflight, and explicit Apply boundary. Flatten only the outer turn presentation; preserve rich math/code cards. Store the already-sanitized local repair feedback alongside the pending correction so a user-triggered retry can tell the provider what failed without transmitting images or enabling rejected text to execute.

**Tech Stack:** Rust 2021, egui/eframe, grafito-ui state, grafito-app proposal preflight, existing Rust unit tests.

---

### Task 1: Define transcript and recovery contracts

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-ui/tests/ui_tests.rs`
- Modify: `crates/grafito-app/src/assistant.rs`

**Step 1: Write failing tests**

- Assert that a pending correction retains both the original question and sanitized `AssistantRepairFeedback` until an explicit retry consumes it.
- Assert that a correction remains eligible for an attachment-bearing rejected graph proposal because the retry deliberately strips attachments.
- Assert that transcript rendering has no role-dependent horizontal layout or outer bubble-width helper, while retaining color-coded role labels and proposal recovery action wiring.

**Step 2: Run focused tests to verify failure**

Run: `cargo test -p grafito-ui assistant --lib` and `cargo test -p grafito-app assistant --lib`

Expected: failure because repair feedback is not retained and the new explicit-correction predicate/transcript contract does not exist.

**Step 3: Implement the smallest safe state and UI changes**

- Remove role-dependent bubble width, alignment, and outer message frames from normal and pending turns.
- Keep all turn bodies at `TYPE_BASE`, full transcript width, and distinguish `Vos` and `Asistente` through their existing role-label colors.
- Keep Apply restricted to `VerifiedAssistantProposal`; make rejected code blocks say only that automatic application is unavailable.
- Render `Pedir una corrección` beside an eligible rejected proposal as well as in the error banner.
- Retain sanitized repair feedback with the correction question; clear it on consume, new request, clear, or error dismissal.

**Step 4: Run focused tests**

Run: `cargo test -p grafito-ui assistant --lib` and `cargo test -p grafito-app assistant --lib`

Expected: pass.

### Task 2: Bridge rejected preflight results to one explicit recovery

**Files:**
- Modify: `crates/grafito-app/src/assistant.rs`

**Step 1: Implement explicit eligibility**

- Add a small predicate separate from automatic repair: it permits one correction only when there is a rejected graph/scene candidate and sanitized feedback, including attachment-bearing initial requests because the correction sends neither images nor consent.
- When automatic repair did not start, offer the correction with its feedback before completing the assistant response.
- Build the user-clicked correction request from the stored feedback, preserving the existing one-attempt cap and no-autonomous-mutation rule.

**Step 2: Verify failure and recovery paths**

Run: `cargo test -p grafito-app assistant --lib`

Expected: accepted candidates remain Apply-only; rejected attachment-bearing proposals expose exactly one safe correction; a second correction remains unavailable.

### Task 3: Verify and document

**Files:**
- Modify: `docs/spec/assistant.md`

**Step 1: Update the explicit-action contract**

- Explain that rejected remote proposals never receive Apply, but may offer one user-clicked correction with sanitized local feedback and no attachments.
- Describe the flat transcript distinction: role labels use color while message bodies share the same layout and scale.

**Step 2: Run verification**

Run: `cargo fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo build --workspace --release`, and `git diff --check`.

**Step 3: Update development graph**

Run: `graphify update .`

**Step 4: Package only if requested**

No commit or package rebuild is part of this task unless the user explicitly requests it.
