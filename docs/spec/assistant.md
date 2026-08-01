# Asistente Seguro con Proveedores Configurables

## Propósito

El asistente de Grafito es una superficie conversacional matemática local-first.
Cada consulta se intenta resolver de manera determinista dentro del proceso antes
de ofrecer una consulta remota. El usuario puede configurar proveedores y modelos
en Configuración avanzada, pero la interfaz normal sólo muestra `Local` o
`Consulta remota autorizada`. Puede enfocar una función seleccionada, mantener una
conversación acotada y, tras consentimiento explícito, enviar imágenes PNG/JPEG
validadas a una configuración remota compatible. Nunca ejecuta texto remoto de
forma autónoma: una acción propuesta sólo puede aplicarse tras un clic explícito
del usuario.

## Privacidad y Amenazas

- El contexto contiene revisión, variables y resúmenes de objetos con una huella
  determinista. No contiene rutas, nombres de archivos ni metadatos de origen.
- Las propuestas tipadas locales están limitadas a `SetVariable` y `CreateGraph`.
  El integrador vuelve a validar revisión/huella, entradas finitas, dominios y
  expresiones antes de usar un `OperationBatch` atómico. Las sugerencias remotas
  sólo pueden proponer capacidades gráficas autocontenidas declaradas localmente:
  funciones, curvas, campos, datos, fractales, visualizaciones complejas,
  superficies, sólidos, atractores y proyecciones 4D. El usuario las revisa,
  prepara o aplica explícitamente tras un preflight aislado de su ruta de render.
- No existen operaciones tipadas para `Script`, procesos, archivos, guardar,
  exportar, borrar ni herramientas de red. La preparación de texto remoto también
  rechaza scripts y comandos que carguen datos externos.
- Los adjuntos sólo aceptan PNG o JPEG. Antes de usar o enviar una imagen se
  comprueban sus bytes decodificados, formato MIME real, dimensiones declaradas
  frente a las reales, límite de píxeles, límite de bytes y cantidad. Los
  proveedores OpenAI reciben un data URL; Minimax M3 recibe bloques Anthropic
  `image/source` con base64 estándar. Ningún payload incluye rutas o nombres de
  archivo de origen.
- Las transcripciones editables de adjuntos se conservan localmente para que el
  usuario las revise; no se incluyen en ningún payload remoto. Tampoco se conservan
  rutas de imágenes. Un proveedor sólo recibe bytes PNG/JPEG decodificados después
  de que el usuario confirme tanto la carga de la imagen como que eligió un modelo
  con visión.
- OpenCode Go acepta exclusivamente la base HTTPS
  `https://opencode.ai/zen/go/v1`; el transporte deriva desde ella
  `/chat/completions`, `/messages` y `/models`. `minimax-m3` usa `/messages` con
  `x-api-key` y `anthropic-version`; los demás modelos OpenAI usan Bearer.
   `fusion` pide primero un borrador textual a Minimax M3 y sólo envía ese borrador
   limitado junto con la consulta original saneada a `deepseek-v4-pro` para
   auditarlo; ninguna de las dos llamadas recibe historial conversacional. Si la
   segunda llamada falla, el borrador se descarta y no se muestra. El digest del
   documento permanece como metadata local de freshness y no se serializa en ningún
   payload remoto. Ollama sólo acepta
  HTTP/HTTPS sobre una IP loopback literal. No se siguen redirecciones y las
  respuestas, identificadores de modelo, turnos, foco y adjuntos tienen límites antes
  de deserializarse o mostrarse.
- Las claves de OpenCode Go se guardan sólo en el llavero del sistema. En Linux el
  crate `keyring` usa Secret Service persistente, no su backend temporal de pruebas.
  Una copia sólo de memoria conserva la sesión recién configurada si una relectura
  transitoria falla. No se guardan en `grafito_config.json`, documentos, logs,
  mensajes de error o payloads.
- Las solicitudes remotas y la consulta de modelos se ejecutan en hilos de trabajo con
  timeout, cancelación cooperativa y un canal de resultado que solicita repaint.
  Cada consulta conserva ID, proveedor y modelo; el selector queda bloqueado hasta
  terminar para que una respuesta anterior nunca aparezca atribuida al modelo nuevo.
  Los fallos de transporte se clasifican sin incluir payloads ni secretos, quedan en
  la tarjeta del asistente, generan un toast y emiten sólo metadatos seguros en logs.
  El turno del usuario se muestra de inmediato; un fallo o cancelación permanece
  visible, pero sólo pares usuario-respuesta completos y acotados se reenvían al
  proveedor. Una consulta local no abre transporte ni consulta el llavero. Si el
  resultado local queda fuera de alcance, el usuario recibe una acción separada
  para autorizar la salida remota. El prompt puede incluir un catálogo de firmas
  relevantes derivado del registro de comandos, dentro del mismo presupuesto de
   entrada.
- `grafito-assistant::harness` expone el recorrido headless local
  request-stage-preview-apply sin recibir proveedores, red, llavero ni tipos egui.
  Sólo acepta `PrivacyMode::LocalOnly` y exige que el contexto de la solicitud
  coincida exactamente con el documento local. El staging usa un `OperationBatch`
  aislado y secuencial: una variable propuesta puede habilitar una gráfica posterior,
  pero el documento vivo no cambia hasta `Apply` explícito. Un receipt local opt-in
  contiene exclusivamente versiones, conteos de delta y compromisos SHA-256 del plan,
  estado base, estado staged y evidencia; nunca contiene prompt, operaciones, expresión,
  documento, etiquetas, rutas, imágenes, proveedores, credenciales ni diagnósticos. El
  compromiso semántico usa la normalización real de guardar y abrir, por lo que ignora
  caches, IDs asignados durante staging e índices reconstruidos. Una propuesta no puede
  sobrescribir una variable propiedad de la spreadsheet, porque su valor se recalcula al
  abrir. El replay vuelve a stagear sobre una copia, valida base, plan, delta, estado staged y
  evidencia, y nunca aplica una mutación. Los compromisos son evidencia de integridad
  local, no cifrado ni un mecanismo de secreto; el cliente decide explícitamente si
  conserva el receipt fuera de la memoria del proceso.
- La interfaz ofrece un catálogo fijo de modelos OpenCode Go/Ollama y agrega los
  IDs descubiertos mediante `/models`; no permite IDs escritos libremente. Kimi y
  Mimo no se ofrecen en OpenCode Go. Cambiar proveedor, modelo o adjunto reinicia la
  autorización de carga. La configuración de proveedor, modelo y clave se abre desde
  el engranaje del asistente, no ocupa el panel de conversación. El proveedor y modelo
   se guardan como preferencias no secretas; una instalación nueva usa OpenCode Go con
   `minimax-m3` sin reemplazar una preferencia ya guardada. La clave guardada se recupera al consultar
  para que un indicador visual transitorio no bloquee solicitudes válidas. El contador
  de entrada reserva también el resumen y encabezado de la función enfocada.
  Los adjuntos sólo se envían cuando el usuario confirma que la configuración remota
   admite imágenes y autoriza el envío por consulta. Iniciar, descartar o reemplazar
   una autorización remota revoca ese consentimiento aunque conserve los adjuntos,
   por lo que el siguiente envío exige una nueva confirmación explícita.
- Las respuestas remotas pueden usar Markdown, tablas y delimitadores LaTex. La
  interfaz representa localmente fracciones, raíces, potencias, subíndices,
  símbolos griegos y relaciones comunes, incluidos `\mathbb`, envoltorios
  tipográficos y `\dfrac`; `$$...$$` inline se procesa antes que `$...$`.
  La sintaxis malformada permanece visible como fuente literal en lugar de perder
   contenido. Sólo un bloque `grafito` de una línea, completo, balanceado y sin
   argumentos vacíos, un bloque `grafito-param` con una asignación finita, o un
   bloque `grafito-scene` de dos a ocho comandos 3D, puede ofrecer aplicación.
   `grafito-command` reconoce esos fences y los convierte en invocaciones tipadas
   ligadas a un `CommandSpec` registrado y a la política local; la UI sólo asocia
   cada tarjeta con el índice de su bloque de código. Ningún texto remoto original
   llega al dispatcher como acción ejecutable. La aplicación valida cada propuesta
   al recibirla sobre un
   documento aislado antes de mostrar una acción: nombre canónico, aridad, vista,
   staging, geometría no vacía y visibilidad. Una escena de flor requiere un tallo,
   un centro y al menos cuatro pétalos; se estiliza, se encuadra y se confirma como
   una única operación de undo. Un comando que requiera etiquetas u objetos
   seleccionados no se propone hasta que ese contexto exista. `Aplicar en Grafito`
    confirma solamente el resultado preflightado mediante el pipeline normal de undo,
    errores y protocolo de construcción; la tarjeta se consume sólo después de un
    commit exitoso y no puede aplicarse dos veces.
   Al confirmar una gráfica 2D o 3D desde la vista opuesta, abre explícitamente la
   perspectiva necesaria. `Editar` prepara y enfoca una entrada visible, pero nunca
   ejecuta. `DomainColoring` se propone con la forma canónica
   `DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]`; su sexto argumento
   debe ser un entero literal entre 16 y 300 (200 por defecto), por lo que un símbolo
   como `r` se rechaza localmente antes de staging. Una propuesta remota descartada
   no recibe `Aplicar`; cuando existe un motivo local saneado puede ofrecer hasta dos
   correcciones acotadas. Cada corrección reenvía sólo la pregunta original, el
   catálogo local, el foco permitido, pares anteriores completos y el diagnóstico
   técnico saneado: nunca adjuntos, consentimiento de imagen, texto de la respuesta
   rechazada, rutas, secretos ni digest de documento. Una corrección estándar puede
   conservar pares anteriores completos; si usa Fusion, sus dos llamadas reciben
   sólo el prompt saneado y el borrador, nunca esos pares. El par activo
   usuario/respuesta rechazado se excluye del historial remoto y la respuesta reparada reemplaza
   exactamente ese turno local. La corrección conserva revisión, digest y foco
   locales del rechazo; si cualquiera cambia antes del retry, se descarta sin
   enviar una nueva solicitud.
    Si el usuario activa de forma explícita el permiso persistido, desactivado por
    defecto, de revisión remota adicional, puede elegir una corrección posterior a
    una propuesta gráfica rechazada. Ningún fallo inicia una segunda solicitud por
    sí solo. Si una acción quedó fuera del cupo local de preflight, no se presume
    que falló ni se escala automáticamente. La identidad técnica de esa ruta queda
    dentro de Configuración avanzada, no cambia preferencias y una salida posterior
    sigue pasando el mismo preflight local y requiere `Aplicar en Grafito` explícito. El transcript
     muestra ambos roles en el mismo ancho y escala mediante bandas editoriales de
     ancho completo, con superficies y etiquetas semánticamente distintas, sin
     alineación lateral ni burbujas de mensajería. Mora es la identidad visual
     local del asistente: usa un PNG embebido y una textura cacheada, no realiza
     requests de red ni se trata como adjunto; permanece estática en reposo y el
     indicador nativo `ThinkingOrb` comunica las solicitudes activas. Una petición de Fourier sólo
     puede proponer una `Function[...]` con una suma parcial finita y valores
     numéricos explícitos; `sum(...)`, coeficientes `a_n`/`b_n`, orden simbólico y
     una transformada general permanecen explicativos, igual que LaTex. El proveedor no puede invocar,
   encadenar ni aplicar una acción sin un clic explícito del usuario.
- Las propuestas tienen un máximo de ocho operaciones y toda respuesta local se
  valida contra los límites de caracteres y pasos. Las ecuaciones locales se
  reconocen por el AST estructural, nunca por muestras; cada solución presentada se
  vuelve a evaluar en los AST originales con un residual finito y dependiente de la
   escala antes de declararse verificada. Antes de normalizarlos o analizarlos, los
   literales científicos deben ser representables como `f64` finitos: un literal con
   significando no nulo que subfluye a cero, o uno que desborda, se rechaza como error
   de precisión de entrada y nunca se redondea a `0`. Los demás literales científicos
   se normalizan antes del análisis estructural y los coeficientes cuadráticos se
   escalan por una potencia de dos fijada por su mayor magnitud antes de calcular el
   discriminante. Su signo sólo es definitivo fuera de la cota conservadora
  `32*epsilon*(b^2 + 4*abs(a*c))` para esos coeficientes escalados. Dentro de esa
  cota sólo se acepta una raíz repetida después de verificarla contra los AST
  originales; si falla, la ecuación queda sin soporte. La extracción estructural
  rechaza sumas o productos de coeficientes que subfluyen, desbordan o absorben una
  contribución no nula por redondeo para no perder silenciosamente el grado. La
  representación mostrada de cada candidato usa una conversión
  de `f64` de ida y vuelta; no redondea valores no nulos a cero o a enteros y no
  fusiona raíces cercanas salvo a precisión de máquina.

## Alcance Excluido

Este asistente no implementa colaboración, cuentas/OpenCode OAuth, sincronización
cloud, navegación web, agentes autónomos con herramientas, ejecución de scripts,
exportación automática, ni clientes web o móviles. Las respuestas remotas no se
convierten en mutaciones sin aprobación. Una sugerencia permitida puede copiarse a
la barra de entrada o aplicarse mediante una acción explícita posterior del usuario;
   en ambos casos usa el mismo `process_input` transaccional que el resto de Grafito.
