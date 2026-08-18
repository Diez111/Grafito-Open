# Motor de animaciones de Grafito (plugin externo)

Para no complejizar el núcleo y conservar eficiencia, el motor de animaciones
matemáticas es un **plugin externo, fuera del proceso Rust**, invocado por IPC.
Se permite otro lenguaje (p. ej. Python + Manim) siempre que el puente hable el
protocolo versionado. La app sigue siendo 100% Rust y funciona sin el motor
(el asistente degrada a explicación).

## Arquitectura

```
grafito-app / grafito-assistant (Rust)
        |  render_scene tool (planificado en el asistente agéntico)
        v
   grafito-anim  (puente Rust: spawn, handshake, jobs, timeouts)
        |  JSON v1 sobre stdio (líneas)
        v
   motor externo (p. ej. crates/grafito-anim/engines/python/manim_engine)
   analiza concepto -> genera escena (Manim) o placeholder -> render -> media
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
- Si el motor no responde el handshake (falta Python/Manim), el puente reporta
  un error claro y el asistente ofrece la explicación sin render.