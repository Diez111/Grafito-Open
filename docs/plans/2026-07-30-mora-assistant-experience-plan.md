# Mora Assistant Experience Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the Assistant turn-height regression and give the local mathematical assistant a lightweight, clear identity named Mora.

**Architecture:** Keep the Assistant state and remote privacy boundary unchanged. `grafito-app` owns a single cached, embedded PNG texture; `grafito-ui` receives only its `TextureId` through a small visual configuration value and renders it in the header, empty state, and pending indicator. The existing `ThinkingOrb` remains the only continuously animated primitive and requests frames only while a request is pending.

**Tech Stack:** Rust 2021, egui 0.29, eframe/wgpu, existing `image` PNG decoder, SVG source asset rasterized to a 128 px PNG at build-time authoring.

---

### Task 1: Lock Down the Transcript Height Regression

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Test: `crates/grafito-ui/src/assistant.rs`

**Step 1: Write the failing test**

Render a short user `ConversationTurn` into a 320x600 egui context and assert that the scoped turn height is less than 35% of the transcript height.

**Step 2: Run test to verify it fails**

Run: `cargo test -p grafito-ui short_user_turn_does_not_consume_transcript_height`

**Step 3: Implement the minimal fix**

Replace the direct footer `ui.with_layout(Layout::right_to_left(...))` with a fixed-height `allocate_ui_with_layout` row sized to `ui.spacing().interact_size.y`.

**Step 4: Run test to verify it passes**

Run the same focused test and the Assistant UI tests.

### Task 2: Add the Mora Asset and Texture Cache

**Files:**
- Create: `assets/mora.svg`
- Create: `assets/mora.png`
- Modify: `crates/grafito-app/src/app.rs`
- Test: `crates/grafito-app/src/app.rs`

**Step 1: Add the original transparent mascot artwork**

Create a compact 128x128 visual of Mora, a calm violet-and-graphite mathematical owl. Rasterize it once to a transparent PNG for the existing `image` decoding path.

**Step 2: Write the failing cache test**

Mirror the splash texture test: requesting the same `ColorImage` twice must retain and reuse the same `TextureId`.

**Step 3: Implement cached embedding**

Add `mora_texture: Option<egui::TextureHandle>` to `GrafitoApp`, decode `include_bytes!("../../../assets/mora.png")` only on the first visible Assistant frame, and expose only `TextureId` to UI code. Failed decoding must degrade to no avatar rather than panic.

**Step 4: Run focused tests**

Run `cargo test -p grafito-app splash_tests --lib` and the new cache test.

### Task 3: Integrate Mora Into the Assistant UI

**Files:**
- Modify: `crates/grafito-ui/src/assistant.rs`
- Modify: `crates/grafito-app/src/assistant.rs`
- Test: `crates/grafito-ui/src/assistant.rs`
- Test: `crates/grafito-ui/tests/ui_tests.rs`

**Step 1: Introduce a visual configuration boundary**

Add an `AssistantVisuals` value carrying an optional local `TextureId`; thread it through the two existing Assistant hosts without exposing app or image-decoding state to `grafito-ui`.

**Step 2: Render meaningful, accessible state**

Use Mora in the header, empty state, and pending card. Name Assistant turns `Mora`; provide a tooltip/accessibility label. Keep idle rendering static. During solving/cancelling, retain the existing `ThinkingOrb` beside the image and its 50 ms repaint schedule.

**Step 3: Improve the entry UX**

Use concise Mora-specific onboarding copy and a clearer composer hint while preserving quick prompts, explicit Apply, attachment consent, and all existing actions.

**Step 4: Add regressions**

Test title/state labels, texture-free fallback rendering, and that the pending state still maps to the existing `ThinkingOrb` rather than a GIF or per-frame texture upload.

### Task 4: Validate and Package

**Files:**
- Modify if needed: `docs/spec/assistant.md`

**Step 1: Check asset budget**

Verify the PNG is 128x128 and modest in size; no additional runtime dependency is introduced.

**Step 2: Run verification**

Run `rustfmt --edition 2021 --check` for touched files, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace --release`.

**Step 3: Update project knowledge**

Run `graphify update .` and record the final assistant UI, asset-cache, and resource-use decisions.
