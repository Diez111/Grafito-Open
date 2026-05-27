# Grafito

**Grafito** is a high-performance, GPU-accelerated interactive geometry and mathematics application inspired by GeoGebra. Built from the ground up in Rust with modern graphics APIs.

## Features (MVP)

- **GPU-Accelerated 2D Canvas** — Powered by `wgpu` (WebGPU), running natively on Vulkan, Metal, and DirectX 12.
- **Interactive Geometry** — Points, lines, circles, polygons with click-to-create tools.
- **Smart Input Bar** — Type coordinates like `(1, 2)` or `A = (3, 4)` to create objects.
- **Pan & Zoom** — Middle-click drag to pan, scroll to zoom with anchor.
- **Algebra View** — Live list of all objects with labels.
- **Properties Panel** — Edit object properties.
- **Grid & Axes** — Auto-generated coordinate grid.

## Tech Stack

| Layer | Technology |
|-------|------------|
| Window & Input | `winit` |
| GPU Rendering | `wgpu` (WebGPU) |
| UI Framework | `egui` |
| Math & Geometry | `glam`, `nalgebra`, `geo` |
| Expression Eval | `evalexpr` |
| Serialization | `serde` + `toml`/`ron` |

## Architecture

```
grafito/
├── crates/
│   ├── grafito-app/      # Main application binary
│   ├── grafito-core/     # Document model, objects, constraints
│   ├── grafito-geometry/ # Math primitives, transforms, expressions
│   ├── grafito-render/   # wgpu renderer (2D shapes, grid, axes)
│   └── grafito-ui/       # egui panels and components
├── docs/                 # Design docs and plans
└── assets/               # Shaders and resources
```

## Building

```bash
# Clone the repo
git clone https://github.com/tu-usuario/grafito.git
cd grafito

# Run in dev mode
cargo run -p grafito-app

# Run release build
cargo run -p grafito-app --release
```

## Controls

| Action | Control |
|--------|---------|
| Pan | Middle-click drag |
| Zoom | Scroll wheel |
| Select object | Click (Select tool) |
| Create point | Click (Point tool) |
| Create line | Click two points (Line tool) |
| Create circle | Click center + edge (Circle tool) |

## Roadmap

- [x] Project scaffolding & workspace
- [x] wgpu renderer with grid & axes
- [x] Basic geometric objects (Point, Line, Circle, Polygon)
- [x] Interactive tools & mouse handling
- [x] Input bar for algebraic entry
- [ ] Function plotting (`y = sin(x)`)
- [ ] Constraints & dynamic geometry
- [ ] Sliders & animations
- [ ] 3D graphics view
- [ ] CAS (Computer Algebra System)
- [ ] Spreadsheet view
- [ ] Scripting engine
- [ ] Export (SVG, PNG, TikZ)

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
