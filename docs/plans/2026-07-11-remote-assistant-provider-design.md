# Remote Assistant Provider and Contextual UX Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Turn the docked assistant into a contextual, multimodal conversation surface backed by OpenCode Go or optional Ollama without exposing credentials or allowing remote text to mutate a document.

**Architecture:** `grafito-assistant` owns validated OpenAI-compatible transport, base-URL completion, bounded request payloads and worker threads. `grafito-app` owns OS credential access, provider/model preferences, request polling, image import, selected-object snapshots and explicit consent. `grafito-ui` remains pure egui state/rendering and emits typed actions only.

**Tech Stack:** Rust 2021, eframe/egui, reqwest blocking worker threads, OpenCode Go OpenAI-compatible API, Ollama OpenAI-compatible loopback API, `keyring`, `rfd`, `image`.

---

## Problem

The current permanent panel advertises a local-only solver, asks for image transcription despite validated image transport already existing, has no provider configuration, cannot use the user-owned OpenCode Go subscription, blocks richer interaction behind a button, and loses the intent of a selected function.

## Reframe

The goal is not an unrestricted agent. It is a focused mathematical collaborator: it understands the selected function or asks one concise clarifying question, can inspect user-approved images through a selected vision-capable model, retains a bounded conversation, and presents suggested next questions. Any document change remains a typed, reviewed, locally validated proposal.

## Scope

In scope:

1. OpenCode Go endpoint `https://opencode.ai/zen/go/v1` with the chat-completions path appended exactly once.
2. Optional loopback-only Ollama profile, provider/model selector, model refresh endpoint, OS-keyring credentials, and in-memory fallback for an unavailable keyring.
3. Non-blocking requests, cancellation, bounded conversation, selected-function snapshots, Enter-to-submit, image picker and explicit image-upload consent.
4. A focused assistant UI that removes the local badge, local-only copy and transcription editor; it displays connection state, focus context, suggestions and safe remote answers.

Out of scope:

1. Anthropic-format OpenCode endpoint support, streaming, account login/OAuth, cloud sync, model billing management, and automatic remote actions.
2. Sending unselected full-document JSON, filesystem paths, unapproved image bytes, API keys, or arbitrary command text to a provider.

## Acceptance Criteria

1. OpenCode Go requests go only to `https://opencode.ai/zen/go/v1/chat/completions`; Ollama requests stay on literal loopback hosts. Redirects, embedded credentials, unbounded responses and invalid endpoints are rejected.
2. A user can choose OpenCode Go or Ollama, select or enter a model, save an OpenCode key in the OS credential store, and never writes the key to Grafito's JSON config, logs, errors or payloads.
3. The UI has no local-only badge, local-only submit label or image-transcription input. A PNG/JPEG picker validates image bytes and sends them only after explicit per-request consent and vision-model acknowledgement.
4. Pressing Enter in the assistant editor submits; Shift+Enter inserts a newline. The request runs off the egui update thread, exposes a cancel action, and updates the response without blocking the canvas.
5. Selecting a `FunctionObj` shows a focused context chip containing its label/expression/domain. That snapshot is sent with the question; without focus, the remote system prompt asks a clarifying question and offers options rather than guessing a target.
6. The assistant retains at most six bounded text turns in memory, offers contextual follow-up chips, and never routes remote content through `process_input`. Existing typed preview/apply validation remains the only mutation path.

## Task 1: Provider Transport and Request Types

**Files:**
- Modify: `crates/grafito-assistant-types/src/lib.rs`
- Modify: `crates/grafito-assistant/src/lib.rs`
- Test: `crates/grafito-assistant/tests/remote_transport.rs`

1. Write failing tests for the official OpenCode host/path, path appending exactly once, explicit API-key worker input, and a bounded focus/history payload.
2. Add `AssistantFocus` and bounded `ConversationTurn` request fields. Include only the focused summary and prior text turns in the remote prompt.
3. Change the OpenCode Go base endpoint and allowlist to `opencode.ai/zen/go/v1`. Add a URL helper that appends `chat/completions` or `models` to an API base URL without discarding `/v1`.
4. Add a worker variant that receives an optional in-memory key instead of reading an environment variable; retain the environment wrapper for compatibility/tests.
5. Add a bounded model-list worker that parses only model identifiers and never returns raw provider metadata or credentials.

## Task 2: Secure App-Owned Connection State

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/grafito-app/Cargo.toml`
- Create: `crates/grafito-app/src/assistant_credentials.rs`
- Modify: `crates/grafito-app/src/lib.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-app/src/assistant_credentials.rs`

1. Add `keyring` only to `grafito-app` through workspace dependencies.
2. Implement credential helpers with fixed service/account identifiers. Store only OpenCode Go keys, return a generic availability error, and never serialize, debug-print or log secrets.
3. Add app-owned remote-job state: receiver, request ID, cancellation token, model list receiver, non-secret provider/model preference and a session-only key fallback.
4. Poll jobs during `GrafitoApp::update`; every worker completion calls `ctx.request_repaint()` and stale request IDs are discarded.
5. Add tests for fixed credential account identifiers and for state transitions that do not require the platform keyring.

## Task 3: Context, Focus and Safe Conversation

**Files:**
- Modify: `crates/grafito-command/src/assistant_context.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `crates/grafito-app/src/input.rs`
- Modify: `crates/grafito-app/src/algebra.rs`
- Test: `crates/grafito-command/src/assistant_context.rs`
- Test: `crates/grafito-app/src/assistant.rs`

1. Write tests that build a finite `AssistantFocus` from a selected function and reject oversized focus/history text.
2. Add a function-focused context helper that emits label, expression, domain and integral metadata without paths/caches/full serialized objects.
3. Keep canvas and algebra selections synchronized through one helper, then snapshot the current selected object at submission time.
4. Build `AssistantRequest` with `PrivacyMode::RemoteAllowed`, the snapshot, at most six previous turns and no transcription. Append user/assistant text turns only after a successful request.
5. Keep remote answers read-only. Continue using `assistant_plan::preview_plan` and `apply_plan` only for typed local proposals.

## Task 4: Multimodal Assistant Experience

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-ui/tests/ui_tests.rs`
- Test: `crates/grafito-app/src/tests.rs`

1. Write failing UI/source tests that prohibit local-only/transcription labels and require provider, attachment, focus and Enter-submit controls.
2. Replace `local_only`, transcription and `SubmitLocal` UI state with provider/model preferences, image-consent state, connection feedback, pending state, focus label, conversation and suggestion chips.
3. Render an accessible compact connection section: OpenCode Go or Ollama, model entry/list, key draft with save/clear actions, model refresh and vision acknowledgement. Do not display an API key after saving.
4. Add a PNG/JPEG picker in the app layer. Decode dimensions, validate bytes before storing, discard the file path immediately and allow removing an attachment.
5. Submit on Enter, preserve Shift+Enter newline, show Cancel while pending, disable duplicate submissions, and use deterministic follow-up chips based on selected function/no focus.
6. Update remote system instructions to explain selected focus and require a clarifying question plus suggested options if there is no unambiguous target.

## Task 5: Documentation, Validation and Packaging

**Files:**
- Modify: `docs/spec/assistant.md`
- Modify: `CHANGELOG.md`
- Modify: `Cargo.toml`
- Modify: `packaging/debian/control`

1. Document remote-consent boundaries, keyring-only persistence, OpenCode Go/Ollama endpoints, image consent, bounded history and the remote-action firewall.
2. Bump the beta version after implementation and describe user-visible assistant improvements.
3. Run the targeted assistant/UI/app tests after each task, then `cargo fmt --all -- --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --locked`, and `cargo build --workspace --release --locked`.
4. Build the Debian package, verify metadata/contents/checksum, install through the user-authorized credential prompt, then verify `dpkg-query`, `dpkg -V` and `/usr/bin/grafito --help`.

## Risks

1. OpenCode's model list may not expose stable vision capability metadata. The UI therefore requires explicit acknowledgement before sending an image and reports provider rejection without retaining source paths.
2. Linux Secret Service may be unavailable. The app permits a session-only key but refuses plaintext config persistence.
3. Blocking HTTP cancellation cannot interrupt an in-flight socket write immediately. The worker runs outside egui and is bounded by the request timeout, with stale results ignored.
4. Remote responses can be adversarial. They stay text-only; no remote text is passed to command parsing, file operations, shell, or document mutation.
