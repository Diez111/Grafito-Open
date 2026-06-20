# Changelog

Todos los cambios notables de este proyecto se documentarán en este archivo.

El formato está basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.0.0/),
y este proyecto adhiere a [Semantic Versioning](https://semver.org/lang/es/spec/v2.0.0.html).

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
