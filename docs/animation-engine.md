# Motor de animaciones de Grafito (puente nativo 100% Rust)

El motor de animaciones es el puente nativo `grafito-anim`: 13 plantillas
canónicas (`CANONICAL_TEMPLATES` en `protocol.rs`) renderizadas en Rust
(`anim_native.rs`) con MP4 vía ffmpeg-sidecar (`FfmpegMissing` honesto sin
`ffmpeg` en PATH). El antiguo `engines/python` (Python + Manim por IPC) ya fue
eliminado del árbol, y los 10 notebooks `.ipynb` de `lab/colab_upload/` también
(el port del lab a `crates/grafito-lab` está en curso; el futuro es el binario).

## Arquitectura

```
grafito-app / grafito-assistant (Rust)
        |  render_scene tool (planificado en el asistente agéntico)
        v
   grafito-anim  (puente Rust: spawn, handshake, jobs, timeouts)
        |  JSON v1 sobre stdio (líneas)
        v
   motor nativo (13 plantillas en crates/grafito-anim/src/anim_native.rs)
   concepto -> frames nativos (o placeholder) -> media (MP4 vía ffmpeg-sidecar)
```

## Protocolo (v1)

Líneas JSON sobre stdin/stdout (ver `crates/grafito-anim/src/protocol.rs`):

- `hello {protocol_version, capabilities}`
- `ping` / `pong`
- `render_request {job_id, template, concept, params, spec, export, canvas}`
- `progress {job_id, step, percent}`
- `render_result {job_id, media_path, frames, duration_ms}`
- `error {job_id, code, message}`
- `shutdown`

## Seguridad y presupuestos del puente

- El motor se lanza perezoso al primer render y se termina al salir (Drop).
- Un job se ejecuta en una cola de 1; timeout por job y cancelación cooperativa.
- Las líneas de salida se acotan; stderr se recoge como diagnóstico sin crashear.
- El motor escribe en su `working_dir` y el puente **rechaza** cualquier ruta de
  artefacto fuera de ese directorio (`validate_media_path`).
- Si falta `ffmpeg` en PATH, el puente reporta `FfmpegMissing` honesto
  y el asistente ofrece la explicación sin render.