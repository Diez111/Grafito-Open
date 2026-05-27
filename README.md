# Grafito

**Grafito** is a high-performance, GPU-accelerated interactive geometry and mathematics application inspired by GeoGebra. Built from the ground up in Rust with modern graphics APIs.

## Features

### 2D Canvas
- **GPU-Accelerated Rendering** — Powered by `wgpu` (WebGPU), running natively on Vulkan, Metal, and DirectX 12.
- **Interactive Geometry** — Points, lines, circles, polygons, ellipses, parabolas, hyperbolas with click-to-create tools.
- **Pan & Zoom** — Drag to pan, scroll to zoom with anchor, zoom-to-fit button.
- **Grid & Axes** — Togglable coordinate grid with snap-to-grid.

### 3D View
- **Orbit Camera** — Right-drag to rotate, scroll to zoom, left-drag to pan.
- **8 Solid Types** — Points, segments, spheres (3 great circles), cubes, pyramids, cones, cylinders, parametric surfaces `z=f(x,y)`.
- **Parametric Curves** — `Curve3D[(t, sin(t), cos(t)), t, 0, 6.28]`.
- **Extrusion** — `Extrude[polygon, height]` creates 3D prisms from 2D polygons.
- **Axis Labels** — X (red), Y (green), Z (blue) with arrow tips.

### CAS (Computer Algebra System)
- **Numeric**: Derivative (finite differences), Integral (Simpson's rule), Root finding (Newton's method), Limits (Richardson extrapolation).
- **Symbolic**: Factor (polynomial root detection), Expand (term distribution), Simplify (constant evaluation).
- **Commands**: `Derivative[expr]`, `Integral[expr,a,b]`, `Solve[expr,a,b]`, `Limit[expr,x0]`, `Factor[expr]`, `Expand[expr]`, `Simplify[expr]`.
- **MPFR Precision** — `rug` crate with configurable 53-256 bit precision for guaranteed accuracy.

### Input Bar (30+ commands)
```
A = (1, 2)           # 2D point
A = (1, 2, 3)        # 3D point
f(x) = sin(x)        # function
a = 5                # variable
Ellipse[(0,0), 3, 2] # conic
Normal[0, 1]         # probability
Integral[sin(x), 0, 1] # CAS
Curve3D[(t, sin(t), cos(t)), t, 0, 6.28] # 3D curve
Script[cmd1; cmd2]   # batch execution
```

### UI/UX
- **Algebra View** — Live list of all objects with labels and selection.
- **Properties Panel** — Measurements (length, radius, area, perimeter, volume, surface area).
- **Variable Sliders** — Animated parametric sliders with play/pause.
- **Spreadsheet View** — Grid A-H × 20 rows with (x,y) coordinate entry.
- **Status Bar** — Object count, variables, view mode, scale, CAS result.
- **Right-Click Context Menu** — Delete, zoom-to-fit, reset view, grid/snap toggles.
- **Undo/Redo** — Ctrl+Z (undo), Ctrl+Y (redo), Delete key (remove selected).

### File Operations
- **Save/Load** — `.grafito` (JSON serialized Document).
- **Export SVG** — Valid vector graphics with grid, axes, all object types, labels.
- **Export PNG** — Rasterized with Bresenham lines, midpoint circles, polygon scanline fill.
- **Export TikZ** — LaTeX-compatible `\begin{tikzpicture}` code.
- **Recent Files** — Last 8 opened files in File menu.

### Tools
- **Geometry**: Tangent, PerpendicularBisector, AngleBisector, Midpoint, Vector, Ray, RegularPolygon, Locus.
- **Transformations**: Translate, Rotate, Dilate, Reflect.
- **Probability**: Normal, Binomial, Poisson distributions.
- **Exam Mode** — Disables CAS, input bar, and file operations.

## Performance Optimizations

- **Parallel Function Evaluation** — `rayon` for 1000-sample parallel rendering.
- **Asymptote Detection** — Interval arithmetic with MPFR directed rounding.
- **Spatial Index** — `rstar` R-tree for O(log n) hit testing (ready, pending integration).
- **Constraint Graph** — DAG-based dependency tracking (ready, pending integration).

## Tech Stack

| Layer | Technology |
|-------|------------|
| Window & Input | `eframe` 0.29 + `winit` |
| GPU Rendering | `wgpu` 22.0 (WebGPU → Vulkan/Metal/DX12) |
| UI Framework | `egui` 0.29 |
| Math & Geometry | `glam`, `nalgebra`, `geo`, `spade`, `robust` |
| Precision Math | `rug` (GMP/MPFR, 53-256 bit configurable) |
| Expression Eval | `evalexpr` |
| Parallelism | `rayon` |
| Spatial Index | `rstar` (R-tree) |
| Serialization | `serde` + `serde_json`/`toml`/`ron` |

## Architecture

```
grafito/
├── crates/
│   ├── grafito-app/       # eframe desktop app (~2200 lines)
│   ├── grafito-core/      # Document, objects (29 variants), constraints, spatial index
│   ├── grafito-geometry/  # Math primitives, precision engine, interval arithmetic, CAS
│   ├── grafito-render/    # wgpu 2D renderer (GPU pipeline, available for future)
│   └── grafito-ui/        # egui panels (algebra view, properties, toolbar)
├── docs/                  # Plans.md and design documents
└── assets/                # Shaders and resources
```

## Building

### Prerequisites
```bash
# System dependencies for MPFR precision
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4
```

### Build & Run
```bash
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo run -p grafito-app
cargo run -p grafito-app --release
```

## Controls

| Action | Control |
|--------|---------|
| Pan (2D) | Drag |
| Zoom (2D) | Scroll wheel |
| Pan (3D) | Left drag (Select tool) |
| Orbit (3D) | Right drag |
| Zoom (3D) | Scroll wheel |
| Create object | Click (Point/Line/Circle/Polygon tools) |
| Select object | Click (Select tool) |
| Undo | Ctrl+Z |
| Redo | Ctrl+Y / Ctrl+Shift+Z |
| Delete selected | Delete |

## Roadmap (v0.2)

- [ ] CI/CD with GitHub Actions
- [ ] Unit & integration tests
- [ ] Integrate spatial index into hit testing
- [ ] Integrate constraint graph into dependency system
- [ ] GPU compute shaders for massive function evaluation
- [ ] Surface of revolution in 3D
- [ ] Command-pattern undo/redo (currently snapshot-based)
- [ ] .ggb file import (GeoGebra XML)

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
