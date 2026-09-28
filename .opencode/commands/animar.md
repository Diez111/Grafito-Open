---
description: Anima un concepto con una plantilla del catálogo nativo (69 templates).
agent: build
---

# /animar <concepto> [dominio]

Animá `<concepto>` eligiendo plantilla del catálogo nativo. Cargá el skill
`animacion-3b1b` primero (catálogo completo + presupuestos + validación).

## Pasos

1. `skill({name:"animacion-3b1b"})` — lee el catálogo y los presupuestos.
2. Elegí template:
   - Si `[dominio]` viene dado (linalg, am1, am2, edo, stats, 3d, 4d, graphs,
     chaos, canon), elegí dentro de esa familia.
   - Si no, usá `sanitize_template("auto", "<concepto>")`
     (`crates/grafito-anim/src/protocol.rs`) o `template_for_concept`.
   - Alias histórico: `pythagoras` → `pitagoras`.
   - Desconocido no vacío → error honesto, nunca degradar en silencio a otra
     plantilla.
3. Presupuestos (no negociables):
   - GIF / PNG-sequence: ≤64 frames (`PREVIEW_SHORT_MAX_FRAMES`).
   - MP4 / WebM long-form: ≤1500 frames (`VIDEO_LONGFORM_MAX_FRAMES`, 50 s a 30 fps).
   - Paso de guion: 4..=16 frames (`PASO_MIN/MAX_FRAMES`, `guion.rs:47-49`).
   - Canvas: 64..=4096 por lado, default 640×480 (`protocol.rs:263`).
   - Timeline: ≤60 000 ms (`MAX_TIMELINE_DURATION_MS`).
   - Mensaje worker: ≤500 chars (`MAX_WORKER_MESSAGE_LEN`); línea IPC ≤64 KiB (`line_cap`).
4. Guion:
   - Corto (default): `short_script("<concepto>")` → 4 actos / 8 pasos
     (`BeatsCorto`, `guion.rs:934-1157`), copy por familia (6 familias).
   - Largo: guion completo con voiceover ≤40 palabras/paso, captions SRT/ASS
     ≤256 KiB (`captions.rs:33`, `guion.rs:64-68`).
5. Protocolo de validación (guion→escena→render→MP4):
   1. `short_script` OK (o guion largo válido).
   2. `escena_para` / `render_*_frames` de la familia OK en 96×64×4 (smoke).
   3. Render real al canvas pedido dentro de presupuestos.
   4. MP4 vía ffmpeg-sidecar; sin `ffmpeg` en PATH → `FfmpegMissing` honesto
      (no fingir video).
6. Reportá: template elegido + familia, frames/canvas/duración, `media_path`
   verificado o error honesto.

## Ejemplos

- `/animar derivada` → `derivative-slope` (canon).
- `/animar autovalores linalg` → `eigenvectores`.
- `/animar calor edo` → `edo-calor-onda`.
- `/animar bautismo de grafos graphs` → `bfs-animado`.
