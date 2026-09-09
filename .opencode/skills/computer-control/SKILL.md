---
name: computer-control
description: Control total del equipo para pruebas E2E en apps nativas (screenshot, video, lanzar apps, mouse/teclado). Úsala para probar Grafito como un usuario real.
---

# computer-control

Plugin global (`~/.config/opencode/plugins/computer-control.ts`) — carga en TODOS los proyectos. Stack COSMIC/Wayland verificado 2026-09-08: `grim`, `gpu-screen-recorder` (detener con SIGINT), `ydotool`, `wl-copy`, `notify-send`.

## Tools (todas vía tool nativo, sin MCP)

- `desktop_screenshot {name}` → PNG en `/tmp/opencode/shots/`. **Siempre primero**: mira antes de actuar y verifica después.
- `desktop_record {action: start|stop, name}` → MP4 30fps en `/tmp/opencode/clips/`. Start devuelve id; stop finaliza y da tamaño.
- `app_launch {command, runArgs}` → PID detached (nativas: `cosmic-term`, `grafito`, `firefox`). Verifica con screenshot.
- `app_quit {pid}` → SIGTERM y SIGKILL si resiste.
- `desktop_input {action, text, x, y, button, clicks}` → `type` texto | `key` tecla/combo (`Enter`, `ctrl+c`) | `click` (`0xC0` izq) | `move` x,y absolutos | `scroll` (negativo baja).
- `desktop_notify {title, body}` → anuncio de inicio/fin de prueba.

## Protocolo de prueba E2E (Grafito)

1. `desktop_notify` "inicio prueba X" → 2. `desktop_record start` → 3. `app_launch grafito` → 4. `desktop_screenshot` → 5. actúa con `desktop_input` (screenshot entre pasos) → 6. `desktop_record stop` → 7. `app_quit` → 8. reporta rutas de evidencia.
- Nunca dejes apps abiertas ni grabaciones corriendo al terminar.
- El socket ydotool vive en `/run/user/1000/.ydotool_socket` (servicio usuario `ydotoold.service`); fallback `/tmp/.ydotool_socket` (demonio root actual).

## Memoria del harness (verificada E2E 2026-09-08)

- Tool `memory` (opencode-mem, scope project/all-projects): guardar hechos y buscarlos cruza sesiones (probado: store → recall en otra sesión).
- `MEMORY.md` + `.jspace/WORKSPACE.md` los curra `memory-keeper` en `session.idle`.
- MCP `memory` oficial (grafo local) como respaldo, global en `~/.config/opencode/opencode.jsonc`.
