# Animación estilo 3b1b — catálogo nativo (69 templates)

> Puente nativo 100% Rust (`grafito-anim` + ffmpeg-sidecar, `engines/python`
> eliminado). Sin `ffmpeg` en PATH → `FfmpegMissing` honesto, nunca video fingido.
> Fuente de verdad: `crates/grafito-anim/src/tpl_*.rs` (`TEMPLATE_IDS`),
> `protocol.rs::CANONICAL_TEMPLATES`, `grafito-app/src/anim_native.rs::NATIVE_TEMPLATES`.

Total: **69 = 13 canónicas + 52 + 3 extra + Laplace 3D** (verificado 2026-09-28).

## 1. Canónicas — cálculo/compleja/dinámica (13)

`protocol.rs:1470-1484` = `anim_native.rs:53-66` = `anim_ui.rs::PLANTILLAS_COMBO`
(sync 13↔13↔13; alias `pythagoras`→`pitagoras`).

| Template | Cuándo |
|---|---|
| `derivative-slope` | Derivada como pendiente / recta tangente |
| `integral-area` | Integral como área bajo curva (Riemann) |
| `taylor-series` | Aproximación polinómica de Taylor |
| `conformal-map` | Mapeo conforme, deformación de grilla |
| `pitagoras` | Teorema de Pitágoras geométrico |
| `euler` | Fórmula de Euler / exponencial compleja |
| `fourier` | Series de Fourier / epiciclos |
| `logistic-bifurcation` | Mapa logístico / bifurcaciones |
| `gradient-field` | Campo de gradientes / flujo |
| `mobius-transform` | Transformación de Möbius |
| `universal` | Placeholder neutro honesto (sin curva matemática falsa) |
| `subspace` | Subespacios / proyecciones |
| `fractal` | Fractales (zoom autosimilar) |

## 2. Álgebra lineal — `tpl_linalg.rs:6` (6)

`vectores-combinacion-lineal`, `matriz-transformacion`, `determinante-area`,
`eigenvectores`, `cambio-de-base`, `producto-cruz`.

Cuándo: vectores, transformaciones de matriz, determinante como área/volumen,
autovalores y autovectores, cambio de base, producto cruzado.
Conceptos tipo "autovalores", "rotación de matriz", "base" → acá, no a canónicas.

## 3. Análisis I — `tpl_am1.rs:6` (6)

`riemann-sums`, `epsilon-delta`, `chain-rule`, `taylor-remainder`,
`improper-integral`, `ode-slope-field`.

Cuándo: sumas de Riemann, definición ε-δ de límite, regla de la cadena,
resto de Taylor, integrales impropias, campo de pendientes EDO 1D.
Si el concepto es de AM1 puro, preferir esta familia sobre la canónica
genérica (ej. `epsilon-delta` antes que `universal`).

## 4. Análisis II — `tpl_am2.rs:6` (6)

`partial-derivatives`, `gradient-descent`, `lagrange-multipliers`,
`double-integral`, `green-stokes`, `jacobian`.

Cuándo: derivadas parciales, descenso por gradiente, multiplicadores de
Lagrange, integrales dobles, Green/Stokes, jacobiano y cambio de variables.

## 5. EDO / señales — `tpl_edo.rs:121-129` (5)

`edo-campo-direcciones`, `edo-convolucion`, `edo-laplace`, `edo-fourier-epiciclos`,
`edo-calor-onda`.

Cuándo: campo de direcciones, convolución (voltear/deslizar/integrar),
Laplace, Fourier por epiciclos, calor y onda (modos que decaen / pulsos que viajan).
Params: `nx`, `ny`, `s`, `t`, `k`, `modo` (`tpl_edo.rs:205-215`).

## 6. Stats / ML — `tpl_stats.rs:8` (8)

`distribuciones`, `limite-central`, `teorema-bayes`, `regresion-lineal`,
`pca-rotacion`, `perceptron-mlp`, `backprop-flujo`, `descenso-gradiente-3d`.

Cuándo: distribuciones, TCL, Bayes, regresión, PCA, perceptrón/MLP,
flujo de backprop, descenso en 3D (este último también vale para `/anim-3d`).

## 7. Superficies 3D — `tpl_3d.rs:6` (6)

`sup-paraboloide-tangente`, `sup-toro-rotante`, `sup-campo-vectorial`,
`sup-interseccion`, `sup-onda-3d`, `sup-silla-descenso`.

Cuándo: superficies con cámara en órbita (ver `/anim-3d`): plano tangente,
toro rotante, campo vectorial sobre superficie, intersección de superficies,
onda 3D, punto silla con descenso. Cámara: azimut 0→2π, elevación fija,
zoom acotado (canvas efectivo 64..=4096).

## 8. 4D / politopos — `tpl_4d.rs:85-91` (5)

`tesseract-xw`, `celda-24`, `hipercubo-corte`, `estereografica`, `simplex-nd`.

Cuándo: rotación en el plano XW (tesseract), 24-cell, corte de hipercubo con
parámetro `w` barrido lineal, proyección estereográfica, simplex n-dim.

## 9. Grafos — `tpl_graphs.rs:62-68` (5)

`moser-spindle-coloreo`, `bfs-animado`, `force-directed`, `unit-distance`,
`camino-minimo` (Dijkstra). Default frames 48 (`GRAPH_DEFAULT_FRAMES`).

Cuándo: coloreo (Moser spindle = 4 colores), BFS por capas, layout
force-directed convergente, grafos unit-distance, camino mínimo.

## 10. Caos / fractales — `tpl_chaos.rs:5` (5)

`chaos-mandelbrot-zoom`, `chaos-lorenz`, `chaos-bifurcacion-barrido`,
`chaos-julia-morph`, `chaos-pendulo-doble`.

Cuándo: zoom Mandelbrot, atractor de Lorenz, barrido de bifurcación,
morph de Julia, péndulo doble. No confundir `chaos-bifurcacion-barrido`
(barrido animado) con la canónica `logistic-bifurcation` (diagrama estático).

## 11. Presupuestos (no negociables)

| Recurso | Constante | Valor | Dónde |
|---|---|---|---|
| GIF / PNG-seq | `PREVIEW_SHORT_MAX_FRAMES` | ≤64 frames | `protocol.rs:347` |
| MP4 / WebM | `VIDEO_LONGFORM_MAX_FRAMES` | ≤1500 (50 s @30fps) | `protocol.rs:351` |
| Paso de guion | `PASO_MIN/MAX_FRAMES` | 4..=16 | `guion.rs:47-49` |
| Player | `PLAYER_MAX_FRAMES` / `TOTAL` | 48 / 96 | `player.rs:296-299` |
| Paramétricas | `PARAMETRIC_MAX_FRAMES` | 48 | `parametric.rs:30` |
| Canvas | `Resolution` | 64..=4096, default 640×480 | `protocol.rs:263` |
| Timeline | `MAX_TIMELINE_DURATION_MS` | ≤60 000 ms | `protocol.rs:937` |
| Mensaje worker | `MAX_WORKER_MESSAGE_LEN` | ≤500 chars | `protocol.rs:124` |
| Línea IPC | `line_cap` | 64 KiB | `protocol.rs:92` |
| Audio | `AudioTrack` | offset 0..=60000, gain 0..=2 | `protocol.rs:468-470` |
| Captions | — | SRT/ASS ≤256 KiB | `captions.rs:33` |
| Voiceover | — | ≤40 palabras/paso; corto 110-130 | `guion.rs:64-68` |

## 12. Protocolo de validación (guion→escena→render→MP4)

1. **Guion**: `short_script("<concepto>")` → 4 actos / 8 pasos, copy por
   familia (`BeatsCorto`, `guion.rs:934-1157`). Largo solo si lo piden.
2. **Escena**: `escena_para` / `describe` / `titulo_para` de la familia
   resuelve el template (cada `tpl_*.rs` expone `es_plantilla_*` honesto:
   desconocido → `Err`, nunca fallback silencioso).
3. **Render**: smoke `render_*_frames(tpl, 96, 64, 4)` OK + frames distintos;
   luego render real dentro de presupuestos.
4. **MP4**: ffmpeg-sidecar; sin binario → `FfmpegMissing` honesto.

Regla de oro: plantilla desconocida no vacía → `Err` honesto
(`sanitize_template`, `protocol.rs:1486+`). `""`/`"auto"` → `template_for_concept`.
