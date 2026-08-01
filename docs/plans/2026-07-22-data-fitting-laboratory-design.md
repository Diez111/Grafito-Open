# Local Data Fitting Laboratory Design Doc

## Problem

Grafito can draw ad-hoc scatter plots and a legacy linear regression, but the
data is duplicated inside visual objects, has no persistent source identity,
and offers no reproducible model diagnostics. Users cannot explicitly import a
local two-column dataset, retain it in a document, fit it, inspect residuals,
or safely repeat the analysis after reopening the document.

## Reframe

The problem is reproducible local calibration, not a generic spreadsheet or a
cloud data platform. The design rejects three assumptions: a source path is
not required once numeric rows are imported; a general nonlinear optimizer is
not necessary for the first useful models; and importing data must not make it
available to an optional remote assistant. The narrowest useful product is a
bounded, two-column local dataset linked to a visible scatter plot and fitted
function.

## Approach

Use a balanced local-first approach. Add `DataTableObj` as the persistent,
path-free source of finite `(x, y)` rows. Extend the existing `FunctionObj`
with optional `FitMetadata`, so fitted curves reuse all CPU/GPU sampling and
export paths instead of adding a parallel renderer. `FitMetadata` references a
`DataTableObj` by `ObjectId` and contains a typed `FitKind` plus residuals,
RMSE, and R-squared diagnostics. The alternatives were a linear-only wedge,
which would not meet the requested fitting workflow, and a generic nonlinear
solver, which adds opaque convergence behavior and unbounded UI work.

## Scope

In scope: explicit native CSV/TSV selection, bounded UTF-8 two-column parsing,
persisted rows without the selected path, linked scatter plots, and linear,
polynomial, exponential, logarithmic, power, and deterministic sinusoidal
fits. Commands are `DataTable`, `FitLinear`, `FitPoly`, `FitExp`, `FitLog`,
`FitPow`, and `FitSin`.

Out of scope: background watching, arbitrary spreadsheet formulas, generic
nonlinear optimization, cloud import, source-path retention, telemetry, and
automatic disclosure of data rows or residuals to the assistant.

## Technical Design

`grafito-geometry::statistics` owns pure `FitKind`, `FitResult`, diagnostics,
expression generation, domain checks, and bounded sinusoidal frequency search.
`grafito-core` owns `DataTableObj` and `FunctionObj::fit`; its reference graph
cascades deletion of linked scatter plots and functions. The app reads a
user-selected file with a byte/row limit, parses CSV or TSV synchronously after
explicit selection, discards the path, and atomically inserts a table plus a
linked scatter plot. Command execution stages all insertions through the
existing transaction path. Assistant context skips any data-bearing or fitted
object, regardless of canvas visibility.

## Acceptance Criteria

1. A CSV or TSV file selected through the native dialog creates a local,
   bounded, path-free `DataTableObj` and linked scatter plot in one undo step.
2. `FitLinear`, `FitPoly`, `FitExp`, `FitLog`, `FitPow`, and `FitSin` create
   linked visible functions with residuals, RMSE, and R-squared in original
   y-units.
3. Invalid lengths, non-finite rows, insufficient data, singular models, and
   invalid logarithmic/power domains return clear errors without mutation.
4. Deleting a table removes linked analysis objects.
5. Old documents deserialize unchanged, and optional assistant context omits
   imported rows and fit diagnostics.

## Test Strategy

Start with geometry tests for exact/noisy fits and explicit domain failures.
Add core persistence/reference tests for path absence and cascade deletion.
Add command transaction tests for every model and atomic failure. Add app
parser tests for CSV/TSV headers, quoted cells, malformed rows, size bounds,
and one-snapshot import behavior. Finish with existing workspace format, lint,
test, release, graph, and package gates.

## Risks

Sinusoidal fitting can overfit or stall: use a documented fixed candidate grid
and a bounded row limit. Large imports can exhaust document budgets: reject by
byte count, row count, non-finite values, and existing serialized-document
validation. Persisting diagnostics could leak through assistant serialization:
exclude all data-bearing objects at the context boundary.
