# Centered Color and Assistant Knowledge Design

## Problem

The Geometry 3D inspector uses egui's native inline color popup for 4D/N-D object colors. It opens from the lower-right of the triggering swatch, looks unlike Grafito's existing HSV picker, and can visually dominate the inspector. Separately, the Assistant receives a byte-limited text catalog assembled from duplicated capability metadata and only the first registered syntax form, so valid aliases and alternate forms can be omitted or rejected.

## Reframe

The problem is not that the Assistant lacks a large external database. It lacks one authoritative, bounded projection of Grafito's executable knowledge. A runtime dependency on Graphify, Python, or the repository graph would add latency, privacy risk, and nondeterminism without making the parser more correct. Graphify remains the development-time source graph; the runtime answer is a Rust-native knowledge graph derived from the command registry and execution policy.

## Approach

- Route every object and polychoron-fill color action through Grafito's existing HSV picker in one centered, theme-owned dialog.
- Represent the color dialog target explicitly so edge color and optional 4D fill color retain their current staged replacement and undo semantics.
- Build a process-local `AssistantKnowledgeGraph` once from `CommandSpec` nodes and assistant execution-policy edges. Retrieval ranks canonical names, aliases, categories, help, forms, and policy keywords deterministically.
- Render all relevant safe syntax forms within the existing remote byte budget. The graph keeps command names, aliases, forms, category, help, 2D/3D view, and render proof together without sending Graphify output, raw document fingerprints, paths, or data rows remotely.
- Correct metadata forms whose registry and live parser already disagree, including optional DomainColoring bounds and optional attractor parameters. Keep target-dependent, file, Script, and destructive commands non-actionable.

## Scope

### In scope

- Centered and theme-consistent color dialog for Algebra, 4D edge color, N-D edge color, and 4D fill color.
- Typed picker target state, staged fill-color edit helper, visual placement/accessibility tests, and no-native-popup regression coverage.
- Rust-native Assistant knowledge graph/retrieval using registry metadata and existing assistant capabilities.
- Deterministic alias/category/form retrieval; all safe registered syntax variants; corrected capability arity for ImplicitCurve and registry metadata for existing optional forms.
- Graphify `0.8.38` remains installed and updated as the offline developer graph.

### Out of scope

- Calling Graphify, parsing Markdown, loading `graphify-out`, or starting Python from the shipped application.
- Sending document fingerprints, private data, file paths, caches, or full project documentation to remote providers.
- Making label-dependent constructions, Scripts, external imports, deletes, exports, or arbitrary commands directly actionable.
- Adding a 4D GPU compute shader in this UI/reliability change.

## Technical Design

`GrafitoApp` replaces `Option<(ObjectId, HsvColorPicker)>` with an explicit active-picker struct containing the object ID, a `ColorPickerTarget`, and picker state. `ObjectColor` uses the existing generic staged replacement. `RegularPolychoronFill` clones and replaces only the optional `fill_color`. The centered non-resizable dialog uses a stable ID, foreground order, safe viewport constraints, Grafito theme tokens, and the custom HSV wheel/preview/favorites renderer. Inspector swatches open this dialog rather than calling `color_edit_button_srgba_unmultiplied`.

`grafito-command::assistant_context` retains execution proof and view policy but creates an immutable `AssistantKnowledgeGraph` from `command_registry::all()` plus policy references. Each node links canonical command, aliases, category, help, every safe syntax form, and optional executable policy. Retrieval tokenizes the question, scores graph edges deterministically, then renders fitting entries under the request budget. Syntax and arity for registered commands come from `CommandSpec`; fallback capability syntax remains only for existing graphable commands not yet represented in the registry. The app still sends only the rendered catalog string through the existing validated request transport, avoiding an unnecessary schema break.

## Acceptance Criteria

1. Clicking a 4D/N-D Inspector color swatch opens the custom Grafito picker centered inside the viewport, never the native trigger-anchored egui picker.
2. Edge-color and fill-color edits change only their intended property, create one undo entry for a real change, and preserve history on a no-op or missing target.
3. Assistant catalog retrieval includes aliases and all relevant safe syntax variants, including the three-argument ImplicitCurve and parametric Surface3D forms, within its byte limit and deterministic order.
4. Every executable graph node resolves to a valid command capability and can only expose literal-safe forms with compatible arity.
5. No Graphify output, raw document fingerprint, local path, data table contents, or credential reaches a remote Assistant request.
6. Existing 2D/Complex GPU and 4D CPU/GPU fallback behavior remains unchanged.

## Test Strategy

- TDD source and pure-function tests for centered color-dialog bounds, picker-target edit semantics, and Inspector routing.
- Unit tests for knowledge-graph aliases, alternate syntax forms, ordering, budget behavior, and unsafe-form exclusion.
- Registry/integration tests for corrected optional public forms and assistant proposal arity.
- Existing Assistant request/transport tests prove byte accounting and privacy boundaries still hold.
- Run focused crate tests, workspace Clippy/tests/release build, graphify update, and package validation.

## Risks

- A full registry form may be label-dependent despite its syntax. Mitigation: filter executable forms by argument kind and retain detached preflight before Apply.
- A centered dialog could constrain narrow windows. Mitigation: use safe viewport constraints and retain the native minimum window size.
- Metadata corrections can expose handler behavior that was accidentally blocked. Mitigation: add command-level tests before changing each signature.
