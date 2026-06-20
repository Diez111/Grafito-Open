<p align="center">
  <img src="assets/grafito-icon-256x256.png" alt="Grafito" width="128" />
</p>

<h1 align="center">Grafito Open</h1>

<p align="center">
  <a href="https://github.com/Diez111/Grafito-Open/releases"><img src="https://img.shields.io/github/v/release/Diez111/Grafito-Open?include_prereleases&label=version&color=blue" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPLv3%2B-blue.svg" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.78%2B-orange.svg" /></a>
  <a href="https://github.com/Diez111/Grafito-Open/stargazers"><img src="https://img.shields.io/github/stars/Diez111/Grafito-Open?style=social" /></a>
  <a href="https://github.com/Diez111/Grafito-Open/actions"><img src="https://img.shields.io/github/actions/workflow/status/Diez111/Grafito-Open/ci.yml?label=CI" /></a>
</p>

<p align="center">
  <b>GPU-accelerated interactive geometry, algebra, statistics, and calculus.</b><br />
  Built from scratch in Rust. Powered by WebGPU. Over 395 CAS commands, native symbolic CAS, 11 linear algebra ops, 4 WGSL compute shaders, export to PNG/PDF/LaTeX.
</p>

---

> :es: Este documento también está disponible en [español](README.md).

## Table of contents

- [Why Grafito Open?](#why-grafito-open)
- [Features](#features)
  - [Interactive 2D Canvas](#interactive-2d-canvas)
  - [Math Engine (`grafito-geometry`)](#math-engine-grafito-geometry)
  - [3D Geometry](#3d-geometry)
  - [Statistics and Probability](#statistics-and-probability)
  - [GPU Compute Shaders (WGSL)](#gpu-compute-shaders-wgsl)
  - [UI / UX](#ui--ux)
  - [Export](#export)
- [Installation](#installation)
- [Quick start](#quick-start)
- [Architecture](#architecture)
- [Command reference](#command-reference)
- [Development](#development)
- [Tests and quality](#tests-and-quality)
- [Contributing](#contributing)
- [License](#license)

---

## Why Grafito Open?

Grafito is an interactive mathematical grapher written from scratch in Rust. Unlike desktop alternatives, it ships a **native symbolic CAS** (no `evalexpr` or SymPy dependency), advanced linear algebra (eigenvalues, SVD, Cholesky), and a WGSL compute shader engine that evaluates **50 opcodes** in parallel on the GPU. The project aims to surpass GeoGebra in mathematical coverage, performance, and code transparency.

- **~61,000 lines of Rust** across 6 crates
- **587 tests** (unit + integration)
- **0 `unsafe` blocks** in production code
- **Complete CI** with `fmt`, `clippy -D warnings`, tests, docs

## Features

### Interactive 2D Canvas

- **36 geometric object types**: points, lines (segment/ray/vector/infinite), circles, polygons, ellipses, rotated parabolas/hyperbolas, functions, advanced conics, text
- **Constructive constraints** (midpoint, perpendicular, parallel, point-on-object) and **numeric constraints** (Levenberg-Marquardt with analytic Jacobians: distance, angle, tangent, coincident, horizontal, vertical, equal-length, symmetry)
- **2D boolean operations** on polygons (union, intersection, difference, XOR)
- **Snap-to-grid** and **snap-to-features** (curves, intersections)
- **Multi-level undo/redo** with document persistence

### Math Engine (`grafito-geometry`)

314 public functions organized in modules:

| Module | Coverage |
|--------|----------|
| `ast` | Parser with ~50 variants (Const, Var, Add/Mul/Div/Pow, direct & inverse trig, hyperbolics, comparisons, piecewise, Re/Im/Arg/Conj, Bessel J/Y/I, Gamma, Erf, Erfc, Digamma, sec/csc/cot, atan2, mod, sign, heaviside, cbrt, clamp, sum, product) |
| `symbolic` | **Native CAS** (no evalexpr): symbolic differentiation with chain rule, polynomial/trig/exp/ln integration, algebraic simplification, solve (linear, quadratic, **Cardano** for cubic, **Ferrari** for quartic, Newton for degree >4), Taylor, limit, factor, expand, substitute |
| `matrices` | add/sub/mul/det/inverse/trace + **eigenvalues, eigenvectors, SVD, LU, QR, Cholesky, rank, null_space, condition_number, Frobenius/spectral norm** (via nalgebra) |
| `statistics` | mean, median, mode, variance, std-dev, min, max, range, quantiles, IQR, covariance, correlation, linear/polynomial/exponential/logarithmic/power regression, histogram, boxplot, frequency table |
| `distributions` | normal PDF/CDF/quantile, binomial PMF/CDF, Poisson PMF/CDF, Student-t PDF, Chi², ANOVA, Beta, Gamma, CI, ChiSqTest |
| `analysis` | Root, Extremum, Inflection, YIntercept, XIntercept, ArcLength, CurvatureAt, TangentAt, NormalAt, VolumeOfRevolution, SurfaceOfRevolution, Analyze |
| `ode` | Euler, RK4, **adaptive RK45** (RKF45 with step control), systems, **backward Euler** with Newton-Jacobian for stiff |
| `boolean` | 2D boolean ops on polygons via `geo` crate |
| `fractals` | Mandelbrot, Julia (dendrite/siegel/galaxy), BurningShip, Tricorn, Newton |
| `attractors` | Lorenz, Rössler, Thomas, Aizawa, Chen, Halvorsen, Dadras, Chua, Sprott, Three-scroll (RK4 integration) |
| `special_functions` | gamma, ln_gamma, beta, bessel_j/y/i, erf, erfc, digamma |
| `special_curves` | cardioid, rose, spirals (Archimedean, logarithmic), Lissajous, epi/hypocycloid, **astroid, deltoid, tractrix, brachistochrone** |
| `conformal` | 13 algebraic maps (Inversion, Power, Exp, Log, trig, Joukowski, Möbius), complex_expr parser, domain coloring |
| `dd` | **Double-double arithmetic** (extended precision) — unique advantage vs GeoGebra |
| `precision` | Constants and numerical utilities |

### 3D Geometry

- 3D Point, 3D Segment
- Solids: sphere, cube, pyramid, cone, cylinder, torus
- **MoebiusStrip**, **3D Parametric Surface** `z = f(x,y)`, **3D Parametric Curve**
- **Attractor3D** with 2D projection of integrated orbit
- **HyperSurface4D** (4D slice)
- Camera with orbit/pan/zoom

### Statistics and Probability

Linear/polynomial/exponential/log/power regression with R², normal/binomial/Poisson/Student-t distributions, interactive histogram, boxplot, frequency table, ANOVA, Chi² test, confidence intervals. All accessible via CAS commands.

### GPU Compute Shaders (WGSL)

Four compute pipelines in `crates/grafito-render/src/`:

| Pipeline | Use | Opcodes |
|----------|-----|---------|
| `function_compute.wgsl` | Evaluate `y = f(x)` on 1D grid | 50 |
| `implicit_compute.wgsl` | Evaluate `f(x,y) = c` for marching squares | 50 |
| `parametric_compute.wgsl` | 2D/3D curves and parametric surfaces | 50 |
| `vector_compute.wgsl` | 2D vector field `(P(x,y), Q(x,y))` | 50 |

**50 opcodes supported in GPU** (vs 22 before v1.0.0): full arithmetic, direct/inverse trig, direct/inverse hyperbolics, sec/csc/cot, atan2, mod, sign, heaviside, cbrt, round, log2, log10, exp2, clamp, **6 comparisons (Lt/Gt/Le/Ge/Eq/Ne → 0/1)**. This enables GPU rendering of expressions like `sin(x)*mod(x,2)`, `atan2(y,x)`, `heaviside(sin(x))`, and comparisons like `piecewise(x<0, -1, 1)`.

Pattern for each pipeline:
1. AST→bytecode (RPN) compilation on CPU
2. Dispatch `pass.dispatch_workgroups(...)` with optimal workgroup_size (64 for 1D, 16×16 for 2D)
3. Async readback
4. CPU-side marching squares or streamline evaluation
5. Cache with keys `(expression, domain, variables, quality)` and padded/snapped view region to avoid invalidation on pan/zoom

### UI / UX

- **50 tools** in grouped toolbar (Select, Point, Line, Circle, Polygon, Function, 3D, Attractor, Fractal, Histogram, Root, Extremum, etc.)
- **Command palette** with fuzzy match (Ctrl+K)
- **Interactive algebra panel**
- **Undo/redo** with document snapshot
- **Tooltips** and **ghost preview** before commit
- **Toast** for CAS error/message feedback
- **Dark/light theme** with centralized design tokens
- **Multi-language** (Spanish default, commands accept aliases like `Function/func`)

### Export

- **JSON** (full document round-trip)
- **SVG** (all 2D objects)
- **PNG** (CPU raster with Bresenham + midpoint circles)
- **PDF** (valid PDF 1.4, content stream with primitives)
- **LaTeX** (standalone `tikz`/`pgfplots` with points, lines, circles, polygons, functions, ellipses, text)

## Installation

<p><details><summary><b>Linux &mdash; Debian / Ubuntu (.deb)</b></summary>

```bash
wget https://github.com/Diez111/Grafito-Open/releases/latest/download/grafito_amd64.deb
sudo dpkg -i grafito_amd64.deb
```
</details></p>

<p><details><summary><b>Windows &mdash; portable .exe</b></summary>

1. Download the `.exe` from [Releases](https://github.com/Diez111/Grafito-Open/releases)
2. Run it directly. No installation required.
</details></p>

<p><details><summary><b>From source</b></summary>

Requirement: [Rust 1.78+](https://rustlang.org), GPU drivers with Vulkan/Metal/DX12 support.

```bash
git clone https://github.com/Diez111/Grafito-Open.git
cd Grafito-Open
cargo build --release -p grafito-app
./target/release/grafito      # Linux/macOS
./target/release/grafito.exe  # Windows
```

For full GPU support, on Linux make sure you have `libvulkan1`, `mesa-vulkan-drivers`, and your GPU's vendor drivers installed.
</details></p>

## Quick start

1. Open Grafito
2. In the input bar (bottom) type:

```
A = (1, 2)
B = (4, 6)
Line[A, B]
```

You'll see point A, point B, and the line that connects them. Now try:

```
Circle[(0, 0), 2]
Function[sin(x)]
Function[x^2 - 2*x + 1]
Parabola[(0, 0), 3]                  // vertex at (0,0), parameter p=3
Ellipse[(0, 0), 3, 2]                // center, semi-axis a, semi-axis b
PolarCurve[1 + cos(t), 0, 2*pi]      // cardioid
ImplicitCurve[x^2 + y^2, 4]          // implicit circle
Mandelbrot[-2.5, 1.5, -1.5, 1.5]     // fractal
Lorenz[0.1, 0, 0]                    // Lorenz attractor
```

For analysis:
```
Analyze[sin(x), -5, 5]               // roots, extrema, inflection
Derivative[x^3, x]                   // "3*x^2" (native CAS)
Solve[x^2 - 4, x]                    // ["2", "-2"]
Eigenvalues[[2,0],[0,3]]             // "[(2, 0), (3, 0)]"
SVD[[1,2],[3,4]]                     // decomposition
TangentAt[sin(x), 1]                 // tangent line
ArcLength[sin(x), 0, pi]             // arc length
VolumeOfRevolution[x^2, 0, 1]        // volume π/3
```

## Architecture

Workspace with 6 crates, acyclic and minimal dependencies:

```
grafito-geometry  (no internal deps) → AST, CAS, matrices, stats, ODE, fractals, conformal
        ↑               ↑
grafito-core      grafito-command          → Document, GeoObject (36 variants), constraints, sampling
        ↑               ↑                       GPU sampling cache, numeric solver
grafito-render    grafito-ui               → WGSL compute pipelines, GPU canvas
        ↑               ↑                       egui toolbar, theme, command palette
        └──── grafito-app ────┘              → run_app() entry point
```

| Crate | Lines | Responsibility |
|-------|-------|----------------|
| `grafito-geometry` | ~14,000 | Pure math engine (AST, CAS, matrices, statistics, ODE, fractals, complex) |
| `grafito-core` | ~6,000 | Document model, constraints, spatial index, sampling cache |
| `grafito-render` | ~4,000 | WGSL compute pipelines + GPU render |
| `grafito-command` | ~5,000 | Shared text command processor (process_input) |
| `grafito-ui` | ~5,000 | egui components (toolbar, command palette, theme, tokens) |
| `grafito-app` | ~8,000 | Desktop entry point, assembles UI + render + events |

Details in [`docs/commands.md`](docs/commands.md) and [`CHANGELOG.md`](CHANGELOG.md).

## Command reference

Over **395 CAS commands** recognized, including Spanish/English aliases. Examples by category:

- **Geometry**: `Point[]`, `Line[]`, `Segment[]`, `Ray[]`, `Vector[]`, `Circle[]`, `Polygon[]`, `Ellipse[]`, `Parabola[]`, `Hyperbola[]`, `Arc[]`
- **Functions**: `Function[]`, `Function[piecewise(x<0, x^2, x>=0, sqrt(x))]`
- **Parametric/Polar/Implicit**: `ParametricCurve[]`, `PolarCurve[]`, `ImplicitCurve[]`
- **Advanced**: `Fractal[]`, `Mandelbrot[]`, `Julia[]`, `Lorenz[]`, `Rossler[]`, `Aizawa[]`, `Chen[]`, `Halvorsen[]`, `Dadras[]`, `Chua[]`, `Sprott[]`
- **3D**: `Point3D[]`, `Sphere[]`, `Cube[]`, `Pyramid[]`, `Cone[]`, `Cylinder[]`, `Torus[]`, `Moebius[]`, `Surface3D[]`, `Curve3D[]`
- **Statistics**: `Histogram[]`, `ScatterPlot[]`, `BoxPlot[]`, `Regression[]`, `Mean[]`, `Median[]`, `StdDev[]`, `ANOVA[]`, `ChiSqTest[]`, `Correlation[]`
- **Calculus**: `Derivative[]`, `Integral[]`, `Limit[]`, `Sum[]`, `Product[]`, `Taylor[]`, `Expand[]`, `Factor[]`, `Simplify[]`, `Substitute[]`
- **Linear algebra**: `Determinant[]`, `Inverse[]`, `Eigenvalues[]`, `Eigenvectors[]`, `SVD[]`, `LU[]`, `QR[]`, `Cholesky[]`
- **Constraints**: `Distance[]`, `Angle[]`, `TangentConstraint[]`, `Coincident[]`, `Horizontal[]`, `Vertical[]`, `EqualLength[]`, `Symmetry[]`
- **Conics**: `EllipseByFoci[]`, `ParabolaByFocusDirectrix[]`, `HyperbolaByFoci[]`, `ConicByFivePoints[]`
- **Booleans**: `PolygonUnion[]`, `PolygonIntersection[]`, `PolygonDifference[]`, `PolygonXor[]`
- **Conformal maps**: `ComplexMapping[1/z, I]`, `ComplexMapping[z^2]`, `Joukowski[1+0.1*cos(t)]`
- **Analysis**: `Root[]`, `Extremum[]`, `Inflection[]`, `XIntercept[]`, `YIntercept[]`, `Analyze[]`, `TangentAt[]`, `NormalAt[]`, `ArcLength[]`, `CurvatureAt[]`, `VolumeOfRevolution[]`, `SurfaceOfRevolution[]`

Full list and syntax in [`docs/commands.md`](docs/commands.md).

## Development

```bash
# Build release
cargo build --release -p grafito-app

# Full verification (local CI)
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release

# Benchmarks (criterion)
cargo bench -p grafito-core
cargo bench -p grafito-render
cargo bench -p grafito-geometry

# Profiling with puffin
cargo run -p grafito-app --features profile -- --profile
# Connect puffin_viewer to 127.0.0.1:8585
```

## Tests and quality

- **587 tests** in the workspace (unit + integration)
- `cargo fmt --check` (CI)
- `cargo clippy --workspace -- -D warnings` (CI)
- `cargo test --workspace` (CI)
- `cargo doc --workspace --no-deps` with `-D warnings` (CI)
- **0 `unsafe` blocks** in production code
- 2 known TODOs (neither critical)
- Build matrix: x86_64 Linux, x86_64 Windows, x86_64 macOS, aarch64 macOS

## Contributing

Contributions are welcome! Please:

1. Fork the project
2. Create a branch (`git checkout -b feature/my-feature`)
3. Make your changes following conventions (Rust 2021, `cargo fmt`, `cargo clippy`)
4. Add tests for new code
5. Make sure `cargo test --workspace && cargo clippy --workspace -- -D warnings` passes
6. Commit with Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`)
7. Push and open a Pull Request

See [`CONTRIBUTING.md`](.github/CONTRIBUTING.md) and [`CODE_OF_CONDUCT.md`](.github/CODE_OF_CONDUCT.md).

## License

**GPL-3.0-or-later**. See [`LICENSE`](LICENSE) for the full text.

```
Grafito Open - GPU-accelerated mathematical graphing calculator
Copyright (C) 2026 Grafito Contributors

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.
```

---

<p align="center">
  Made with Rust · <a href="https://github.com/Diez111/Grafito-Open">Diez111/Grafito-Open</a>
</p>
