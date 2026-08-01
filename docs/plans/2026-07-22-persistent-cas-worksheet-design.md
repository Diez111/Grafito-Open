# Persistent CAS Worksheet Design Doc

## Problem

The CAS panel keeps results only in `GrafitoApp::cas_history`, so mathematical
work disappears when a document is saved, opened, or replaced. A user cannot
revisit the derivation or an error that led to a construction.

## Reframe

The immediate need is a durable, local record of submitted CAS work, not a
general notebook editor. Editable cells would introduce draft persistence,
recomputation semantics, and lifecycle complexity before the document model
has a proven worksheet boundary. Probability/inference studios and dynamics
controls are separate products and remain follow-up slices of Task 6.

Three assumptions are intentionally rejected:

- A worksheet result should not silently change when variables change or a
  document is reopened; it is a historical result from the submitted command.
- Failed commands are useful mathematical evidence and should persist as error
  cells, while their uncommitted input remains available for correction.
- Existing assistant context must not gain document-wide serialization just to
  support worksheets; worksheet contents remain local by default.

## Approach

Implement the narrowest durable wedge: an append-only list of bounded CAS
cells owned by `Document`. A dedicated command transaction evaluates a command
against staged state, appends a typed success or error cell to the same staged
document, validates it, and commits one revision. The native CAS panel becomes
the only caller of this path and reads its history from `Document`.

Alternatives considered:

- Keep only `GrafitoApp::cas_history`: smallest code change, but fails save,
  open, undo, and privacy-boundary requirements.
- Persist every keystroke as editable cells: richer notebook UX, but requires
  draft-aware Save/New/Open/Exit handling equivalent to the spreadsheet.
- Store a generic command log: would collect non-CAS activity and blur a
  focused local worksheet into construction history.

## Scope

In scope:

- Bounded, serializable CAS input/output/status cells in `Document`.
- Schema v3 writes with schema v2 and legacy document readability.
- Atomic execution plus cell insertion, including error cells.
- Native CAS panel rendering, clear action, one-step undo, and persistence.
- Tests for bounds, migration, atomicity, undo, and assistant-context privacy.

Out of scope:

- Editable/reorderable cells, variable recomputation, formatted step trees,
  attachments, import/export, or background evaluation.
- Probability/inference views and dynamics-control persistence.
- Any assistant upload, telemetry, account, network access, or source-path
  retention.

## Technical Design

`grafito-core::Document` owns `Vec<CasWorksheetEntry>` with a serde default.
Each entry stores `input`, `output`, and `CasWorksheetStatus::{Success, Error}`.
The core API exposes read-only cells, bounded append, and clear. It limits cell
count, input bytes, output bytes, and aggregate bytes. Raw JSON validation
checks the cell-array length before `Document` deserialization, while semantic
validation checks all remaining bounds.

`grafito-command::process_cas_worksheet_cell` uses the existing in-place parser
on detached state. A successful command retains its mutations; a failed command
discards parser mutations. Both append the worksheet cell to staged state, so a
cell and any resulting geometry form one atomic document revision. Empty or
oversized input is rejected without creating a cell.

`GrafitoApp::submit_cas_worksheet_cell` records one pre-operation undo snapshot
when the document changes, reports the existing toast/status feedback, and
retains erroneous input for correction. The CAS panel renders
`Document::cas_worksheet()` rather than transient `cas_history`; Clear removes
all persisted cells in one undoable document change.

The assistant context continues to enumerate only variables and visible,
non-private geometry. It deliberately has no worksheet field or accessor.

## Acceptance Criteria

1. Submitting `Simplify[x + 0]` from the CAS panel creates one local success
   cell containing input and output; save/open preserves it exactly.
2. A CAS command that creates geometry and its worksheet cell undo/redo as one
   document revision.
3. A failed non-empty CAS command creates one local error cell without keeping
   partial geometry mutations; the input remains editable for correction.
4. Empty and over-limit inputs create no cell or document mutation; malformed
   persisted worksheets above any bound are rejected before or during document
   validation.
5. Documents saved with schema v2 or without worksheet data still load with an
   empty worksheet; newly saved documents use schema v3.
6. Clear Worksheet is undoable and never exposes worksheet input/output through
   `document_context` or an optional assistant request.

## Test Strategy

- Core persistence/property tests cover schema migration, exact round trip,
  limits, malformed raw arrays, and clear behavior.
- Command integration tests cover success, error rollback, geometry-plus-cell
  atomicity, and rejected inputs.
- App tests cover snapshot behavior, input retention on error, and clear/undo.
- Full workspace fmt, strict Clippy, tests, release build, diff check, graph
  update, Debian packaging, Windows cross-build, and Wine help smoke test run
  before completion.

## Risks

- Persisted history can grow document/undo memory. Fixed count, per-cell, and
  aggregate-byte caps make the cost bounded.
- Existing generic command feedback treats errors as non-mutating. The CAS
  worksheet submission path performs its own semantic-difference snapshot so
  error cells remain undoable without weakening generic command behavior.
- A later editable worksheet could lose unsubmitted text. This slice has no
  drafts; a future editor must use the existing spreadsheet dirty/save staging
  pattern.
