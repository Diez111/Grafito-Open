# Offline Dynamic Math Workbench Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extend Grafito as a private, native mathematical workbench with automatic animation, professional visualization, and the highest-value local analytical workflows.

**Architecture:** Reuse `Document.variables` and its existing renderer invalidation pipeline for all real, spatial, and complex animation. Keep every mathematical result, imported dataset, animation setting, and visual artifact in the local document; network remains disabled unless the user explicitly invokes an already-consented remote assistant request. Deliver data fitting, locus, CAS worksheet, opacity controls, and UI upgrades as independent vertical slices so each can be tested and shipped safely.

**Tech Stack:** Rust 2021, egui/eframe, wgpu, serde JSON, existing Grafito core/command/render/UI crates.

---

## Product Boundaries

- No web view, npm package, account, telemetry, cloud sync, or background network access.
- Imported data and images require explicit native file selection; source paths and metadata are not retained unless the user opts in.
- Solver and animation work must be bounded, deterministic, and report domain/resource failures rather than freezing the UI.
- Dark and light themes share semantic tokens; translucent colors must preserve alpha and retain accessible text feedback.

### Task 1: Native ThinkingOrb [tdd:required]

**Files:**
- Modify: `crates/grafito-ui/src/animation.rs`
- Modify: `crates/grafito-ui/src/assistant.rs`
- Test: `crates/grafito-ui/tests/ui_tests.rs`

**DoD:** `Listening`, `Solving`, `Shaping`, and `Cancelling` draw natively in both themes, expose an accessibility label, and the assistant uses `Solving`/`Cancelling` without any web dependency.

### Task 2: General Variable Animation [tdd:required]

**Files:**
- Modify: `crates/grafito-core/src/document.rs`
- Modify: `crates/grafito-core/src/validation.rs`
- Modify: `crates/grafito-app/src/app.rs`
- Modify: `crates/grafito-command/src/commands.rs`
- Modify: `crates/grafito-command/src/command_registry.rs`
- Test: `crates/grafito-core/tests/document_integration.rs`
- Test: `crates/grafito-command/tests/command_transactions.rs`
- Test: `crates/grafito-app/src/tests.rs`

**DoD:** `Animate[]` creates a looping local `phase` parameter; `Animate[name]` and bounded forms configure existing scalar parameters atomically. The existing function, parametric, 3D, and complex render paths update through document variables without a second clock or network access. Existing documents deserialize to `PingPong` mode.

### Task 3: Alpha-Safe Object Styling [tdd:required]

**Files:**
- Modify: `crates/grafito-ui/src/color_picker.rs`
- Modify: `crates/grafito-ui/src/theme.rs`
- Modify: `crates/grafito-app/src/algebra.rs`
- Modify: `crates/grafito-app/src/render_2d.rs`
- Test: `crates/grafito-ui/tests/ui_tests.rs`
- Test: `crates/grafito-app/src/tests.rs`

**DoD:** Color editing preserves existing alpha, provides an opacity control and checkerboard preview, and correctly composes translucent colors in dark/light themes and CPU/GPU rendering.

### Task 4: Local Data Fitting Laboratory [tdd:required]

**Files:**
- Modify: `crates/grafito-geometry/src/statistics.rs`
- Modify: `crates/grafito-core/src/object.rs`
- Modify: `crates/grafito-command/src/commands.rs`
- Modify: `crates/grafito-app/src/panels.rs`
- Test: `crates/grafito-command/tests/`
- Test: `crates/grafito-core/tests/`

**DoD:** Explicit local CSV/TSV import feeds linked document data; polynomial, exponential, logarithmic, power, sinusoidal, and user-selected fits provide residuals, RMSE, and clear invalid-data errors without retaining source paths or transmitting rows.

### Task 5: Persistent Dynamic Locus and Trace [tdd:required]

**Files:**
- Modify: `crates/grafito-core/src/object.rs`
- Modify: `crates/grafito-core/src/document.rs`
- Modify: `crates/grafito-command/src/commands.rs`
- Modify: `crates/grafito-app/src/input.rs`
- Modify: `crates/grafito-app/src/render_2d.rs`
- Test: `crates/grafito-command/tests/`
- Test: `crates/grafito-core/tests/`

**DoD:** A local driver/target relationship produces a bounded, persistent locus or trace that updates deterministically under variable animation and records no pointer telemetry.

### Task 6: Persistent CAS Worksheet and Analysis Studios [tdd:required]

**Files:**
- Modify: `crates/grafito-core/src/document.rs`
- Modify: `crates/grafito-core/src/persistence.rs`
- Modify: `crates/grafito-app/src/panels.rs`
- Modify: `crates/grafito-command/src/commands.rs`
- Test: `crates/grafito-core/tests/persistence_properties.rs`
- Test: `crates/grafito-app/src/tests.rs`

**DoD:** CAS cells, probability/inference views, and dynamics controls persist locally, make assumptions and numerical tolerances visible, and never include worksheet or dataset contents in an optional assistant request without per-request consent.

### Task 7: Release and UX Verification [tdd:required]

**Files:**
- Modify: `README.md`
- Modify: `docs/commands.md`
- Modify: `CHANGELOG.md`
- Test: relevant crate tests and render regressions

**DoD:** Each completed vertical slice has direct unit/integration coverage, visual/accessibility smoke coverage where feasible, `cargo fmt --all -- --check`, strict workspace Clippy, workspace tests, release build, graph update, and rebuilt Debian/Windows artifacts.
