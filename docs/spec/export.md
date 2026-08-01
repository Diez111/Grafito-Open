# Exportación profesional

Grafito exporta el lienzo 2D actual a SVG, PNG o TikZ. Los tres formatos usan la
misma escena CPU evaluada, los mismos límites de vista, visibilidad, colores,
rellenos y grosores. Un objeto visible nunca se omite silenciosamente.

## Contrato

- Una exportación correcta devuelve `ExportReport`, con ruta, formato, cantidad
  de objetos exportados y ocultos, cantidad de primitivas y conteo por tipo.
- Si existe un objeto visible no compatible, `ExportError::UnsupportedObjects`
  enumera de forma determinista su tipo, etiqueta e identificador. No se crea ni
  reemplaza el archivo de destino.
- Geometría malformada, no finita o no representable produce
  `ExportError::InvalidObject`. Los límites de dimensión, píxeles, geometría y
  salida producen `ExportError::ResourceLimit`.
- SVG, PNG y TikZ se codifican por completo antes de escribir. La escritura usa
  un temporal en el mismo directorio, sincroniza su contenido y sólo entonces
  reemplaza atómicamente el destino. Cualquier error anterior conserva el
  archivo existente.
- Los objetos con `visible = false` no se dibujan ni bloquean la exportación; el
  informe los cuenta como ocultos.

## Matriz de soporte

`Sí` significa representación real mediante la escena CPU compartida. `No`
significa rechazo explícito, nunca una aproximación falsa.

| `GeoObject` | SVG | PNG | TikZ | Política |
|---|---:|---:|---:|---|
| `Point` | Sí | Sí | Sí | Marcador, color y tamaño |
| `Line` | Sí | Sí | Sí | Segmento/recta/semirrecta recortada a la vista |
| `Circle` | Sí | Sí | Sí | Contorno y relleno |
| `Polygon` | Sí | Sí | Sí | Vértices evaluados, contorno y relleno |
| `Pencil` | Sí | Sí | Sí | Polilínea acotada |
| `Function` | Sí | Sí | Sí | Muestras compartidas, discontinuidades y relleno |
| `Text` | Sí | Sí | Sí | XML/TeX escapado; fuente integrada en PNG |
| `Ellipse` | Sí | Sí | Sí | Cónica rotada muestreada |
| `Parabola` | Sí | Sí | Sí | Cónica rotada recortada a la vista |
| `Hyperbola` | Sí | Sí | Sí | Dos ramas rotadas y recortadas |
| `Point3D` | No | No | No | Espacio 3D |
| `Segment3D` | No | No | No | Espacio 3D |
| `Plane3D` | No | No | No | Espacio 3D |
| `Line3D` | No | No | No | Espacio 3D |
| `Sphere3D` | No | No | No | Espacio 3D |
| `Cube3D` | No | No | No | Espacio 3D |
| `Tetrahedron3D` | No | No | No | Espacio 3D |
| `Pyramid3D` | No | No | No | Espacio 3D |
| `Cone3D` | No | No | No | Espacio 3D |
| `Cylinder3D` | No | No | No | Espacio 3D |
| `Torus3D` | No | No | No | Espacio 3D |
| `MoebiusStrip` | No | No | No | Superficie 3D |
| `Surface3D` | No | No | No | Superficie 3D/GPU |
| `ParametricCurve2D` | Sí | Sí | Sí | Muestras paramétricas compartidas |
| `ParametricCurve3D` | No | No | No | Espacio 3D |
| `PolarCurve` | Sí | Sí | Sí | Muestras polares compartidas y relleno |
| `ImplicitCurve` | Sí | Sí | Sí | Marching squares compartido; relleno de regiones |
| `VectorField2D` | Sí | Sí | Sí | Flechas normalizadas en la vista |
| `ComplexGrid` | No | No | No | Render complejo/GPU |
| `ComplexMapping` | No | No | No | Mapeo complejo no representable fielmente |
| `ComplexIntegral` | No | No | No | Decoración dependiente de objeto complejo |
| `Attractor3D` | No | No | No | Espacio 3D |
| `Fractal2D` | No | No | No | Render fractal especializado |
| `HyperSurface4D` | No | No | No | Proyección 4D especializada |
| `RegularPolychoron4D` | No | No | No | Politopo regular 4D; rechazo explícito |
| `RegularPolytopeND` | No | No | No | Politopo regular N-D; rechazo explícito |
| `VectorField3D` | No | No | No | Espacio 3D |
| `Histogram` | Sí | Sí | Sí | Barras, borde y relleno |
| `ScatterPlot` | Sí | Sí | Sí | Pares completos y tamaño de marcador |
| `BoxPlot` | Sí | Sí | Sí | Cuartiles, mediana, bigotes y atípicos |
| `RegressionLine` | Sí | Sí | Sí | Recta y puntos fuente |
| `DataTable` | No | No | No | Fuente local no visual; permanece oculta del canvas y de la exportación |
| `PhasePortrait` | Sí | Sí | Sí | Segmentos del muestreador compartido |
| `Transformed` | No | No | No | Transformación compleja; rechazo explícito |

La prueba de inventario compara esta política codificada con las variantes
declaradas en `GeoObject`. Agregar una variante nueva sin decidir su política de
exportación hace fallar la suite.

## Interfaz

El menú **Archivo > Exportar**, la paleta de comandos y el panel de vista abren
siempre un selector de ruta. El resultado completo queda en la respuesta
persistente de la aplicación y también se muestra como notificación. La
exportación LaTeX del protocolo de construcción usa la misma escritura atómica.
