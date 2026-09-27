# Paridad 3Blue1Brown ↔ Grafito

Benchmark de cobertura: episodios de 3b1b vs templates de animación de Grafito.
Fecha: 2026-09-27. Agente 17/20.

## Metodología (honesta)

- **Web:** `websearch` (2 queries: EoLA y EoC) devolvió **cero resultados**.
  `webfetch` a `3blue1brown.com/lessons` y a `?topic=linear-algebra` / `?topic=calculus`
  rinde solo el shell SPA (sin datos de episodios en estático). Confirmado por esa vía
  que existen los tópicos `linear-algebra`, `calculus`, `differential-equations`,
  `neural-networks`, `probability`. Wikipedia (`en.wikipedia.org/wiki/3Blue1Brown`)
  confirma canal/series/Manim pero **no lista episodios**.
- **Episodios:** títulos de playlists públicas estables (EoLA 2016, EoC 2017, DE, NN, proba)
  + citas in-repo: `tpl_linalg.rs:10-21` (caps EoLA + slugs `span`, `linear-transformations`,
  `determinant`, `eigenvalues`, `change-of-basis`, `cross-products`),
  `tpl_am1.rs:12-19` (EoC caps 4/7/8/11), `tpl_stats.rs:10-19` (NN caps 1/2/4/7, Bayes 2019, CLT 2023),
  `tpl_edo.rs:117-127` (serie DE). Lo no confirmado en esta sesión va como **no verificado** (no se inventa).
- **Templates:** los 65 IDs están verificados en fuente (ver Inventario). Lo que **no** se hizo:
  ejecutar `cargo` ni renders (orden: sin gates) — los estados son por coincidencia temática
  estática ID→capítulo, no por inspección de píxeles.

Estados: **cubierto** (hay template dedicado al tema del episodio),
**parcial** (cubre una parte / por aproximación), **ausente** (nada lo cubre),
**no verificado** (episodio o template sin confirmar en esta sesión).

## Essence of Linear Algebra (caps 1–16)

| Cap | Episodio | Template(s) Grafito | Estado | Nota |
|-----|----------|---------------------|--------|------|
| 1 | Vectores | `vectores-combinacion-lineal` | cubierto | tpl_linalg cap. 1–2 |
| 2 | Combinaciones lineales, span, base | `vectores-combinacion-lineal`, `subspace` | cubierto | span explícito en ambos |
| 3 | Transformaciones lineales y matrices | `matriz-transformacion` | cubierto | grilla que se deforma I→A |
| 4 | Multiplicación de matrices (composición) | `matriz-transformacion` | parcial | interpola una matriz; composición A·B no dedicada |
| 5 | Transformaciones lineales en 3D | `producto-cruz` | parcial | vectores 3D sí; transformaciones 3D como tales no |
| 6 | Determinante | `determinante-area` | cubierto | cuadrado→paralelogramo, área = det |
| 7 | Inversas, espacio columna, núcleo | — | ausente | sin template de inversa/rango/nulidad |
| 8 | Matrices no cuadradas | — | ausente | nada sobre m×n entre dimensiones |
| 9 | Producto punto y dualidad | — | ausente | sin template dedicado |
| 10 | Producto cruz | `producto-cruz` | cubierto | área con signo del paralelogramo |
| 11 | Producto cruz vía transformaciones | `producto-cruz` | cubierto | mismo template, enfoque dual |
| 12 | Regla de Cramer | — | ausente | sin template dedicado |
| 13 | Cambio de base | `cambio-de-base` | cubierto | "grilla de Jennifer" |
| 14 | Eigenvectores y eigenvalores | `eigenvectores` | cubierto | Av = λv, direcciones en su span |
| 15 | Espacios vectoriales abstractos | — | ausente | axiomático/funciones como vectores, sin template |
| 16 | s/d | — | no verificado | la serie pública lista 15 caps; verificar bonus en `?topic=linear-algebra` |

## Essence of Calculus (caps 1–13)

| Cap | Episodio | Template(s) Grafito | Estado | Nota |
|-----|----------|---------------------|--------|------|
| 1 | La esencia del cálculo | `universal` | parcial | panorama general; placeholder neutro honesto |
| 2 | La paradoja de la derivada | `derivative-slope` | cubierto | canónica |
| 3 | Fórmulas de derivación (geometría) | `derivative-slope` | cubierto | barrido x0/span |
| 4 | Regla de la cadena y del producto | `chain-rule` | cubierto | AM1, cita EoC cap. 4 |
| 5 | ¿Qué tiene e de especial? | `euler` | parcial | cubre e vía series; e como límite/crecimiento no dedicado |
| 6 | Derivación implícita | `derivative-slope`, `chain-rule` | parcial | aproximación; sin curva implícita dedicada |
| 7 | Límites (ε–δ) | `epsilon-delta` | cubierto | AM1, cita EoC cap. 7 |
| 8 | Integración y TFC | `integral-area`, `riemann-sums` | cubierto | área + rectángulos n→∞, cita EoC cap. 8 |
| 9 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| 10 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| 11 | Series de Taylor | `taylor-series`, `taylor-remainder` | cubierto | orden 1..=7 + resto de Lagrange |
| 12 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| 13 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| extra | Integrales impropias (criterio-p) | `improper-integral` | cubierto | AM1/OpenStax, fuera de la serie numerada |

## Ecuaciones diferenciales (serie DE)

| Episodio | Template(s) Grafito | Estado | Nota |
|----------|---------------------|--------|------|
| DE1 — EDOs, guía turística | `edo-campo-direcciones`, `ode-slope-field` | parcial | campos de pendientes sí; resto del tour (espacio de fases, etc.) no |
| DE2 — ¿Qué es una EDP? | `edo-calor-onda` | parcial | calor/onda como ejemplo; EDP en general no |
| DE3 — Ecuación del calor | `edo-calor-onda` | cubierto | modos que decaen |
| DE4 — Series de Fourier (epiciclos) | `edo-fourier-epiciclos`, `fourier` | cubierto | epiciclos que dibujan; canónica `fourier` además |
| DE5 — Transformada de Laplace (numeración exacta no verificada) | `edo-laplace` | cubierto | proyección sobre e^(−st) |
| extra — Convolución (curso MIT 18.S191) | `edo-convolucion` | cubierto | fuera de la serie DE numerada |

## Redes neuronales (caps 1–7)

| Cap | Episodio | Template(s) Grafito | Estado | Nota |
|-----|----------|---------------------|--------|------|
| 1 | ¿Qué es una red neuronal? | `perceptron-mlp` | cubierto | activaciones 2→2→1, pesos fijos |
| 2 | Descenso por gradiente | `gradient-descent`, `descenso-gradiente-3d` | cubierto | bowl + 12 pasos GD |
| 3 | ¿Qué hace backprop realmente? | `backprop-flujo` | cubierto | flujo de gradientes exactos |
| 4 | Cálculo de backprop | `backprop-flujo`, `chain-rule` | cubierto | regla de la cadena explícita |
| 5 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| 6 | s/d | — | no verificado | título exacto no confirmado en esta sesión |
| 7 | Cómo guardan hechos los LLMs (MLP) | `perceptron-mlp` | parcial | cita in-repo `tpl_stats.rs:18`; MLP genérico, no key-value de hechos |

## Probabilidad

| Episodio | Template(s) Grafito | Estado | Nota |
|----------|---------------------|--------|------|
| Bayes, geometría del cambio de creencias (2019) | `teorema-bayes` | cubierto | prior→posterior por áreas |
| ¿Qué es el TLC? (2023) | `limite-central` | cubierto | medias muestrales→campana |
| Distribuciones (normal μ/σ, binomial p) | `distribuciones` | cubierto | anima μ/σ y p |
| Otros videos del tópico `probability` | — | no verificado | lista completa no confirmada en esta sesión |

Extras fuera de serie (verificados, sin episodio 1:1): `regresion-lineal`,
`pca-rotacion` (serie LA), `gradient-field`, `mobius-transform`, `conformal-map`,
`pitagoras`, `logistic-bifurcation`, `chaos-*` (5), `sup-*` (6), `tesseract-xw`,
`celda-24`, `hipercubo-corte`, `estereografica`, `simplex-nd`, grafos (5),
más `partial-derivatives`, `lagrange-multipliers`, `double-integral`,
`green-stokes`, `jacobian` (multivariable, serie 3b1b posterior no mapeada acá).

## Conteo final

46 filas: **25 cubiertos · 8 parciales · 5 ausentes · 8 no verificados**.

| Serie | Cub. | Parc. | Aus. | No verif. | Filas |
|-------|------|-------|------|-----------|-------|
| EoLA | 8 | 2 | 5 | 1 | 16 |
| EoC | 6 | 3 | 0 | 4 | 13 |
| DE | 4 | 2 | 0 | 0 | 6 |
| NN | 4 | 1 | 0 | 2 | 7 |
| Proba | 3 | 0 | 0 | 1 | 4 |
| **Total** | **25** | **8** | **5** | **8** | **46** |

## Top-10 ausentes priorizados (próxima ola)

1. **Inversa / espacio columna / núcleo** (EoLA 7) — ausente núcleo del curso.
2. **Matrices no cuadradas** (EoLA 8) — transformaciones entre dimensiones.
3. **Producto punto y dualidad** (EoLA 9) — prerrequisito de backprop/atención.
4. **Regla de Cramer** (EoLA 12) — determinante aplicado, barato sobre `determinante-area`.
5. **Espacios vectoriales abstractos** (EoLA 15) — cierra la serie.
6. **Transformaciones 3D** (EoLA 5, hoy parcial) — elevar `producto-cruz` a grilla 3D deformable.
7. **Composición de matrices** (EoLA 4, hoy parcial) — A·B animada sobre `matriz-transformacion`.
8. **Derivación implícita** (EoC 6, hoy parcial) — curvas de nivel + tangentes (reúsa AM2).
9. **Atención / transformers** (NN 5–6, no verificado) — el hueco moderno más visible.
10. **Cross-entropy / funciones de loss** (NN/proba) — puente natural entre `distribuciones` y `gradient-descent`.

## Inventario de templates verificados (65)

13 canónicas (`protocol.rs:1470-1484`): `derivative-slope`, `integral-area`,
`taylor-series`, `conformal-map`, `pitagoras`, `euler`, `fourier`,
`logistic-bifurcation`, `gradient-field`, `mobius-transform`, `universal`,
`subspace`, `fractal`.

52 nuevas (un `TEMPLATE_IDS` por archivo, longitudes afirmadas por sus tests):
`tpl_linalg.rs:50` (6), `tpl_graphs.rs:62` (5), `tpl_4d.rs:85` (5),
`tpl_edo.rs:121` (5), `tpl_3d.rs:98` (6), `tpl_chaos.rs:120` (5),
`tpl_am1.rs:69` (6), `tpl_am2.rs:97` (6), `tpl_stats.rs:53` (8).
`tpl_pipeline.rs` **no** tiene `TEMPLATE_IDS` (infra de pipeline, verificado por grep).
Lista: `vectores-combinacion-lineal`, `matriz-transformacion`, `determinante-area`,
`eigenvectores`, `cambio-de-base`, `producto-cruz`, `moser-spindle-coloreo`,
`bfs-animado`, `force-directed`, `unit-distance`, `camino-minimo`, `tesseract-xw`,
`celda-24`, `hipercubo-corte`, `estereografica`, `simplex-nd`,
`edo-campo-direcciones`, `edo-convolucion`, `edo-laplace`, `edo-fourier-epiciclos`,
`edo-calor-onda`, `sup-paraboloide-tangente`, `sup-toro-rotante`,
`sup-campo-vectorial`, `sup-interseccion`, `sup-onda-3d`, `sup-silla-descenso`,
`chaos-mandelbrot-zoom`, `chaos-lorenz`, `chaos-bifurcacion-barrido`,
`chaos-julia-morph`, `chaos-pendulo-doble`, `riemann-sums`, `epsilon-delta`,
`chain-rule`, `taylor-remainder`, `improper-integral`, `ode-slope-field`,
`partial-derivatives`, `gradient-descent`, `lagrange-multipliers`,
`double-integral`, `green-stokes`, `jacobian`, `distribuciones`, `limite-central`,
`teorema-bayes`, `regresion-lineal`, `pca-rotacion`, `perceptron-mlp`,
`backprop-flujo`, `descenso-gradiente-3d`.
