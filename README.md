<p align="center">
  <img src="assets/grafito-icon-256x256.png" alt="Grafito" width="128" />
</p>

<h1 align="center">Grafito</h1>

<p align="center">
  <a href="https://github.com/Diez111/Grafito/releases"><img src="https://img.shields.io/github/v/release/Diez111/Grafito?include_prereleases&label=version&color=blue" alt="Version" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPLv3%2B-blue.svg" alt="License: GPLv3+" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.78%2B-orange.svg" alt="Rust 1.78+" /></a>
  <a href="https://github.com/Diez111/Grafito/stargazers"><img src="https://img.shields.io/github/stars/Diez111/Grafito?style=social" alt="Stars" /></a>
</p>

<p align="center">
  <b>GPU-accelerated interactive geometry, algebra, statistics, and calculus</b><br />
  Built from scratch in Rust. Powered by WebGPU.
</p>

---

## Installation

**Debian / Ubuntu (`.deb`)**

```bash
wget https://github.com/Diez111/Grafito/releases/latest/download/grafito_amd64.deb
sudo dpkg -i grafito_amd64.deb
```

**Build from source**

```bash
# Dependencies
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4

# Clone and build
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo run -p grafito-app --release
```

> Requires Rust 1.78+. GPU compute shaders require Vulkan, Metal, or DX12 support.

---

## Features

### Interactive 2D Canvas

| | | |
|---|---|---|
| **32+ object types** | Points, Lines, Circles, Ellipses, Parabolas, Hyperbolas, Polygons, Regular Polygons, Functions, Parametric Curves, Polar Curves, Implicit Curves | |
| **Constructions** | Tangent, Perpendicular Bisector, Angle Bisector, Midpoint, Vector, Ray, Intersect, Locus | |
| **Transformations** | Translate, Rotate, Dilate, Reflect (objects and whole shapes) | |
| **Boolean operations** | Polygon Union, Intersection, Difference, XOR | |
| **Conics** | By foci, focus-directrix, or 5 arbitrary points. All support arbitrary rotation | |
| **Implicit curves** | `ImplicitCurve[x^2 + y^2 = 1]`, `ImplicitCurve[x*y = 1]`, `ImplicitCurve[x^3 + y^3 - 3xy = 0]` | |
| **Complex mapping** | `ComplexMapping[1/z, target]` applies any complex function to any 2D object, with automatic dotted asymptotes through singularities | |
| **Vector fields** | `VectorField2D[f(x,y), g(x,y)]` with normalized arrow tips | |
| **Fractals** | Mandelbrot, Julia, Burning Ship, Tricorn, Newton — parallel `rayon` evaluation with smooth HSV coloring | |
| **Statistics plots** | Histogram, Scatter Plot, Box Plot (with outliers), Regression Line | |
| **Expression binding** | Any parameter can be bound to a symbolic expression that re-evaluates when variables change | |
| **Numeric constraint solver** | Newton's method with analytical Jacobians: Distance, Angle, Tangent, Coincident, Horizontal, Vertical, Equal Length, Symmetry | |

### Math Engine (`grafito-geometry`)

**Computer Algebra System**

| Operation | Command | Method | Precision |
|-----------|---------|--------|-----------|
| Symbolic derivative | `Derivative[expr]` | AST with 29 pattern variants + chain rule | Exact |
| Numerical derivative | `Derivative[f, x]` | Centered differences O(h²), h=1e-6 | ~1e-12 |
| Definite integral | `Integral[expr, a, b]` | Adaptive Gauss-Legendre 5-point | ~1e-12 |
| Definite integral (GPU) | `Integral[f, a, b]` | Hybrid GPU/CPU path, Simpson n=1000 | O(h⁴) |
| Roots / Intercepts | `Root[f]`, `XIntercept[f]`, `YIntercept[f]` | Newton + bisection + refinement | 1e-9 |
| Extrema | `Extremum[f]` | Roots of first derivative | 1e-9 |
| Inflection points | `Inflection[f]` | Roots of second derivative | 1e-9 |
| Full analysis | `Analyze[f]` | All features combined: roots, extrema, inflection, intercepts, asymptotes, Taylor | 1e-9 |
| Taylor series | `Taylor[expr, x, x0, n]` | Symbolic differentiation + factorial | Order n |
| Factorization | `Factor[poly]` | Integer root detection in [-20, 20] | Symbolic |
| Expansion | `Expand[(a+b)(c+d)]` | Algebraic distribution | Symbolic |
| Simplification | `Simplify[expr]` | Constant folding + identities, 2-pass | Symbolic |
| Limits | `Limit[expr, x -> x0]` | Richardson extrapolation + bilateral | ~1e-4 |
| Matrices | `Determinant[[...]]`, `Inverse[[...]]` | Gaussian elimination with partial pivoting | ~1e-15 |
| Arc length | `ArcLength[f, a, b]` | Integral of sqrt(1 + f'²) | Numerical |
| Curvature | `CurvatureAt[f, x]` | kappa = |f''| / (1+f'²)^(3/2) | Numerical |
| Volume of revolution | `VolumeOfRevolution[f, a, b]` | pi * integral(f²) | Numerical |
| Surface of revolution | `SurfaceOfRevolution[f, a, b]` | 2*pi * integral(|f| * sqrt(1+f'²)) | Numerical |

**Unified expression system**: recursive AST parser, LaTeX preprocessor (`\frac`, `\sqrt`, `\sin`, `\pi`), implicit multiplication (`2x`, `x y`, `(x+1)(x-1)`), complex number parser with dedicated arithmetic.

**14 functions for special curves**: Cardioid, Rose, Archimedean Spiral, Logarithmic Spiral, Lissajous, Epicycloid, Hypocycloid.

**10 strange attractors**: Lorenz, Rossler, Thomas/Butterfly, Aizawa, Chen, Halvorsen, Dadras, Chua, Sprott, Three-Scroll — all solved with adaptive RK4.

**Differential equations**: Euler and RK4 integrators (1st order and 2D systems).

**Special functions**: Gamma (Lanczos g=7), Beta, Bessel J/Y/I (series up to 100 terms), Error (Abramowitz & Stegun 7.1.26), Complementary Error, Digamma (recurrence + asymptotic).

**4D objects**: Hypercube (tesseract) and Hypersphere (3-sphere) with rotation and perspective projection.

### Statistics and Probability

**Descriptive**: Mean, Median, Mode, StdDev, Variance, Quantile, IQR, Covariance, Correlation.

**Regressions**: Linear (least squares with R²), Polynomial (Gaussian elimination), Exponential, Logarithmic, Power.

**17 probability distributions** (PDF, CDF, Quantile where available): Normal, Binomial, Poisson, t-Student, Chi-squared, F, Exponential, Geometric, Hypergeometric, Logistic, Weibull, Uniform, Gamma, Beta, Cauchy, Pareto, Rayleigh, Laplace, Negative Binomial.

**Inference**: t-test (1 and 2 samples), z-test, Chi-squared test, ANOVA (1-way), Confidence intervals (mean with t-Student for n < 30, proportion).

### 3D Viewport

| Object | Rendering | Notes |
|--------|-----------|-------|
| Point, Segment | Projected with clipping | — |
| Sphere | 3 orthogonal circles (32 segments) | Per-vertex Phong lighting |
| Cube | 12 edges with face normals | ±X, ±Y, ±Z lighting |
| Pyramid, Cone, Cylinder | Wireframe with lateral normals | — |
| Torus | Wireframe circles | — |
| Parametric surface | 20x20 mesh grid | GPU-evaluated |
| Parametric curve 3D | 500-segment polyline | GPU-evaluated |
| Strange attractor | Cached RK4 polyline | Color by object |
| Hypercube 4D | 16 vertices, 32 edges, 4D rotation | Perspective projection |
| Hypersphere 4D | Lat/lon mesh, 4D rotation | Wireframe |

**Camera**: orbit (right-drag), zoom (scroll), pan (left-drag in Select tool). Painter's algorithm depth sorting with simplified Phong illumination (ambient 0.3 + diffuse 0.7).

### GPU Compute Shaders (WebGPU via wgpu)

All three pipelines compile a WGSL shader with the user's expression embedded at runtime, create a bind group, and dispatch workgroups to fill a staging buffer readable by the CPU. Falls back to CPU path automatically if the GPU is unavailable.

| Pipeline | Evaluates | Usage |
|----------|-----------|-------|
| `function_compute` | `y = f(x)` on a 1D grid | Explicit function plots |
| `implicit_compute` | `f(x, y) = c` on a 2D grid | Implicit curves via marching squares |
| `parametric_compute` | `(x(t), y(t))` and surfaces `(x(u,v), y(u,v), z(u,v))` | Parametric curves and 3D surfaces |

### UI / UX

| Panel | Access | Purpose |
|-------|--------|---------|
| **Algebra** | Tab A | Object list, input bar, variables with sliders, animations, per-type filter |
| **Toolbar** | Tab T | 12 groups, 40+ tools with vector icons, contextual 2D/3D hiding, keyboard shortcuts |
| **Command Palette** | Ctrl+K | 70+ indexed commands across 12 categories, arrow navigation, syntax template insertion |
| **Math Keyboard** | Footer | 4 tabs: numeric, functions, letters, advanced |
| **Properties** | Right panel | Type, editable label, visibility toggle, color picker, real-time measurements |
| **Spreadsheet** | Tab S | Full grid editor, create points from cell coordinates |

**Tools**: Select, Point, Line, Segment, Ray, Vector, Perpendicular, Circle, Tangent, Polygon, Regular Polygon, Pencil, Eraser, Ellipse by Foci, Parabola (Focus-Directrix), Hyperbola by Foci, Conic by 5 Points, Function, Parametric Curve 2D, Polar Curve, Implicit Curve, Vector Field 2D, Locus, Distance, Area, Angle, Slope, Point 3D, Sphere 3D, Cube 3D, Root, Extremum, Inflection, Y-Intercept, X-Intercept, Intersect, Analyze, Slider, Button, Image.

**Quality of life**: ghost preview for construction tools, ripple effects on clicks, hierarchical snap (snap to analysis features, intersections, and curve edges), instant dark/light theme toggle.

### File Formats

| Format | Direction | Supported types |
|--------|-----------|-----------------|
| `.grafito` | Save / Load (JSON) | Full document serialization |
| SVG | Export | Point, Line, Circle, Polygon, Ellipse, Parabola, Hyperbola, Function, Text, ScatterPlot, RegressionLine + grid/axes |
| PNG | Export | Point, Line, Circle, Polygon, Ellipse, Function (Bresenham, midpoint circle, scanline fill) |
| TikZ | Export | LaTeX-compatible code for all 2D primitives |

---

## Architecture

```
grafito/
├── crates/
│   ├── grafito-app/         Desktop application (eframe) — UI, input, render orchestration
│   ├── grafito-core/        Document model, 32+ geometric object types, spatial index, constraints
│   ├── grafito-geometry/    Math engine: CAS, statistics, ODE, fractals, attractors, curves, booleans
│   ├── grafito-render/      wgpu render pipeline — 2D/3D tessellation, compute shaders, lighting
│   ├── grafito-ui/          egui components — toolbar, command palette, properties, color picker, themes
│   └── grafito-command/     Shared text command processor for desktop and FFI frontends
└── assets/                  Icons, WGSL shaders
```

| Crate | Key files |
|-------|-----------|
| `grafito-geometry` | `ast.rs`, `expr.rs`, `analysis.rs`, `boolean.rs`, `statistics.rs`, `ode.rs`, `fractals.rs`, `complex_expr.rs`, `special_functions.rs` |
| `grafito-core` | `document.rs`, `object.rs`, `analyzable.rs`, `constraints.rs`, `numeric_solver.rs`, `numeric_constraints.rs`, `spatial.rs` |
| `grafito-render` | `lib.rs`, `function_compute.rs`, `implicit_compute.rs`, `parametric_compute.rs`, `vector_compute.rs` |
| `grafito-ui` | `lib.rs`, `toolbar.rs`, `command_palette.rs`, `color_picker.rs`, `theme.rs` |
| `grafito-app` | `main.rs`, `app.rs`, `canvas.rs`, `snap.rs`, `tool_dispatcher.rs`, `algebra.rs`, `export.rs` |
| `grafito-command` | `commands.rs` |

---

## Tech Stack

| Layer | Technology | Version |
|-------|------------|---------|
| Language | Rust | 1.78+ |
| GUI | `eframe` / `egui` | 0.29 |
| GPU | `wgpu` (WebGPU -> Vulkan/Metal/DX12) | 22.0 |
| Linear Algebra | `glam`, `nalgebra` | 0.29 / 0.33 |
| Computational Geometry | `geo`, `spade` (Delaunay), `robust` | 0.29 / 2.10 / 1.1 |
| Arbitrary Precision | `rug` (GMP/MPFR, 53-256 bit) | 1.28 |
| Expression Parser | `evalexpr` | 11.3 |
| Complex Numbers | `num-complex` | 0.4 |
| Parallelism | `rayon` | 1.10 |
| Spatial Index | `rstar` (R-tree) | 0.12 |
| Serialization | `serde` + `serde_json` / `toml` / `ron` | 1.0 |
| Image Export | `image` | 0.25 |
| UUID | `uuid` | 1.10 |
| File Dialogs | `rfd` | 0.14 |

---

## Performance

| Technique | Where | Gain |
|-----------|-------|------|
| GPU compute shaders | Function/implicit/parametric evaluation | Massive parallelism on 1D/2D grids |
| Attractor cache | Hash(parameters) -> cached points | Only recompute on param change |
| Parallel fractals | `rayon::par_iter` over pixel rows | 4-8x speedup |
| Batch evaluation | `eval_batch_1d/2d` with AST fast path + fallback | Reduced parser overhead |
| Compiled expression cache | Reuse `evalexpr` trees for bound objects | Avoid re-tokenization |
| R-tree spatial index | `rstar` for O(log n) hit testing | 32+ object types at interactive rates |
| Analytical Jacobians | Constraint solver Newton method | Faster convergence vs. numerical gradients |
| Document snapshots | Clone on version change, `Arc::make_mut` for view | Avoid per-frame allocation |

---

## Controls

| Action | Input |
|--------|-------|
| Pan 2D | Drag on empty space, Space+drag, or middle-click |
| Zoom | Scroll wheel |
| Create object | Click (Point: single click) |
| Close polygon | Right-click (3+ vertices) |
| Cancel point | Right-click (1 pending), Escape |
| Select object | Click (Select tool) |
| Deselect | Click on empty space |
| Orbit 3D | Right-drag |
| Undo / Redo | Ctrl+Z / Ctrl+Y |
| Delete | Delete key |
| Command palette | Ctrl+K |
| Tools | F1 (Select), F2 (Point), F3 (Line), F4 (Circle), F5 (Polygon), F6 (Function), F8 (Sphere 3D), F9 (Cube 3D) |
| Analysis shortcuts | R (Root), E (Extremum), N (Inflection), Ctrl+Y (Y-Intercept), Ctrl+A (Analyze) |

---

## Development

### Verify

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

### Profile

```bash
cargo run -p grafito-app --features profile -- --profile
# Then connect puffin_viewer to 127.0.0.1:8585
```

### GPU compute shaders

Each pipeline embeds a WGSL shader with the user's expression compiled to bytecode at runtime. The shader iterates over `code_len` instructions, evaluates on a grid, and writes results to a staging buffer. See `grafito-render/src/<name>_compute.rs` + `<name>_compute.wgsl`.

---

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Write tests for new functionality
4. Run `cargo fmt --all && cargo clippy --workspace -- -D warnings && cargo test --workspace`
5. Commit using [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`)
6. Push and open a Pull Request

See [AGENTS.md](AGENTS.md) for detailed architecture and development conventions.

---

## License

GNU General Public License v3.0 or later. See [LICENSE](LICENSE).

```
Grafito — Interactive GPU-accelerated geometry, algebra, and calculus
Copyright (C) 2025-2026  Diez111

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.
```
