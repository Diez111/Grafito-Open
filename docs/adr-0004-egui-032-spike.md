# ADR-0004 — Spike egui 0.29 → 0.32 (Ola 5): diferido con evidencia

## Estado
Spike ejecutado 2026-09-17. **Diferido**: no se migra en esta ola. Queda cuantificado y listo para retomar en rama dedicada.

## Contexto
El plan de perf (Ola 5) pedía un spike de egui 0.32 (mejoras de perf/render en 0.30–0.32: `Atoms`, tessellation, `Popup` API, `CornerRadius`, text layout). La duda: ¿conviene subir ahora o el costo rompe el presupuesto de la ola?

## Evidencia (probe real, no estimación)

Worktree desechable `/tmp/opencode/egui-spike` (HEAD `0c27f60`), `egui = "0.32"` en `[workspace.dependencies]`, `cargo check -p grafito-ui` con target propio:

- **MSRV**: egui/eframe 0.32 exigen Rust 1.85 — compatible con nuestra MSRV 1.92 y `deny.toml` (egui-wgpu 0.32 → `wgpu 25.0.0`, MSRV 1.84; `winit 0.30.7`).
- **312 errores en `grafito-ui` solo** (crate sin wgpu; el más egui-heavy):

| Error | N | Naturaleza |
|---|---|---|
| `E0308` (mismatched types) | 89 | Mayormente `Margin::same(f32)` → `i8` (epaint 0.32 volvió a `i8`; nuestros tokens son `f32`) |
| `Frame::none` deprecado | 55 | `Frame::NONE` / `Frame::new()` |
| `Frame::rounding` deprecado | 51 | `corner_radius` |
| `E0061` (args) | 24+4+3 | Firmas movidas (`Popup`, `Margin`, `rect_filled`) |
| `Rounding` deprecado | 23+4 | Alias → `CornerRadius` |
| `Button::rounding` deprecado | 23 | `corner_radius` |
| `E0308` args incorrectos | 20 | `CornerRadius` u8, `Shadow` offsets f32 |
| `Memory::{close,toggle,is_open}_popup`→`Popup` | 13 | API de popups rework 0.32 |
| `egui::popup` no existe | 2 | Módulo movido |
| `Visuals::window_rounding` eliminado | 1 | Campos de `Visuals` rework |

Distribución: `assistant.rs` 195, `theme.rs` 34, `color_picker.rs` 28, `toolbar.rs` 22, resto ≤11. Los deprecados cuentan como error porque el workspace tiene `deprecated = "deny"` (el probe los expone todos).

- **Lo que falta fuera de `grafito-ui`** (no sondeado en profundidad, cuantificado por grep):
  - `grafito-render` usa `wgpu 22` directo (11 `Instance::new`, 9 `ShaderModuleDescriptor`, 7 `ComputePipelineDescriptor`, 5 `create_buffer_init`) → subir a `wgpu 25` sí o sí: egui-wgpu 0.32 no interopera con wgpu 22, y `deny.toml` (`multiple-versions = "deny"`) prohíbe convivir ambos.
  - `grafito-app`: eframe 0.32 (winit 0.30.7, `run_native` y `WgpuConfiguration` tocan nuestro arranque), canvas/`egui_wgpu::Renderer` (interop de texturas y compute propios), `PGO`/CI sin cambios.

## Decisión
Diferir. Razones:
1. El costo real es **multi-día** (≈300 fixes mecánicos solo en `grafito-ui` + migración wgpu 22→25 en `grafito-render`, que es el crate con cómputo GPU propio y el pipeline async de Ola 3 recién estabilizado).
2. No hay un cuello de botella medido que egui 0.32 resuelva hoy (puffin/criterion de §2 en `docs/profiling.md`: los costos están en sampler/alloc/texturas, ya atacados en Olas 1–4).
3. La decisión vigente del repo es consolidar 0.29 (MEMORY.md 2026-09-05; pin en `Cargo.toml`); romperla sin delta medido viola la regla de ADR-0003 §"Regla que queda".

## Plan de retomada (cuando toque)
1. Rama `egui-032` + este ADR como contrato; worktree con target aislado (como este spike).
2. Migrar primero `grafito-render` a wgpu 25 *con egui 0.29* (desacoplado): `cargo check -p grafito-render -p grafito-app` + tests GPU (`WGPU_BACKEND=vulkan GRAFITO_REQUIRE_GPU_TESTS=1 cargo test -p grafito-render --test gpu_compute`).
3. Luego egui 0.32 en `grafito-ui` (los ~312 errores son casi todos rename/`as i8`; `assistant.rs` primero por volumen).
4. Cerrar con `eframe 0.32` en `grafito-app` y los 4 gates del repo + sesión E2E con `computer-control`.
