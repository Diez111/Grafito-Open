<p align="center">
  <img src="assets/grafito-icon-256x256.png" alt="Grafito" width="128" />
</p>

<h1 align="center">Grafito</h1>

<p align="center">
  <a href="https://github.com/Diez111/Grafito/releases"><img src="https://img.shields.io/github/v/release/Diez111/Grafito?include_prereleases&label=versi%C3%B3n&color=blue" alt="Versión" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licencia-GPLv3%2B-blue.svg" alt="Licencia: GPLv3+" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.78%2B-orange.svg" alt="Rust 1.78+" /></a>
  <a href="https://github.com/Diez111/Grafito/stargazers"><img src="https://img.shields.io/github/stars/Diez111/Grafito?style=social" alt="Estrellas" /></a>
</p>

<p align="center">
  <b>Aplicación interactiva de geometría, álgebra, estadística y cálculo acelerada por GPU.</b><br />
  Construida desde cero en Rust. Impulsada por WebGPU.
</p>

---

- [Instalación](#instalación)
- [Funcionalidades](#funcionalidades)
  - [Canvas 2D interactivo](#canvas-2d-interactivo)
  - [Motor matemático](#motor-matemático-grafito-geometry)
  - [Estadística y probabilidad](#estadística-y-probabilidad)
  - [Vista 3D](#vista-3d)
  - [Compute shaders GPU](#compute-shaders-gpu-webgpu-vía-wgpu)
  - [UI / UX](#ui--ux)
  - [Formatos de archivo](#formatos-de-archivo)
- [Arquitectura](#arquitectura)
- [Stack tecnológico](#stack-tecnológico)
- [Rendimiento](#rendimiento)
- [Controles](#controles)
- [Desarrollo](#desarrollo)
- [Contribuir](#contribuir)
- [Licencia](#licencia)

---

## Instalación

**Debian / Ubuntu (`.deb`)**

```bash
wget https://github.com/Diez111/Grafito/releases/latest/download/grafito_amd64.deb
sudo dpkg -i grafito_amd64.deb
```

**Compilar desde el código fuente**

```bash
# Dependencias
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4

# Clonar y compilar
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo run -p grafito-app --release
```

> Requiere Rust 1.78+. Los compute shaders GPU necesitan soporte Vulkan, Metal o DX12.

---

## Funcionalidades

### Canvas 2D interactivo

| Funcionalidad | Descripción |
|---------------|-------------|
| **32+ tipos de objetos** | Puntos, Rectas, Círculos, Elipses, Parábolas, Hipérbolas, Polígonos, Polígonos regulares, Funciones, Curvas paramétricas, Curvas polares, Curvas implícitas |
| **Construcciones** | Tangente, Mediatriz, Bisectriz, Punto medio, Vector, Semirrecta, Intersección, Lugar geométrico |
| **Transformaciones** | Traslación, Rotación, Dilatación, Reflexión (puntos y objetos completos) |
| **Operaciones booleanas** | Unión, Intersección, Diferencia y XOR de polígonos |
| **Cónicas** | Por focos, foco-directriz o 5 puntos arbitrarios. Todas con rotación arbitraria |
| **Curvas implícitas** | `ImplicitCurve[x^2 + y^2 = 1]`, `ImplicitCurve[x*y = 1]`, `ImplicitCurve[x^3 + y^3 - 3xy = 0]` |
| **Mapeos complejos** | `ComplexMapping[1/z, objeto]` aplica cualquier función compleja a un objeto 2D, con asíntotas punteadas automáticas en singularidades |
| **Campos vectoriales** | `VectorField2D[f(x,y), g(x,y)]` con puntas de flecha normalizadas |
| **Fractales** | Mandelbrot, Julia, Burning Ship, Tricornio, Newton — evaluación paralela con `rayon` y coloración HSV suave |
| **Gráficos estadísticos** | Histograma, Diagrama de dispersión, Diagrama de caja (con outliers), Recta de regresión |
| **Enlace de expresiones** | Cualquier parámetro puede ligarse a una expresión simbólica que se reevalúa al cambiar variables |
| **Solver de restricciones numéricas** | Método de Newton con Jacobianos analíticos: Distancia, Ángulo, Tangencia, Coincidencia, Horizontal, Vertical, Igual longitud, Simetría |

### Motor matemático (`grafito-geometry`)

**Sistema de Álgebra Computacional (CAS)**

| Operación | Comando | Método | Precisión |
|-----------|---------|--------|-----------|
| Derivada simbólica | `Derivative[expr]` | AST con 29 variantes + regla de cadena | Exacta |
| Derivada numérica | `Derivative[f, x]` | Diferencias centradas O(h²), h=1e-6 | ~1e-12 |
| Integral definida | `Integral[expr, a, b]` | Gauss-Legendre 5-puntos adaptativo | ~1e-12 |
| Integral definida (GPU) | `Integral[f, a, b]` | Híbrido GPU/CPU, Simpson n=1000 | O(h⁴) |
| Raíces / Interceptos | `Root[f]`, `XIntercept[f]`, `YIntercept[f]` | Newton + bisección + refinamiento | 1e-9 |
| Extremos | `Extremum[f]` | Raíces de la derivada primera | 1e-9 |
| Inflexiones | `Inflection[f]` | Raíces de la derivada segunda | 1e-9 |
| Análisis completo | `Analyze[f]` | Raíces, extremos, inflexión, interceptos, asíntotas, Taylor | 1e-9 |
| Serie de Taylor | `Taylor[expr, x, x0, n]` | Diferenciación simbólica + factorial | Orden n |
| Factorización | `Factor[polinomio]` | Detección de raíces enteras en [-20, 20] | Simbólica |
| Expansión | `Expand[(a+b)(c+d)]` | Distribución algebraica | Simbólica |
| Simplificación | `Simplify[expr]` | Constant folding + identidades, 2 pasadas | Simbólica |
| Límites | `Limit[expr, x -> x0]` | Extrapolación de Richardson + bilateral | ~1e-4 |
| Matrices | `Determinant[[...]]`, `Inverse[[...]]` | Eliminación Gaussiana con pivoteo parcial | ~1e-15 |
| Longitud de arco | `ArcLength[f, a, b]` | Integral de sqrt(1 + f'²) | Numérica |
| Curvatura | `CurvatureAt[f, x]` | κ = |f''| / (1+f'²)^(3/2) | Numérica |
| Volumen de revolución | `VolumeOfRevolution[f, a, b]` | π * integral(f²) | Numérica |
| Superficie de revolución | `SurfaceOfRevolution[f, a, b]` | 2π * integral(|f| * sqrt(1+f'²)) | Numérica |

**Sistema de expresiones unificado**: parser AST recursivo, preprocesador LaTeX (`\frac`, `\sqrt`, `\sin`, `\pi`), multiplicación implícita (`2x`, `x y`, `(x+1)(x-1)`), parser de números complejos con aritmética dedicada.

**14 curvas especiales**: Cardioide, Rosa, Espiral de Arquímedes, Espiral logarítmica, Lissajous, Epicicloide, Hipocicloide.

**10 atractores extraños**: Lorenz, Rössler, Thomas/Butterfly, Aizawa, Chen, Halvorsen, Dadras, Chua, Sprott, Three-Scroll — resueltos con RK4 adaptativo.

**Ecuaciones diferenciales**: integradores Euler y RK4 (1er orden y sistemas 2D).

**Funciones especiales**: Gamma (Lanczos g=7), Beta, Bessel J/Y/I (series hasta 100 términos), Error (Abramowitz & Stegun 7.1.26), Error complementario, Digamma (recurrencia + asintótica).

**Objetos 4D**: Hipercubo (teseracto) e Hiperesfera (3-esfera) con rotación y proyección perspectiva.

### Estadística y probabilidad

**Descriptiva**: Media, Mediana, Moda, Desviación estándar, Varianza, Cuantil, IQR, Covarianza, Correlación.

**Regresiones**: Lineal (mínimos cuadrados con R²), Polinomial (eliminación Gaussiana), Exponencial, Logarítmica, Potencial.

**17 distribuciones de probabilidad** (PDF, CDF, Cuantil donde aplica): Normal, Binomial, Poisson, t-Student, Chi-cuadrado, F, Exponencial, Geométrica, Hipergeométrica, Logística, Weibull, Uniforme, Gamma, Beta, Cauchy, Pareto, Rayleigh, Laplace, Binomial Negativa.

**Inferencia**: t-test (1 y 2 muestras), z-test, Chi-cuadrado, ANOVA (1 vía), Intervalos de confianza (media con t-Student para n < 30, proporción).

### Vista 3D

| Objeto | Renderizado | Notas |
|--------|-------------|-------|
| Punto, Segmento | Proyectado con clipping | — |
| Esfera | 3 círculos ortogonales (32 segmentos) | Iluminación Phong por vértice |
| Cubo | 12 aristas con normales de cara | Iluminación ±X, ±Y, ±Z |
| Pirámide, Cono, Cilindro | Wireframe con normales laterales | — |
| Toro | Círculos en wireframe | — |
| Superficie paramétrica | Malla 20x20 | Evaluada por GPU |
| Curva paramétrica 3D | Polilínea de 500 segmentos | Evaluada por GPU |
| Atractor extraño | Polilínea RK4 cacheada | Color por objeto |
| Hipercubo 4D | 16 vértices, 32 aristas, rotación 4D | Proyección perspectiva |
| Hiperesfera 4D | Malla lat/lon, rotación 4D | Wireframe |

**Cámara**: órbita (clic derecho + arrastrar), zoom (scroll), paneo (clic izquierdo en Select). Depth sorting con algoritmo del pintor e iluminación Phong simplificada (ambiental 0.3 + difusa 0.7).

### Compute shaders GPU (WebGPU vía wgpu)

Los tres pipelines compilan un shader WGSL con la expresión del usuario embebida en tiempo de ejecución, crean un bind group y despachan workgroups para llenar un buffer de staging legible por CPU. Caen automáticamente al camino CPU si la GPU no está disponible.

| Pipeline | Evalúa | Uso |
|----------|--------|-----|
| `function_compute` | `y = f(x)` en grilla 1D | Gráficas de funciones explícitas |
| `implicit_compute` | `f(x, y) = c` en grilla 2D | Curvas implícitas vía marching squares |
| `parametric_compute` | `(x(t), y(t))` y superficies `(x(u,v), y(u,v), z(u,v))` | Curvas y superficies paramétricas 2D/3D |

### UI / UX

| Panel | Acceso | Propósito |
|-------|--------|-----------|
| **Álgebra** | Tab A | Lista de objetos, barra de entrada, variables con sliders, animaciones, filtro por tipo |
| **Barra de herramientas** | Tab T | 12 grupos, 40+ herramientas con iconos vectoriales, ocultamiento contextual 2D/3D, atajos de teclado |
| **Paleta de comandos** | Ctrl+K | 70+ comandos indexados en 12 categorías, navegación con flechas, inserción de plantillas |
| **Teclado matemático** | Pie | 4 pestañas: numérico, funciones, letras, avanzado |
| **Propiedades** | Panel derecho | Tipo, etiqueta editable, visibilidad, selector de color, mediciones en tiempo real |
| **Hoja de cálculo** | Tab S | Editor de celdas completo, crear puntos desde coordenadas |

**Herramientas**: Seleccionar, Punto, Recta, Segmento, Semirrecta, Vector, Perpendicular, Círculo, Tangente, Polígono, Polígono regular, Lápiz, Borrador, Elipse por focos, Parábola (foco-directriz), Hipérbola por focos, Cónica por 5 puntos, Función, Curva paramétrica 2D, Curva polar, Curva implícita, Campo vectorial 2D, Lugar geométrico, Distancia, Área, Ángulo, Pendiente, Punto 3D, Esfera 3D, Cubo 3D, Raíz, Extremo, Inflexión, Intersección Y, Intersección X, Intersección, Analizar, Deslizador, Botón, Imagen.

**Calidad de vida**: previsualización fantasma en construcciones, efectos ripple al hacer clic, snap jerárquico (a features de análisis, intersecciones y bordes de curvas), toggle instantáneo de tema claro/oscuro.

### Formatos de archivo

| Formato | Dirección | Tipos soportados |
|---------|-----------|-----------------|
| `.grafito` | Guardar / Cargar (JSON) | Serialización completa del documento |
| SVG | Exportar | Punto, Recta, Círculo, Polígono, Elipse, Parábola, Hipérbola, Función, Texto, ScatterPlot, RegressionLine + grilla/ejes |
| PNG | Exportar | Punto, Recta, Círculo, Polígono, Elipse, Función (Bresenham, midpoint circle, relleno scanline) |
| TikZ | Exportar | Código LaTeX para todas las primitivas 2D |

---

## Arquitectura

```
grafito/
├── crates/
│   ├── grafito-app/         Aplicación de escritorio (eframe) — UI, entrada, orquestación de render
│   ├── grafito-core/        Modelo de documento, 32+ tipos de objetos geométricos, índice espacial, restricciones
│   ├── grafito-geometry/    Motor matemático: CAS, estadística, EDO, fractales, atractores, curvas, booleanas
│   ├── grafito-render/      Pipeline de render wgpu — teselado 2D/3D, compute shaders, iluminación
│   ├── grafito-ui/          Componentes egui — barra de herramientas, paleta de comandos, propiedades, color picker, temas
│   └── grafito-command/     Procesador compartido de comandos de texto para escritorio y FFI
└── assets/                  Iconos, shaders WGSL
```

| Crate | Archivos clave |
|-------|----------------|
| `grafito-geometry` | `ast.rs`, `expr.rs`, `analysis.rs`, `boolean.rs`, `statistics.rs`, `ode.rs`, `fractals.rs`, `complex_expr.rs`, `special_functions.rs` |
| `grafito-core` | `document.rs`, `object.rs`, `analyzable.rs`, `constraints.rs`, `numeric_solver.rs`, `numeric_constraints.rs`, `spatial.rs` |
| `grafito-render` | `lib.rs`, `function_compute.rs`, `implicit_compute.rs`, `parametric_compute.rs`, `vector_compute.rs` |
| `grafito-ui` | `lib.rs`, `toolbar.rs`, `command_palette.rs`, `color_picker.rs`, `theme.rs` |
| `grafito-app` | `main.rs`, `app.rs`, `canvas.rs`, `snap.rs`, `tool_dispatcher.rs`, `algebra.rs`, `export.rs` |
| `grafito-command` | `commands.rs` |

---

## Stack tecnológico

| Capa | Tecnología | Versión |
|------|-----------|---------|
| Lenguaje | Rust | 1.78+ |
| GUI | `eframe` / `egui` | 0.29 |
| GPU | `wgpu` (WebGPU → Vulkan/Metal/DX12) | 22.0 |
| Álgebra lineal | `glam`, `nalgebra` | 0.29 / 0.33 |
| Geometría computacional | `geo`, `spade` (Delaunay), `robust` | 0.29 / 2.10 / 1.1 |
| Precisión arbitraria | `rug` (GMP/MPFR, 53-256 bit) | 1.28 |
| Parser de expresiones | `evalexpr` | 11.3 |
| Números complejos | `num-complex` | 0.4 |
| Paralelismo | `rayon` | 1.10 |
| Índice espacial | `rstar` (R-tree) | 0.12 |
| Serialización | `serde` + `serde_json` / `toml` / `ron` | 1.0 |
| Exportación de imágenes | `image` | 0.25 |
| UUID | `uuid` | 1.10 |
| Diálogos de archivo | `rfd` | 0.14 |

---

## Rendimiento

| Técnica | Dónde | Beneficio |
|---------|-------|-----------|
| Compute shaders GPU | Evaluación de funciones/implícitas/paramétricas | Paralelismo masivo en grillas 1D/2D |
| Caché de atractores | Hash(parámetros) → puntos cacheados | Solo recalcula al cambiar parámetros |
| Fractales paralelos | `rayon::par_iter` sobre filas de píxeles | 4-8x speedup |
| Evaluación por lotes | `eval_batch_1d/2d` con fast path AST + fallback | Menor overhead de parser |
| Caché de expresiones compiladas | Reutilización de árboles `evalexpr` en objetos ligados | Evita re-tokenización |
| Índice espacial R-tree | `rstar` para hit testing O(log n) | 32+ tipos de objetos a tasas interactivas |
| Jacobianos analíticos | Solver de restricciones Newton | Convergencia más rápida vs gradientes numéricos |
| Snapshots de documento | Clon sólo en cambio de versión, `Arc::make_mut` para view | Evita asignaciones por frame |

---

## Controles

| Acción | Entrada |
|--------|--------|
| Pan 2D | Arrastrar en vacío, Espacio+arrastrar, o clic medio |
| Zoom | Rueda del ratón |
| Crear objeto | Clic (Punto: clic simple) |
| Cerrar polígono | Clic derecho (3+ vértices) |
| Cancelar punto | Clic derecho (1 pendiente), Escape |
| Seleccionar objeto | Clic (herramienta Seleccionar) |
| Deseleccionar | Clic en vacío |
| Orbitar 3D | Clic derecho + arrastrar |
| Deshacer / Rehacer | Ctrl+Z / Ctrl+Y |
| Eliminar | Suprimir |
| Paleta de comandos | Ctrl+K |
| Herramientas | F1 (Seleccionar), F2 (Punto), F3 (Recta), F4 (Círculo), F5 (Polígono), F6 (Función), F8 (Esfera 3D), F9 (Cubo 3D) |
| Atajos de análisis | R (Raíz), E (Extremo), N (Inflexión), Ctrl+Y (Intersección Y), Ctrl+A (Analizar) |

---

## Desarrollo

### Verificar

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

### Perfilar

```bash
cargo run -p grafito-app --features profile -- --profile
# Luego conectar puffin_viewer a 127.0.0.1:8585
```

### Compute shaders GPU

Cada pipeline embebe un shader WGSL con la expresión del usuario compilada a bytecode en tiempo de ejecución. El shader itera sobre `code_len` instrucciones, evalúa en una grilla y escribe los resultados en un buffer de staging. Ver `grafito-render/src/<nombre>_compute.rs` + `<nombre>_compute.wgsl`.

---

## Contribuir

1. Hacé un fork del repositorio
2. Creá tu rama de feature (`git checkout -b feature/nueva-funcionalidad`)
3. Escribí tests para la funcionalidad nueva
4. Ejecutá `cargo fmt --all && cargo clippy --workspace -- -D warnings && cargo test --workspace`
5. Commiteá usando [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`)
6. Pusheá y abrí un Pull Request

Consultá [AGENTS.md](AGENTS.md) para convenciones de arquitectura y desarrollo.

---

## Licencia

GNU General Public License v3.0 o posterior. Ver [LICENSE](LICENSE).

```
Grafito — Geometría, álgebra y cálculo interactivos acelerados por GPU
Copyright (C) 2025-2026  Diez111

Este programa es software libre: puede redistribuirlo y/o modificarlo
bajo los términos de la Licencia Pública General de GNU publicada por
la Free Software Foundation, ya sea la versión 3 de la Licencia, o
(a su elección) cualquier versión posterior.
```
