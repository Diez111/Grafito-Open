# Grafito v1.0.0-beta

**Grafito** es una aplicación interactiva de geometría, álgebra, estadística y cálculo de alto rendimiento, acelerada por GPU, inspirada en GeoGebra y construida desde cero en Rust con APIs gráficas modernas.

---

## Arquitectura

```
grafito/
├── crates/
│   ├── grafito-app/        # Aplicación de escritorio eframe — UI, entrada, orquestación de renderizado
│   ├── grafito-core/       # Modelo de documento, 32+ tipos de objetos geométricos, índice espacial, restricciones
│   ├── grafito-geometry/   # Motor matemático: CAS, estadística, EDO, fractales, atractores, curvas, booleanas
│   ├── grafito-render/     # Pipeline GPU wgpu — teselado 2D/3D, shaders de cómputo, iluminación
│   ├── grafito-ui/         # Componentes egui — barra de herramientas, paleta de comandos, propiedades, selector de color, temas
│   └── grafito-command/    # Procesador compartido de comandos de texto para frontends de escritorio y FFI
└── assets/                 # WGSL shaders
```

---

## Motor Matemático (grafito-geometry)

### Computer Algebra System (CAS)

| Operación | Comando | Método | Precisión |
|-----------|---------|--------|-----------|
| Derivada simbólica | `Derivative[expr]` | AST 29 variantes con regla de cadena | Exacta |
| Derivada numérica | `Derivative[f, x]` | Diferencias finitas centradas O(h²) | h=1e-6 |
| Integral definida | `Integral[expr, a, b]` | Gauss-Legendre 5-puntos adaptativo | ~1e-12 |
| Integral numérica | `Integral[f, a, b]` | Simpson compuesto (n=1000) | O(h⁴) |
| Raíces | `Solve[f, a, b]` | Bisección adaptativa 4000 pasos | 1e-8 |
| Límites | `Limit[expr, x→x₀]` | Extrapolación de Richardson + bilateral | 1e-4 |
| Factorización | `Factor[polinomio]` | Detección de raíces enteras en [-20,20] | Simbólica |
| Expansión | `Expand[(a+b)(c+d)]` | Distribución algebraica | Simbólica |
| Simplificación | `Simplify[expr]` | Constant folding + identidades | 2-pass |
| Serie de Taylor | `Taylor[expr, x, x₀, n]` | Diferenciación simbólica + factorial | Orden n |
| Raíces / interceptos | `Root[f]`, `YIntercept[f]` | Newton/bisección + refinamiento | 1e-9 |
| Extremos | `Extremum[f]` | Raíces de la derivada primera | 1e-9 |
| Inflexiones | `Inflection[f]` | Raíces de la derivada segunda | 1e-9 |
| Análisis completo | `Analyze[f]` | Todas las características anteriores combinadas | 1e-9 |
| Matrices | `Determinant[[...]]`, `Inverse[[...]]` | Eliminación Gaussiana con pivoteo parcial | ~1e-15 |

### Motor de Expresiones

- **Parser AST recursivo**: 29 variantes de expresiones con diferenciación simbólica completa
- **Preprocesador LaTeX**: `\frac`, `\sqrt`, `\sin`, `\cos`, `\pi`, notación inversa
- **Dual-path**: fast path (AST) + slow path (evalexpr) con fallback automático
- **Multiplicación implícita**: `2x`, `x y`, `(x+1)(x-1)` → parseo correcto
- **Complejos**: Parser dedicado para números complejos con aritmética, exponenciales, trigonométricas

### Estadística y Probabilidad

#### Estadística Descriptiva
| Función | Descripción |
|---------|-------------|
| `Mean[{data}]` | Media aritmética |
| `Median[{data}]` | Mediana con ordenamiento |
| `Mode[{data}]` | Moda |
| `StdDev[{data}]` | Desviación estándar (Bessel) |
| `Variance[{data}]` | Varianza muestral |
| `Quantile[{data}, q]` | Cuantil por interpolación lineal |
| `IQR[{data}]` | Rango intercuartílico |
| `Covariance[{xs}, {ys}]` | Covarianza |
| `Correlation[{xs}, {ys}]` | Correlación de Pearson |

#### Regresiones
| Tipo | Comando | Método |
|------|---------|--------|
| Lineal | `LinearRegression[{xs}, {ys}]` | Mínimos cuadrados con R² |
| Polinomial | `PolynomialRegression[{xs}, {ys}, deg]` | Eliminación Gaussiana |
| Exponencial | Interno | Transformación logarítmica |
| Logarítmica | Interno | Transformación logarítmica |
| Potencial | Interno | Doble transformación logarítmica |

#### Gráficos Estadísticos
| Tipo | Comando |
|------|---------|
| Histograma | `Histogram[{data}, bins]` |
| Diagrama de dispersión | `ScatterPlot[{xs}, {ys}]` |
| Diagrama de caja | `BoxPlot[{data}]` (con outliers) |
| Recta de regresión | `LinearRegression[{xs}, {ys}]` |
| Tabla de frecuencias | `FrequencyTable[{data}]` |

#### 14 Distribuciones de Probabilidad
| Distribución | PDF/PMF | CDF | Cuantil |
|-------------|---------|-----|---------|
| Normal | ✅ | ✅ | ✅ (Acklam) |
| Binomial | ✅ | ✅ | — |
| Poisson | ✅ | ✅ | — |
| t de Student | ✅ | ✅ | — |
| Chi-cuadrado | ✅ | ✅ | — |
| F de Fisher | ✅ | ✅ | — |
| Exponencial | ✅ | ✅ | — |
| Geométrica | ✅ | ✅ | — |
| Hipergeométrica | ✅ | — | — |
| Logística | ✅ | ✅ | — |
| Weibull | ✅ | ✅ | — |
| Uniforme | ✅ | ✅ | — |
| Gamma | ✅ | — | — |
| Beta | ✅ | — | — |
| Cauchy | ✅ | ✅ | — |
| Pareto | ✅ | ✅ | — |
| Rayleigh | ✅ | ✅ | — |
| Laplace | ✅ | ✅ | — |
| Binomial Negativa | ✅ | ✅ | — |

#### Inferencia Estadística
| Test | Comando |
|------|---------|
| t-test 1 muestra | `TTest[{data}, mu0]` |
| t-test 2 muestras | `TTest2[{data1}, {data2}]` |
| z-test | `ZTest[{data}, mu0, sigma]` |
| Chi-cuadrado | `ChiSqTest[{obs}, {exp}]` |
| ANOVA 1 vía | `ANOVA[{g1}, {g2}, ...]` |
| Intervalo confianza (media) | `CIMean[{data}, conf]` |
| Intervalo confianza (proporción) | `CIProportion[successes, n, conf]` |

### Ecuaciones Diferenciales (ODE)

| Método | Comando | Orden | Precisión |
|--------|---------|-------|-----------|
| Euler | `ODE[dy/dt, t0, y0, t_end, steps, "euler"]` | 1er orden | O(h) |
| Runge-Kutta 4 | `ODE[f, t0, y0, t_end, steps]` (default RK4) | 1er orden | O(h⁴) |
| Euler (sistema) | `ODESystem[f1, f2, t0, y10, y20, t_end]` | Sistema 2D | O(h) |
| RK4 (sistema) | `ODESystem[f1, f2, ..., "rk4"]` | Sistema 2D | O(h⁴) |

### Funciones Especiales

| Función | Comando | Método |
|---------|---------|--------|
| Gamma Γ(x) | `Gamma[x]` | Lanczos g=7 |
| Log-Gamma | `LnGamma[x]` | vía Gamma |
| Beta B(a,b) | `Beta[a, b]` | Γ(a)Γ(b)/Γ(a+b) |
| Bessel Jₙ(x) | `BesselJ[n, x]` | Serie hasta 100 términos |
| Bessel Yₙ(x) | `BesselY[n, x]` | Aproximación numérica |
| Bessel Iₙ(x) | `BesselI[n, x]` | Serie modificada |
| Error erf(x) | `Erf[x]` | Abramowitz & Stegun 7.1.26 |
| Error complementario | `Erfc[x]` | 1 - erf(x) |
| Digamma ψ(x) | `Digamma[x]` | Recurrencia + asintótica |

### Curvas Especiales

| Curva | Comando | Ecuación |
|-------|---------|----------|
| Cardioide | `Cardioid[a]` | r = a(1+cos θ) |
| Rosa | `Rose[a, n, d]` | r = a·cos(n/d·θ) |
| Espiral Arquimediana | `ArchimedeanSpiral[a, b, θmax]` | r = a + bθ |
| Espiral Logarítmica | `LogarithmicSpiral[a, b, θmax]` | r = a·e^(bθ) |
| Lissajous | `Lissajous[a, b, fx, fy, δ]` | x=A·sin(fx·t+δ), y=B·sin(fy·t) |
| Epicicloide | `Epicycloid[r, k]` | Traza de círculo rodante exterior |
| Hipocicloide | `Hypocycloid[r, k]` | Traza de círculo rodante interior |

### Atractores Extraños (10 sistemas)

Resueltos con integrador **Runge-Kutta 4** de paso adaptativo.

| Atractor | Comando | Parámetros default | Dimensión fractal |
|----------|---------|-------------------|-------------------|
| Lorenz | `Lorenz[]` | σ=10, ρ=28, β=8/3 | ~2.06 |
| Rössler | `Rossler[]` | a=0.2, b=0.2, c=5.7 | ~2.01 |
| Thomas (Butterfly) | `Thomas[]` / `Butterfly[]` | b=0.208186 | ~2.20 |
| Aizawa | `Aizawa[]` | a=0.95, b=0.7, c=0.6, d=3.5, e=0.25, f=0.1 | ~2.10 |
| Chen | `Chen[]` | a=35, b=3, c=28 | ~2.15 |
| Halvorsen | `Halvorsen[]` | a=1.89 | ~2.10 |
| Dadras | `Dadras[]` | p=3, q=2.7, r=1.7, s=2, e=9 | ~2.10 |
| Chua | `Chua[]` | α=15.6, β=28, m₀=-1.143, m₁=-0.714 | ~2.20 |
| Sprott | Interno | a=2.07, b=1.79 | — |
| Three-Scroll | Interno | a=0.4, b=0.01, c=0.3, d=0.4, e=0.01, f=0.3 | — |

### Fractales

| Fractal | Comando | Renderizado |
|---------|---------|-------------|
| Mandelbrot | `Mandelbrot[max_iter]` | Coloración HSV suave, paralelo (rayon) |
| Julia | `Julia[cr, ci, max_iter]` | Conjuntos: dendrita, Siegel, galaxia |
| Burning Ship | `BurningShip[]` | |Re(z)| + i|Im(z)|² + c |
| Tricornio | Interno | conj(z)² + c |
| Newton (z³-1) | Interno | Método de Newton en complejos |

### Objetos 4D (Proyección)

| Objeto | Comando | Proyección |
|--------|---------|------------|
| Hipercubo (Teseracto) | `Hypercube[a1, a2, a3]` | Rotación 4D → perspectiva 3D → proyección 2D |
| Hiperesfera (3-esfera) | `Hypersphere[a1, a2, a3]` | 3 ángulos de rotación 4D, wireframe lat/lon |

---

## Compute Shaders GPU

Grafito aprovecha `wgpu` para evaluar geometría masivamente en paralelo mediante compute shaders escritos en WGSL.

| Pipeline | Función | Uso típico |
|----------|---------|------------|
| `function_compute` | Evalúa `y = f(x)` en una grilla 1D | Gráficas de funciones explícitas |
| `implicit_compute` | Evalúa `f(x, y) = c` sobre una grilla 2D | Curvas implícitas vía marching squares |
| `parametric_compute` | Evalúa `(x(t), y(t))` y superficies `(x(u,v), y(u,v), z(u,v))` | Curvas y superficies paramétricas 2D/3D |

Cada pipeline compila un shader con la expresión embebida, crea un bind group con buffers de entrada/salida y lanza `dispatch_workgroups` para llenar un buffer de staging legible por CPU. El renderizador cae automáticamente al camino CPU si el pipeline GPU no está disponible.

---

## Canvas 2D

### Objetos Soportados (32+ tipos)

| Categoría | Tipos |
|-----------|-------|
| Puntos y líneas | `Point`, `Line`, `Text` |
| Cónicas | `Circle`, `Ellipse`, `Parabola`, `Hyperbola` |
| Curvas | `Function`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve` |
| Polígonos | `Polygon`, `RegularPolygon` |
| Operaciones booleanas 2D | `PolygonUnion`, `PolygonIntersection`, `PolygonDifference`, `PolygonXor` |
| Campos | `VectorField2D` (flechas normalizadas con punta) |
| Complejos | `ComplexGrid` (malla deformada), `ComplexMapping` |
| Fractales | `Fractal2D` (Mandelbrot, Julia, Burning Ship) |
| Estadística | `Histogram`, `ScatterPlot`, `BoxPlot`, `RegressionLine` |

### Herramientas Interactivas

- **Click-and-drag**: Clic para colocar; arrastrar en vacío panea la vista; arrastrar un punto libre lo mueve
- **Construcción**: Tangent, PerpendicularBisector, AngleBisector, Midpoint, Vector, Ray, Intersect
- **Transformaciones**: Translate, Rotate, Dilate, Reflect
- **Restricciones numéricas**: Distance, Angle, Tangent, Coincident, Horizontal, Vertical, EqualLength, Symmetry
- **Cónicas**: EllipseByFoci, ParabolaByFocusDirectrix, HyperbolaByFoci, ConicByFivePoints
- **Booleanas 2D**: PolygonUnion, PolygonIntersection, PolygonDifference, PolygonXor
- **Herramientas contextuales**: Se ocultan/muestran según modo 2D/3D automáticamente

---

## Operaciones Booleanas 2D

Grafito incluye operaciones booleanas exactas sobre polígonos 2D usando la librería `geo`:

| Operación | Comando | Resultado |
|-----------|---------|-----------|
| Unión | `PolygonUnion[p1, p2]` | Polígono que cubre el área combinada |
| Intersección | `PolygonIntersection[p1, p2]` | Región común a ambos polígonos |
| Diferencia | `PolygonDifference[p1, p2]` | `p1` con el área de `p2` removida |
| Diferencia simétrica | `PolygonXor[p1, p2]` | Áreas que pertenecen a solo uno de los polígonos |

Estas operaciones son accesibles tanto desde la toolbar como desde la paleta de comandos.

---

## Enlace de Expresiones

Casi cualquier parámetro geométrico puede ligarse a una expresión simbólica que se reevalúa automáticamente cuando cambian las variables del documento.

| Objeto | Sintaxis | Parámetros ligados |
|--------|----------|-------------------|
| Punto | `PointExpr[x_expr, y_expr]` | Coordenadas `x`, `y` |
| Círculo | `CircleExpr[centro, radius_expr]` | Radio |
| Recta | `LineExpr[(x1,y1), (x2,y2)]` | Coordenadas de los extremos |
| Polígono | `PolygonExpr[...]` | Cada vértice |
| Función | `FunctionExpr[expr]` | Expresión y dominio |
| Curva paramétrica | `ParametricExpr[x(t), y(t), t_min, t_max]` | Funciones paramétricas |
| Superficie 3D | `SurfaceExpr[x(u,v), y(u,v), z(u,v), ...]` | Funciones paramétricas |

Esto permite crear familias de objetos, animaciones con sliders y construcciones dependientes de variables globales.

---

## Solver de Restricciones Numéricas

Grafito resuelve restricciones geométricas numéricas mediante un método de Newton con Jacobianos analíticos. El solver mantiene un grafo de dependencias y propaga los cambios en orden topológico.

| Restricción | Comando | Descripción |
|-------------|---------|-------------|
| Distancia | `Distance[A, B, 5]` | Fija `|AB| = 5` |
| Ángulo | `Angle[l1, l2, 90]` | Fija el ángulo entre dos rectas |
| Tangencia | `Tangent[c1, c2]` | Fuerza tangencia entre círculos o círculo/recta |
| Coincidencia | `Coincident[A, B]` | Dos puntos comparten posición |
| Horizontal | `Horizontal[s]` | Segmento o recta horizontal |
| Vertical | `Vertical[l]` | Segmento o recta vertical |
| Igual longitud | `EqualLength[s1, s2]` | Dos segmentos con la misma longitud |
| Simetría | `Symmetry[P, Q, eje]` | `P` y `Q` son simétricos respecto a `eje` |

---

## Cónicas Avanzadas

Además de las cónicas canónicas, Grafito soporta construcciones de cónicas por restricciones geométricas:

| Cónica | Comando | Definición |
|--------|---------|------------|
| Elipse por focos | `EllipseByFoci[F1, F2, P]` | Focos `F1`, `F2` y punto `P` por el que pasa |
| Parábola por foco y directriz | `ParabolaByFocusDirectrix[F, d]` | Foco `F` y recta directriz `d` |
| Hipérbola por focos | `HyperbolaByFoci[F1, F2, P]` | Focos `F1`, `F2` y punto `P` por el que pasa |
| Cónica por cinco puntos | `ConicByFivePoints[A, B, C, D, E]` | Única cónica general por cinco puntos |

Todas las cónicas soportan rotación arbitraria y renderizado correcto de ramas/aberturas.

---

## Canvas 3D

### Renderizado

- **Depth sorting manual** (painter's algorithm): objetos ordenados lejos→cerca
- **Iluminación Phong simplificada**: componente ambiental (0.3) + difusa (0.7) por vértice
- **Cámara orbital**: rotación (botón derecho), zoom (scroll), pan (botón izquierdo en Select)
- **Proyección perspectiva**: clipping near-plane, frustum culling

### Objetos 3D con Iluminación

| Objeto | Renderizado | Iluminación |
|--------|------------|-------------|
| Punto 3D | Punto proyectado | — |
| Segmento 3D | Línea con clipping | — |
| Esfera | 3 círculos ortogonales (32 segmentos) | Normal por punto |
| Cubo | 12 aristas con normales de cara | Por cara (±X, ±Y, ±Z) |
| Pirámide | Base + 4 aristas laterales | Normal de cara triangular |
| Cono | Círculo base (32 seg) + líneas al ápice | Normal lateral |
| Cilindro | 2 círculos + 4 líneas verticales | Normal radial + tapas |
| Superficie paramétrica | Malla 20×20 evaluada | — |
| Curva paramétrica 3D | 500 segmentos evaluados | — |
| Atractor extraño | Polilínea RK4 (cacheada) | Color por objeto |
| Hipercubo 4D | 16 vértices, 32 aristas, rotación 4D → proyección | — |
| Hiperesfera | Malla lat/lon con rotación 4D | — |
| Campo vectorial 3D | Flechas en grilla 3D | — |

---

## UI/UX

### Paneles

| Panel | Acceso | Funcionalidad |
|-------|--------|---------------|
| **Álgebra** | Tab A | Lista de objetos, input bar, variables, sliders, animación, filtros por tipo |
| **Herramientas** | Tab T | Toolbar con 30+ herramientas, iconos vectoriales, shortcuts, ocultamiento contextual 2D/3D; incluye construcciones, restricciones numéricas, cónicas y booleanas 2D |
| **Tabla de Valores** | Tab # | Próximamente |
| **Hoja de Cálculo** | Tab S | Grid completo con celdas editables, creación de puntos desde coordenadas |
| **Propiedades** | Selección | Panel derecho con tipo, label editable, visibilidad, color, mediciones |

### Command Palette (Ctrl+K)

- Búsqueda por nombre de comando o categoría
- 70+ comandos indexados en 12 categorías
- Navegación con flechas, Enter para seleccionar
- Inserta plantilla de sintaxis en la barra de entrada

### Math Keyboard

- 4 pestañas: `123` (numérico), `f(x)` (funciones), `ABC` (letras), `3D` (avanzado)
- Atajos para atractores, fractales, estadística, matrices, curvas especiales

### Temas y Animaciones

- **Tema Claro/Oscuro**: Toggle instantáneo desde la barra superior
- **Ripple effects**: Ondas expansivas en clicks del canvas
- **Previsualización fantasma**: Objetos semitransparentes al pasar el mouse

---

## Sistema de Archivos

| Formato | Exportación | Tipos soportados |
|---------|-------------|-----------------|
| `.grafito` | Save/Load (JSON) | Documento completo serializado |
| **SVG** | Exportar SVG | Point, Line, Circle, Polygon, Ellipse, Parabola, Hyperbola, Function, Text, ScatterPlot, RegressionLine + grid/axes |
| **PNG** | Exportar PNG | Point, Line, Circle, Polygon, Ellipse, Function, ScatterPlot, RegressionLine (Bresenham, midpoint circle, scanline fill) |
| **TikZ** | Exportar TikZ | Point, Line, Circle, Polygon, Ellipse, Parabola, Hyperbola, Function, Text, ScatterPlot, RegressionLine |

---

## Optimizaciones de Rendimiento

| Técnica | Aplicación |
|---------|-----------|
| **Compute shaders GPU** | Evaluación masiva de funciones, curvas y superficies en `wgpu` |
| **Cache de atractores** | Hash de parámetros → puntos cacheados, solo recalcula al cambiar params |
| **Fractales paralelos** | `rayon::par_iter` sobre filas de píxeles, speedup 4-8x |
| **Evaluación en lote** | `eval_batch_1d/2d` con fast path AST + fallback evalexpr |
| **Caché de expresiones compiladas** | Reutilización de expresiones `evalexpr` para objetos ligados |
| **Depth sorting** | Ordenamiento por distancia a cámara, evita overdraw innecesario |
| **Spatial index R-tree** | `rstar` para O(log n) hit testing en 32+ tipos de objetos |
| **Ocultamiento contextual** | Herramientas y objetos filtrados por modo 2D/3D |
| **Solver con Jacobianos analíticos** | Convergencia más rápida en restricciones numéricas |

---

## Tests y Benchmarks

### Tests de integración headless

```bash
cargo test --workspace
```

- Tests del modelo de documento sin inicializar GPU.
- Tests del renderizador con contexto `wgpu` headless.
- Tests CLI del punto de entrada de `grafito-app`.

### Benchmarks

```bash
cargo bench --workspace
```

Los benchmarks cubren:
- Evaluación masiva de funciones (CPU vs GPU).
- Muestreo de curvas paramétricas y superficies.
- Convergencia del solver de restricciones numéricas.
- Operaciones booleanas 2D sobre polígonos complejos.

---

## Perfilado de Rendimiento

El binario desktop soporta instrumentación opcional de frames mediante `puffin`. Actívala con la feature `profile` y el flag `--profile`:

```bash
cargo run -p grafito-app --features profile -- --profile
```

Esto habilita scopes en `app_update`, UI, input, restricciones, preparación/pintado del canvas y cómputos GPU, e inicia un servidor `puffin_http` en `127.0.0.1:8585`. Conecta `puffin_viewer` a esa dirección para ver el flamegraph. Sin la feature la instrumentación se elimina en tiempo de compilación y no hay overhead.

---

## Stack Tecnológico

| Capa | Tecnología | Versión |
|------|-----------|---------|
| Lenguaje | Rust | 1.78+ |
| GUI Framework | `eframe` / `egui` | 0.29 |
| GPU Rendering | `wgpu` (WebGPU → Vulkan/Metal/DX12) | 22.0 |
| Álgebra Lineal | `glam`, `nalgebra` | 0.29 / 0.33 |
| Geometría Computacional | `geo`, `spade` (Delaunay), `robust` | 0.29 / 2.10 / 1.1 |
| Precisión Arbitraria | `rug` (GMP/MPFR, 53-256 bit) | 1.28 |
| Expresiones | `evalexpr` | 11.3 |
| Números Complejos | `num-complex` | 0.4 |
| Paralelismo | `rayon` | 1.10 |
| Indexación Espacial | `rstar` (R-tree) | 0.12 |
| Serialización | `serde` + `serde_json`/`toml`/`ron` | 1.0 |
| Exportación Imágenes | `image` | 0.25 |
| UUID | `uuid` | 1.10 |
| Diálogos de Archivo | `rfd` | 0.14 |

---

## Controles

| Acción | Control |
|--------|---------|
| Pan 2D | Arrastrar en vacío (cualquier herramienta), Espacio + arrastrar, o botón medio |
| Pan 2D alternativo | Botón derecho (si no hay polígono en curso) |
| Zoom 2D | Scroll wheel |
| Pan 3D | Arrastrar (cualquier herramienta), Espacio + arrastrar, o botón medio |
| Orbitar 3D | Botón derecho + arrastrar |
| Zoom 3D | Scroll wheel |
| Crear objeto | Click (Point: clic simple) |
| Cerrar polígono | Click derecho (3+ vértices) |
| Cancelar punto | Click derecho (1 punto pendiente) |
| Seleccionar | Click (Select tool) |
| Deseleccionar | Click en vacío |
| Command Palette | Ctrl+K |
| Undo | Ctrl+Z |
| Redo | Ctrl+Y / Ctrl+Shift+Z |
| Eliminar | Delete |
| Herramientas | F1 (Select), F2 (Point), F3 (Line), F4 (Circle), F5 (Polygon), F6 (Function) |
| Cancelar/Escape | Esc |

---

## Construcción

### Requisitos
```bash
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4
```

### Compilar y Ejecutar
```bash
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo run -p grafito-app --release
```

---

## Roadmap v0.9

### Implementado

- [x] GPU compute shaders para evaluación masiva de funciones
- [x] GPU compute shaders para curvas y superficies paramétricas
- [x] Operaciones booleanas 2D (Union, Intersection, Difference, Xor)
- [x] Enlace de expresiones para puntos, círculos, líneas, polígonos, funciones, curvas y superficies
- [x] Solver de restricciones numéricas (Distance, Angle, Tangent, Coincident, Horizontal, Vertical, EqualLength, Symmetry)
- [x] Construcciones avanzadas de cónicas (EllipseByFoci, ParabolaByFocusDirectrix, HyperbolaByFoci, ConicByFivePoints)
- [x] Renderizado de parábolas e hipérbolas rotadas
- [x] Tests de integración headless y benchmarks
- [x] Toolbar y comandos para restricciones, cónicas y booleanas

### Pendiente

- [ ] Superficies de revolución 3D
- [ ] Import `.ggb` (GeoGebra XML)
- [ ] Animación temporal para atractores (morphing entre parámetros)
- [ ] Vista de Probabilidad interactiva (PDF/CDF con sliders)
- [ ] Tests end-to-end con interacción de usuario simulada
- [ ] Exportación a formatos adicionales (PNG/SVG mejorados, PDF)
- [ ] Soporte de scripting con macros y construcciones personalizadas

---

## Licencia

Licenciado bajo la **GNU General Public License, versión 3 o (a su elección) cualquier versión posterior**.

El texto completo de la licencia está disponible en [LICENSE](LICENSE).

Este es software libre: puede redistribuirlo y/o modificarlo bajo los
términos de la GNU General Public License publicada por la Free Software
Foundation, ya sea la versión 3 de la Licencia, o (a su elección)
cualquier versión posterior.

Este programa se distribuye con la esperanza de que sea útil, pero
SIN NINGUNA GARANTÍA; sin siquiera la garantía implícita de
COMERCIABILIDAD o IDONEIDAD PARA UN PROPÓSITO PARTICULAR. Vea la
GNU General Public License para más detalles.

Debería haber recibido una copia de la GNU General Public License
junto con este programa. Si no, vea <https://www.gnu.org/licenses/>.
