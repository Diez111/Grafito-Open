# Changelog

Todos los cambios notables de este proyecto se documentarán en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/),
y este proyecto adhiere a [Semantic Versioning](https://semver.org/lang/es/spec/v2.0.0.html).

## [1.2.21-beta] - Unreleased

#### Añadido
- **Núcleo agéntico en Rust (`grafito-agent`)**: integración de las capacidades del DeepSeek Harness en el asistente — schema de herramientas (`ToolSchema`/`ToolCall`/`ToolResult`), loop de agente acotado (`run_agent`) con presupuesto de turnos, timeout global y cancelación cooperativa, y enrutamiento de modelos por tarea (`ModelRoute` fast/reasoner/audit) con normalización de acentos.
- **Adaptador de red con tool calling (`grafito-assistant::agent`)**: nuevo transporte OpenAI-compatible con `tools`, parsing de `tool_calls`, y herramientas seguras (`evaluate_expr`, `grafito_docs`, `ask_user` con consentimiento) que nunca mutan el documento ni acceden a archivos; `request_agent_on_worker` expone eventos de actividad.
- **Sistema de plugins declarativos (`grafito-plugins`)**: manifiestos `grafito-plugin.toml` (instrucciones, tools, comandos, escenas y motores externos), validación fail-closed, registry con fingerprints y activación auto/manual, más UI en la configuración del asistente para listar y activar/desactivar plugins.
- **Instrucciones de plugins en el system prompt**: `AssistantRequest.system_instructions` acotada se inyecta en los payloads OpenAI/Anthropic/Fusion para dar contexto pedagógico a cada consulta.
- **Motor de animaciones externo (`grafito-anim`)**: puente IPC sobre stdio con protocolo JSON v1 (handshake, jobs, progreso, artefactos, errores), validación de rutas dentro del directorio de trabajo y presupuestos; incluye el plugin `manim_engine` (Python/Manim) con fallback sin dependencias.
- **Animaciones de UI del asistente**: las respuestas se revelan por bloques con fade determinista al reloj de egui, tarjetas verificadas con elevación al hover y resaltes de acción.
- **Plugin J-Space por defecto** (`plugins/j-space`): ledger de tarea Goal/Core/Verified/Open/Next, gating fast/full/loop (`TaskBand`) y primera persona funcional con done-check, reimplementados en `grafito-agent` (`JSpaceLedger`, `run_agent_with_ledger`) e inyectados al contexto del agente.
- **Permiso completo para el asistente**: con `assistant_full_permission` (default ON) la consulta remota arranca automáticamente cuando la resolución local no alcanza y hay proveedor listo — sin cartel de autorización; el consentimiento de imágenes se otorga automático (la capacidad de visión del modelo sigue siendo un requisito real).
- **Asistente más ancho por defecto**: el panel pasa de 320px a 400px (rango 340–460) y la cuadrícula de canvas conserva presupuesto real.
- **Modo agente en la UI (F.8)**: toggle en configuración; cuando está activo, el asistente usa el loop con herramientas seguras y muestra en el chat la **actividad de cada tool** y una tarjeta **colapsable con el ledger J-Space** mientras trabaja (resultados con `done-check`, cancelación cooperativa).
- **Corpus semilla del solver (Fase A)**: 31 casos de referencia versionados (aritmética, lineal, cuadrática, rechazos y gráfica) en `crates/grafito-assistant/tests/solver_corpus.rs`.
- **Solver local pedagógico con CAS nativo**: el asistente resuelve sin red `derivar/derivada de`, `integrar/integral de` y `límite de … en n` usando `grafito-geometry::symbolic` (derivada, integral y límite), con pasos que citan la regla del CAS; prompts de ejemplo pedagógicos en el estado vacío.
- **Animaciones reproducidas en el chat**: botón «Animá» invoca el motor externo (`grafito-anim` + plugin j-space/engine), decodifica el GIF y lo reproduce como frames en vivo en la tarjeta de animación del asistente (con límites y degradación clara si el motor no está instalado).
- **Tabla de valores eliminada**: se retira la tabla x|f(x) del panel derecho y del drawer (perspectivas Álgebra-CAS y Cálculo pasan a sin panel derecho); la pestaña «Datos» conserva estadística. El comando `DataTable` (objeto de datos para regresión) se mantiene.
- **Inspector y UX**: estado vacío del inspector y panel vacío centrados verticalmente, texto de ayuda más claro, y pestaña de datos renombrada a «Datos».
- **Diálogo de guardado robusto**: el diálogo Guardar/Descartar/Cancelar vuelve a estar siempre centrado y con ancho fijo, scroll acotado para textos largos, texto envuelto y botones alineados a la derecha con «Guardar» primario (id explícito para evitar posiciones recordadas).
- **Asistente consciente del motor**: el ambito de capacidades ahora nombra el CAS simbólico (derivada, integral, límite, Taylor, Solve, Factor) y aclara qué resuelve el motor local sin red, además de los análisis numéricos y las perspectivas.
- **Animaciones siempre visibles**: el botón «Animá» ya no exige instalar nada — si el motor externo (j-space/engine + Manim) no está, Grafito genera la animación **nativa en Rust** (recta tangente deslizante sobre parábola) y la reproduce igual en el chat.
- **UI de plugins pulida**: filas tipo tarjeta con nombre/descripción y toggle a la derecha, mensaje aclarando que son opcionales y que el asistente y las animaciones nativas funcionan sin ellos.
- **Pizarra nativa (tipo Excalidraw)**: overlay a pantalla completa con toolbar flotante redondeada estilo macOS, lienzo con grid/pan/zoom, herramientas Lápiz/Rectángulo/Elipse/Flecha/Texto/Borrador/Seleccionar, limpiar y cerrar (Esc). Botón «Pizarra» en la barra superior. Modelo en el crate `grafito-whiteboard` (headless, testeado).
- **Tema macOS**: paleta pulida de macOS en claro y oscuro (acento azul #0A84FF/#007AFF, paneles translúcidos suaves, canvas #f5f5f7), conservando la legibilidad (contraste AA) y la profundidad de las superficies.
- **Iconos minimalistas**: subconjunto redibujado a mano (pizarra, flecha, más/menos, formas, cuaderno, compás) en estilo monocromo adaptable a claro/oscuro.
- **AGENTS.md**: comandos de verificación y convenciones para agentes.
- **Pizarra fluida**: el trazo del lápiz se dibuja **en vivo** mientras arrastrás (antes solo aparecía al soltar), con repintado a 60 fps durante el gesto para una interacción natural.
- **IA que entiende la pizarra**: botón «IA» en la pizarra envía una descripción estructurada del dibujo al asistente (DeepSeek V4 Flash) y lo explica; el análisis sigue funcionando con el overlay abierto y queda listo el seam para un modelo de visión ultra barato (MiniMax/MiMo 2.5-VL) sin tocar el flujo del usuario.
- **Animaciones sin fricción**: se quitó el botón «Animá» del compositor (diseño pobre e innecesario); ahora la IA genera la animación sola cuando le pedís «animá…», con **progreso en vivo en el chat**, los jobs del motor externo tienen timeouts cortos (2s idle / 15s job) para caer al generador nativo, y la animación **siempre aparece** (nunca queda colgada).
- **Pizarra con asistente integrado**: el asistente aparece dentro del modo pizarra (panel derecho, ocultable con un botón en la toolbar); el análisis con IA del dibujo funciona con el overlay abierto.
- **Pestaña «Datos» eliminada**: se quita la pestaña del sidebar y su panel vacío («Sin panel aquí»); el menú Paneles se reordena (Álgebra, Herramientas, CAS, Vista). El código del panel de estadística queda anotado sin entrada en la UI (reactivable).
- **Barra «Entrada…» inferior oculta**: los comandos matemáticos se cargan por la sección algebraica; se deja de dibujar la barra inferior redundante, manteniendo la entrada del drawer y la paleta.
- **Zoom 3D sin límites artificiales**: el rango de distancia de la cámara 3D pasa de 0.5–200 a **0.1–5000** con plano lejano de 10000 (y near coherente), así podés acercarte 5x más y alejarte 25x más sin que el recorte por far corte la escena; tests de clipping ajustados a la nueva profundidad.
- **Asistente con diseño editorial y telemetría**: se elimina el estilo de burbuja tipo WhatsApp (tarjetas neutras separadas por rol), los encabezados y textos quedan **centrados**, y cada respuesta del asistente muestra su **telemetría de salida** («~N token de salida (est.)») al estilo del harness; el rol usa acento para distinguir agente/usuario.
- **Fusión de modelos corregida**: se elimina toda referencia a “minimax-m3”. La configuración ahora parte de **DeepSeek Flash** como modelo de razonamiento por defecto (siempre el más barato y suficiente) y añade **MiMo 2.5-VL (Xiaomi)** como modelo multimodal/visión para imágenes y vídeo; DeepSeek Flash reintenta y corrige cuando una respuesta falla. Constantes, payloads, copy de la UI y tests realineados.
- **Núcleo pedagógico (ADR-0001) + memoria del usuario**: nuevo crate `grafito-profile` (capa hoja, headless y testable) con `StudentProfile`: nivel, XP, ramas (cobertura + dominio EMA), historial de aprendizaje acotado y exámenes; `record_outcome`/`record_exam` actualizan la memoria, `recommend_next()` prioriza lo no cubierto/más débil y `memory()` genera el resumen comprimido que el tutor «Mora» inyecta al prompt de cada turno. Documentado en `docs/adr-0001-pedagogical-core.md`.
- **Tutor conectado a la app**: el perfil se carga/persiste en `grafito_profile.json`, la **memoria del estudiante entra en el contexto de cada turno** (sufijo «[Perfil del estudiante]» en las instrucciones del sistema), aparece la **tarjeta «Tutor»** en el asistente (nivel, % de ramas, próxima recomendación), el botón **«¿Qué sigo estudiando?»** le pide a Mora el plan y el **feedback ✓/✗** registra aciertos/fallos por rama (clasificador de tema) y persiste el progreso.

#### Cambiado
- **Eficiencia del transporte remoto**: el cliente HTTP bloqueante ahora es compartido entre peticiones (pool de conexiones reutilizado con timeout por petición), en lugar de crear un `Client` por llamada.
- **Empaquetado `.deb`**: el logotipo de Grafito es fuente de verdad del ícono del escritorio; el build falla si falta y además instala la variante scalable (`hicolor/scalable/apps/grafito.svg`), con pruebas de empaquetado para el ícono y el `.desktop`.
- **Hoja de cálculo eliminada**: se quita la entrada "Hoja"/spreadsheet de la UI (drawer, tool, panel derecho y estados de edición); el formato de documento conserva los campos legacy para abrir documentos antiguos.
- **Plugins del sistema**: los plugins por defecto se instalan en el paquete (`/usr/share/grafito/plugins`) y la app los carga junto a la carpeta del usuario (`PluginRegistry::load_many`).

#### Corregido
- **Instalación del logotipo**: el paquete del instalador ahora incluye explícitamente el ícono SVG scalable y verifica la presencia de cada tamaño antes de empaquetar (evita distribuir launcher sin logo).

## [1.2.20-beta] - 2026-07-12

#### Añadido
- **Escenas de asistente verificadas**: una respuesta puede proponer una escena `grafito-scene` de flor con tallo, centro y pétalos. Grafito la ejecuta y encuadra sobre un documento aislado antes de habilitar su aplicación atómica.
- **Tetraedro sólido nativo**: `Tetrahedron[x, y, z, edge]` persiste un tetraedro regular por centroide y arista, renderiza cuatro caras triangulares rellenas y seis aristas en GPU y fallback CPU, y queda disponible para propuestas verificadas del asistente.
- **Politopos regulares 4D y N-D**: Grafito incorpora pentácoron, teseracto, 16-celdas, 24-celdas, 120-celdas y 600-celdas con topología canónica exacta, seis planos de rotación 4D, proyección segura a 3D y renderizado GPU con profundidad. Las familias simplex, hipercubo y politopo cruzado también están disponibles entre 3 y 10 dimensiones.
- **Comandos y herramientas multidimensionales**: `Pentachoron4D`, `Tesseract4D`, `SixteenCell4D`, `TwentyFourCell4D`, `OneTwentyCell4D`, `SixHundredCell4D`, `SimplexND`, `HypercubeND` y `CrossPolytopeND` se validan antes de crear objetos. El asistente puede proponerlos mediante tarjetas verificadas y la vista 3D incluye un grupo 4D e inspector de rotaciones.

#### Cambiado
- **Formato de documento**: los guardados actuales usan schema v5 para `Tetrahedron3D` y los politopos tipados 4D/N-D; los envelopes v1 a v4 siguen siendo legibles.
- **Renderizado multidimensional**: las proyecciones 4D estáticas reutilizan `WorldMesh` y sus streams GPU con profundidad; durante movimiento usan un fallback CPU acotado. La selección usa la misma fase de rotación que el objeto visible.

#### Corregido
- **Propuestas remotas**: los botones de aplicar o editar aparecen sólo para comandos que pasaron parseo, staging y prueba local de geometría; una sugerencia inválida queda como texto sin capacidad de mutar el documento.
- **DeepSeek en OpenCode Go**: cada consulta conserva proveedor y modelo de origen, bloquea cambios mientras está en curso, muestra el modelo consultado y transforma fallos de transporte en error visible y toast. El parser acepta contenido final de Chat Completions como texto o bloques de texto y rechaza respuestas parciales o no mostrables.
- **Flor y render 3D**: las superficies paramétricas tienen fallback CPU mientras GPU prepara la escena; las escenas verificadas ajustan la cámara a todos sus componentes y no componen targets GPU de una clave de escena anterior.
- **Curve3D y sólidos**: `Curve3D[..., parametro, minimo, maximo]` conserva y evalúa el parámetro declarado. Curvas inválidas y dominios no ordenados fallan antes de mutar; radios, tamaños y alturas 3D deben ser finitos y positivos.
- **Límites de proyección 4D/N-D**: escalas finitas pero no renderizables se rechazan antes de persistir o mutar el documento, evitando objetos invisibles por desbordamiento.
- **Edición de propiedades**: las modificaciones de objetos, colores y metadata de variables se aplican en staging validado, sin snapshots ni serialización de documentos durante frames inactivos.
- **Variables y planilla**: cambios de variables recalculan fórmulas de la planilla y su geometría dependiente antes de confirmar; los valores propiedad de una celda sólo se editan desde esa celda.

## [1.2.19-beta] - 2026-07-12

#### Corregido
- **Compositor responsive del asistente**: el alto inferior se calcula a partir del contexto, presupuesto y adjuntos acotados, en vez de reutilizar un alto persistido de egui. El editor vuelve a quedar anclado al borde inferior y el transcript recupera toda la altura disponible.
- **Campo de consulta**: el área de texto tiene una superficie y color explícitos, diferenciados de la franja del compositor para que el placeholder y el texto escrito permanezcan visibles en temas claro y oscuro.

## [1.2.18-beta] - 2026-07-12

#### Corregido
- **Superficies propuestas por IA**: las superficies vectoriales `Surface3D[(X(x,y), Y(x,y), Z(x,y)), xmin, xmax, ymin, ymax]` ahora aceptan `x,y` como parámetros del parche y se normalizan de forma segura a la representación interna `u,v`. Se rechaza la mezcla ambigua de ambos pares antes de crear un objeto.
- **Compositor del asistente**: los controles de adjuntar imagen, enviar y cancelar comparten una única fila centrada. Se elimina el botón triangular `Play` duplicado y el contador de presupuesto baja a una línea independiente para no desbordar paneles estrechos.

## [1.2.17-beta] - 2026-07-12

#### Corregido
- **Superficies paramétricas 3D**: `Surface3D` acepta la forma vectorial `Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]` que generan las propuestas del asistente, además de la forma con tres expresiones separadas. Las componentes se validan antes de modificar el documento.
- **Corazón 3D**: la superficie paramétrica del corazón se cubre desde el parser hasta el mesh world-space GPU, evitando que una propuesta válida termine interpretada como una superficie explícita inválida.
- **Antialiasing 3D**: el target offscreen 3D usa el mismo MSAA que la superficie de eframe y resuelve a una textura muestreable antes de componer. La composición usa alpha premultiplicado, evitando bordes oscuros y doble aplicación de alpha.
- **Coordenadas 3D finitas**: el render de superficies deja de descartar geometría enfocada por la cámara sólo por superar una cota absoluta de 1000 unidades.

## [1.2.16-beta] - 2026-07-12

#### Corregido
- **Escenas GPU wire-only**: cubos sin relleno, atractores, líneas y otras escenas 3D que sólo generan aristas inicializan ahora sus recursos, target de profundidad y composición sin depender de triángulos opacos.
- **Fallback de render fiable**: 2D y 3D conservan el renderizador CPU completo hasta que el callback GPU haya preparado con éxito la clave exacta de documento, cámara, viewport y calidad. Un fallo o una recompilación GPU ya no deja el canvas vacío.
- **Carga de documentos**: abrir un archivo sustituye e invalida explícitamente el snapshot GPU, caches visuales y protocolo de construcción, evitando reutilizar una escena vacía de un documento anterior.
- **Asistente gráfico**: `Editar` abre la perspectiva 2D/3D que requiere el comando; `Aplicar` verifica que la geometría propuesta intersecte el viewport y la cámara actual antes de confirmar. Los sólidos básicos aceptan expresiones numéricas finitas como `2*pi`.
- **Geometría distante y layout**: el mesh 3D conserva coordenadas finitas enfocadas por la cámara sin una cota global arbitraria; el panel del asistente cede ancho antes de reducir en exceso el canvas.

## [1.2.15-beta] - 2026-07-12

#### Añadido
- **Asistente gráfico con capacidades**: el asistente puede ofrecer y preflightar comandos autocontenidos para curvas, campos, datos, fractales, complejos, sólidos, superficies, atractores y proyecciones 4D. Cada propuesta declara aridad, vista requerida y ruta de render; las construcciones que requieren objetos etiquetados siguen pidiendo sólo el contexto faltante.

#### Corregido
- **Visualización 3D/4D**: aplicar una gráfica desde el asistente abre la perspectiva 2D o 3D requerida. Las proyecciones de hipercubo e hiperesfera mantienen su overlay CPU cuando el renderizador GPU está activo.
- **Pruebas de render**: la geometría estática ahora cubre curvas paramétricas, polares e implícitas, gráficos estadísticos y retratos de fase; las esferas transparentes no escriben profundidad sobre geometría posterior.
- **Layout responsive**: los paneles laterales se reservan antes del teclado matemático, por lo que el teclado ocupa sólo la columna central y ya no recorta el transcript ni el compositor. La conversación usa toda la altura disponible y el contexto seleccionado se trunca visualmente con el detalle completo al pasar el cursor.
- **Recursos complejos**: `HeatMap` y `ComplexSurface` validan expresión, límites y resolución antes de crear objetos.

## [1.2.14-beta] - 2026-07-12

#### Añadido
- **Coloración de dominio verificable**: `DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]` queda registrado, valida expresión compleja, límites y resolución, y permite visualizar módulo y fase en el plano 2D.

#### Cambiado
- **Propuestas del asistente**: el catálogo contextual ofrece sólo `Function` para gráficas reales o `DomainColoring` para consultas complejas. Solicitudes polares, paramétricas, implícitas, vectoriales o 3D no reciben una sustitución engañosa.

#### Corregido
- **Aplicar en Grafito**: antes de confirmar una propuesta del asistente se ejecuta en un documento aislado y se exige geometría 2D propia, sin ejes, grilla ni objetos previos. Propuestas vacías o inválidas no alteran el documento, el undo ni el protocolo.
- **Panel Complejos**: la acción rápida usa coloración de dominio ejecutable y diferencia esta visualización de una rejilla transformada como `ComplexGrid[1/z]`.

## [1.2.13-beta] - 2026-07-12

#### Corregido
- **Gráficas propuestas por el asistente**: las expresiones aceptan la notación habitual `Sin[x]`/`Cos[t]` dentro de funciones y curvas; una expresión que no compila deja de consumir el presupuesto de muestreo adaptativo y de congelar la interfaz sin producir geometría.
- **Acciones de comandos**: las tarjetas `grafito` truncadas o con argumentos vacíos ya no ofrecen aplicar/editar. El validador rechaza aridad incorrecta para funciones y curvas paramétricas, y todo rechazo muestra un toast además del error en el asistente. `Editar` enfoca la siguiente entrada de comandos visible.
- **Texto matemático**: `$$...$$` inline, `\mathbb`, envoltorios tipográficos comunes y `\dfrac` se muestran como matemática legible en vez de fragmentarse o exponer la fuente cruda.
- **Teclado matemático**: inicia visible, permanece disponible al cambiar cualquier perspectiva y no se oculta automáticamente en ventanas bajas; el usuario conserva el control manual de visibilidad.

## [1.2.12-beta] - 2026-07-12

#### Añadido
- **Matemática tipografiada nativa**: las tarjetas LaTex del asistente ahora componen fracciones anidadas, raíces, potencias, subíndices, letras griegas, relaciones y operadores comunes sin dependencias adicionales. Las fórmulas extensas desplazan horizontalmente dentro de su tarjeta.
- **Inicio orientado**: una conversación vacía muestra una guía compacta y acciones rápidas sin consumir historial ni ser reenviada al proveedor.

#### Cambiado
- **Legibilidad del asistente**: LaTeX inline deja de heredar el estilo azul de código y usa tipografía matemática proporcional; las macros no soportadas o malformadas permanecen visibles como fuente literal.
- **Compositor del chat**: reserva una altura estable, reduce el editor inactivo a una fila, muestra el contexto seleccionado en una línea y oculta contadores sin relevancia hasta acercarse al límite.

## [1.2.11-beta] - 2026-07-12

#### Añadido
- **Respuesta inmediata del asistente**: el mensaje del usuario se incorpora al transcript al enviarse y una onda animada de cuatro puntos indica que el modelo sigue procesando o cancelando la consulta.
- **Acciones aplicables desde el chat**: un bloque `grafito` válido ofrece `Aplicar en Grafito`, que ejecuta la operación mediante el pipeline normal de validación, undo, errores y protocolo de construcción. `Editar` conserva la revisión manual en la barra de comandos.
- **Contexto de herramientas**: el asistente recibe un catálogo acotado de firmas relevantes generado desde el registro canónico de comandos, incluyendo guía exacta para funciones explícitas, curvas paramétricas, polares e implícitas.

#### Corregido
- **Historial de conversación**: mensajes fallidos o cancelados quedan visibles, pero no se reenvían como contexto remoto incompleto; sólo se usan intercambios usuario-respuesta completos.
- **Comandos sin argumentos**: sugerencias registradas como `Aizawa[]` ya no se rechazan por el validador del asistente, sin admitir argumentos vacíos dentro de comandos que los requieren.

## [1.2.10-beta] - 2026-07-12

#### Corregido
- **Transcript del asistente**: las burbujas vuelven a apilar encabezado, texto, listas, tablas y fórmulas verticalmente. Se elimina el desborde que comprimía respuestas en columnas angostas.
- **Espacio de trabajo responsive**: hasta 1120 px de ancho el shell oculta rail y drawer izquierdo, mantiene el asistente y restituye la barra de comandos inferior para preservar el canvas. El teclado matemático cede espacio por debajo de 760 px de alto.
- **Uso vertical del panel**: el compositor reduce el editor vacío, acorta el contexto seleccionado y mueve los límites detallados al tooltip. Tablas y bloques de código desplazan horizontalmente dentro de su burbuja en lugar de recortar el panel.

## [1.2.9-beta] - 2026-07-12

#### Añadido
- **Chat matemático enriquecido**: las respuestas del asistente muestran encabezados, listas, bloques de código, tablas Markdown y expresiones LaTex legibles en tarjetas matemáticas, sin exponer el marcado crudo.
- **Comandos preparados**: el asistente puede proponer comandos registrados de Grafito en bloques `grafito`; el usuario los prepara en la barra de entrada y decide si los ejecuta. Scripts y comandos de carga externa se rechazan. Ninguna respuesta remota modifica el documento por sí sola.
- **Visión Minimax M3**: `minimax-m3` acepta PNG/JPEG validados mediante bloques Anthropic base64 tras la confirmación explícita de capacidad y autorización de carga.

#### Cambiado
- **Conversación del asistente**: el historial ahora tiene scroll propio y el compositor permanece al pie del panel, con tarjetas redondeadas e iconos vectoriales para adjuntar, enviar, quitar imágenes y preparar comandos.
- **Prompt remoto**: solicita respuestas matemáticas estructuradas, tablas y LaTex, además de sugerencias de graficación/análisis estrictamente acotadas.

#### Corregido
- **Adjuntos Minimax M3**: se eliminó el bloqueo que rechazaba imágenes antes de construir el payload Anthropic. Fusion sigue sin imágenes porque su auditoría final con DeepSeek es sólo textual.
- **Consentimiento de imagen**: la autorización de carga se revoca al iniciar cada envío; conservar un adjunto requiere confirmarlo otra vez en la siguiente consulta.
- **Formato estándar**: se aceptan tablas Markdown sin barras exteriores y expresiones LaTex de bloque multilínea con `$$...$$` o `\[...\]`.

## [1.2.8-beta] - 2026-07-12

#### Cambiado
- **Configuración del asistente**: proveedor, modelo, actualización de catálogo y clave API se trasladan a un diálogo abierto desde el engranaje del asistente; la conversación queda libre de controles de conexión. El proveedor y modelo se conservan como preferencias no secretas.

#### Corregido
- **Clave API persistente en Linux**: Grafito deja de usar el backend temporal `mock` de `keyring` y usa Secret Service con persistencia `UntilDelete`. Guardar ya no desaparece al volver a leer ni al reiniciar la aplicación.
- **Resiliencia de sesión**: una clave guardada también se conserva sólo en memoria durante la sesión actual, evitando que una falla transitoria de relectura del llavero bloquee una consulta recién configurada.
- **Estado de proveedor**: cambiar proveedor invalida la disponibilidad de clave anterior y vuelve a comprobar el llavero al abrir configuración.

## [1.2.7-beta] - 2026-07-12

#### Añadido
- **Minimax M3 y Fusion**: `minimax-m3` usa el endpoint Anthropic Messages de OpenCode Go; `fusion` hace que Minimax redacte una respuesta y que `deepseek-v4-pro` la audite antes de mostrarla. Si la auditoría falla, el borrador no se expone.

#### Cambiado
- **Modelos OpenCode Go**: el catálogo prioriza `deepseek-v4-pro`, `deepseek-v4-flash`, `minimax-m3`, `fusion` y GLM; Kimi y Mimo se omiten incluso si aparecen durante la actualización remota.
- **Panel de conexión**: proveedor, modelo, actualización y clave ahora tienen filas completas en una tarjeta compacta, evitando controles cortados en paneles angostos. Enter en el campo de clave la guarda de forma explícita.

#### Corregido
- **Clave guardada**: Consultar y actualizar modelos recuperan la clave del llavero al momento de usarse; ya no quedan bloqueados por el indicador visual de clave tras reiniciar Grafito.
- **Fusion y adjuntos**: modelos con protocolo o capacidad de imagen no confirmados bloquean adjuntos antes de transmitir datos.

## [1.2.6-beta] - 2026-07-12

#### Cambiado
- **Asistente compacto**: el panel prioriza pregunta y conversación, muestra el contexto de función sólo cuando existe y reduce la configuración, sugerencias y texto persistente que no aporta a la consulta actual.
- **Selector de modelos**: OpenCode Go y Ollama ofrecen listas desplegables con modelos conocidos y los detectados mediante actualización; ya no aceptan IDs de modelo escritos libremente.
- **Límites y adjuntos**: se muestra el uso real de bytes de entrada, los límites de respuesta/tiempo/imágenes y el estado de importación dentro de la sección correspondiente.

#### Corregido
- **Consentimiento de visión**: cambiar proveedor, modelo o adjunto invalida las confirmaciones necesarias; una importación de imagen bloquea el envío hasta terminar.
- **Historial contextual**: los turnos se recortan de forma segura y sólo se reenvían pares usuario-respuesta completos que caben en el presupuesto, evitando bloqueos de solicitudes futuras.

## [1.2.5-beta] - 2026-07-11

#### Añadido
- **Asistente con proveedores**: integración segura con OpenCode Go y Ollama, consulta de modelos, solicitudes en segundo plano, cancelación y claves del usuario en el llavero del sistema.
- **Contexto de función**: una función seleccionada aporta expresión y dominio a la consulta; el asistente conserva una conversación de sesión acotada y propone próximos pasos completables.
- **Imágenes para visión**: selector PNG/JPEG validado, sin rutas de origen, con autorización separada para enviar bytes al modelo elegido.

#### Cambiado
- **Experiencia del asistente**: Enter envía la consulta, Shift+Enter conserva una línea nueva, y el panel reemplaza el flujo de resolución local por conversación contextual con proveedor y modelo configurables.

#### Eliminado
- **Transcripción manual**: se retiró el editor de transcripción de imágenes y la etiqueta/copy de asistente local.

## [1.2.4-beta] - 2026-07-11

#### Cambiado
- **Asistente persistente**: el asistente local ahora es un panel lateral derecho siempre visible que reserva espacio antes de renderizar el lienzo 2D o 3D.
- **Shell de escritorio**: los drawers contextuales sólo coexisten con el asistente a partir de 1360 px de ancho; la ventana nativa mantiene un mínimo de 960 x 600 px para conservar un área de trabajo útil.

#### Eliminado
- **Launcher del asistente**: se retiraron el botón vectorial dentro del canvas, la ventana flotante, el estado `open` y el icono exclusivo que permitían solapar o cerrar el asistente.

## [1.2.3-beta] - 2026-07-11

#### Añadido
- **Entrada natural de integrales**: `f(x): ∫e−x2dx` se interpreta como la integral acumulada `f(x) = ∫₀ˣ exp(-x²) dx`, con validación atómica para no crear funciones ni variables fantasma.
- **Asistente acoplado al lienzo**: control vectorial persistente, accesible y localizado en el canvas; abre un único panel local con contenido desplazable.

#### Cambiado
- **Espacio de trabajo inicial**: documento vacío y teclado matemático oculto al iniciar, para priorizar el lienzo.
- **Contraste de objetos**: los objetos nuevos usan un trazo neutro visible sobre temas claros y oscuros.

#### Corregido
- **Asistente y protocolo**: se eliminaron el botón textual flotante, el acoplamiento tardío y los controles de reordenar/desactivar del protocolo que no modificaban la construcción real.
- **Cambio de herramienta**: cancelar una construcción pendiente ahora limpia todos sus puntos, ghost y estado transitorio.
- **Integración Linux**: `StartupWMClass` coincide con el `app_id` de la ventana para asociar correctamente el icono del lanzador.

## [1.2.1-beta] - 2026-07-04

#### Añadido
- **Derivadas de Wirtinger**: Nuevos operadores `deriv_z(f)` y `deriv_z_conj(f)` en expresiones complejas, facilitando el análisis y la visualización de derivadas respecto a $z$ y $\bar{z}$.

#### Corregido
- **Filtro de ruido en Domain Coloring**: Añadidas salvaguardas de magnitud mínima (`mag < 1e-6`) para pintar píxeles negros en lugar de ruido de fase caótico de punto flotante en la evaluación de funciones complejas idénticamente nulas (como `deriv_z_conj(z^2) = 0`), tanto en CPU como en el shader WGSL de la GPU.
- **Soporte de Modos de Coloración**: Corregido el renderer en 2D que anteriormente ignoraba el campo `domain_coloring_mode`, permitiendo visualizar correctamente el HSL Clásico, Retrato de Fase Puro, y las rejillas Polar y Cartesiana Conformes.
- **Solapamiento de Perspectivas**: Limpiar variables temporales de la animación trigonométrica al cambiar de perspectiva para evitar paneles superpuestos en la vista de Complejos.
- **Barra de Entrada Duplicada**: Ocultar la barra de entrada inferior duplicada en la perspectiva de Complejos, ya que el panel izquierdo cuenta con su propia barra dedicada.
- **Empaquetado y Compilación en build-deb.sh**: Compilar explícitamente el paquete `grafito-app` antes de copiar el binario para empaquetar, garantizando que el paquete `.deb` contenga siempre la última versión construida.

## [1.2.0-beta] - 2026-06-30

#### Añadido
- **Geometría analítica 3D universitaria**: nuevos objetos `Plane3D` y
  `Line3D` para planos `ax + by + cz + d = 0` y rectas punto+dirección.
  Nuevos comandos `Plane3D[a,b,c,d]`, `Plane3D[P1,P2,P3]`,
  `Line3D[x0,y0,z0,dx,dy,dz]`, `Line3D[P1,P2]`, `EquidistantFrom[...]`
  y `Solve3DGeometry[...]`. Se resuelve el caso tipo UTN de puntos sobre
  un eje equidistantes de un plano y una recta, creando los `Point3D`
  solución automáticamente.
- **Módulo `planes3d` en `grafito-geometry`**: añade `Plane3D`, `Line3D`,
  distancia punto-plano, distancia punto-recta, proyección sobre plano y
  punto más cercano en recta, con tests del problema `x+z+4=0` y
  `r=(1,1,2)+β(1,1,0)`.
- **Paquete UTN de álgebra lineal y geometría analítica**: nuevos comandos
  `Intersection3D`, `Projection3D`, `PlaneThroughLines`,
  `PlaneThroughLinePoint`, `LineRelation3D`, `Rank`, `NullSpace`,
  `LinearSolve`, `Eigenvalues`, `ConditionNumber`, `P2Dependence`,
  `P2Basis`, `P2Equations`, `SubspaceDimension`, `SubspaceBasis`,
  `SubspaceSum`, `SubspaceIntersection`, `OrthogonalComplement`,
  `SolveLine3DParameters` y `MatrixParamSolve` para ejercicios universitarios
  con planos, rectas, matrices, polinomios de `P2` y subespacios de `Rn`.
- **Comandos avanzados de álgebra lineal**: `Transpose`, `Trace`,
  `Eigenvectors`, `LU`, `QR`, `Cholesky` y `SVD`, exponiendo rutinas ya
  disponibles en `grafito-geometry` desde el procesador CAS compartido.
- **Comandos AM2 de cálculo multivariable**: `Gradient`,
  `DirectionalDerivative`, `TangentPlane`, `Divergence`, `Curl`,
  `DoubleIntegral` y `SurfaceArea`, con soporte para variables explícitas,
  puntos/vectores numéricos y bounds internos constantes o dependientes de la
  variable exterior.
- **Solvers ODE avanzados expuestos por comando**: `ODE[...]` acepta ahora
  métodos `rk45`/`rkf45` y `backward_euler`; `ODESystem[...]` acepta
  `rk45`/`rkf45`. Se mantienen `euler` y `rk4` como antes.
- **Comandos de sucesiones y series**: `SequenceLimit`, `SeriesSum`,
  `RatioTest` y `RootTest` para límites heurísticos de sucesiones, sumas
  finitas y criterios básicos de convergencia de series.
- **Animación trigonométrica integrada al documento**: el panel ahora soporta
  `sin`, `cos`, `tan`, `cot`, `sec` y `csc`, sincroniza variables
  `trig_t`/`trig_value` y mantiene objetos reales `TrigGraph` y `TrigValue`
  para que la animación no quede aislada de la escena.
- **Guards NaN en `ast.rs`** (`eval_2d`): `Sqrt(negativo)`, `Ln/Log(≤0)`,
  `Pow(base negativa, exponente fraccionario)`, `Asin/Acos(|x|>1)`,
  `Acosh(<1)`, `Atanh(|x|≥1)`, y clamp de `Sinh/Cosh/Tanh` a 0 para
  `|x|>1e9` producen ahora `NaN` explícito en lugar de propagarse
  silenciosamente.
- **Cap de profundidad DFS en el grafo de restricciones** (`MAX_DFS_DEPTH = 512`):
  evita desbordamiento de pila ante saves maliciosos con cadenas profundas
  de restricciones.
- **Validación ampliada de documentos**: límite en la cantidad total de
  restricciones y en la longitud de etiquetas de objetos, además de los
  tipos ya validados.
- **Render 3D de planos y rectas**: `Plane3D` se visualiza como parche
  translúcido con wireframe y `Line3D` como recta extendida centrada en su
  punto de paso.

#### Corregido
- **Bug de `Extrude` con `return` prematuro** (`document.rs`): el brazo
  `Extrude` usaba `return;` cuando `height≈0` o el polígono tenía <3
  vértices, lo que abortaba `apply_constructive_constraints` entera y
  saltaba todas las restricciones posteriores. Cambiado a `continue;`.
- **`remove_object` dejaba outputs huérfanos**: al borrar un objeto libre
  que alimentaba una restricción (p. ej. `Midpoint(A,B)→M`), el grafo se
  limpiaba pero `M` permanecía en `Document.objects` como geometría
  fantasma. Ahora la cascada elimina los outputs recursivamente y limpia
  la selección.
- **`arc_length`, `volume_of_revolution` y `surface_of_revolution`
  silenciaban `NaN`**: el `unwrap_or(0.0)` convertía errores de evaluación
  (incluyendo `NaN`) en `0.0`, dando resultados silenciosamente
  incorrectos. Ahora propagan `NaN` vía `unwrap_or(f64::NAN)`.
- **Dependencia `glam` duplicada en el build**: `grafito-complex`
  declaraba `glam = "0.24"` (sin usarla), arrastrando `glam 0.24.2`
  además de `0.29.3` y duplicando el binario. Eliminada la dependencia
  muerta; `num-complex` alineada al workspace.
- **Panel trigonométrico apilado**: al activar la animación trigonométrica ya
  no se crea un panel intermedio junto al protocolo de construcción; reemplaza
  el panel derecho activo y se adapta con scroll a anchos reducidos.

#### Eliminado
- **Código muerto**: `node_count` (`render_2d.rs`), `find_nearest_feature`
  (`input.rs`), `export_pdf` sin callers y su test, y `draw_ripples` stub
  vacío. Removidos los `#[allow(dead_code)]` falsos positivos en
  `export_png`/`export_latex`/`escape_latex`, `ToolState` y el enum `Op`.

## [1.1.9-beta] - 2026-06-29

#### Añadido
- **Funciones complejas reales (CPU + WGSL)**: `conj(z)`, `re(z)`, `im(z)`, `arg(z)` ya
  no devuelven NaN. Implementadas en Rust (`ComplexMatrix`/`ComplexExpr::eval`) y
  en el shader `complex_compute.wgsl` (GPU dispatch para `ComplexMapping`).
- **Funciones complejas especiales (CPU, fallback NaN en GPU)**:
  - `erf(z)` — función de error compleja, series de Taylor + fórmula asintótica.
  - `lambert_w(z)` — iteración de Newton (rama principal W₀).
  - `zeta(z)` — zeta de Riemann, fórmula de Borwein + ecuación funcional
    para `Re(s) < 0`.
  - `bessel_y(z)` — Bessel de segunda clase, serie con números armónicos
    para n=0, relación `Y_n = (J_n cos(nπ) - J_{-n}) / sin(nπ)` para n≠0.
- **Gamma complejo (Lanczos)**: implementación de la función Gamma para
  argumentos complejos usando la fórmula de Lanczos con g=7 y 9 coeficientes
  (precisión ~15 dígitos). Para `Re(z) < 0.5` se aplica la fórmula de
  reflexión Γ(z) = π / (sin(πz) · Γ(1-z)).
- **BesselJ complejo (series + integral)**: la implementación por series
  converge para `|z| < 20` y la representación integral (cuadratura
  trapezoidal con 256 puntos) para `|z| ≥ 20`.
- **GPU compute pipeline para domain coloring** (`DomainColoringComputePipeline`):
  nuevo shader `domain_coloring_compute.wgsl` que evalúa `f(z)` sobre una
  grilla 2D en paralelo y produce colores RGBA con la coloración HSL
  (matiz = arg(f(z)), luminosidad = atan(ln(|f(z)|)) / (π/2) · 0.5 + 0.5).
  Soporta hasta **250 000 celdas** (500×500) en un solo dispatch.
  Reemplaza la evaluación CPU per-cell de `render_mode = 1` con un speedup
  masivo durante pan/zoom.
- **Panel "Animación Trigonométrica"** (menú Herramientas): dibuja el
  círculo unitario con el vector radio animado en el ángulo `t`, las
  proyecciones a los ejes (coseno verde, seno azul) y un gráfico 2D
  sincronizado de `sin(t)`/`cos(t)`/`tan(t)`. Controles play/pause,
  slider de velocidad (±3 rad/s) y slider manual del ángulo (±2π).
  Usa la variable `trig_angle` y se integra con el loop de animación
  del documento.
- **Comando `ComplexSurface[...]`**: crea una `Surface3DObj` con la flag
  `is_complex = true`. El sampler evalúa la expresión compleja y grafica
  `z = |f(x + iy)|` como superficie 3D sobre el plano complejo. Alias:
  `complexsurface`, `complex_surface`, `csurface`.
- **Comando `Quadrants[...]`**: nuevo `render_mode = 4` para `ComplexGrid`
  que pinta los cuatro cuadrantes del plano complejo con colores
  distintivos (rojo/verde/azul/amarillo), etiquetas Q1-Q4 y marcas +Re/+Im.
  Alias: `quadrants`, `cuadrantes`.
- **Opción `Tool::TrigAnimation`**: nueva herramienta para abrir el panel
  de animación trigonométrica directamente desde la toolbar.
- **Geometría analítica 3D universitaria**: nuevos objetos `Plane3D` y
  `Line3D` para planos `ax + by + cz + d = 0` y rectas punto+dirección.
  Nuevos comandos `Plane3D[a,b,c,d]`, `Plane3D[P1,P2,P3]`,
  `Line3D[x0,y0,z0,dx,dy,dz]`, `Line3D[P1,P2]`, `EquidistantFrom[...]`
  y `Solve3DGeometry[...]`. Se resuelve el caso tipo UTN de puntos sobre
  un eje equidistantes de un plano y una recta, creando los `Point3D`
  solución automáticamente.
- **Módulo `planes3d` en `grafito-geometry`**: añade `Plane3D`, `Line3D`,
  distancia punto-plano, distancia punto-recta, proyección sobre plano y
  punto más cercano en recta, con tests del problema `x+z+4=0` y
  `r=(1,1,2)+β(1,1,0)`.
- **Paquete UTN de álgebra lineal y geometría analítica**: nuevos comandos
  `Intersection3D`, `Projection3D`, `PlaneThroughLines`,
  `PlaneThroughLinePoint`, `LineRelation3D`, `Rank`, `NullSpace`,
  `LinearSolve`, `Eigenvalues`, `ConditionNumber`, `P2Dependence`,
  `P2Basis`, `P2Equations`, `SubspaceDimension`, `SubspaceBasis`,
  `SubspaceSum`, `SubspaceIntersection`, `OrthogonalComplement`,
  `SolveLine3DParameters` y `MatrixParamSolve` para ejercicios universitarios
  con planos, rectas, matrices, polinomios de `P2` y subespacios de `Rn`.

#### Cambiado
- **Coloración de dominio unificada a HSL**: tanto el renderer wgpu
  (`lib.rs:1826-1851`) como el egui CPU (`render_2d.rs:2528-2535`) ahora
  usan `lightness = atan(ln(mag)) / (π/2) · 0.5 + 0.5` (escala logarítmica,
  estilo cplot) en vez de la fórmula HSV anterior. La opacidad de saturación
  se eleva a 0.85 para colores más vivos.
- **GPU path integrado en `add_complex_grid_geometry_gpu`**: el método
  estático `add_complex_grid_geometry` se mantiene como fallback CPU; el
  método de instancia `add_complex_grid_geometry_gpu` delega al
  `DomainColoringComputePipeline` si hay device/queue disponibles.
- **Render 3D de planos y rectas infinitas**: `Plane3D` se visualiza como
  un parche translúcido con wireframe y `Line3D` como una recta extendida
  centrada en su punto de paso, en los builders 3D estático y runtime.

#### Corregido
- **Bug Gamma y BesselJ stubs**: ambos devolvían `NaN` literal desde
  siempre. Reemplazados por las implementaciones reales descritas arriba.
- **Comentario engañoso en `ComplexBytecodeProgram.constants`**: decía
  "Wait, complex constants!" pero el código ya guardaba pares (re, im).
  Documentado formalmente.
- **Convención `complex_sqrt` GPU**: el shader usaba `vec2<f32>` para
  constantes (vector 2D), no `f64` escalar. Confirmado y unificado en
  los nuevos shaders.
- **Kernel de matrices subdeterminadas**: `null_space` ahora completa la
  nullidad faltante con RREF cuando la SVD devuelve `Vᵀ` reducido para
  matrices anchas de rango fila completo. Esto corrige intersecciones de
  subespacios como `span(e1,e2) ∩ span(e2,e3) = span(e2)`.

## [1.1.4-beta] - 2026-06-22

#### Añadido
- **Ventana modal "Acerca de Grafito"**: el botón del menú Ayuda ahora abre una
  ventana modal con la versión actual, una descripción en español de qué es
  Grafito y un resumen de los cambios principales de la release 1.1.4.
- **Etiqueta dinámica de versión en el menú**: el texto "Acerca de Grafito
  vX.Y.Z" se construye desde `env!("CARGO_PKG_VERSION")`, así que siempre
  refleja la versión publicada sin tener que editarlo a mano.
- **Bump de workspace**: la versión en `Cargo.toml` (`[workspace.package]`)
  pasa de `1.0.0-beta` a `1.1.4-beta`. El `.deb` ahora se llama
  `grafito_1.1.4-beta_amd64.deb`.
- **Descripción de paquete `.deb` en español**: el campo `Description` de
  `packaging/debian/control` ahora describe Grafito en español y menciona
  las 10 perspectivas y los mapeos conformes.
- **Hogar nuevo del repositorio**: el `Homepage` del `.deb` apunta a
  `https://github.com/Diez111/Grafito-Open`.

#### Cambiado
- **`--help` y la barra de splash**: muestran `v1.1.4-beta` y un tagline
  en español. La etiqueta "v1.0.0-beta" hardcodeada se eliminó.

#### Corregido
- **Botón "Acerca de" sin acción**: el handler de `Acerca de Grafito v...`
  en el menú Ayuda era un `if ui.button(...).clicked() {}` vacío. Ahora
  abre la ventana modal "Acerca de Grafito".

## [1.1.3-beta] - 2026-XX-XX

#### Añadido
- **Mapeos conformes algebraicos**: nuevo módulo `grafito_geometry::conformal::algebraic_mappings::ConformalMap` que implementa 13 mapeos algebraicos de primera clase (`1/z`, `z^n`, `exp(z)`, `log(z)`, `ln(z)`, `sin(z)`, `cos(z)`, `tan(z)`, `sinh(z)`, `cosh(z)`, `sqrt(z)`, `z+1/z` (Joukowski), `(a*z+b)/(c*z+d)` (Möbius) y `1/(z-a)`). El `ComplexMappingObj` cachea automáticamente el mapeo reconocido al construirse, evitando parsear el AST en cada evaluación.
- **Wrapper `Value` para evaluación polimórfica**: nuevo enum `grafito_geometry::value::Value::{Real, Complex}` con promoción automática Real↔Complex. `Expr::eval_value` es la nueva API que evalúa expresiones con aritmética mixta; las APIs legacy (`eval`, `eval_2d`, `eval_3d`, `eval_at`) siguen funcionando intactas.
- **Módulo `conformal/`**: `complex_expr` se movió a `grafito_geometry::conformal::complex_expr`. Re-export de retrocompatibilidad en `lib.rs` mantiene `use grafito_geometry::complex_expr` funcionando.
- **Sistema unificado de iconos vectoriales** (`grafito_ui::icons`): nuevo módulo con 70+ iconos outlined estilo macOS/iOS. Reemplaza los emojis sueltos y las letras-símbolo en el sidebar, algebra, command palette y otros componentes. Todos los iconos se dibujan con `egui::Painter` (sin dependencia del font del sistema), garantizando apariencia idéntica en Windows, macOS y Linux.
- **Design tokens** (`grafito_ui::tokens`): escalas tipográficas (ratio 1.13 entre TYPE_XS=11 y TYPE_XXL=28), escala de spacing (base 4px) y radios (4, 8, 12) en constantes públicas.
- **19 tokens semánticos nuevos en `Theme`**: input_bar_bg, sidebar_bg, sidebar_tab_active_bg, sidebar_tab_inactive, sidebar_tab_active, status_bar_bg, separator, text_secondary, text_tertiary, text_label, accent_muted, accent_strong, warning, grid_line, grid_minor, axis_label, object_polygon, object_label, ghost_preview, newly_created_glow, selection_outline, hover_overlay. La función `current_theme(ctx)` resuelve el tema activo en runtime.
- **Splash screen al inicio**: durante 1.5 segundos al abrir Grafito, se muestra un overlay negro con el logo (assets/grafito-icon-256x256.png), el nombre, la versión y el tagline. Hace fade-out en los últimos 500ms.
- **Empty state en el panel de álgebra**: cuando el documento está vacío, en vez de una lista vacía se muestra un mensaje con icono vectorial grande y la instrucción "Escribí en la barra inferior para crear tu primer objeto".
- **Hover overlay coherente**: los items de la lista de álgebra muestran un highlight sutil al pasar el mouse, usando el token `theme.hover_overlay`.

#### Cambiado
- **Render de `ComplexMapping`**: cuando la expresión es un mapeo algebraico reconocido, se evalúa directamente con la fórmula cerrada (camino rápido). Para expresiones arbitrarias, se mantiene el path original con `eval_complex_batch`.
- **Sample de `ImplicitCurve` en `ComplexMapping`**: se filtran segmentos degenerados (longitud < 1e-3) que marching squares generaba en celdas inestables, evitando el "relleno espurio" del disco al mapear una circunferencia.
- **Sidebar**: las 6 pestañas ahora usan iconos vectoriales en lugar de letras sueltas ("A", "H", "C", etc.) y emojis. Se ve consistente con la toolbar.
- **Theme storage**: `DARK` y `LIGHT` ahora son `Lazy<Theme>` (con `once_cell`) en lugar de `const Theme`, lo que permite usar `Color32::from_rgba_unmultiplied` con alpha. Esto requirió agregar `once_cell` como dependencia del workspace.

#### Corregido
- **`ComplexMapping[1/z, ...]`**: el lexer del parser complejo insertaba un `*` implícito entre `Number` e `Ident`, así que `1/z` se tokenizaba como `1*z` y se evaluaba incorrectamente. Ahora se reconoce como `ConformalMap::Inversion` y se aplica la fórmula algebraica cerrada `1/z`, sin pasar por el parser. El resultado sobre la circunferencia unitaria es la circunferencia unitaria, como debe ser.
- **`ComplexMapping[log(z), ...]`**: idem; `log(z)` sobre la circunferencia unitaria devuelve el segmento en el eje imaginario (rama principal del log complejo), no un "disco" espurio.
- **`ComplexMapping[z^2, ...]`**: idem; `z^2` sobre la circunferencia unitaria devuelve la circunferencia unitaria, sin la línea vertical asintótica incorrecta que aparecía antes.
- **Línea vertical asintótica incorrecta**: cuando el target era una `ImplicitCurve`, el sample generaba segmentos extra que el renderer interpretaba como singularidad, dibujando asíntotas donde no las había. El filtro de segmentos degenerados + el camino algebraico eliminan este artefacto.
- **Inconsistencia de colores entre paneles**: `panels.rs::panel_theme`, `algebra.rs`, `tools_panel.rs`, `ui.rs` y `keyboard.rs` reinventaban la paleta con valores distintos. Ahora todos usan el `Theme` central, así el azul de acento es el mismo en todos lados.
- **Relleno del `ImplicitCurve` se salía del contorno**: el relleno por defecto de regiones (`x² + y² ≤ 1` etc.) usaba una grilla 80×80 y pintaba cada celda como un rectángulo completo. El centro de la celda podía estar dentro de la región pero sus esquinas afuera, así que el rectángulo se extendía más allá del contorno y se veían cuadrados pixelados sobresaliendo. Ahora se usa **scanline fill real**: por cada fila de píxeles se muestrea el campo escalar en cada columna, se encuentran los cruces de signo (con refinamiento lineal para precisión sub-píxel) y se rellena entre pares de cruces con la regla par-impar. El borde del relleno coincide con la curva, sin sobrepasarla. Aplica también al `ComplexMapping` de la región (p. ej. `1/z` sobre la circunferencia unitaria).
- **"Crash" y lag severo al usar `ImplicitCurve` con cualquier expresión**: el render de relleno evaluaba el campo escalar (`eval_2d`) **una vez por cada píxel** del canvas. Para un canvas de 1920×1080 con expresiones simples como `x²+y² < 1` eran ~4M de evaluaciones AST por frame (~300ms), y con expresiones complejas superaban 1 segundo por frame. La app se sentía colgada. Ahora el scanline usa **stride adaptativo**: stride=8 para ASTs pequeños (hasta 30 nodos, ~250 FPS) y stride=16 para ASTs grandes (~100 FPS). Además el AST se **cachea** en `ImplicitCurveObj` para no reparsear en cada frame. Resultado: el relleno es fluido incluso con expresiones de muchos nodos.
- **"No grafica" al cambiar la expresión de una `ImplicitCurve` ya creada**: el cache de `segments_or_compute` reusaba los segmentos viejos si el view y el grid_size no cambiaban, sin verificar que la expresión (`expr_lhs`/`expr_rhs`/`operator`) tampoco hubiera cambiado. Si el usuario editaba la fórmula, veía los segmentos del contorno antiguo (o nada). Ahora el cache compara también la expresión antes de reusar.
- **"No grafica" en el relleno de `ImplicitCurve` con stride grande**: el scanline con stride=8/16 era demasiado agresivo para muchas regiones. Con un view típico (`[-1.5, 1.5]`), el stride=8 saltaba el disco entero porque cada sample en world-x se separaba 0.024 unidades, dejando solo 1-2 samples sobre un disco de radio 1. Reducimos el stride a 2/4: stride=2 para ASTs pequeños (hasta 30 nodos) y stride=4 para ASTs grandes. Eso da 2-4× speedup vs stride=1, suficiente para mantener 60+ FPS, y detecta correctamente regiones de cualquier tamaño.
- **Crash al usar `x² + y²` (y otros caracteres Unicode superscript)**: el parser de Grafito solo reemplazaba `x²` por `x^2` en el command processor pero NO `y²`/`z²`/`t²`/etc. El `preprocess_expr` del crate `grafito-geometry` no reemplazaba ninguno. Peor aún, `find_standalone_sum_product` usaba char indices con byte slices (`expr[i..]`) y panickeaba con `"start byte index 2 is not a char boundary; it is inside '²'"`. Ahora: 1) el command processor reemplaza TODOS los superscripts Unicode comunes (`x²`/`y²`/`z²`/`t²`/`r²`/`a²`/`b²`/`c²`/`n²`/`θ²`/`φ²`/`x³`/`y³`/`z³`); 2) `preprocess_expr` también los reemplaza (para archivos .json guardados); 3) `find_standalone_sum_product` y `find_matching_close` ahora usan byte indices consistentes y no panican con UTF-8.
- **"No grafica" para `<`, `>`, `<=`, `>=` en `ImplicitCurve`**: el fill por default tenía `alpha = 0.2`, que es casi invisible. El outline (línea sólida) se ve claramente pero el fill (región translúcida) no se distingue del fondo. Subimos el alpha del fill a `0.5` para que sea claramente visible sin ocultar el outline. Los 5 operadores (`<`, `>`, `<=`, `>=`, `=`) ahora grafican correctamente: `=` solo dibuja el contorno; `<`/`<=` dibujan el interior; `>`/`>=` dibujan el exterior.
- **Cache del AST mezclaba lhs y rhs en `ImplicitCurveObj`**: el primer cache de AST usaba un solo slot que se sobreescribía entre llamadas a `get_cached_ast("lhs", ...)` y `get_cached_ast("rhs", ...)`, devolviendo el AST incorrecto en frames alternados. Ahora `get_cached_asts` devuelve ambos ASTs juntos en un solo slot indexado por el hash combinado, evitando la confusión.

## [1.1.2-beta] - 2026-06-16

#### Añadido
- **Comandos de medición**: `Area[objeto]`, `Circumference[objeto]`, `Center[objeto]`, `Length[objeto]`, `Slope[objeto]`. `Area` dibuja un polígono sombreado con el valor del área. `Center` crea un punto en el centro de Círculo, Elipse, Parábola o Hipérbola.
- **Comandos de construcción geométrica**: `Sector[centro, radio, angulo]` (sector circular con polígono sombreado), `Arc[centro, radio, ang1, ang2]` (arco circular).
- **Comandos CAS de cálculo diferencial/integral**: `TangentAt[función, x]` (línea tangente), `NormalAt[función, x]` (línea normal), `ArcLength[función, a, b]` (∫√(1+f'²)dx), `CurvatureAt[función, x]` (κ = |f''| / (1+f'²)^(3/2)), `VolumeOfRevolution[función, a, b]` (π∫f²dx), `SurfaceOfRevolution[función, a, b]` (2π∫|f|√(1+f'²)dx). Alias en español: `TangenteEn`, `NormalEn`, `LongitudArco`, `CurvaturaEn`, `VolumenRevolucion`, `SuperficieRevolucion`.
- **Snap a intersecciones**: nueva función `snap_to_intersections` que computa intersecciones entre pares de objetos visibles (Line-Line, Line-Circle, Circle-Circle, Function-Line, Function-Function) al hacer hover cerca del cursor. Muestra "Intersección: (x, y)" como etiqueta de snap.
- **Grupos nuevos en la toolbar**: `ANALYSIS` (Root, Extremum, Inflection, YIntercept, XIntercept, Intersect, Analyze), `CONSTRAINT` (Coincident, DistanceConstraint, Angle, Horizontal, Vertical, EqualLength, Symmetry) y `BOOLEAN` (Union, Intersection, Difference, XOR).
- **Iconos vectoriales**: `icon_analysis` (mira con curva + marcador de raíz), `icon_constraint` (regla con bola en cada extremo).
- **Panel de álgebra**: ahora muestra área, perímetro, longitud y volumen calculados en tiempo real para Línea, Círculo, Elipse, Polígono, Esfera 3D, Cubo 3D, Cilindro 3D, Cono 3D y Segmento 3D.
- **Reflect mejorado**: `Reflect[objeto, punto, punto]` ahora refleja objetos completos (Point, Line, Circle, Polygon) preservando el label con sufijo `'`, en lugar de solo crear un punto reflejado R'.

#### Cambiado
- **Tool Angle**: ahora dibuja un arco visual (sector poligonal sombreado) entre los rayos medidos, en vez de solo un label de texto flotante.
- **Tool Area**: dibuja un polígono sombreado relleno para áreas de círculo, polígono y área bajo curva (función), con un color azul semitransparente distintivo.
- **Color picker**: rueda HSV rediseñada con un gradiente Mesh ultra-suave (64 segmentos) en vez de sectores poligonales discretos. Ajuste fino de layout (136 px, 280 px altura).
- **Toolbar**: reorganización en secciones lógicas: 12 grupos (antes 10), añadidas herramientas `Ray`, `Vector`, `ImplicitCurve`, `VectorField2D`, `ConicByFivePoints` que estaban disponibles por comando pero no en la UI.

#### Corregido
- **Toolbar roto**: 21 herramientas tenían mapeos erróneos (`Tool::Select` o duplicados) — `Segment` apuntaba a `Line`, `RegularPolygon` a `Polygon`, `EllipseByFoci`/`ParabolaByFocusDirectrix`/`HyperbolaByFoci` a `Select`, `ParametricCurve2D`/`PolarCurve` a `Select`. Todas corregidas a sus `Tool` correspondientes. Eliminadas entradas inexistentes (Pirámide, Cono, Cilindro, Toro, Hipercubo 4D, Texto) y duplicados (Círculo centro-radio).
- **Snap a curva roto**: `snap_to_curve` descartaba los resultados para Círculo y Línea con `let _ = c;` y `let _ = l;` sin crear el `SnapResult`. Ahora proyecta correctamente el punto del cursor sobre el borde del círculo o la línea.
- **Cierre de color picker**: se podía cerrar con escape pero dejaba el toggle del panel desincronizado — corregido con `fixed_size` consistente.

## [1.1.0-beta] - 2026-06-16

#### Añadido
- **`ComplexMapping[expr, target]`**: aplica una expresión compleja arbitraria a un objeto del documento. Soporta `Line`, `Polygon`, `Function`, `ImplicitCurve`, `ParametricCurve2D` y `PolarCurve` como targets. Ejemplo: `ImplicitCurve[x^2 + y^2 = 1]; ComplexMapping[1/z, c]` invierte el círculo. Las singularidades (puntos donde `expr` explota, p.ej. `1/z` cerca del origen) se manejan con **asíntotas automáticas punteadas** en la dirección de la tangente previa. Si no hay tangente previa, se marca con una `X` roja. Alias en español: `MapeoComplejo`, `MapeoComplejoCompleja`, `TransformadaCompleja`.
- Tests de integración en `crates/grafito-command/tests/complex_mapping.rs` cubriendo los 6 tipos de target.
- **`student_t_quantile(p, nu)`**: cuantil de la distribución t-Student por bisección. Usado en `confidence_interval_mean` para `n < 30` (antes usaba la normal, subestimando el intervalo).
- **`Matrix::checked_get` / `checked_set`**: variantes seguras de `get`/`set` que devuelven `None`/`false` ante índices fuera de rango.
- **8 tests nuevos en `interval.rs`** (crosses_zero, contains, definitely_positive/negative, safe_sample, detect_asymptotes, midpoint) y 1 test en `ode.rs` (`euler_zero_steps`).
- **3 tests nuevos en `document.rs`** para `migrate_complex_symbol` (variante con subíndice, base, y sin coincidencia).

#### Cambiado
- `erf` y `gamma_ln` en `statistics.rs` ahora delegan en `crate::special_functions::erf` / `ln_gamma` (implementaciones canónicas) en lugar de las aproximaciones locales.
- `confidence_interval_mean` usa t-Student para muestras pequeñas (`n < 30`) y normal para `n ≥ 30`, en lugar de usar siempre la normal.
- Botón "Salir" del menú ahora usa `ctx.send_viewport_cmd(Close)` en vez de `std::process::exit(0)`, permitiendo un cierre ordenado de wgpu/egui sin abortar operaciones en vuelo.
- Snapshot del documento en `GrafitoApp` se clona solo cuando cambia `version`; cambios de view (pan/zoom) usan `Arc::make_mut` para evitar clones por frame.
- `configure_modern_style` se aplica solo cuando cambia el tema, no en cada frame.
- Eliminado el camino CPU de `marching_squares_contour` y la utilidad `hsv_to_rgb` (sustituidos por el pipeline GPU `ImplicitComputePipeline` y `fractal_color_hsv` respectivamente).

#### Corregido
- **Seguridad numérica en GPU/WGSL**:
  - Protección contra stack underflow/overflow en los 4 shaders (`function`, `implicit`, `parametric`, `vector`): `sp < 0 || sp >= STACK_SIZE` devuelve `NaN` en lugar de corromper memoria de la pila.
  - `log`/`sqrt` con argumento no válido ahora devuelven `NaN` en GPU en vez de `0.0` (antes silenciaba el error y generaba gráficas planas o discontinuidades).
  - División por cero en `cs_main` cuando `params.n == 0` o `params.grid_size == 0` (`max(n-1, 1)` para evitar `0/0`).
  - `ImplicitCompute` ahora limita a 256 constantes y simula la profundidad real de la pila (antes solo contaba el tamaño del bytecode).
  - `readback` de los 3 pipelines (`function`, `implicit`, `parametric`) propaga el fallo con `AtomicBool` en vez de devolver datos vacíos silenciosamente.
- **Funciones matemáticas**:
  - `BesselJ`/`BesselY`/`BesselI` validan el orden con `bessel_order()`: NaN/Infinito → 0, valores fuera de `[-1000, 1000]` se saturan (antes saturaban a `i32::MIN`/`i32::MAX` y producían resultados arbitrarios).
  - `Sec`/`Csc`/`Cot` devuelven `NaN` en la singularidad (p.ej. `sec(pi/2)`) en lugar de `±Infinity` (que rompía el render y los snapshots GPU).
  - `bessel_order` se aplica tanto en `Expr::eval_*` como en el `evalexpr` context y en `simplify_once` (Const-Const fold).
- **Color clamping**: `to_color32`, `algebra.rs`, `export.rs` (SVG) y ghost rendering clampean los componentes RGBA a `[0, 255]` antes de la conversión a `u8`, evitando overflow y valores basura en objetos con color fuera de rango.
- **Estabilidad / panics**:
  - Reemplazo de `unwrap()` por `?`/`ok_or`/`continue` en `algebra.rs` (panel de variables), `app.rs` (acciones de restricción, ícono fallback), `snap.rs`, `tool_dispatcher.rs`, `commands.rs` (Integral/Plot args).
  - `cached_vars_list.lock().unwrap()` → `unwrap_or_else(|p| p.into_inner())` en `document.rs` para tolerar envenenamiento del mutex.
  - `lock_or_die` en `migrate_complex_symbol`: la rama `is_subscript` se evaluaba como `rest.is_empty() && rest.chars().all(...)` (siempre falsa si `rest` no estaba vacío) — corregido a `||` para que `z₁` migre a `w₁` y no solo a `w`.
- **Hit-test**: `Document::hit_test` ahora ordena candidatos por distancia real y devuelve el más cercano en lugar del primero coincidente, evitando seleccionar un objeto lejano cuando hay solapamiento.
- **Restricciones numéricas**: `DistanceEq`, `AngleEq`, `TangentEq`, `EqualLengthEq` reemplazan `if len < 1e-12 { return Vec::new(); }` por `.max(1e-12)` para que el solver reciba un Jacobiano finito en configuraciones degeneradas en lugar de abortar el paso.
- **ODE**: `euler_system`/`runge_kutta_4_system` validan `steps == 0` (devuelven el estado inicial) y toleran `|f(t,y)| ≠ n` rellenando con ceros; evita panic por `IndexOutOfBounds` cuando la ED entrega dimensiones inconsistentes.
- **Geometría**:
  - `safe_sample` con `n < 2` devuelve `vec![]` en lugar de `0/0`.
  - `cardioid` y `epicycloid` con `steps == 0` devuelven `vec![]` sin dividir por cero.
  - `compute_fractal` con `width == 0 || height == 0` retorna temprano; `fractal_color_hsv` con `max_iter == 0` retorna negro opaco en vez de NaN.
- **Estadísticas**: `histogram` ignora valores no finitos (NaN/Inf) en vez de contaminar bins con índices enormes.
- **Intérprete de comandos**:
  - `Tangent` cuando el punto está dentro del círculo ahora avisa "no hay tangentes" en vez de éxito silencioso.
  - Comandos 3D (`Point3D`, `Segment3D`, `Sphere`, `Cube`, etc.) en contexto 2D devuelven error explícito en vez de fallar más adelante.
  - `Script` con recursión profunda (≥ 6 niveles) aborta con error claro en vez de stack overflow.
  - `expand_all_cas` limita a 50 iteraciones para prevenir expansión infinita.
  - `Plot[expr, var]` y `Integral[expr, var, ...]` ahora usan `replace_variable` (límite de palabra) en vez de `String::replace` para no corromper nombres de funciones (p.ej. `exp(t)` se quedaba como `xxp(x)` en vez de `exp(x)`).
  - `parse_point_str` quita solo el par externo de paréntesis en vez de un nivel global (soporta tuplas anidadas).
  - `parse_brace_list` ignora elementos vacíos tras `,` (sintaxis `{1,,2}` ya no rompe el parser).
  - `is_function_lhs` usa `starts_with(|c| c.is_ascii_digit())` en lugar de `chars().next().unwrap()`.
  - Mensaje de `Intersect` ahora reporta el número de intersecciones encontradas.
- **Renderer 3D**: `face_normal` protege contra producto cruz de longitud cero (triángulos coplanarios) devolviendo `(0, 1, 0)` en vez de `NaN`.
- **DD / análisis simbólico**:
  - `DD::sin` con entrada no finita devuelve `(NaN, NaN)` en vez de propagar un resultado basura.
  - `PartialOrd` para `DD` ahora compara por `hi` y luego por `lo` (preservando precisión DD) en vez de convertir a `f64` con truncamiento.
- **App / UX**:
  - Errores de `save_state`/`load_state` se muestran como toasts en vez de solo `log::error!` (antes el usuario no se enteraba del fallo).
  - `marching_squares_contour` muerto eliminado del binario.

## [1.0.0-beta] - 2026-06-15

#### Añadido
- **Lápiz y Borrador (`Pencil` / `Eraser`)**: nuevo tipo `PencilObj` para dibujo a mano alzada; polilínea con color, grosor y hit-testing por segmento. Soporte para stylus/touch (botones Primary, Secondary y Middle) y deshacer con un solo `Ctrl+Z`. Botones de toolbar `Lápiz` y `Borrador`.
- **Motor de análisis matemático unificado** en `grafito-geometry` (`analysis.rs`): raíces, extremos, puntos de inflexión, interceptos, asíntotas y Taylor para funciones explícitas, curvas paramétricas 2D, curvas polares, curvas implícitas y campos vectoriales 2D.
- **`XIntercept`**: nueva `AnalysisFeature` para intersección con el eje X. Integrada en `Root` (que ahora también devuelve `XIntercept`), `Analyze` y en la heurística de snap jerárquico.
- Puente `grafito-core/src/analyzable.rs` para analizar cualquier `GeoObject` desde la UI y los comandos.
- Comandos de análisis: `Root`, `Extremum`, `Inflection`, `YIntercept` y `Analyze` (con alias en español).
- Herramientas de toolbar: `Root`, `Extremum`, `Inflection`, `YIntercept`, `Analyze`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve`, `VectorField2D`.
- Tests de integración para comandos de análisis en `crates/grafito-command/tests/analysis_commands.rs`.
- **Tool ghost universal**: preview translúcido para `Function`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve`, `VectorField2D`, `Segment`, `Ray`, `Vector` y `RegularPolygon`. Marcas de eje para puntos de intercepto (rojo en eje X, azul en eje Y) para distinguirlos claramente.
- Atajos de teclado para análisis: `R` (Raíz), `E` (Extremo), `N` (Inflexión), `Ctrl+Y` (Intersección Y), `Ctrl+A` (Analizar).
- Unificación parcial del estado pendiente: `Line`, `Circle`, `Polygon`, `Tangent` y `Perpendicular` ahora usan `ToolState.pending` y comparten el mismo ghost preview.
- Renombrado de las restricciones numéricas `Distance` / `Angle` a `DistanceConstraint` / `AngleConstraint` para diferenciarlas de las herramientas de medición geométrica.

#### Cambiado
- Snap jerárquico de clic por herramienta: `Root` snap-ea a `Root`/`XIntercept`, `Extremum` a extremos, `Inflection` a inflexiones, `YIntercept`/`XIntercept` a los interceptos correspondientes.
- Hover analytics simplificado: el debounce temporal se sustituyó por un debounce espacial (>5 px) y solo se actualiza cuando no se está arrastrando.

#### Corregido
- `unwrap()` críticos en `app.rs` (acción `Symmetry`, icono fallback).
- Botón `Tangent` duplicado en la toolbar.
- Clamp de componentes de color en `render_2d::to_color32` para evitar overflows.
- Grilla logarítmica que fallaba con dominios visibles negativos.
- Renderizado de parábolas degeneradas (`p <= 0`).
- Dominio de `acos` en la herramienta `Angle` (clamp a `[-1, 1]`).
- Normalización de comandos `YIntercept` y `Analyze` en el parser CAS.
- Etiquetado de funciones creadas con `f(x) = ...` ahora usa solo `f`, permitiendo `Root[f]`.

## [0.9.0-beta.1] - 2026-06-14

### v0.9.16-alpha

#### Añadido
- Botones de toolbar para restricciones numéricas (`Distance`, `Angle`, `Tangent`, `Coincident`, `Horizontal`, `Vertical`, `EqualLength`, `Symmetry`).
- Botones de toolbar para construcciones de cónicas (`EllipseByFoci`, `ParabolaByFocusDirectrix`, `HyperbolaByFoci`, `ConicByFivePoints`).
- Botones de toolbar para operaciones booleanas 2D (`PolygonUnion`, `PolygonIntersection`, `PolygonDifference`, `PolygonXor`).
- Comandos de texto para todas las nuevas herramientas anteriores.
- Iconos vectoriales personalizados para cada nueva herramienta.

#### Cambiado
- Toolbar reorganizada en secciones: básicas, 3D, construcciones, restricciones, cónicas y booleanas.

### v0.9.15-alpha

#### Añadido
- Renderizado de parábolas rotadas alrededor de su vértice.
- Renderizado de hipérbolas rotadas, incluyendo ambas ramas.
- Hit-testing actualizado para cónicas rotadas.

#### Corregido
- Corrección de discontinuidades en el trazado de hipérbolas cerca de las asíntotas.

### v0.9.14-alpha

#### Añadido
- Jacobianos analíticos para el solver de restricciones numéricas.
- Caché de expresiones compiladas (`evalexpr`) para acelerar la evaluación repetida.
- Benchmarks de rendimiento para evaluación de funciones, muestreo paramétrico y resolución de restricciones.

#### Cambiado
- Mejora de convergencia del solver numérico gracias a los Jacobianos analíticos.

#### Corregido
- Invalidación de caché al modificar variables globales del documento.

### v0.9.13-alpha

#### Añadido
- Tests de integración headless para el modelo de documento.
- Tests de integración headless para el renderizador GPU sin necesidad de ventana.
- Tests CLI para el punto de entrada de `grafito-app`.

#### Cambiado
- Separación de la inicialización gráfica para facilitar tests headless.

### v0.9.12-alpha

#### Cambiado
- Refactorización del punto de entrada de `grafito-app` para desacoplar UI, render y CLI.
- Modularización interna que facilita la ejecución de benchmarks y tests sin el entorno gráfico completo.

#### Eliminado
- Código muerto relacionado con el antiguo bucle de eventos monolítico.

### v0.9.11-alpha

#### Añadido
- Restricción constructiva `EllipseByFoci` para elipses definidas por dos focos y un punto.
- Restricción constructiva `ParabolaByFocusDirectrix` para parábolas definidas por foco y directriz.
- Restricción constructiva `HyperbolaByFoci` para hipérbolas definidas por dos focos y un punto.
- Restricción constructiva `ConicByFivePoints` para cónicas generales por cinco puntos.
- Resolución algebraica de la matriz general de cónica a partir de cinco puntos.

### v0.9.10-alpha

#### Añadido
- Restricción numérica `Coincident` para forzar la coincidencia de dos puntos.
- Restricción numérica `Horizontal` para alinear segmentos o rectas horizontalmente.
- Restricción numérica `Vertical` para alinear segmentos o rectas verticalmente.
- Restricción numérica `EqualLength` para igualar longitudes de dos segmentos.
- Restricción numérica `Symmetry` para simetría de dos puntos respecto a una recta.
- Detección de ciclos en el grafo de dependencias de restricciones.

### v0.9.9-alpha

#### Añadido
- Solver de restricciones numéricas basado en método de Newton.
- Restricción numérica `Distance` para fijar distancias entre puntos.
- Restricción numérica `Angle` para fijar ángulos entre rectas.
- Restricción numérica `Tangent` para imponer tangencia entre círculos y rectas.
- Propagación de restricciones en orden topológico según dependencias.

#### Cambiado
- Refactor de parámetros de restricciones para soportar grados de libertad variables.

### v0.9.8-alpha

#### Añadido
- Enlace de expresiones para objetos `Line` (`start_x_expr`, `start_y_expr`, `end_x_expr`, `end_y_expr`).
- Enlace de expresiones para polígonos (`x_exprs`, `y_exprs` por vértice).
- Enlace de expresiones para funciones (`expr`, `domain_min_expr`, `domain_max_expr`).
- Enlace de expresiones para curvas paramétricas 2D y polares.
- Reevaluación automática de parámetros ligados al cambiar variables.

#### Cambiado
- Separación entre valor base y expresión ligada en los objetos geométricos.

### v0.9.7-alpha

#### Añadido
- Pipeline de cómputo GPU `parametric_compute` para evaluación masiva de curvas paramétricas 2D.
- Pipeline de cómputo GPU `parametric_compute` para evaluación de superficies paramétricas 3D.
- Shader WGSL de muestreo paramétrico con soporte para expresiones en `t`, `u` y `v`.

#### Cambiado
- El muestreo de curvas paramétricas usa cómputo GPU cuando está disponible, con fallback CPU.

### v0.9.6-alpha

#### Añadido
- Pipeline de cómputo GPU `function_compute` para evaluación masiva de funciones explícitas `y = f(x)`.
- Shader WGSL `function_compute.wgsl` con soporte para operadores aritméticos, trigonométricos y exponenciales.
- Caché de muestreo de funciones con clave basada en expresión, dominio y calidad.

#### Cambiado
- El renderizado de funciones explícitas utiliza resultados precalculados por GPU cuando es posible.

#### Corregido
- Recálculo de funciones únicamente cuando cambian el dominio visible o los parámetros.

---

[1.2.20-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.20-beta
[1.2.19-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.19-beta
[1.2.18-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.18-beta
[1.2.17-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.17-beta
[1.2.16-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.16-beta
[1.2.15-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.15-beta
[1.2.14-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.14-beta
[1.2.13-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.13-beta
[1.2.12-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.12-beta
[1.2.11-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.11-beta
[1.2.10-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.10-beta
[1.2.9-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.9-beta
[1.2.8-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.8-beta
[1.2.7-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.7-beta
[1.2.6-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.6-beta
[1.2.5-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.5-beta
[1.2.4-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.4-beta
[1.2.3-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.2.3-beta
[1.2.1-beta]: https://github.com/Diez111/Grafito-Open/releases/tag/v1.2.1-beta
[1.2.0-beta]: https://github.com/Diez111/Grafito-Open/releases/tag/v1.2.0-beta
[1.1.4-beta]: https://github.com/Diez111/Grafito-Open/releases/tag/v1.1.4-beta
[1.0.0-beta]: https://github.com/Diez111/Grafito/releases/tag/v1.0.0-beta
[0.9.0-beta.1]: https://github.com/Diez111/Grafito/releases/tag/v0.9.0-beta.1
[v0.9.16-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.16-alpha
[v0.9.15-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.15-alpha
[v0.9.14-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.14-alpha
[v0.9.13-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.13-alpha
[v0.9.12-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.12-alpha
[v0.9.11-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.11-alpha
[v0.9.10-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.10-alpha
[v0.9.9-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.9-alpha
[v0.9.8-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.8-alpha
[v0.9.7-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.7-alpha
[v0.9.6-alpha]: https://github.com/Diez111/Grafito/releases/tag/v0.9.6-alpha
