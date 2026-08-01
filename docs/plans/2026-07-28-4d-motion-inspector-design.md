# Inspector 4D y controles de movimiento

## Problema

El inspector de politopos 4D concentra controles sin jerarquía visual y deja la
animación en un panel separado, con un botón sólo de icono demasiado pequeño.
Un gesto manual pausa la animación correctamente, pero el usuario no recibe una
forma obvia, cercana al objeto, de reanudarla ni de regular su velocidad.

## Replanteo

La necesidad no es añadir más sliders: es que el inspector explique el estado
de la proyección y concentre las acciones más frecuentes. La animación sigue
siendo una preferencia transitoria de la aplicación, no una propiedad del
documento ni del politopo.

## Enfoque

Adoptar una composición de inspector en capas:

1. Cabecera con identidad del politopo y selector de tipo.
2. Tarjeta de animación 4D con acción primaria textual, estado legible y
   velocidad global ajustable.
3. Controles visuales compactos para escala, aristas y relleno.
4. Rotaciones manuales bajo una sección avanzada separada.

Se conserva el control en el panel Vista, pero ambos controles comparten el
mismo estado transitorio. La acción principal confirma visualmente si la
animación se inició o pausó.

## Alcance

Incluye:

- Botón directo `Iniciar animación`/`Pausar animación` en inspectores 4D.
- Multiplicador de velocidad de 0.25x a 2.0x, con restablecimiento a 1.0x.
- Aplicación del multiplicador tanto a la órbita 3D como a la fase 4D.
- Diseño de tarjeta centrado, contrastado y accesible, usando tokens y tema.
- Scroll vertical del inspector para pantallas de baja altura.
- Pruebas de límites de velocidad, avance y presencia de controles.

No incluye:

- Persistir la velocidad entre lanzamientos.
- Cambiar los ángulos guardados automáticamente.
- Añadir animación a politopos N-D de dimensión distinta de cuatro.

## Diseño técnico

`GrafitoApp` recibe `multidimensional_motion_speed: f32` con valor inicial
`1.0`. Una función pura normaliza valores no finitos y limita el multiplicador
al intervalo `[0.25, 2.0]`. `advance_multidimensional_motion` utiliza ese valor
para escalar la órbita de cámara y la fase 4D, manteniendo la fase y la cámara
fuera de `Document`.

`draw_right_properties_panel` usa una tarjeta reutilizable de movimiento para
los politopos 4D. La tarjeta contiene una acción de ancho completo con etiqueta
textual, un indicador de estado, slider de velocidad y restablecimiento. El
panel Vista conserva una versión compacta del mismo control. Los cambios de
animación no crean snapshots ni entradas de undo.

## Criterios de aceptación

1. Tras pan, zoom u órbita, un politopo 4D puede reanudar la animación desde
   su propio inspector con una acción textual visible.
2. El estado activo/en pausa se entiende sin depender sólo de un icono o color.
3. El multiplicador de velocidad regula la órbita y la rotación 4D, se limita a
   0.25x--2.0x y no muta el documento.
4. Los controles de rotación siguen disponibles, pero no compiten visualmente
   con la acción de movimiento.
5. El inspector se puede desplazar verticalmente y mantiene contraste y targets
   de interacción prácticos.

## Estrategia de pruebas

- Pruebas unitarias de normalización y escala de velocidad en `app.rs`.
- Prueba de no-mutación de documento para el estado de movimiento.
- Prueba de integración de UI que verifica los controles directos, etiquetas y
  accesibilidad declarada en el inspector.
- Clippy, tests del crate y build release antes de empaquetar.

## Riesgos

- La velocidad no debe reactivar la GPU durante movimiento; se preserva el
  camino CPU Preview existente.
- El inspector ya tiene muchos campos; las rotaciones se presentan como una
  sección avanzada para evitar que oculten la acción principal.
- Un objeto oculto o una vista no 3D no puede animarse; el control comunica la
  condición en lugar de aparentar que funcionó.
