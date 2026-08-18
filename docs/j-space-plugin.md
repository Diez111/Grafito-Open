# Plugin J-Space del asistente (por defecto)

Grafito incluye por defecto el plugin `plugins/j-space` (categoría skills,
activación auto). Define cómo Mora estructura tareas largas para reducir la
pérdida de realización de capacidad: estabilizar el estado de tarea, controlar
la profundidad del razonamiento y no declarar completado sin verificar.

## Qué hace

- **Ledger de tarea** `Goal / Core / Verified / Open / Next`, compacto y acotado,
  inyectado en el contexto del agente para tareas largas o multi-herramienta.
- **Gating de profundidad** `fast` (paso único), `full` (pocas operaciones) y
  `loop` (tareas largas con tools + ledger), implementado en `grafito-agent`
  (`TaskBand` + `JSpaceLedger`).
- **Primera persona funcional** y **monitor→control** en las instrucciones,
  con un **done-check** que marca `verified=false` si la respuesta deja
  pendientes declarados.

## Atribución y licencia

El patrón está inspirado en el reporte *DeepSeek V4 × J-Space Capability
Realization Report* de Tiger3807861189 y en la suite J-Space Cognition
(https://github.com/Tiger3807861189/J-Space-Cognition-Suite-V3.6). El reporte
está bajo licencia CC BY-ND 4.0; Grafito **no distribuye texto derivado** del
reporte: reimplementa la metodología en Rust con redacción propia y sólo cita y
enlaza la fuente. El ledger `Goal/Core/Verified/Open/Next` es también el patrón
que usa el propio harness (jspace_state).

## Instalación / actualización

- En el repositorio: `plugins/j-space/` se carga automáticamente al arrancar la
  app (directorio usuario `./plugins`; el paquete instala los suyos en
  `/usr/share/grafito/plugins`).
- Puede desactivarse desde Configuración del asistente -> Plugins.
