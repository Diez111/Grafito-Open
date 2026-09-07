---
name: rust-performance
description: Perf Rust para Grafito egui/wgpu. Usa puffin, rayon, wgpu compute, lyon, spade. Mide antes de optimizar.
---

# rust-performance

## Reglas
- Mide primero: `cargo run -p grafito-app --features profile` + `puffin_viewer`, `criterion` benches.
- `egui` con `features=["rayon"]` tessellation paralela. Target 1-2ms/frame.
- `wgpu 22`: batch draws, cache LRU 128, `bytemuck`, `Shape::Callback` bajo overlay egui.
- `lyon` tessellation, `spade` Delaunay `bulk_load_cdt`, `rstar` índice espacial.
- `MAX_UNDO 50 / 50MiB VecDeque pop_front O(1)`, `checked_mul` OOM guard, `validate_media_path relative_to` anti TOCTOU + `O_NOFOLLOW`.

## Gates
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## Skills externas bajo demanda
- `apollographql/rust-best-practices`, `leonardomso/rust-skills`, `salmanneomtech/rust-engineer/rust-systems`, `cazala/webgpu-skill`, `Zuytan/ui-design`.
