# Integración del DeepSeek Harness en el asistente de Grafito

Este documento registra el análisis del repositorio `deepseek-ai/deepseek-harness`
y cómo sus capacidades se integran en el asistente de Grafito **en Rust**, sin
depender del runtime de Node en la aplicación.

## Qué se analizó

El harness instalado como `@deepseek-ai/dsh@0.1.0-rc.6` está formado por ~130
paquetes `@deepseek-ai/dsh-*`. Para este trabajo se estudiaron los que aportan
capacidades de agente: `dsh-agent-loop` (loop de turnos), `dsh-llm` y adapters
(contrato de proveedor), `dsh-tools`/`dsh-typert-registry` (schema y registry de
herramientas), `dsh-token-meter`/compaction (presupuesto de contexto), `dsh-goal`
/plan-mode (objetivos) y `dsh-client-ui-trajectory` (actividad visible).

## Correspondencia DSH -> Grafito (Rust)

| Capacidad DSH | Equivalente Grafito (crate Rust) | Estado |
| --- | --- | --- |
| dsh-llm / adapters / fusion | grafito-assistant::agent (RemoteAgentCompleter) + transportes existentes | Implementado
| dsh-tools / typert-registry | grafito-agent::schema (ToolSchema, ToolCall, ToolResult) | Implementado
| dsh-agent-loop | grafito-agent::loop_engine (run_agent con presupuestos y eventos) | Implementado
| dsh-token-meter / compaction | grafito-agent::budget + grafito-ui conversación acotada | Implementado (parcial)
| dsh-goal / plan-mode | ProposedPlan + staging/replay (grafito-command) | Ya existente; extendido a planes multi-paso en la hoja de ruta
| J-Space gating + ledger (plugin por defecto) | grafito-agent::router::TaskBand + grafito-agent::ledger::JSpaceLedger (Goal/Core/Verified/Open/Next) | Implementado (plugins/j-space)
| dsh-client-ui-trajectory | fila de actividad de tools (futuro wiring de UI) | Planificado
| Enrutamiento de modelos | grafito-agent::router (ModelRoute: fast/reasoner/audit) | Implementado

## Qué se excluyó y por qué

El harness ejecuta herramientas potentes (bash, filesystem, MCP, subagentes,
workflows) porque es un agente de desarrollo. Grafito es una app de matemática de
escritorio con una postura de seguridad estricta, por lo que se portan **sólo el
protocolo, el schema, el loop acotado y el router**, y las herramientas se
limitan a un conjunto seguro:

- `evaluate_expr` : evalúa expresiones con grafito-geometry.
- `grafito_docs`  : catálogo acotado de comandos verificados.
- `ask_user`      : rechaza si no hay consentimiento explícito en la UI.

Quedan fuera: bash/fs/net que no sea el provider configurado, MCP, orquestación
de subagentes y runtimes de sandbox. Las propuestas que mutan el documento
siempre pasan por el pipeline existente con **Apply explícito del usuario**.

## Postura de seguridad

1. Ninguna tool muta el documento sola: todo pasa por `ProposedPlan` + Apply.
2. I/O de archivos y red fuera del provider configurado: no disponibles por defecto.
3. Resultados acotados (MAX_TOOL_RESULT_CHARS) y validación de schemas.
4. Loop acotado (max_tool_turns), timeout global y cancelación cooperativa.
5. Los errores nunca exponen claves, rutas ni cuerpos del proveedor.
