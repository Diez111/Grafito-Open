# Changelog

Todos los cambios notables de este proyecto se documentarán en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/),
y este proyecto adhiere a [Semantic Versioning](https://semver.org/lang/es/spec/v2.0.0.html).

## [Unreleased]

#### Añadido
- Motor de análisis matemático unificado en `grafito-geometry` (`analysis.rs`): raíces, extremos, puntos de inflexión, interceptos, asíntotas y Taylor para funciones explícitas, curvas paramétricas 2D, curvas polares, curvas implícitas y campos vectoriales 2D.
- Puente `grafito-core/src/analyzable.rs` para analizar cualquier `GeoObject` desde la UI y los comandos.
- Comandos de análisis: `Root`, `Extremum`, `Inflection`, `YIntercept` y `Analyze` (con alias en español).
- Herramientas de toolbar: `Root`, `Extremum`, `Inflection`, `YIntercept`, `Analyze`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve`, `VectorField2D`.
- Tests de integración para comandos de análisis en `crates/grafito-command/tests/analysis_commands.rs`.
- Tool ghost universal: preview translúcido para `Function`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve`, `VectorField2D`, `Segment`, `Ray`, `Vector` y `RegularPolygon`.
- Atajos de teclado para análisis: `R` (Raíz), `E` (Extremo), `N` (Inflexión), `Ctrl+Y` (Intersección Y), `Ctrl+A` (Analizar).
- Unificación parcial del estado pendiente: `Line`, `Circle`, `Polygon`, `Tangent` y `Perpendicular` ahora usan `ToolState.pending` y comparten el mismo ghost preview.

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
