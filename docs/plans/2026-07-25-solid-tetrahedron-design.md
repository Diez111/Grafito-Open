# Tetraedro Solido Design Doc

## Problem

El asistente puede construir un tetraedro solo como seis `Segment3D`, por lo
que el usuario ve un alambre y no las cuatro caras triangulares de un solido.
No existe una primitiva nativa que modele ni renderice un tetraedro regular
relleno.

## Reframe

El problema no es generar coordenadas de cuatro puntos, sino representar un
solido 3D con topologia, persistencia, renderizado GPU/CPU y aplicacion segura
desde el asistente. Se descartaron tres supuestos:

1. Seis aristas bastan para un tetraedro: no satisfacen la expectativa visual
   de caras solidas.
2. `Pyramid3D` se puede reutilizar: su base cuadrada produce cinco caras, no
   cuatro.
3. Hace falta un `Polyhedron` generico: ampliaria innecesariamente el parser,
   la validacion de topologia y la superficie de ataque.

## Approach

Agregar una primitiva especializada `Tetrahedron3D` que representa solamente
un tetraedro regular. Persiste el centro y la longitud de arista, y deriva sus
vertices, cuatro caras triangulares y seis aristas en un unico lugar.

Es la opcion equilibrada: resuelve el objeto pedido en todos los renderers y
en el flujo del asistente sin introducir formatos arbitrarios de malla. La
alternativa estrecha de conservar segmentos no ofrece caras; la alternativa
amplia de un poliedro generico requiere validacion de indices, winding,
manifold y limites de recursos.

## Scope

En alcance:

- `Tetrahedron[x, y, z, edge]` con centroide `(x, y, z)` y arista finita
  estrictamente positiva.
- Persistencia, bounds, picking grueso, renderizado GPU `WorldMesh` y fallback
  CPU de cuatro caras rellenas y seis aristas.
- Catalogo y guia del asistente para proponer directamente el comando seguro.
- Pruebas de geometria, comando, persistencia, preflight y renderizado.

Fuera de alcance:

- `Polyhedron` o una malla arbitraria introducida por comandos.
- Herramienta de construccion por clics y edicion independiente de vertices.
- Sustituir o cambiar la semantica de `Pyramid3D`.

## Technical Design

`grafito-geometry` define `Tetrahedron3D { center: Point3D, edge_length: f64 }`.
El comando y la validación del documento rechazan valores no finitos o no
positivos antes de que puedan persistirse o renderizarse. Para una arista `a`,
usa `h = a * sqrt(2 / 3)` y, antes de trasladar por el centro, deriva:

```text
apex = ( 0,  3h/4,           0)
b0   = (-a/2, -h/4, -a/(2sqrt(3)))
b1   = ( a/2, -h/4, -a/(2sqrt(3)))
b2   = ( 0,   -h/4,  a/sqrt(3))
```

Los indices de caras exteriores son `[1,2,3]`, `[0,3,2]`, `[0,1,3]` y
`[0,2,1]`; las seis aristas se derivan de esos vertices. Las pruebas verifican
que cada arista tiene longitud `a`, que el centroide coincide con `center`,
que las normales apuntan hacia afuera y que el volumen es
`a^3 / (6sqrt(2))`.

`grafito-core` envuelve el tipo en `Tetrahedron3DObj`, lo incorpora a
`GeoObject`, validacion, bounds, serializacion y esquema de persistencia. El
schema se incrementa para que documentos anteriores sigan leyendose y una
version anterior rechace de forma clara el nuevo tag de enum.

`grafito-render` agrega cuatro triangulos de relleno y seis aristas a
`WorldMesh`; `grafito-app` comparte los vertices/caras derivados para el
fallback CPU y picking por AABB. El comando evalua cuatro argumentos finitos,
crea el objeto de forma transaccional y se registra como capacidad ejecutable
del asistente. La guia remota usa `Tetrahedron[...]`, no `Script` ni
`Segment3D`.

## Acceptance Criteria

1. `Tetrahedron[0,0,0,2]` crea exactamente un objeto 3D regular con cuatro
   caras triangulares y seis aristas derivadas.
2. Los parametros invalidos (arista cero, negativa, no finita o coordenadas
   no finitas) fallan sin mutar el documento.
3. El objeto aparece lleno tanto en `WorldMesh` como en el fallback CPU y
   conserva un contorno de seis aristas.
4. Guardar y abrir un documento conserva centro, arista, estilo, visibilidad y
   geometria derivada.
5. Una propuesta del asistente con `Tetrahedron[...]` supera preflight solo
   cuando queda visible, y se aplica exclusivamente mediante la accion
   explicita existente.
6. El asistente deja de generar el workaround de seis `Segment3D` para un
   tetraedro solicitado.

## Test Strategy

- Unitarios de `Tetrahedron3D` para invariantes metricas, winding, volumen y
  entradas invalidas.
- Integracion de comandos para creacion, errores atomicos y round-trip de
  persistencia.
- Tests de render para cuatro triangulos, seis aristas, bounds y picking.
- Tests de asistente para catalogo, preflight y guia de propuesta directa.
- Ejecutar `cargo fmt --all`, Clippy estricto, tests del workspace, build
  release y `graphify update .` antes de empaquetar.

## Risks

- Winding incorrecto puede ocultar caras con culling: se prueba la orientacion
  exterior y se reutiliza el flujo de triangulos de solidos existentes.
- Divergencia GPU/CPU puede mostrar dos topologias distintas: ambos consumen
  las caras derivadas del mismo tipo geometrico.
- Un cambio de enum puede afectar documentos guardados: se cubre con la
  migracion de schema y pruebas de compatibilidad.
