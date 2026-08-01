# Politopos Regulares 4D y N-D Design Doc

## Problem

Grafito solo representa un teseracto wireframe y una muestra incorrecta de
"hiperesfera" como overlays CPU con un tipo de superficie textual. No hay
comandos verificables, topologia exacta, proyeccion 4D completa ni ruta GPU
para el pentacoron, 16-celdas, 24-celdas, 120-celdas o 600-celdas.

## Reframe

Una figura 4D no se puede mostrar directamente en una pantalla 2D. El
producto debe ser un modelo topologico exacto en 4D, proyectado a 3D y luego
renderizado con la camara 3D existente, con controles de rotacion en los seis
planos de SO(4). Para dimensiones `n >= 5` no existen mas familias de
politopos convexos regulares: el soporte generico se limita correctamente a
simplex, hipercubo y politopo cruzado.

## Approach

Se elegira una arquitectura tipada de topologia inmutable y proyeccion
numerica separada. Las coordenadas canonicas, aristas, caras y celdas se
generan una vez en `f64`; cada frame solo rota y proyecta los vertices. Las
caras trianguladas y las aristas se emiten a `WorldMesh`, que ya las agrupa en
dos draws GPU con profundidad. Esta ruta cubre incluso el 120-celdas sin un
shader nuevo: sus 1,200 aristas y 2,160 triangulos de caras son pequenos
frente al presupuesto actual.

Alternativas descartadas:

- Extender `HyperSurface4DObj.surface_type`: conserva tipos stringly,
  topologia opaca y el overlay CPU sin profundidad.
- Crear un pipeline WGSL 4D inmediatamente: duplica clipping, lineas gruesas
  y transparencia para una carga de vertices que no justifica esa complejidad.

## Scope

Incluye:

- Pentacoron, teseracto, 16-celdas, 24-celdas, 120-celdas y 600-celdas.
- `SimplexND`, `HypercubeND` y `CrossPolytopeND` para dimensiones seguras.
- Proyeccion perspectiva 4D a 3D, seis rotaciones 4D y reduccion determinista
  N-D a 4D/3D.
- Comandos directos, persistencia, preflight del asistente, UI de creacion,
  rendering GPU, fallback CPU, LOD y documentacion.

No incluye:

- Politopos convexos regulares adicionales en `n >= 5`, porque no existen.
- Un renderer WGSL que procese posiciones 4D crudas en la primera iteracion.
- Exportacion de mallas 4D sin una especificacion de formato.

## Technical Design

`grafito-geometry` recibira `polytopes.rs` con `Point4D`,
`RegularPolychoron`, `Polytope4DTopology` y generadores exactos. El 600-celdas
se basa en las raices H4; sus celdas tetraedricas se enumeran con cliques de
aristas y planos soporte. El 120-celdas se obtiene como su dual, por lo que
hereda aristas, pentagonos y celdas sin buscar ciclos flotantes arbitrarios.

`grafito-core` modelara `RegularPolychoron4DObj` y `RegularPolytopeNDObj`
tipados. Ambos almacenan escala, color de arista/relleno, seis o mas angulos
de rotacion, y opciones de representacion. La topologia se deriva, nunca se
persiste como una malla duplicada. Las entradas limitan dimensiones, vertices,
caras, escala y angulos finitos antes de crear un objeto.

`grafito-render::depth_3d` proyectara vertices canonicos en `f64`, rechazara
denominadores cercanos a cero y solo despues convertira a `f32`. Triangula
caras convexas ordenadas, coloca rellenos opacos en el stream de profundidad,
rellenos translucidos y aristas en el stream wire. Preview emite aristas;
Normal/High habilitan caras segun presupuesto. La fase de animacion invalida
solo la geometria proyectada, no la topologia.

## Acceptance Criteria

1. Cada comando 4D crea una primitiva tipada y genera exactamente sus cuentas
   `(V, E, F, C)`: `(5,10,10,5)`, `(16,32,24,8)`, `(8,24,32,16)`,
   `(24,96,96,24)`, `(600,1200,720,120)` y `(120,720,1200,600)`.
2. Todas las aristas canonicas tienen longitud uniforme y las caras/celdas no
   contienen indices duplicados; se cumple `V - E + F - C = 0`.
3. Los seis planos `xy`, `xz`, `xw`, `yz`, `yw`, `zw` cambian la proyeccion de
   forma finita y el clipping evita division por cero.
4. Las figuras estaticas entran en `WorldMesh`; el renderer GPU recibe caras
   y aristas con la profundidad actual. Preview degrada grandes figuras sin
   perder interaccion.
5. El asistente propone comandos directos verificables, no escenas de cientos
   de segmentos. Una propuesta invalida no muta el documento.
6. `SimplexND`, `HypercubeND` y `CrossPolytopeND` respetan limites de
   dimension y presupuesto antes de asignar memoria.

## Test Strategy

Pruebas unitarias verifican coordenadas, incidencias, dualidad, conteos y
proyeccion. Pruebas de comando/persistencia comprueban validacion atomica y
compatibilidad de schema. Pruebas headless de `WorldMesh` verifican streams,
triangulos, aristas, LOD y geometria finita. Pruebas de asistente ejercitan un
fence por cada familia. Las pruebas GPU existentes validan el compositing con
profundidad una vez que la malla llega al pipeline.

## Risks

- La topologia del 120/600-celdas requiere tolerancias robustas; se usaran
  coordenadas canonicas, longitudes conocidas y planos soporte, nunca vertices
  proyectados ni `f32`.
- La transparencia no es independiente del orden; se conservara la politica
  actual de orden por profundidad y los rellenos opacos seran el valor por
  defecto.
- Una animacion continua puede reconstruir geometria cada frame; la topologia
  se cachea y Preview limita las caras durante movimiento.
