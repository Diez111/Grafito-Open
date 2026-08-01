# Persistent Dynamic Locus Design Doc

## Problem

Grafito reserves `Locus`, but currently rejects it because no persisted model
can remember a driver/target relationship or safely capture the resulting
trajectory during local animation.

## Reframe

The need is not another static sampled curve. It is a bounded local record of
the target point after the document has reached a valid, stable geometric
state. Pointer locations, timing, frames, and screen coordinates are neither
inputs nor persisted data.

## Approach

Extend the existing persisted `PencilObj` with optional `LocusBinding` metadata
instead of adding a new `GeoObject` variant. A bound pencil keeps the existing
bounded polyline rendering, picking, spatial indexing, export, and persistence
paths, while the binding provides semantic driver/target references.

The binding is also represented by a `Locus` constructive constraint. The
constraint is pure during propagation; `Document` appends at most one target
sample after constructive and numeric propagation has stabilized. This avoids
duplicate samples caused by numeric solver passes.

## Scope

In scope:

- `Locus[driver, target]` for two distinct 2D points.
- A persisted `LocusBinding { driver, target }` with bounded world-coordinate
  samples and endpoint-preserving decimation through `MAX_PENCIL_POINTS`.
- One initial target sample, then one distinct sample after a committed driver
  or target update, including local variable animation.
- Cascade deletion, validation, serialization round trip, assistant-context
  exclusion, command palette, autocomplete, and native two-click tool support.
- A visible algebra summary and a small native end marker/label for active
  loci while preserving the normal Pencil render path.

Out of scope:

- A geometric `Trace` command alias; `Trace` remains the established matrix
  trace command.
- Static parameter sweeping presented as a dynamic locus.
- Frame timestamps, pointer events, screen positions, network activity, or
  remote assistant disclosure of trajectory samples.
- Unbounded sampling, background workers, or a second animation clock.

## Technical Design

`crates/grafito-core/src/pencil.rs` will define serializable `LocusBinding` and
an optional `PencilObj::locus_binding`. A normal pencil remains unchanged when
the field is absent. A dynamic locus has a generated `L` label, references its
driver and target through `GeoObject::referenced_object_ids`, and is classified
as private for `document_context`.

`Document::try_add_locus(driver, target)` validates both inputs as distinct
points, initializes a bound pencil from the target position, and adds it as a
`Locus` constructive output. Constraint validation requires exact input/output
types and matching binding IDs. The `Locus` propagation arm has no geometry
mutation. After the final solver pass, `capture_locus_samples` resolves each
reachable Locus constraint in deterministic constraint order and appends only a
new finite target position.

Bound parameter recomputation will report changed point/circle IDs internally.
`advance_variable_animations` will perform variable updates, bound recompute,
constraint propagation, trace capture, validation, and one revision update on a
detached document. A propagation failure leaves the live animation/document
unchanged.

`Locus[driver, target]` is a registered construction command. `Tool::Locus`
selects driver then target from the canvas and calls the same core method. It is
placed in the Geometry 2D constraint group. The existing Pencil CPU/GPU shape
path draws the samples; app rendering adds a non-persistent end marker and
label for loci only.

## Acceptance Criteria

1. `Locus[A, B]` creates a persistent linked locus only for two distinct
   points and reports clear, atomic errors for missing, ambiguous, same, or
   non-point inputs.
2. Moving a driver updates dependent geometry first and appends one distinct
   target sample only after the final valid state.
3. `Animate` updates a variable-bound driver/target and appends one sample per
   successful animation update without a pointer or timestamp field.
4. Samples never exceed `MAX_PENCIL_POINTS`; decimation preserves first/latest
   endpoints, and all persisted coordinates remain finite.
5. Save/load preserves the relationship and later updates continue tracing;
   deleting either input cascades to the locus.
6. The locus is visible/selectable/exportable as a local polyline but omitted
   from assistant context.
7. The command palette/autocomplete and two-click native tool expose the
   feature without changing the matrix `Trace` command.

## Test Strategy

- Core integration tests cover construction, propagation order, one-sample
  capture, animation, duplicate suppression, deletion cascade, and capacity.
- Persistence tests cover round trip and malformed/missing binding references.
- Command transaction tests cover success, label ambiguity, type/same-input
  rejection, and command atomicity.
- App/UI tests cover toolbar visibility, status text, autocomplete, and the
  active-locus render path.
- Finish with workspace formatting, strict Clippy, workspace tests, release
  build, diff check, graph update, and fresh Debian/Windows artifacts.

## Risks

- Capturing inside the constructive solver loop would duplicate samples. The
  post-stabilization capture point prevents that.
- A live locus changes the document every animation update. The existing
  8,192-point bound and one batched revision per animation tick contain memory
  and cache pressure.
- Existing `PencilObj` users must retain their prior semantics. Optional serde
  metadata and unchanged default rendering preserve compatibility.
