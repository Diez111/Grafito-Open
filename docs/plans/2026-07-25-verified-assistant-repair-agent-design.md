# Verified Assistant Repair Agent Design

## Problem

The remote assistant can invent unsupported syntax, while the current local
preflight discards its concrete failure reason and offers only one manual,
generic correction. A manual `Script` containing otherwise valid `Segment3D`
commands is also corrupted by command-wide implicit multiplication before its
nested commands run.

## Reframe

The goal is not unrestricted model tool execution. The goal is a bounded
proposal repair loop: the model receives authoritative syntax relevant to the
request, Grafito validates its proposed action on an isolated document, and
only a locally verified proposal is exposed for explicit user application.

## Approach

Use the command registry as the source of documented command syntax, retain
the assistant graph capability table as the execution policy, and add typed,
sanitized preflight feedback to one attachment-free automatic repair request.
General scenes remain bounded to 2-8 self-contained graph commands and are
applied atomically only after a combined render proof.

## Scope

In scope:

1. Preserve raw `Script` bodies until nested commands normalize themselves.
2. Add missing stable registry metadata for directly executable 3D commands.
3. Build a bounded relevant catalog from documented commands plus executable
   graph capabilities, distinguishing executable syntax from reference-only
   syntax.
4. Capture bounded, sanitized proposal failures and include them in one repair
   request when no command survives preflight.
5. Automatically retry once without attachments, then show only locally
   verified cards.
6. Accept homogeneous `grafito-scene` blocks such as six `Segment3D` edges;
   keep the existing multi-type flower scene checks and styling.

Out of scope:

1. Remote model function calls, shell access, file access, imports, exports,
   deletion, or autonomous document mutation.
2. Sending every command signature on every request.
3. Adding a general filled `Polyhedron` object or claiming that it exists.

## Technical Design

`grafito-command` exposes registry-based descriptions for all stable commands.
`assistant_context` ranks them by request relevance and marks only graph
capabilities as executable. The remote prompt instructs the model to emit
`grafito` or `grafito-scene` fences only for executable entries.

The app preflight returns a typed local failure instead of reducing every
failure to `is_ok()`. Failures contain only an identifier, a finite reason code,
and trusted registry signatures. `AssistantRequest` carries optional bounded
repair feedback. The transport renders it after the tool catalog.

When a response has one or more action candidates but none verify, the app
starts one repair request with the original user question, the same document
snapshot, no attachments, and sanitized feedback. The rejected provider text
is not committed to conversation history. Cancellation, provider/model or
document/focus changes, transport errors, attachments, or a consumed retry
budget stop the loop. A successful repaired response follows the existing
explicit Apply and re-preflight flow.

For a `grafito-scene`, same-command components are staged together and proven
through their common render route. Mixed scenes continue to use the existing
flower-specific structural checks. No scene is applied unless every nested
command creates the expected render-space object and the combined scene is
visible.

## Acceptance Criteria

1. `Script[Segment3D[...];Segment3D[...]]` creates both segments atomically.
2. The assistant catalog contains the exact trusted `Segment3D` syntax for a
   tetrahedron request and never advertises `Polyhedron` or `NumericArray`.
3. An unsupported or invalid remote fence is never actionable.
4. With no verified candidate and no attachments, Grafito sends at most one
   repair request containing sanitized failure feedback and no rejected raw
   command body, paths, credentials, document serialization, or attachments.
5. A repaired valid command or homogeneous scene is locally staged, rendered,
   and shown as an explicit Apply card only after verification.
6. A failed repair leaves the live document, undo history, and remote action
   boundary unchanged.

## Test Strategy

Add command regression coverage for nested `Segment3D` scripts. Add catalog
and registry consistency tests in `grafito-command`. Add request validation and
remote-prompt budget tests in assistant types/transport. Add app tests for
typed rejection feedback, retry eligibility, generic Segment3D scenes, and
the existing flower path. Keep UI tests asserting that only verified proposals
expose Apply/Edit controls.

## Risks

1. Registry metadata can lag parser fallbacks. Local parse, transaction, and
   render preflight remains the final authority.
2. Repair feedback can become a prompt-injection or privacy channel. It is
   typed, byte-bounded, generated locally, and contains only trusted syntax
   and sanitized identifiers.
3. Automatic retries can increase remote cost. The loop is capped at one,
   disallows attachment retransmission, and retains the existing timeout and
   cancellation behavior.
