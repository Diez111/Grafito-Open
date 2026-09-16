# ADR-0003 — Librerías externas del plan GeoGebra (2026-09-16)

## Estado
Aceptado. Solo 2 dependencias nuevas en todo el plan (`num-rational`+`num-traits` ya estaban; `iroh`+`tokio` tras flag). Todo lo demás in-house.

## Veredictos con evidencia

| Crate | Licencia | MSRV | Decisión | Motivo |
|---|---|---|---|---|
| `kurbo 0.13.1` | Apache/MIT | 1.85 | **Rechazada por ahora** | Nuestras Bézier son de N puntos; kurbo es cúbico. Sin delta aplicado real. Reingreso si llega representación por segmentos cúbicos. Se agregó y quitó sin dejar rastro en el lock. |
| `iroh 1.2.0` | MIT/Apache | 1.91 | **Aceptada** (flag `aula-iroh`) | Ping/pong real verificado en test. Lecciones: `tls-ring` obligatorio (default no trae proveedor), `preset Minimal` para tests offline, el runtime debe bombearse tras `send` (micro-flush incorporado), dial y accept van en hilos distintos. |
| `statrs 0.19.1` | MIT | 1.89 | **No hizo falta** | Inversas por Newton-bisección sobre CDFs propias (deterministas, testeadas). |
| `feanor-math` / `algebraeon` | MIT / GPL-3.0-only | n/d | **No hicieron falta** | Buchberger propio + Rabinowitsch cubrieron Gröbner y Prove. |
| `symbolica 3.0.0` | non-standard | — | **Rechazada** | Rompe supply-chain (`deny.toml`). |
| `loro` | MIT | — | **Rechazada** | CRDT propio (`WhiteboardCrdt` HLC+LWW+merge) ya cumple; delta nulo. |
| `egg` / `dashu` / `malachite` | MIT / MIT / LGPL | — | **Rechazadas** | `num-rational`+ memo propio cubren; e-graphs overkill para `Simplify`. |
| `groebner 0.2.0` | MIT | 1.70 | **Rechazada** (ADR-0002) | Bug de solidez demostrado (`minimize_basis` pierde generadores). |
| `geo 0.33.1` | MIT/Apache | 1.88 | **Diferida** | Migración semver propia por RUSTSEC-2025-0165; ola aparte. |

## Regla que queda
Nueva dependencia solo si: (1) MSRV ≤ 1.92 verificada en crates.io, (2) licencia en `deny.toml`, (3) delta aplicado que lo propio no cubre, (4) spike con test que la justifique o fallback in-house documentado. `wasm` (web/móvil) queda como ADR aparte, fuera de este plan.
