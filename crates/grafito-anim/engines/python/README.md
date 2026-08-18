# Grafito Manim Engine (plugin externo)

Motor de animaciones matemáticas de Grafito como plugin **fuera del proceso**
(Rust no ejecuta Python; el puente `grafito-anim` habla un protocolo JSON v1
sobre stdio).

## Instalación

```bash
cd crates/grafito-anim/engines/python
pip install -e .          # opcional: para que `python3 -m manim_engine` resuelva
# y opcionalmente: pip install manim teniendo un sistema con ffmpeg
```

## Uso

- Sin Manim instalado, el motor genera un artefacto placeholder (PNG/GIF) para
  que el pipeline sea comprobable.
- Con Manim instalado y export `mp4`/`gif`, genera y renderiza una escena real
  de `FunctionGraph`.

## Protocolo

Líneas JSON sobre stdin/stdout (ver `crates/grafito-anim/src/protocol.rs`):

- `hello {protocol_version, capabilities}`
- `ping` / `pong`
- `render_request {job_id, template, concept, params, spec, export, canvas}`
- `progress {job_id, step, percent}`
- `render_result {job_id, media_path, frames, duration_ms}`
- `error {job_id, code, message}`
- `shutdown`

El motor escribe SIEMPRE dentro del directorio de trabajo; el puente rechaza
cualquier ruta de artefacto fuera de ese directorio.