# ADR-0002 — Crates externos CAS/geometría: veredicto con evidencia (2026-09-15)

## Estado
Aceptado. `groebner` crate rechazado por bug de solidez demostrado; `rssn`/`kika`/`dgs`/`cvmath` como referencia documentada, cero dependencias nuevas salvo `num-rational`+`num-traits` (maduros, MIT/Apache). En su lugar: Buchberger exacto propio sobre Q + memo de expansión + booleanas honestas.

## Contexto
Se evaluó integrar `rssn` (CAS con Gröbner/DAG), `kika` (predicados robustos),
`dgs` (DSL geometría dinámica) y `cvmath` (tipos math) para llevar el CAS y la
geometría al máximo sin romper MSRV 1.92 ni el supply-chain (`deny.toml`,
`cargo audit`, 17 jobs CI).

## Evidencia (verificada, no supuesta)
- `rssn 0.2.11–0.2.13`: `rust_version 1.96.0` (crates.io API) → no compila con
  el workspace (`rust-version = "1.92"`, box con `rustc 1.92.0`, CI
  `['1.92', stable]`). Grafo real: ~35 deps normales (`faer`, `ndarray 0.16`,
  `argmin`, `statrs`, `rustfft`, `rand ×3 majors`, `nalgebra ^0.35` que duplica
  el 0.33 del workspace, más `vergen`/`cbindgen` como build-deps). 118k líneas.
  Features confirmados en su README (DAG canónico, Gröbner, simplificación con
  relaciones), pero el costo MSRV + grafo no se justifica.
- `groebner 0.2.0` (sdiehl, MIT, MSRV 1.70): se integró en spike y se
  **rechazó por bug de solidez demostrado**: ante `x+y-3, x-y-1` devuelve la
  base `[y-1]`, que NO genera el ideal (falta `x-2`; la variedad del sistema es
  el punto (2,1), la de `[y-1]` es una recta). Causa: `minimize_basis` elimina
  `f1` y `f2` mutuamente por divisibilidad de líderes sin conservar ninguno.
  Su `is_groebner_basis` no lo detecta (solo chequea S-pares internos).
- `kika 0.7.1` (MIT/Apache, MSRV 1.85, cero deps): **no implementa operaciones
  booleanas** (solo predicados, Delaunay/CDT y triangulación de polígonos).
  Su delta contra el stack actual (`robust 1.2.0` + `spade 2.15.1` + `geo 0.29.3`
  con `earcutr`/`i_overlay`, todo ya en `Cargo.lock`) es ~nulo. Crate de 3
  semanas, 144 descargas: no apto como dep de este repo.
- `dgs 0.1.1`: 11 días de vida, 3.5k líneas, sin `rust_version`. Solo ideas DSL.
- `cvmath`: duplica `glam`+`nalgebra`+`geo`. Descartado.
- `geo 0.33.1` declara `rust_version 1.88` (compatible MSRV), pero migrar
  `0.29→0.33` es semver migration con revisión API propia (relacionado con
  RUSTSEC-2025-0165 en `i_overlay <1.10`); se difiere a ola propia.

## Decisiones
1. **Buchberger exacto propio** (`cas.rs`): núcleo genérico `GbCoef` sobre
   `BigRational` (cero exacto, sin `eps`), espejo del `buchberger_run` sólido;
   vía exacta primera con autoverificación de S-pares + fallback `f64` que
   conserva la taxonomía (`Unsupported`/`ResourceLimit` → `Eliminate[...]`).
   Cotas B2.4 intactas + `MAX_EXACT_BASIS_POLYS 64`/`MAX_EXACT_BASIS_TERMS 1024`.
2. **`buchberger_normal_form` pública**: resto exacto módulo una base, para
   simplificar con relaciones laterales (`x^2` bajo `x^2+y^2-1` → `1-y^2`).
3. **DAG-lite en `expand()`**: memo de subexpansiones con huella + sonda
   `structurally_eq` (anticolisión) y recarga exacta de presupuesto: misma
   trayectoria, mismos errores, solo velocidad (`MAX_EXPAND_MEMO_ENTRIES 1024`).
4. **Booleanas honestas** (`boolean.rs` + comandos): piezas con agujeros
   preservados (materializados como bordes sin relleno `Uh₁`, nada se pierde en
   silencio), orden canónico por esquina mínima (etiquetas deterministas) y
   pre-chequeo atómico `MAX_OBJECT_COUNT` con mensaje que nombra la operación.
5. **MSRV 1.92 intacta.** El camino del bump (15 archivos + toolchain 1.96 +
   re-resolve del lock) queda inventariado y rechazado por costo/beneficio.

## Consecuencias
- 2 deps nuevas mínimas y maduras: `num-rational 0.4`, `num-traits 0.2`.
- `geo 0.29→0.33` pendiente como ola propia (driver: RUSTSEC-2025-0165).
- `rssn`/`kika`/`dgs` quedan como referencia arquitectónica, no como deps.
