# Grafito v0.9.0-alpha

**Grafito** es una aplicación interactiva de geometría, álgebra, estadística y cálculo de alto rendimiento, acelerada por GPU, inspirada en GeoGebra y construida desde cero en Rust con APIs gráficas modernas.

---

## Arquitectura

```
grafito/
├── crates/
│   ├── grafito-app/        # eframe desktop app — UI, input, rendering orchestration
│   ├── grafito-core/       # Document model, 32 geometric object types, spatial index, constraints
│   ├── grafito-geometry/   # Math engine: CAS, statistics, ODE, fractals, attractors, curves, matrices
│   ├── grafito-render/     # wgpu GPU pipeline — 2D/3D tessellation, depth sorting, lighting
│   └── grafito-ui/         # egui components — toolbar, command palette, properties, color picker, themes
└── assets/                 # WGSL shaders
```

---

## Soporte Android

Grafito soporta compilación para Android usando Android Studio (Flatpak en Linux).

### Requisitos

- Android Studio (Flatpak): `flatpak install com.google.AndroidStudio`
- NDK instalado desde Android Studio
- cargo-ndk: `cargo install cargo-ndk`

### Configuración Rápida

```bash
# Configurar entorno automáticamente
./scripts/setup-android-env.sh

# Aplicar variables de entorno
source ~/.grafito-android-env

# Compilar para Android
./scripts/build-android.sh

# Instalar en dispositivo conectado
./scripts/install-android.sh
```

### Documentación Completa

Ver [ANDROID.md](ANDROID.md) para guía completa de compilación, instalación y solución de problemas.

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

## Canvas 2D

### Objetos Soportados (32 tipos)

| Categoría | Tipos |
|-----------|-------|
| Puntos y líneas | `Point`, `Line`, `Text` |
| Cónicas | `Circle`, `Ellipse`, `Parabola`, `Hyperbola` |
| Curvas | `Function`, `ParametricCurve2D`, `PolarCurve`, `ImplicitCurve` |
| Polígonos | `Polygon`, `RegularPolygon` |
| Campos | `VectorField2D` (flechas normalizadas con punta) |
| Complejos | `ComplexGrid` (malla deformada), `ComplexMapping` |
| Fractales | `Fractal2D` (Mandelbrot, Julia, Burning Ship) |
| Estadística | `Histogram`, `ScatterPlot`, `BoxPlot`, `RegressionLine` |

### Herramientas Interactivas

- **Click-and-drag**: Arrastrar para posicionar con previsualización fantasma semitransparente
- **Construcción**: Tangent, PerpendicularBisector, AngleBisector, Midpoint, Vector, Ray
- **Transformaciones**: Translate, Rotate, Dilate, Reflect
- **Herramientas contextuales**: Se ocultan/muestran según modo 2D/3D automáticamente

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
| **Herramientas** | Tab T | Toolbar con 16 herramientas, iconos vectoriales, shortcuts, ocultamiento contextual 2D/3D |
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
| **Cache de atractores** | Hash de parámetros → puntos cacheados, solo recalcula al cambiar params |
| **Fractales paralelos** | `rayon::par_iter` sobre filas de píxeles, speedup 4-8x |
| **Evaluación en lote** | `eval_batch_1d/2d` con fast path AST + fallback evalexpr |
| **Depth sorting** | Ordenamiento por distancia a cámara, evita overdraw innecesario |
| **Spatial index R-tree** | `rstar` para O(log n) hit testing en 32 tipos de objetos |
| **Ocultamiento contextual** | Herramientas y objetos filtrados por modo 2D/3D |

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
| Pan 2D | Arrastrar (solo herramienta Select) |
| Zoom 2D | Scroll wheel |
| Pan 3D | Botón izquierdo (Select tool) |
| Orbitar 3D | Botón derecho |
| Zoom 3D | Scroll wheel |
| Crear objeto | Click (Point: arrastrar y soltar) |
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

- [ ] Superficies de revolución 3D
- [ ] GPU compute shaders para evaluación masiva de funciones
- [ ] Import `.ggb` (GeoGebra XML)
- [ ] Animación temporal para atractores (morphing entre parámetros)
- [ ] Vista de Probabilidad interactiva (PDF/CDF con sliders)
- [ ] Tests de integración end-to-end

---

## Licencia

Licenciado bajo:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

a elección del usuario.
