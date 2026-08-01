# Grafito Project Specification

## Propósito

Grafito es una aplicación desktop de matemática visual. Su ruta de producto tiene tres niveles: universidad e ingeniería confiable, geometría dinámica generalista y CAS científico amplio.

## Política de conectividad

- Grafito es local-first y no implementa colaboración, cuentas, sincronización cloud, relay, telemetría, clientes web, PWA ni servicios de red de producto.
- La única excepción de red es el asistente matemático remoto que el usuario habilita y autoriza explícitamente; el modo local sigue disponible sin conectividad.
- Las funciones de archivo, cálculo, documentos, lecciones, evaluación, importación, exportación y recuperación deben funcionar sin conexión.

## Contrato de confianza

1. Un resultado no puede ocultar un error de dominio, singularidad, no-convergencia, truncamiento ni límite de recursos.
2. Las operaciones deben declarar si son exactas, aproximadas, no soportadas o fallidas.
3. Una entrada inválida no debe alterar el documento.
4. Las capacidades `Experimental` o `Placeholder` deben estar etiquetadas y no se publicitan como estables.
5. Todo documento debe ser versionado, validado antes de usarlo y guardado de forma atómica.

## Ciclo de vida del documento

- Un espacio nuevo no tiene ruta y parte limpio. La identidad del documento conserva su ruta actual y una línea base semántica que excluye revisión, cachés, calidad de render y tamaño transitorio del viewport.
- `Guardar` reutiliza la ruta actual y sólo solicita una cuando todavía no existe. `Guardar como...` siempre solicita un destino nuevo. Antes de escribir, los borradores de celdas que difieren del documento se validan y aplican sobre una copia aislada; sólo una escritura durable reemplaza el documento vivo, limpia los borradores y actualiza ruta y línea base.
- `Nuevo`, `Abrir`, `Salir` y el cierre nativo requieren `Guardar`, `Descartar` o `Cancelar` cuando hay cambios semánticos o borradores de celdas pendientes. Un error o cancelación de guardado conserva el documento, los borradores, la ventana y la decisión pendiente.
- `Abrir` solicita el archivo después de resolver los cambios pendientes. El archivo se carga y valida por completo antes de reemplazar el documento vivo; sólo el reemplazo confirmado reinicia historial y estado transitorio asociado.

## Prioridad de producto

1. Seguridad de recursos y corrección matemática.
2. Persistencia, comandos, errores e historial transaccionales.
3. Render consistente y accesible, con 3D de profundidad real.
4. Ampliación de producto GeoGebra y CAS sólo sobre esos contratos.

## No objetivos de Phase 0

- Cambiar el formato público de documentos sin migrador.
- Reescribir todo el CAS o el renderer en una única entrega.
- Declarar paridad con GeoGebra o un CAS científico antes de completar sus corpus de aceptación.
- Añadir funciones online, colaboración, PWA, cloud sync, relay o telemetría.

## Criterios de lanzamiento estable A

- Cero defectos P0/P1 conocidos de corrección matemática, corrupción, OOM o freeze por input válido.
- Corpus científico de 2.000 casos y 200 pruebas visuales golden verde en Linux, Windows y macOS.
- Exportación completa o con omisiones declaradas.
- Todos los controles estables operables por teclado y con contraste WCAG AA.
