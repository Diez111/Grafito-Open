# Gates honestos: bench-regression y GPU (auditoría G17, 2026-09-05)

> Alcance TESTS: **NO** se edita `.github/workflows/ci.yml` (fuera de scope).
> Este documento deja constancia del hallazgo para que el gate se endurezca
> en otro cambio. Tests que lo acompañan:
> `crates/grafito-render/tests/gpu_fail_closed_report.rs` (reporta, no falla).

## 1. Bench-gate que pasa sin baseline — HALLAZGO (no editado)

Ubicación: `.github/workflows/ci.yml:416-456` (`bench-regression`).

```yaml
- name: Run benches with --test harness
  run: cargo bench --workspace --benches --locked -- --test
- name: Save baseline 'main' and check regression >10%
  run: |
    cargo bench --workspace --benches --locked -- --save-baseline main --test || true
```

Problemas:

1. `|| true` traga cualquier fallo → el step siempre es verde aunque el
   bench panicée o la compilación falle en ese punto.
2. `--test` corre el harness de criterion en modo test (verifica que los
   benches *corren*), no mide ni compara tiempos.
3. `--save-baseline main` guarda un baseline fresco en *cada* run, pero
   ningún step posterior hace `--baseline main` + comparación de medias ni
   gate `>10%`. El nombre del job ("regression (>10% gate)") promete un
   control que el YAML no implementa.
4. El artefacto `target/criterion/` se sube 7 días, pero ningún job lo
   descarga como baseline previo, así que no hay comparación inter-run.

Efecto: una regresión de rendimiento >10% (p.ej. `Vec::remove(0)` O(n) en
`undo_stack`, tessellation sin rayon, `fill_compute` siempre activo con
128 MiB) **pasaría el CI en verde**.

Endurecimiento propuesto (fuera de scope, para otro PR):

```yaml
- run: cargo bench ... -- --save-baseline main --test   # sin `|| true`
- uses: actions/download-artifact@v4  # baseline main anterior
- run: cargo bench ... -- --baseline main  # criterion compara y falla si >10%
  # o `criterion-compare` / `bencher` con threshold explícito y `exit 1`.
```

Mientras tanto, `cargo bench -- --test` solo garantiza "los benches
compilan y no panican", no "no hay regresión".

## 2. GPU fail-closed local — DOCUMENTADO + test de reporte

Ubicación: `crates/grafito-render/tests/gpu_compute.rs:41-90`.

- `gpu_context_or_skip()` pide adapter Vulkan; si no hay GPU y
  `GRAFITO_REQUIRE_GPU_TESTS` **no** está seteada → `eprintln!(skip)` +
  `return` temprano (test en verde sin haber probado nada).
- Si la var está seteada a `1`/`true`/cualquiera distinto de `0`/`false` →
  `panic!` fail-closed con el motivo.
- En CI (`ci.yml` job `gpu-compute`) se exporta
  `WGPU_BACKEND=vulkan` + `GRAFITO_REQUIRE_GPU_TESTS=1` con drivers
  `mesa-vulkan-drivers`, así que el skip no es silencioso allí.

En local típico (sin GPU / sin la var) `cargo test -p grafito-render`
da verde con 0 cobertura GPU real. El nuevo test
`gpu_fail_closed_report.rs`:

- siempre pasa, pero imprime `GPU coverage: SKIPPED-LOCAL …` o
  `STRICT …` según el entorno, para que el log distinga ambos casos;
- fija la semántica `"0"`/`"false"` = off (si alguien la cambia a
  `is_some()`, el test de paridad falla).

Para exigir GPU en local:

```bash
GRAFITO_REQUIRE_GPU_TESTS=1 WGPU_BACKEND=vulkan cargo test -p grafito-render --test gpu_compute
```

## 3. Nota honesta sobre `empty_expr_test` ("x ++ y")

`cargo test -p grafito-core --test empty_expr_test -- --nocapture`
(2026-09-05) muestra `invalid: parsed OK` para `"x ++ y"`: el parser lo
acepta como `x + (+y)` (`parse_unary` consume `+` inicial,
`crates/grafito-geometry/src/ast.rs:3534-3536`). Por eso
`crates/grafito-core/tests/empty_expr_test.rs` **no** afirma
`is_none` para ese caso (sería un test mentiroso en rojo permanente),
sino que fija el contrato real (`is_some` + misma evaluación que
`"x + y"`) y añade el caso verdaderamente inválido `"x +* y" → None`.
