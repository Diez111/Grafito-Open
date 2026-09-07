---
name: profiling
description: Mide con puffin, criterion y wgpu-profiler antes de optimizar. Usa feature profile.
---

# profiling

```bash
cargo run -p grafito-app --features profile -- --profile
# + puffin_viewer aparte
cargo bench -p grafito-geometry
cargo bench -p grafito-render
```

- `puffin 0.19` 50-200ns/span + `puffin_egui` + `wgpu_profiler TIMESTAMP_QUERY`.
- Feature `profile = ["puffin","puffin_http","eframe/puffin"]` en `grafito-app/Cargo.toml`.
- Decide con datos: 1-2ms/frame egui, 250k cells/dispatch compute, regresión >10% bloquea.
