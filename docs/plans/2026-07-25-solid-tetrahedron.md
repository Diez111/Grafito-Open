# Solid Tetrahedron Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a native, persistent and visibly filled regular `Tetrahedron[x, y, z, edge]` primitive.

**Architecture:** Persist only a tetrahedron centroid and edge length in a new `GeoObject` wrapper. Derive vertices, outward faces and edges in `grafito-geometry`, then consume that one topology in validation, both 3D render paths, picking, commands and the assistant's existing explicit-Apply pipeline.

**Tech Stack:** Rust 2021, serde, egui/wgpu, `glam`, Cargo workspace tests.

---

### Task 1: Define the regular tetrahedron geometry and core object

**Files:**
- Modify: `crates/grafito-geometry/src/types3d.rs:319-425`
- Modify: `crates/grafito-core/src/object.rs:11-64, 1168-1322`
- Modify: `crates/grafito-core/src/validation.rs:413-892`
- Modify: `crates/grafito-core/src/document.rs:2944-3277`
- Test: inline geometry tests and `crates/grafito-core/tests/domain_validation.rs`

**Step 1: Write the failing tests**

Add tests that construct `Tetrahedron3D::new(Point3D::new(0.0, 0.0, 0.0), 2.0)` and assert:

```rust
assert_eq!(tetrahedron.vertices().len(), 4);
assert_eq!(tetrahedron.faces().len(), 4);
assert_eq!(tetrahedron.edges().len(), 6);
assert_relative_eq!(tetrahedron.volume(), 8.0 / (6.0 * 2.0_f64.sqrt()));
```

Assert that each edge has length 2, the vertex mean is the center, face normals
point away from the center and invalid dimensions are rejected without a
document mutation.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-geometry tetrahedron --lib`

Expected: compile failure because `Tetrahedron3D` does not exist.

**Step 3: Write the minimal implementation**

Add `Tetrahedron3D { center, edge_length }`, validate finite positive inputs,
and derive the four fixed Y-up vertices, outward face index triples and six
edge pairs. Add `Tetrahedron3DObj` with the standard `id`, `label`, `color`,
`visible`, `width` and `fill_color` fields; add all `GeoObject` accessors,
`RenderSpace::D3`, validation and spatial-index exclusion.

**Step 4: Run the focused tests to verify they pass**

Run: `cargo test -p grafito-geometry tetrahedron --lib && cargo test -p grafito-core domain_validation --test domain_validation`

Expected: PASS.

**Step 5: Preserve working-tree boundaries**

Do not stage or commit unless explicitly requested. Inspect the diff before
starting the next task.

### Task 2: Persist and parse the primitive atomically

**Files:**
- Modify: `crates/grafito-core/src/persistence.rs:13-113, 416-438`
- Modify: `crates/grafito-command/src/command_registry.rs:1755-1800`
- Modify: `crates/grafito-command/src/commands.rs:2-13, 2457-2590, 2797-2800`
- Test: `crates/grafito-command/tests/analytic_geometry_3d.rs`
- Test: `crates/grafito-command/tests/command_transactions.rs`
- Test: `crates/grafito-command/tests/command_registry.rs`

**Step 1: Write the failing tests**

Add a successful `Tetrahedron[1,2,3,2*pi]` test that finds exactly one typed
object with center `(1,2,3)` and edge `2*pi`. Add isolated transaction tests
for zero, negative and non-finite edge values, preserving the document before
the command. Add a save/load round trip that preserves the object and styles.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-command tetrahedron`

Expected: command is unrecognized or the new registry test fails.

**Step 3: Write the minimal implementation**

Register `Tetrahedron[x, y, z, edge]` with four `Number` arguments and
`CreatesObject`. Parse with the existing finite-expression helper, reject
nonpositive edge with the established Spanish error style, create the object
through `insert_command_object!`, and add it to the recognized 3D bad-arity
fallback. Update persistence schema handling only as required by the existing
serde envelope contract; preserve older document readability.

**Step 4: Run the focused tests to verify they pass**

Run: `cargo test -p grafito-command tetrahedron && cargo test -p grafito-core persistence --lib`

Expected: PASS.

**Step 5: Preserve working-tree boundaries**

Do not stage or commit unless explicitly requested.

### Task 3: Render four faces in GPU and CPU 3D paths

**Files:**
- Modify: `crates/grafito-render/src/depth_3d.rs:112-170, 596-697, 1108-1345`
- Modify: `crates/grafito-app/src/render_3d.rs:265-322, 368-467, 1240-1285, 1472-1601`
- Test: `crates/grafito-render/tests/headless_render.rs`
- Test: `crates/grafito-app/src/tests.rs`

**Step 1: Write the failing tests**

Create a visible opaque tetrahedron and assert `WorldMesh` reserves/emits four
solid triangles and six wire edges. Add a hidden-object case and a 3D picker
case that gets a coarse AABB hit for the object.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-render tetrahedron && cargo test -p grafito-app tetrahedron --lib`

Expected: compile failure because the renderer has no tetrahedron branch.

**Step 3: Write the minimal implementation**

Add tetrahedron output accounting and `append_tetrahedron` to `WorldMesh`,
using four derived face triples, `append_solid_triangle` and six wire lines.
Add CPU fallback bounds, painter ordering, coarse picking and four depth-sorted
projected fill triangles plus their six edge overlays. Respect `visible` and
`fill_color` alpha exactly as existing solids do.

**Step 4: Run the focused tests to verify they pass**

Run: `cargo test -p grafito-render tetrahedron && cargo test -p grafito-app tetrahedron --lib`

Expected: PASS.

**Step 5: Preserve working-tree boundaries**

Do not stage or commit unless explicitly requested.

### Task 4: Expose the command to the verified assistant

**Files:**
- Modify: `crates/grafito-command/src/assistant_context.rs:238-324, 487-652`
- Modify: `crates/grafito-assistant/src/lib.rs:45-48, 991-1004, 1065-1119`
- Test: `crates/grafito-command/src/assistant_context.rs:849-857`
- Test: `crates/grafito-app/src/assistant.rs:2729-2772, 2940-2980`
- Test: `crates/grafito-assistant/src/lib.rs:1729-1765, 2132-2189`

**Step 1: Write the failing tests**

Assert that the catalog declares `Tetrahedron` as a four-argument
`WorldMeshThreeD` capability and that a visible direct proposal preflights.
Assert each remote transport prompt tells the model to emit one fenced
`Tetrahedron[x,y,z,edge]` command, while `Polyhedron` remains unsupported.

**Step 2: Run the focused tests to verify they fail**

Run: `cargo test -p grafito-command assistant_context && cargo test -p grafito-assistant tetrahedron`

Expected: current guidance still requires six `Segment3D` commands.

**Step 3: Write the minimal implementation**

Register the existing `ThreeD`/`WorldMeshThreeD` capability and replace only
tetrahedron-specific guidance with the direct supported command. Preserve the
labeled legacy wireframe parser for old assistant responses and preserve all
explicit Apply/preflight controls.

**Step 4: Run the focused tests to verify they pass**

Run: `cargo test -p grafito-command assistant_context && cargo test -p grafito-app assistant && cargo test -p grafito-assistant tetrahedron`

Expected: PASS.

**Step 5: Preserve working-tree boundaries**

Do not stage or commit unless explicitly requested.

### Task 5: Format, verify, graph and package

**Files:**
- Modify: generated `graphify-out/` metadata only through `graphify update .`
- Build: `packaging/build/grafito_1.2.20~beta_amd64.deb`

**Step 1: Run the required verification gates**

Run:

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
graphify update .
```

**Step 2: Rebuild and validate the package**

Run the repository packaging command, inspect the generated artifact hash, and
only reinstall with the existing privileged workflow if the build succeeds.

**Step 3: Report evidence**

Report focused and workspace test results, renderer fallback coverage, package
version/hash and any GPU-only check that could not run in the environment.
