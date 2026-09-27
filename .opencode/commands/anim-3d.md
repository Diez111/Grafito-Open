---
description: Escena 3D/4D con cámara (órbita, zoom, superficies, cortes).
agent: build
---

# /anim-3d <concepto> [template]

Escenas con cámara para superficies, sólidos, 4-politopos y descenso 3D.
Cargá el skill `animacion-3b1b` primero (familias `3d` / `4d` + presupuestos).

## Pasos

1. `skill({name:"animacion-3b1b"})` — secciones familias `3d` y `4d`, cámara.
2. Elegí template (default por concepto si `[template]` no viene):
   - Superficies: `sup-paraboloide-tangente`, `sup-toro-rotante`,
     `sup-campo-vectorial`, `sup-interseccion`, `sup-onda-3d`,
     `sup-silla-descenso` (`tpl_3d.rs:6`).
   - 4D: `tesseract-xw`, `celda-24`, `hipercubo-corte`, `estereografica`,
     `simplex-nd` (`tpl_4d.rs:5`).
   - Descenso/optimización 3D: `descenso-gradiente-3d` (`tpl_stats.rs`),
     `gradient-descent` (`tpl_am2.rs`).
3. Cámara (convención del módulo):
   - Órbita: ángulo azimutal barrido lineal 0→2π en el total de frames;
     elevación fija salvo que el concepto pida picado/contrapicado.
   - Zoom: factor de escala acotado, nunca lleva el canvas bajo 64px
     efectivos ni sobre 4096.
   - Superficies: wireframe + plano tangente / curva de intersección según
     template; cortes 4D: parámetro de corte `w` barrido lineal.
4. Presupuestos (idem `/animar`):
   - GIF ≤64 frames, MP4 ≤1500 frames; smoke previo 96×64×4.
   - Canvas 64..=4096, default 640×480; timeline ≤60 s.
5. Validación (guion→escena→render→MP4):
   1. Guion corto con familia de forma/cálculo según concepto.
   2. Smoke `render_*_frames(template, 96, 64, 4)` OK.
   3. Render real; verificar frames distintos (no congelados) y acotados.
   4. MP4 vía ffmpeg-sidecar o `FfmpegMissing` honesto.
6. Reportá: template, barrido de cámara (órbita/zoom/corte), frames/canvas,
   `media_path` o error honesto.

## Ejemplos

- `/anim-3d toro` → `sup-toro-rotante` con órbita 0→2π.
- `/anim-3d hipercubo` → `tesseract-xw` (rotación plano XW).
- `/anim-3d silla descenso` → `sup-silla-descenso`.
