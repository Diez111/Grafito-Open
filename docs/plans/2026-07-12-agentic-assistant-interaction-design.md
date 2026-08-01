# Agentic Assistant Interaction Design

## Problem

The assistant delays the submitted user message until a remote completion
arrives. It also turns a model-proposed Grafito command into text in the shared
command bar, so a request such as graphing a quadratic function does not change
the document unless the user performs an extra, disconnected step.

## Reframe

Grafito needs a responsive mathematical co-pilot, not unrestricted remote code
execution. The assistant must make work-in-progress visible immediately, know
the relevant native tools, and offer an understandable one-click document
action that follows the same undo, validation, transaction, and construction
protocol as every other Grafito command.

## Approach

Keep an ordered transcript of visible turns. A user turn is appended before the
remote worker starts; an assistant turn is appended only on success. Unanswered
user turns remain visible after an error or cancellation, while outbound
history scans backward for complete User-to-Assistant pairs only. This keeps
the visible conversation honest without letting a failed request corrupt remote
context.

While a request is active, the transcript renders an assistant-side four-dot
waveform and requests a low-frequency egui repaint. No timer, task, or
animation state is stored outside the existing request state.

The command registry supplies a compact, deterministic catalog of signatures
relevant to the current prompt. The provider receives that catalog as bounded
tool context, and the system prompt requires an exact one-line `grafito` fence
for unambiguous graphing and document-action requests. Each candidate is
validated locally against the registry. The chat offers `Aplicar en Grafito`,
which is an explicit user approval and delegates to
`execute_command_and_record`; scripts and external-data commands remain
excluded.

## Scope

In scope:

1. Immediate user-turn display, persistent errors/cancellations, and an
   animated pending waveform.
2. Pair-aware bounded outbound history.
3. Relevant registry-derived tool context and explicit graph-command guidance.
4. One-click, validated application of a remote command through the normal
   transaction, undo, toast, and construction-protocol path.

Out of scope:

1. Autonomous execution without a user click.
2. Provider-side function calling, tools with filesystem/network access,
   scripts, import/export, streaming, or multi-step autonomous loops.
3. A complete language understanding or model-specific tool API.

## Technical Design

`AssistantPanelState::begin_request(question)` appends the trimmed user turn
before setting pending state. `complete_request(answer)` appends only the
assistant result. `trim_conversation` removes oldest complete exchanges before
orphan turns, and `conversation_within_budget` selects only adjacent complete
exchanges from newest to oldest.

`draw_pending_indicator` lives in `grafito-ui` beside the ordinary assistant
turns. It paints four vertically phased dots from egui input time and schedules
the next repaint only while `is_pending` is true.

`grafito-command::assistant_context::assistant_tool_catalog` ranks documented
registry entries against the request and returns complete signatures under a
caller-provided byte budget. `AssistantRequest.tool_catalog` is bounded,
validated, budgeted, and appended to the remote prompt. The dynamic catalog
keeps the model aligned with the same command definitions used by the app,
without a dependency from the transport crate back to the dispatcher.

`AssistantUiAction::ApplyCommand` repeats local candidate validation and calls
`GrafitoApp::execute_command_and_record`. The existing command transaction
provides atomic document updates, undo snapshots, visible errors, and protocol
records. The remote model never invokes this action directly.

## Acceptance Criteria

1. Submitting a request displays the user message during the remote wait and a
   four-dot waveform until the worker reaches a terminal state.
2. Failed or cancelled messages remain in the transcript but are never sent as
   incomplete conversation history.
3. An unambiguous quadratic graph request leads the provider to offer
   `Function[expr]` in one valid `grafito` block.
4. A user can apply a valid proposed command directly from the chat; the result
   uses the normal undo and construction-protocol pipeline.
5. Script and external-data proposals cannot be inserted or applied.
6. Tool context remains deterministic and inside the request byte budget.

## Test Strategy

Add state tests for immediate turns, failed-orphan exclusion, pair budgeting,
and bounded trimming. Test command-catalog relevance and bounds, graph prompt
guidance, validator handling for zero-argument registered commands, and safe
candidate rejection. Run focused crates before workspace formatting, clippy,
tests, release build, package build, and installation verification.

## Risks

1. A model can still propose an invalid mathematical command. Local registry
   validation and the transactional executor return a visible error without a
   partial document change.
2. A catalog that is too large competes with problem context. The caller gives
   it a strict byte budget after reserving the user prompt and focused object.
3. Autonomous mutation would make provider output overly privileged. Applying
   any proposal remains a visible user decision and all I/O commands stay
   unavailable.
