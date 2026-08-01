# Assistant Complex Repair and Fusion Fallback Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Mora generate executable complex-analysis commands reliably, diagnose invalid proposals precisely, and recover a failed MiniMax M3 proposal through one bounded Fusion review before exposing an explicit retry.

**Architecture:** Preserve the local preflight and explicit Apply boundary as the only execution authority. The user-selected MiniMax M3 request remains the primary route; only when the user explicitly enables Fusion fallback and every actionable graph proposal fails local preflight, an internal Fusion repair route sends only the original question, trusted catalog, current safe context, and sanitized failure feedback to MiniMax M3 plus the existing DeepSeek v4 Pro audit. The selected model remains MiniMax M3, so an internal reviewer cannot overwrite user preference or clear the conversation.

**Tech Stack:** Rust 2021, egui, existing `grafito-assistant-types`, OpenCode Go MiniMax Messages transport, existing Fusion audit transport, local command registry/preflight.

---

### Task 1: Make DomainColoring Executable in the Assistant Catalog

**Files:**
- Modify: `crates/grafito-command/src/assistant_context.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Test: `crates/grafito-command/tests/domain_coloring_commands.rs`
- Test: `crates/grafito-app/src/assistant.rs`

**Step 1: Write failing regressions**

Cover the exact user request:

```text
DomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2, r]
```

Assert that the assistant-side validation rejects `r` before staging and emits trusted syntax that says `resolution` is a literal integer from 16 through 300. Assert that the executable catalog shows the canonical six-field form and the same constraint.

**Step 2: Run the focused tests to prove red**

Run:

```bash
cargo test -p grafito-command domain_coloring
cargo test -p grafito-app domain_coloring
```

**Step 3: Add the narrow typed rule**

Use the command's existing canonical bounds and validate only the sixth `DomainColoring` argument as a literal integer in `16..=300`. Do not reinterpret `r` as a color mode or silently coerce document variables. Keep the valid five-argument user command and six-argument `200` form executable.

**Step 4: Verify green**

Run the focused tests and retain transactional command behavior.

### Task 2: Preserve a Bounded Repair Session Instead of a One-Shot Flag

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `crates/grafito-assistant-types/src/lib.rs` only if a small typed route/session field must cross crate boundaries
- Test: `crates/grafito-ui/src/assistant.rs`
- Test: `crates/grafito-app/src/assistant.rs`

**Step 1: Write failing state tests**

Cover:

1. Two correction passes are possible at most; a third is unavailable.
2. The replacement targets the response belonging to the original request, not any later assistant turn.
3. A repair request excludes the rejected active assistant response from remote history.
4. Every repair has empty attachments and `image_upload_consent = false`.
5. A reply with prose/no actionable proposal remains visibly unsuccessful and can offer the remaining bounded retry.

**Step 2: Run focused repair tests to prove red**

Run:

```bash
cargo test -p grafito-app assistant_repair
cargo test -p grafito-ui proposal_correction
```

**Step 3: Replace the single-use correction state**

Track a request-bound repair session with source question, target turn/request ID, sanitized feedback, attempt count, and route. Set `MAX_ASSISTANT_PROPOSAL_CORRECTIONS` to two total repair passes. Invalidate it when provider/model, document/focus, or conversation changes. Keep historical rejected text visible locally, but preserve the pre-source complete exchange history and omit the entire active user/rejected-assistant pair from repair payload history.

**Step 4: Add exact terminal feedback**

After a repair response with zero verified proposals, retain a clear local error such as `No se obtuvo una propuesta verificable` instead of clearing the previous diagnosis silently. Do not create an Apply action.

### Task 3: Escalate a Failed MiniMax M3 Proposal to Fusion Once

**Files:**
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-assistant/src/lib.rs` only for route-specific repair prompt/budget coverage
- Test: `crates/grafito-app/src/assistant.rs`
- Test: `crates/grafito-assistant/tests/remote_transport.rs`

**Step 1: Write failing behavior tests**

For an OpenCode Go `minimax-m3` request where all graph proposals fail local preflight, assert:

1. Exactly one internal Fusion repair is launched only when the user enabled Fusion fallback; disabled fallback never changes remote destination.
2. The selected model remains `minimax-m3`; no preference/history reset occurs.
3. Fusion receives the original question, trusted catalog and sanitized feedback, but not images, consent, document digest, file paths, or rejected raw answer text.
4. A valid Fusion response replaces the failed answer, is locally preflighted, and remains Apply-only.
5. A Fusion transport failure or invalid Fusion response never loops automatically and leaves one clearly labeled explicit retry if budget remains.

**Step 2: Run tests to prove red**

Run:

```bash
cargo test -p grafito-app fusion
cargo test -p grafito-assistant --test remote_transport fusion
```

**Step 3: Implement a typed internal route**

Add a persisted `allow_fusion_fallback` checkbox, defaulting to false, whose text discloses that a failed MiniMax proposal may be sent through MiniMax M3 plus DeepSeek v4 Pro. Capture that opt-in when the initial request launches. Introduce an internal `RemoteRoute` distinct from the user-selected provider/model. Its freshness check binds to the selected MiniMax M3 configuration and immutable document/focus snapshot, while its transport settings use existing `fusion`. Never mutate the UI-selected model to trigger this route. Only an all-rejected MiniMax M3 graph/scene result may auto-escalate, and only once per request.

**Step 4: Make cost and state explicit**

Pending UI must identify the Fusion review. Fusion remains two remote text legs, so the request builder must reserve enough room for a nonempty audit draft and stop safely on budget exhaustion. Local preflight, not DeepSeek, remains the verifier of executable geometry.

### Task 4: Change First-Run Default Without Overwriting Preferences

**Files:**
- Modify: `crates/grafito-app/src/utils.rs`
- Modify: `crates/grafito-ui/src/assistant.rs`
- Test: `crates/grafito-app/src/utils.rs`
- Test: `crates/grafito-ui/src/assistant.rs`

**Step 1: Write default/migration regressions**

Assert a newly created preference/state defaults to OpenCode Go `minimax-m3`, while an existing saved `deepseek-v4-pro` preference remains untouched.

**Step 2: Implement the two first-run defaults**

Change only fallback/default values; do not migrate, overwrite, or silently switch a stored user selection.

### Task 5: Document and Verify the Whole Flow

**Files:**
- Modify: `docs/spec/assistant.md`
- Modify: `README.md` only to remove stale `DomainColoring` resolution examples

**Step 1: Document the contract**

Describe the canonical `DomainColoring` form, literal resolution restriction, default M3 route, bounded Fusion escalation, attachment exclusion, no rejected-answer replay, explicit Apply, and local-preflight authority.

**Step 2: Run verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

**Step 3: Refresh project knowledge**

Run `graphify update .` and record the model-routing, bounded repair, and command-syntax decisions in project memory. Do not commit because the worktree contains unrelated changes.

## Acceptance Criteria

1. `DomainColoring[(z^2 - 1)/(z^2 + 1), -2, 2, -2, 2]` and its literal-resolution form are actionable only after local preflight; the `r` form is rejected with an exact local explanation.
2. Fresh installations default to MiniMax M3 without changing existing saved model choices.
3. With explicit Fusion fallback enabled, an all-rejected MiniMax M3 graph proposal triggers no more than one automatic Fusion review and never triggers a document mutation; disabled fallback never contacts DeepSeek.
4. Repairs/Fusion never send attachments, image consent, rejected raw answer text, local paths, document digest, or secrets.
5. Every recovery output is locally parsed, staged, render-checked, and requires explicit Apply.
6. The user receives a regenerated response or a truthful terminal error; repair attempts are bounded and visible.

## Risks

- Fusion adds a MiniMax draft plus DeepSeek audit after an initial M3 request, increasing latency/cost. Mitigation: only all-rejected M3 graph/scene responses trigger it once and the UI states that Fusion is running.
- More repair context could accidentally replay rejected remote text or attachments. Mitigation: construct repair history from the original question plus completed prior exchanges only, and retain the existing type/transport-level image prohibition.
- Internal Fusion transport must not look like the selected model changed. Mitigation: separate internal route metadata from the user preference used for freshness checks.
