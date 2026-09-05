# Fuzz Findings — Grafito

Estado a 2026-09-04: 0 crashes reproducibles.

- Ejecución: `.github/workflows/fuzz.yml` semanal (dom 03:17 UTC) + manual, 7 targets (`ast_parser`, `complex_parser`, `document_deserialization`, `command_atomicity`, `validation_limits`, `spreadsheet_recompute`, `anim_wire`) con `-max_total_time=180` (cap 21 min) y `-max_len=65536`.
- Corpus semilla commiteado: `fuzz/corpus/<target>/` — `anim_wire` 9 vectores (`z^2`, `exp(x)`, `__import__('os')`, hello/progress/result/error v1, pong, 66 KiB oversize >line_cap 64 KiB) documentados en `fuzz/fuzz_targets/anim_wire.rs:12-15` y golden `crates/grafito-anim/tests/anim_wire_fuzz_corpus.rs`; otros targets 1–2 seeds.
- Cobertura: boundaries no confiables (parsers, validación, wire IPC). Un run limpio no prueba ausencia de defectos.
- Artefactos: `fuzz/artifacts/` (gitignored) subidos 14 días como `fuzz-crashes-*` si aparece crash.
- Triage pendiente: ampliar `anim_wire` con trazas reales del worker cuando CI nightly reporte cobertura; mantener `fuzz/README.md` y este archivo actualizados por hallazgo.

Próxima revisión: tras el siguiente run semanal (2026-09-07).
