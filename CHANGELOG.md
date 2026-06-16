# Changelog

Todos los cambios notables de este proyecto se documentarán en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/),
y este proyecto adhiere a [Semantic Versioning](https://semver.org/lang/es/spec/v2.0.0.html).

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
