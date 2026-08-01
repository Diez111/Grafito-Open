# Verified Assistant Graph Proposals Design

## Problem

The remote assistant can currently propose nearly every registered command even when
the model has no object-label context, the documented signature diverges from the
handler, the output is only visible in 3D, or the resulting expression has no drawable
geometry. A successful command toast can therefore leave an empty canvas.

## Reframe

The product promise is not that the model can emit every internal command. It is that an
assistant proposal either creates a visible graph in the current 2D canvas or is not
offered as an executable action. Advanced construction, analysis, and 3D operations stay
available through normal UI/CAS flows until they have an equally strong verification path.

## Approach

1. Expose a deliberately small assistant proposal allowlist: explicit real functions and
   2D complex domain coloring. The catalog and app-side validator use the same function.
2. Register `DomainColoring` with its actual handler syntax and validate complex
   expressions, finite ordered bounds, and bounded resolution before mutation.
3. On `Aplicar en Grafito`, execute the exact command against a detached staged document.
   Hide pre-existing objects in a detached inspection clone and require nonempty object
   geometry from `Renderer::build_geometry_static(..., include_overlays=false)`.
4. Commit only the preflighted staged document, preserving undo, toast, and construction
   protocol behavior. A rejected proposal never mutates the live document.
5. Teach the model the narrow contract: use only catalog entries, lower-case calls with
   parentheses, prefer `DomainColoring` for a complex function, and ask a concise question
   when the request requires labels, a 3D view, or unsupported construction.
6. Correct the Complex panel guidance to offer domain coloring rather than claiming a
   deformed `ComplexGrid` is phase coloring.

## Scope

Included:

- `Function[expr]` and `DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]` proposals.
- Detached preflight plus non-overlay geometry check.
- Direct command/renderer/app integration regressions.
- Prompt, catalog, panel hints, and user feedback corrections.

Excluded:

- Autonomous command execution.
- Object-label constructions, constraints, transformations, 3D/4D, and text-only commands.
- Pixel readback or a dependency on a physical GPU; CPU geometry is the acceptance gate.

## Technical Design

`grafito-command::assistant_context` owns `is_assistant_proposable(canonical)` and filters
the remote catalog through it. The app validator uses the same predicate before accepting a
fenced proposal. `Document::detached_clone_for_staging` exposes the existing cache-detached
transaction clone.

`GrafitoApp` adds a private assistant preflight routine: stage through `process_input`, reject
errors, record newly-created object IDs, hide objects existing before the proposal in a second
detached clone, and build 2D geometry with overlays disabled. Empty vertices or indices reject
the proposal with an actionable toast. A successful preflight replaces the live document with
the already staged state and uses the normal snapshot/outcome/protocol helpers exactly once.

`DomainColoring` becomes registered and enforces a parseable complex expression bound to the
current complex symbol, finite ordered domain, and a conservative `[16, 300]` resolution.

## Acceptance Criteria

1. The assistant catalog never includes unverified, label-dependent, 3D, text-only, or known
   non-rendered commands.
2. `Function[sin(x)/(x^2+1)]` and `DomainColoring[1/z,-2,2,-2,2,160]` preflight and commit.
3. `Function[1/0]`, malformed complex expressions, zero/oversized resolution, and invalid
   bounds reject without document/undo/protocol mutation.
4. A proposal cannot claim success if isolated new-object geometry is empty.
5. Complex panel wording accurately distinguishes domain coloring from a transformed grid.
6. Tests cover command parsing, catalog filtering, staged atomicity, and static geometry.

## Test Strategy

- `grafito-command` integration tests for DomainColoring validation and atomic failures.
- `grafito-render` headless tests for nonempty Function/DomainColoring geometry without
  overlays and invalid-expression emptiness.
- `grafito-app` tests for allowlisted assistant validation and preflight accept/reject paths.
- Existing GPU tests remain supplemental: CPU geometry is mandatory and GPU adapter tests
  validate the alternate execution path where available.

## Risks

- Static geometry does not cover every renderer route, hence the allowlist is intentionally
  limited to variants proven by headless geometry tests.
- A valid graph outside the view is rejected rather than silently committed; this is safer than
  a blank success and can later evolve into an explicit "apply and frame" action.
- Building the Rust workspace can recreate substantial artifacts; validation stays scoped to
  touched crates and all `target/` artifacts are cleaned after verification.
