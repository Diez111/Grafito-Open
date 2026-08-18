# Grafito Plans

Creado: 2026-07-10

Este plan sustituye el roadmap heredado. La prioridad es convertir las capacidades actuales en matemáticas confiables y una aplicación segura antes de ampliar el producto.

## Reglas de ejecución

- Toda tarea de código usa TDD salvo que indique `[tdd:skip:...]`.
- Un resultado matemático no puede ocultar `NaN`, infinito, error de dominio, falta de convergencia ni límite de recursos.
- Las tareas que toquen archivos ya modificados requieren primero integrar o aislar los cambios existentes.
- No se anuncian capacidades como estables hasta contar con pruebas de referencia y documentación consistente.

## Phase 0: Línea base e integración segura

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| 0.1 | Inventariar y verificar los cambios locales existentes por área antes de cualquier merge funcional. [tdd:skip:integration-audit] | Cada archivo sucio está clasificado como integrar, dividir o descartar con pruebas asociadas; no se pierde trabajo ajeno. | - | cc:完了 |
| 0.2 | Crear fixtures y pruebas de regresión para los P0 matemáticos y de recursos. [tdd:required] | Las pruebas fallan en HEAD para integral indefinida, trigonometría grande, histogramas, fractales y Script; los casos son independientes. | 0.1 | cc:完了 |
| 0.3 | Imponer límites consistentes a histogramas, fractales, scripts, comandos, AST y exportación. [tdd:required] | Entradas límite-1/límite/límite+1 no hacen OOM, freeze ni panic; carga, constructor y comando comparten invariantes. | 0.2 | cc:完了 |
| 0.4 | Corregir los resultados matemáticos falsos y propagar dominio/no-convergencia. [tdd:required] | No hay fallback de integral indefinida a `[0,1]`; simplificación y derivadas auditadas coinciden con valores de referencia. | 0.2 | cc:完了 |
| 0.5 | Hacer la ejecución de comandos atómica ante parseo, validación y Script fallidos. [tdd:required] | Un `CommandOutcome::Error` deja hash semántico, variables, objetos y constraints idénticos; Script es all-or-nothing. | 0.2 | cc:完了 |
| 0.6 | Ejecutar y corregir la batería workspace con la Fase 0 integrada. [tdd:skip:verification-only] | `fmt`, `clippy -D warnings`, tests y release build pasan; no hay P0/major abiertos de la fase. | 0.3, 0.4, 0.5 | cc:完了 |
| 0.7 | Corregir los pánicos y freezes P0 descubiertos en el explorador trigonométrico, evaluador `clamp`, tablas de valores y `SeriesSum`. [tdd:required] | Zoom extremo no hace panic; límites inválidos devuelven error; tablas y series se mantienen dentro de presupuestos explícitos. | 0.6 | cc:完了 |
| 0.8 | Corregir series finitas, asíntotas y análisis de rectas que hoy pueden publicar resultados matemáticamente falsos. [tdd:required] | `sum`/`product` preservan exactamente todos sus términos acotados; funciones sin asíntota no reciben una; intersecciones e interceptos respetan `LineKind`. | 0.7 | cc:完了 |

## Phase H: Asistente local-first y harness nativo

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| H.1 | Enrutar `Submit` por resolución local, pedir autorización explícita para red y aplicar `ProposedPlan` de forma atómica. [tdd:required] | Aritmética local no consulta keyring ni red; casos no soportados muestran una acción de autorización; Apply crea exactamente un undo y rechaza bases obsoletas. | 0.8 | cc:完了 |
| H.2 | Separar la presentación pública de la identidad técnica y normalizar transcript matemático. [tdd:required] | Fuera de Configuración avanzada sólo aparecen `Local` o `Consulta remota autorizada`; no se truncan respuestas válidas de 4096 caracteres ni se pierden delimitadores matemáticos comunes. | H.1 | cc:完了 |
| H.3 | Diferenciar respuestas explicativas de propuestas ejecutables y eliminar escalamiento remoto automático. [tdd:required] | Una respuesta remota sin acción termina correctamente; una reparación sigue requiriendo propuesta; ningún fallo inicia una segunda consulta sin una acción nueva del usuario. | H.1 | cc:完了 |
| H.4 | Exponer catálogo de herramientas derivado del registro y migrar propuestas a invocaciones tipadas. [tdd:required] | Cada entrada visible resuelve a un `CommandSpec`; UI no es autoridad de parsing; acciones desconocidas, de archivos, scripts o red no son ejecutables. | H.1, 1.4 | cc:DONE |
| H.5 | Extraer el harness headless, receipts de staging/evidencia y replay local opt-in. [tdd:required] | El flujo request-plan-preview-apply puede ejecutarse sin egui, red ni keyring; replay local valida base, delta y evidencia sin guardar contenido sensible. | H.1, 1.3 | cc:DONE |

## Phase 1: Contratos de núcleo confiable

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| 1.1 | Introducir errores y resultados matemáticos tipados con dominio, estimación y convergencia. [tdd:required] | APIs migradas devuelven `Exact`, `Approximate`, `DomainError`, `NotConverged`, `Unsupported` o `ResourceLimit`; no hay clasificación por texto. | 0.6 | cc:TODO |
| 1.2 | Versionar el formato de documentos, validar referencias y guardar atómicamente. [tdd:required] | Envelope con `schema_version`, migración de JSON legado, roundtrip, archivos truncados y referencias inválidas cubiertos. | 0.6 | cc:完了 |
| 1.3 | Introducir `OperationBatch` y `ChangeSet` para mutaciones de documento e historial. [tdd:required] | Cambios se validan antes de commit, incrementan una revisión por operación y undo/redo restaura estado semántico. | 0.6 | cc:完了 |
| 1.4 | Crear un registro declarativo de comandos y alimentar parser, paleta, autocomplete y documentación. [tdd:required] | Todo comando estable tiene ID, firmas, aliases, ayuda y handler; no hay entradas de UI o docs que no resuelvan. | 0.5, 1.1 | cc:TODO |

## Phase A: Universidad e ingeniería confiable

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| A.1 | Construir corpus de referencia de cálculo, álgebra lineal, estadística, ODE y geometría. [tdd:required] | 2.000 casos versionados con oráculo, tolerancia y semilla; CI ejecuta el corpus en Linux, Windows y macOS. | 1.1 | cc:TODO |
| A.2 | Robustecer cálculo, estadística, ODE, matrices y solver según contratos numéricos. [tdd:required] | Cada operación estable informa precisión/residuo; los oráculos A.1 cumplen sus tolerancias. | A.1 | cc:TODO |
| A.3 | Unificar escena 2D, paridad CPU/GPU, cachés externas y pruebas golden. [tdd:required] | Producción y tests usan el mismo preparador de escena; 200 goldens y diferencia CPU/GPU <=0.5 px. | 1.3 | cc:TODO |
| A.4 | Rehacer 3D sobre world-space, profundidad, clipping y convención única de ejes. [tdd:required] | Depth test, selección por rayo y 100 escenarios de oclusión/geometría 3D pasan. | A.3 | cc:TODO |
| A.5 | Cerrar accesibilidad, navegación por teclado, exportación honesta y documentación generada. [tdd:required] | Todos los controles estables son accesibles; export informa omisiones; UI/docs/registro coinciden. | 1.4, A.3 | cc:TODO |

## Phase B: Producto de geometría dinámica nativo

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| B.1 | Completar herramientas y objetos persistentes reales, hoja incremental y workflows guiados. [tdd:required] | 120 operaciones de construcción tienen objetos reales, undo y E2E; no hay placeholders visibles. | A.5 | cc:TODO |
| B.2 | Importar/exportar `.ggb`, CSV y recursos autocontenidos. [tdd:required] | Corpus compatible preserva >=98% al importar y >=95% al exportar; pérdidas se informan. | 1.2, B.1 | cc:TODO |
| B.3 | Completar experiencia desktop offline: shell responsive, touch opcional, i18n ES/EN, accesibilidad y modo examen aplicado en cada ingreso de operación. [tdd:required] | No hay panel o herramienta esencial inaccesible a 960 px; WCAG 2.2 AA desktop; operaciones prohibidas se bloquean fuera de UI sin usar backend ni red. | B.1, B.2 | cc:TODO |

## Phase D: Espacio de trabajo local durable

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| D.1 | Persistir protocolo de construcción, datasets, resultados CAS/análisis fijados y preferencias de workspace en un formato versionado. [tdd:required] | Abrir/guardar/restaurar conserva cada dato document-bound y undo/redo no deja historial visual divergente. | 1.2, 1.3 | cc:TODO |
| D.2 | Añadir autosave, journal local, recuperación tras crash, versiones locales y archivos recientes. [tdd:required] | Simulaciones de cierre/crash recuperan transacciones confirmadas sin sobrescribir el documento original. | D.1 | cc:TODO |
| D.3 | Completar integración de archivos nativa: `.grafito`, argumentos CLI, asociaciones MIME, print/PDF, assets persistentes y reportes de pérdidas de formato. [tdd:required] | Abrir desde SO/CLI funciona; assets y exportación tienen contratos explícitos sin red. | D.1, B.2 | cc:TODO |

## Phase E: Tutor y educación local verificable

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| E.1 | Integrar tutor local determinista, derivaciones verificadas y focus para geometría, datos, matrices y constraints. [tdd:required] | El usuario puede pedir pista/verificación/paso siguiente sin red; cada paso expone regla y comprobación. | 1.1, D.1 | cc:TODO |
| E.2 | Añadir lecciones, tareas, pistas escalonadas, objetivos estructurales y evidencia de completitud locales. [tdd:required] | Paquetes de lección funcionan offline y aceptan construcciones alternativas válidas. | E.1, B.1 | cc:TODO |
| E.3 | Endurecer distribución y calidad desktop: tests visuales, E2E nativo, firmas, provenance, installers y actualizaciones fail-closed. [tdd:required] | Linux/Windows/macOS validan apertura, recuperación, GPU fallback y artefactos firmados sin dependencia de servicios de producto. | A.1, A.5, D.2 | cc:TODO |

## Phase C: CAS científico amplio

| Task | Contenido | DoD | Depends | Status |
|------|-----------|-----|---------|--------|
| C.1 | Incorporar torre numérica exacta y de precisión arbitraria. [tdd:required] | Integer/Rational/BigFloat/Complex preservan precisión solicitada y pasan corpus exacto. | 1.1, 1.2 | cc:TODO |
| C.2 | Crear IR simbólico canónico con assumptions, ramas e intervalos certificados. [tdd:required] | Simplificaciones conservan dominio; el intervalo contiene el oráculo en el corpus certificado. | C.1 | cc:TODO |
| C.3 | Añadir álgebra polinómica, solve, cálculo simbólico, transformadas y API headless cancelable. [tdd:required] | 21.000 casos simbólicos y 20.000 puntos numéricos cumplen el corpus C; ningún comando bloquea UI. | C.2, A.2 | cc:TODO |
## Fase F: Asistente agéntico, plugins y pedagogía (2026-08)

Entregas (TDD):

| Task | Contenido | Estado |
|------|-----------|--------|
| F.1 | Núcleo agéntico en Rust (grafito-agent): loop acotado, schema de tools, router de modelos (fast/reasoner/audit) inspirado en deepseek-harness. | cc:完了 |
| F.2 | Adaptador de red con tool calling y herramientas seguras (evaluate_expr, grafito_docs, ask_user) administradas por grafito-assistant::agent; eventos de actividad. | cc:完了 |
| F.3 | Sistema de plugins declarativo (grafito-plugins): manifiesto grafito-plugin.toml, validación fail-closed, fingerprint y activación; UI en ajustes del asistente; instrucciones inyectadas al system prompt. | cc:完了 |
| F.4 | Motor de animaciones externo (grafito-anim): puente IPC stdio con protocolo JSON v1, presupuestos y validación de rutas; plugin Python/Manim con fallback sin dependencias. | cc:完了 |
| F.5 | Animaciones de UI del asistente: revelado por bloques, elevación de tarjetas y transiciones deterministas al reloj de egui. | cc:完了 |
| F.6 | Eficiencia del transporte (cliente compartido) y logotipo garantizado en el .deb (hicolor + scalable, build fail-closed). | cc:完了 |
| F.7 | Plugin J-Space por defecto (ledger+band), permiso completo (respuestas en línea automáticas), hoja de cálculo eliminada de la UI, asistente más ancho (400px) y plugins del sistema en el paquete. | cc:完了 |
| F.8 | Pendiente de wiring en la app: fila de actividad de tools, modo agente visual y modo tutor con planes multi-paso. | cc:TODO |

