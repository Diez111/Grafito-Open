# ADR-0001 — Núcleo pedagógico: el asistente como tutor-orquestador con memoria

## Estado
Aceptado (borrador fundacional; el asistente ya es el punto de orquestación).

## Contexto y propósito
El objetivo final de Grafito es que cualquier persona pueda aprender cualquier tema de
forma pedagógica. Eso exige dos pilares: (1) herramientas de exploración (2D/3D, CAS,
pizarra, animaciones) y (2) un tutor que las orqueste y muestre progreso constante.
El asistente (Mora) es la estrella: gestiona sesiones, sugiere herramientas, explica
paso a paso y anima conceptos. Para ser un tutor profesional necesita memoria del
usuario: nivel, ramas cubiertas/faltantes, dominio por rama y exámenes.

## Decisiones de arquitectura
1. Asistente como orquestador, no solo chat: decide qué herramienta usar según el
   objetivo y la encadena; el usuario ve el avance en el chat.
2. Memoria de usuario en un crate puro grafito-profile (capa hoja, sin egui):
   StudentProfile con nivel, XP, ramas (cobertura + dominio EMA) e historial. Se
   persiste como JSON acotado; el prompt del asistente inyecta un resumen comprimido
   (memory()) para que el tutor adapte el plan y vaya ramificando.
3. Fusión de modelos ya definida: DeepSeek Flash (razonamiento, el más barato) +
   MiMo 2.5-VL (Xiaomi) para visión/video; el perfil alimenta el contexto de ambos.
4. Progreso visible en todo momento: cada interacción registra eventos de aprendizaje
   (Prompt, Correcto, Incorrecto, Examen) que actualizan la memoria.

## Trade-offs
- Pro: un solo crate de memoria testable headless, sin deuda con la UI, reutilizable.
- Contra: la inyección de contexto y el panel de Progreso integran UI (fase siguiente);
  hoy se entrega el modelo + tests como fundación.
- Contra: la memoria no debe crecer sin límite: se recorta por cantidad de eventos y
  longitud del resumen (límites explícitos).

## Flujo de datos
Interacción del usuario → Mora (DeepSeek Flash) → sugerencia + ejecución de herramienta
→ evento de aprendizaje → StudentProfile (JSON) → memory() → contexto del siguiente turno.
El perfil nunca toca egui; lo consumen la app y el prompt.

## Checklist arquitectónico
- [x] Requisitos: tutor con memoria, progreso visible, orquestación de herramientas.
- [x] Modularidad: grafito-profile de capa hoja.
- [x] Escalabilidad: persistencia acotada y resúmenes comprimidos.
- [x] Mantenibilidad: convenciones del workspace (fmt/clippy/test).
- [x] Seguridad: sin secretos; entradas validadas (finitude, límites).

## Anti-patterns evitados
- God Object: el perfil no dibuja ni calcula; solo estado de aprendizaje.
- Big Ball of Mud: la memoria es un crate dedicado, no campos sueltos en la app.
