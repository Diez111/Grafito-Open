<p align="center">
  <img src="assets/grafito-icon-256x256.png" alt="Grafito" width="128" />
</p>

<h1 align="center">Grafito</h1>

<p align="center">
  <a href="https://github.com/Diez111/Grafito/releases"><img src="https://img.shields.io/github/v/release/Diez111/Grafito?include_prereleases&label=versi%C3%B3n&color=blue" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licencia-GPLv3%2B-blue" /></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.78%2B-orange" /></a>
  <a href="https://github.com/Diez111/Grafito/stargazers"><img src="https://img.shields.io/github/stars/Diez111/Grafito?style=social" /></a>
</p>

<p align="center">
  <b>Geometría interactiva, álgebra computacional, estadística y cálculo &mdash; acelerados por GPU.</b><br />
  Construido en Rust con WebGPU. Inspirado en GeoGebra.
</p>

---

- [Instalación](#instalación)
- [Primeros pasos](#primeros-pasos)
- [Interactuar con Grafito](#interactuar-con-grafito)
- [Referencia de comandos](#referencia-de-comandos)
- [Canvas 2D](#canvas-2d)
- [Canvas 3D](#canvas-3d)
- [Arquitectura](#arquitectura)
- [Controles](#controles)
- [Desarrollo](#desarrollo)
- [Contribuir](#contribuir)
- [Licencia](#licencia)

---

## Instalación

<p><details><summary><b>Linux &mdash; Debian / Ubuntu (.deb)</b></summary>

```bash
wget https://github.com/Diez111/Grafito/releases/latest/download/grafito_amd64.deb
sudo dpkg -i grafito_amd64.deb
```
</details></p>

<p><details><summary><b>Windows &mdash; .exe portátil</b></summary>

1. Descargá el `.exe` desde [Releases](https://github.com/Diez111/Grafito/releases)
2. Ejecutalo directamente. No requiere instalación.
</details></p>

<p><details><summary><b>Compilar desde el código fuente</b></summary>

```bash
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4
git clone https://github.com/Diez111/Grafito.git
cd grafito
cargo run -p grafito-app --release
```
> Requiere Rust &ge; 1.78. Los compute shaders GPU necesitan Vulkan, Metal o DX12.
</details></p>

---

## Primeros pasos

Al abrir Grafito ves un canvas con ejes cartesianos y una barra de entrada en el panel izquierdo. Tenés **dos formas** de crear cosas:

### Por comandos (escribiendo)

Escribí en la barra de entrada y presioná Enter. Grafito interpreta notación matemática natural:

```text
(x - 2)^2 + (y - 3)^2 = 25          # círculo centrado en (2,3) radio 5
y = sin(x)                           # función seno
r = 2*cos(3*theta)                   # curva polar (rosa de 3 pétalos)
(x(t), y(t)) = (cos(t), sin(t))      # curva paramétrica (círculo)
(2, 5)                               # punto en (2, 5)
Derivative[sin(x)]                   # derivada simbólica: cos(x)
Root[sin(x)]                         # raíces de la función
Taylor[exp(x), x, 0, 4]              # serie de Taylor: 1 + x + x²/2 + ...
```

### Por herramientas (haciendo clic)

La barra de herramientas (abajo) tiene íconos para dibujar directamente sobre el canvas:
- **F1** Seleccionar &bull; **F2** Punto &bull; **F3** Recta &bull; **F4** Círculo &bull; **F5** Polígono
- Cada grupo se despliega con &blacktriangledown; y muestra herramientas relacionadas

Probá: seleccioná **Círculo**, hacé clic en el canvas para el centro, después clic en otro punto cualquiera. El círculo se crea con radio igual a la distancia entre los dos puntos.

---

## Interactuar con Grafito

### La barra de entrada

Es el campo de texto en el panel izquierdo. Escribí cualquier comando o expresión y presioná **Enter**. Grafito evalúa la expresión y crea el objeto correspondiente. Si la expresión contiene un error, el campo se pone rojo y muestra un mensaje.

**Atajos útiles:**
- `Ctrl+K` abre la paleta de comandos con todos los comandos disponibles, navegables con flechas
- `Ctrl+Z` deshace la última acción, `Ctrl+Y` rehace
- `Escape` cancela el objeto en construcción

### Variables y sliders

Escribí un número o una expresión con una variable nueva y Grafito crea un **slider** automáticamente:

```text
k = 3                         # crea un slider "k" que va de 0 a 10
a*sin(x)                      # crea la variable "a" ligada a un slider
```

Arrastrá el slider en el panel izquierdo y la función `a*sin(x)` se actualiza en tiempo real. También podés animarlos (&blacktriangleright;).

### Enlace de expresiones

Casi cualquier parámetro de un objeto puede ligarse a una expresión. Escribí `PointExpr[...]`, `CircleExpr[...]`, etc., usando variables globales o sliders. El objeto se recalcula automáticamente cada vez que las variables cambian.

### Panel de análisis

Seleccioná una función desde el panel de Álgebra (pestaña **A**) y hacé clic en **Analizar** en la barra de herramientas o escribí `Analyze[f]`. Grafito calcula raíces, extremos, inflexiones, asíntotas e interceptos, y marca los puntos sobre el canvas.

---

## Referencia de comandos

Todos los comandos usan la sintaxis `Comando[arg1, arg2, ...]`. Casi todos tienen alias en español (ej: `Derivada`, `Integral`). Escribí `Ctrl+K` para ver la lista completa.

### Creación de objetos

| Sintaxis | Resultado |
|----------|-----------|
| `(x, y)` | Punto 2D |
| `(x, y, z)` | Punto 3D |
| `y = expr` o `f(x) = expr` | Función y = f(x) |
| `r = expr` o `r(theta) = expr` | Curva polar |
| `(x(t), y(t)) = (fx, fy)` | Curva paramétrica 2D |
| `lhs = rhs` o `lhs >= rhs` | Curva implícita |
| `expr` con `x` libre | Función (variable implícita) |
| `expr` numérica | Crea un slider con ese valor |
| `k = valor` | Crea un slider nombrado |

### Álgebra computacional (CAS)

| Comando | Ejemplo | Resultado |
|---------|---------|-----------|
| `Derivative` / `Derivada` | `Derivative[x^3]` | `3*x^2` |
| `Integral` / `Integrar` | `Integral[sin(x), x, 0, pi]` | `2` (definida) |
| `Solve` / `Resolver` | `Solve[x^2 - 4]` | `{-2, 2}` |
| `Limit` / `Limite` | `Limit[sin(x)/x, x, 0]` | `1` |
| `Factor` / `Factorizar` | `Factor[x^2 + 5x + 6]` | `(x+2)*(x+3)` |
| `Expand` / `Expandir` | `Expand[(x+1)^2]` | `x^2 + 2*x + 1` |
| `Simplify` / `Simplificar` | `Simplify[2x + x]` | `3*x` |
| `Taylor` | `Taylor[cos(x), x, 0, 5]` | `1 - x^2/2 + x^4/24` |

### Análisis de funciones

| Comando | Resultado |
|---------|-----------|
| `Root[f]` | Raíces (f(x) = 0), marca puntos en el canvas |
| `Extremum[f]` | Máximos y mínimos locales |
| `Inflection[f]` | Puntos de inflexión |
| `YIntercept[f]` | Intersección con el eje Y |
| `XIntercept[f]` | Intersecciones con el eje X |
| `Analyze[f]` | Análisis completo (todo lo anterior + asíntotas) |
| `Intersect[a, b]` | Intersección entre dos objetos |

### Cálculo avanzado

| Comando | Ejemplo |
|---------|---------|
| `TangentAt` / `TangenteEn` | `TangentAt[sin(x), 0]` &rarr; recta tangente en x=0 |
| `NormalAt` / `NormalEn` | `NormalAt[sin(x), pi/2]` &rarr; recta normal |
| `ArcLength` / `LongitudArco` | `ArcLength[x^2, 0, 1]` |
| `CurvatureAt` / `CurvaturaEn` | `CurvatureAt[x^3, 0]` &rarr; curvatura y radio |
| `VolumeOfRevolution` / `VolumenRevolucion` | `VolumeOfRevolution[sin(x), 0, pi]` |
| `SurfaceOfRevolution` / `SuperficieRevolucion` | `SurfaceOfRevolution[x^2, 0, 1]` |

### Construcciones geométricas

| Comando | Descripción |
|---------|-------------|
| `Segment[a, b]` | Segmento entre dos puntos |
| `Ray[origen, direccion]` | Semirrecta |
| `Vector[origen, destino]` | Vector |
| `Perpendicular[punto, recta]` | Recta perpendicular |
| `Midpoint[a, b]` | Punto medio entre dos puntos |
| `Intersect[a, b]` | Intersección(es) entre dos objetos |
| `RegularPolygon[centro, lados, radio]` | Polígono regular (3 a 64 lados) |
| `Reflect[objeto, p1, p2]` | Reflejar objeto respecto a una recta |
| `Translate[objeto, vector]` | Trasladar objeto |
| `Rotate[objeto, centro, angulo]` | Rotar objeto (grados) |
| `Dilate[objeto, centro, factor]` | Dilatar objeto |

### Restricciones numéricas

| Comando | Descripción |
|---------|-------------|
| `DistanceConstraint[a, b, valor]` | Fija la distancia entre a y b |
| `AngleConstraint[l1, l2, grados]` | Fija el ángulo entre dos rectas |
| `TangentConstraint[c1, c2]` | Fuerza tangencia entre círculos |
| `Coincident[a, b]` | Fuerza coincidencia de dos puntos |
| `Horizontal[segmento]` | Fuerza horizontalidad |
| `Vertical[segmento]` | Fuerza verticalidad |
| `EqualLength[s1, s2]` | Fuerza igualdad de longitud |
| `Symmetry[p, q, eje]` | Fuerza simetría de p y q |

### Estadística

Escribí listas de datos con `{ }` o usá variables:

```text
datos = {1.2, 3.5, 2.1, 4.8, 2.9}
Mean[{1.2, 3.5, 2.1, 4.8, 2.9}]        # media
StdDev[datos]                             # desviación estándar
Histogram[datos, 5]                       # histograma con 5 bins, dibuja barras
ScatterPlot[{xs}, {ys}]                   # diagrama de dispersión
BoxPlot[datos]                            # diagrama de caja con outliers
LinearRegression[{xs}, {ys}]              # recta de regresión + R²
Correlation[{xs}, {ys}]                   # correlación de Pearson
```

| Comando adicional | Resultado |
|-------------------|-----------|
| `Median[datos]` | Mediana |
| `Mode[datos]` | Moda |
| `Variance[datos]` | Varianza muestral |
| `Quantile[datos, q]` | Cuantil (0 a 1) |
| `IQR[datos]` | Rango intercuartílico |
| `Covariance[{xs}, {ys}]` | Covarianza |

### Fractales y atractores

```text
Mandelbrot[256]                     # fractal de Mandelbrot
Julia[-0.70176, -0.3842, 128]       # conjunto de Julia
BurningShip[128]                    # fractal Burning Ship
Lorenz[10, 28, 2.666]              # atractor de Lorenz
Rossler[0.2, 0.2, 5.7]            # atractor de Rössler
Thomas[0.208186]                    # mariposa de Thomas
```

### Otros comandos

| Comando | Descripción |
|---------|-------------|
| `ComplexMapping[1/z, obj]` | Mapeo complejo sobre cualquier objeto 2D |
| `VectorField2D[u, v]` | Campo vectorial (x,y) &rarr; (u,v) con flechas |
| `Piecewise[{expr1, cond1}, {expr2, cond2}]` | Función por partes |
| `DomainColoring[1/z]` | Coloración de dominio complejo |
| `HeatMap[exp(-(x^2 + y^2))]` | Mapa de calor |
| `Erase[label]` / `EraseAll` | Borrar objetos |
| `Script[comando1; comando2; ...]` | Ejecutar varios comandos a la vez |

---

## Canvas 2D

Grafito renderiza más de 30 tipos de objetos geométricos sobre un canvas interactivo. Podés crear objetos escribiendo comandos o usando las herramientas de la barra inferior.

### Objetos disponibles

Puntos, rectas, segmentos, semirrectas, vectores, círculos, elipses, parábolas, hipérbolas, polígonos, polígonos regulares, funciones explícitas, curvas paramétricas 2D, curvas polares, curvas implícitas, campos vectoriales 2D, lugares geométricos, dibujos a mano alzada (lápiz y borrador).

### Operaciones booleanas entre polígonos

```text
PolygonUnion[p1, p2]           # unión
PolygonIntersection[p1, p2]    # intersección
PolygonDifference[p1, p2]      # diferencia (p1 menos p2)
PolygonXor[p1, p2]             # diferencia simétrica (XOR)
```

### Cónicas por construcción geométrica

```text
EllipseByFoci[F1, F2, P]                 # elipse dados focos y punto
ParabolaByFocusDirectrix[F, directriz]   # parábola por foco y directriz
HyperbolaByFoci[F1, F2, P]               # hipérbola por focos y punto
ConicByFivePoints[A, B, C, D, E]         # cónica por 5 puntos
```

### Curvas especiales

```text
Cardioid[1.5]                           # cardioide r=a(1+cos θ)
Rose[2, 3, 1]                           # rosa r = a*cos(n/d*θ)
Lissajous[3, 4, 1, 1, 90]              # curva de Lissajous
ArchimedeanSpiral[0.5, 0.2, 10]        # espiral de Arquímedes
LogarithmicSpiral[1, 0.2, 10]          # espiral logarítmica
Epicycloid[2, 3]                        # epicicloide
```

### Mapeos complejos

Aplicá cualquier función de variable compleja a un objeto 2D:

```text
Circle[c, 3]
ComplexMapping[1/z, c]                   # invierte el círculo
ComplexMapping[exp(z), c]                # exponencial compleja
ComplexMapping[z^2, c]                   # transformación cuadrática
```

Las singularidades generan automáticamente asíntotas punteadas.

#### Coloración de dominio (domain coloring) acelerada por GPU

La coloración de dominio (estilo cplot) muestra el argumento de `f(z)` como
matiz y `log|f(z)|` como luminosidad. En Grafito 1.1.9-beta la evaluación
de la grilla 2D corre en un compute shader WGSL (`DomainColoringComputePipeline`),
permitiendo hasta 500×500 = **250 000 celdas evaluadas en un solo dispatch GPU**
con fallback automático a CPU si la GPU no está disponible.

```text
DomainColoring[1/z, -2, 2, -2, 2, 400]   # inversión con domain coloring
DomainColoring[z^6+1, -1.5, 1.5, -1.5, 1.5, 400]
DomainColoring[exp(z), -3, 3, -3, 3, 400]
DomainColoring[gamma(z), -3, 3, -3, 3, 400]   # ceros y polos de Γ(z)
```

#### Funciones complejas especiales

Grafito incluye un conjunto amplio de funciones complejas, disponibles tanto
en la línea de comandos como dentro de cualquier expresión:

| Función | Descripción | GPU |
|---------|-------------|:---:|
| `sin`, `cos`, `tan`, `sec`, `csc`, `cot` | Trigonométricas | ✓ |
| `asin`, `acos`, `atan` | Inversas | ✓ |
| `sinh`, `cosh`, `tanh`, `coth` | Hiperbólicas | ✓ |
| `asinh`, `acosh`, `atanh` | Hiperbólicas inversas | ✓ |
| `exp`, `log`/`ln`, `sqrt`, `pow` | Exponenciales | ✓ |
| `conj(z)` | Conjugado `x − iy` | ✓ |
| `re(z)`, `im(z)` | Parte real / imaginaria | ✓ |
| `arg(z)` | Argumento `atan2(im, re)` | ✓ |
| `abs(z)` | Módulo | ✓ |
| `gamma(z)` | Función Gamma (Lanczos + reflexión) | CPU |
| `bessel_j(z)` | Bessel de 1ª clase (series + integral) | CPU |
| `bessel_y(z)` | Bessel de 2ª clase (serie + narmónicos) | CPU |
| `erf(z)` | Función de error (series + asintótica) | CPU |
| `lambert_w(z)` | Lambert W (Newton) | CPU |
| `zeta(z)` | Zeta de Riemann (Borwein + funcional) | CPU |

> **Nota:** las funciones especiales (`gamma`, `zeta`, `erf`, `lambert_w`,
> `bessel_j`, `bessel_y`) se evalúan en CPU. Si la GPU las encuentra en el
> bytecode, devuelve NaN en esa celda — el sampler cae al path CPU
> automáticamente. Esto preserva la velocidad de domain coloring para
> funciones trigonométricas y algebraicas, mientras permite usar las
> especiales en el sampler interactivo.

#### Cuadrantes del plano complejo

```text
Quadrants[-5, 5, -5, 5]
```

Pinta los cuatro cuadrantes Q1-Q4 con colores distintivos, etiquetas
`+Re`/`+Im` y marcas en los ejes. Útil como overlay pedagógico para
entender inversiones, conjugaciones y mapeos conformes.

#### Superficie 3D de módulo complejo

```text
ComplexSurface[z^2 + 1, -3, 3, -3, 3, 50]      # |z² + 1|
ComplexSurface[1/z, -2, 2, -2, 2, 50]          # singularidad en el origen
ComplexSurface[gamma(z), -3, 3, -3, 3, 60]     # polos de Γ(z)
```

Genera una `Surface3DObj` cuya altura es `|f(x + iy)|`. Se renderiza con
el pipeline GPU de superficies paramétricas existente. Alias:
`complexsurface`, `csurface`.

#### Animación trigonométrica

Menú **Herramientas → Animación Trigonométrica** (o `Tool::TrigAnimation`)
abre un panel lateral con:

- **Círculo unitario** con vector radio animado y proyecciones a ejes
  (coseno en verde, seno en azul).
- **Gráfico 2D** sincronizado de `sin(t)`, `cos(t)` o `tan(t)` con
  línea vertical marcando el ángulo actual.
- **Controles** play/pause, slider de velocidad (±3 rad/s), slider
  manual del ángulo (±2π) y etiqueta del valor numérico en tiempo real.

### Solver de restricciones

Grafito resuelve sistemas de restricciones geométricas con el método de Newton usando Jacobianos analíticos. Las restricciones se pueden combinar: por ejemplo, un triángulo con distancias fijas y un ángulo recto.

---

## Canvas 3D

El visor 3D se activa automáticamente cuando creás objetos 3D. Podés orbitar la cámara arrastrando con el botón derecho, hacer zoom con la rueda del ratón, y paneo arrastrando con la herramienta Seleccionar.

### Objetos 3D

Puntos, segmentos, esferas, cubos, pirámides, conos, cilindros, toros, cintas de Moebius, superficies paramétricas, curvas 3D, atractores extraños, hipercubos 4D (proyectados), hiperesferas 4D (proyectadas), campos vectoriales 3D.

### Ejemplos

```text
Sphere[r]                           # esfera de radio r
Cube[l]                             # cubo de lado l
ParametricCurve3D[cos(t), sin(t), t/5, 0, 20]    # hélice
Lorenz[]                            # atractor de Lorenz en 3D
Hypercube[angle1, angle2, angle3]   # teseracto proyectado
```

### Renderizado GPU

El canvas 2D y 3D se renderizan con `wgpu` (WebGPU) usando compute shaders que compilan las expresiones del usuario a bytecode WGSL en tiempo de ejecución. Si la GPU no está disponible, Grafito usa automáticamente el camino de CPU. Esto acelera la evaluación de funciones, curvas implícitas y superficies paramétricas en órdenes de magnitud comparado con la CPU.

Pipelines de cómputo disponibles (`crates/grafito-render/src/`):

- `function_compute` — evalúa `y = f(x)` en una grilla 1D.
- `implicit_compute` — evalúa `f(x, y) = 0` sobre una grilla 2D para marching squares.
- `parametric_compute` — evalúa curvas 2D/3D y superficies paramétricas.
- `vector_compute` — evalúa campos vectoriales 2D.
- `complex_compute` — evalúa expresiones complejas sobre vértices (usado en `ComplexMapping`).
- `domain_coloring_compute` — **nuevo en 1.1.9-beta**: evalúa `f(z)` sobre una grilla 2D y produce colores HSL por celda (hasta 250 000 celdas por dispatch, usado por `DomainColoring[...]`).
- `fill_compute` — pinta regiones rellenas de curvas implícitas.

---

## Arquitectura

```
grafito/
├── crates/
│   ├── grafito-app/         Aplicación de escritorio (eframe/egui) — UI, entrada, orquestación
│   ├── grafito-core/        Modelo de documento, objetos geométricos, índice espacial, restricciones
│   ├── grafito-geometry/    Motor matemático — CAS, estadística, EDOs, fractales, booleanas
│   ├── grafito-render/      Pipeline gráfico wgpu — render 2D/3D, compute shaders, iluminación
│   ├── grafito-ui/          Componentes egui — barra de herramientas, paleta de comandos, temas
│   └── grafito-command/     Procesador de comandos de texto (compartido con FFI)
└── assets/                  Iconos, shaders WGSL
```

---

## Controles

| Acción | Entrada |
|--------|--------|
| Pan 2D | Arrastrar en vacío / Espacio+arrastrar / clic medio |
| Zoom 2D/3D | Rueda del ratón |
| Crear objeto | Clic (Punto: clic simple) |
| Cerrar polígono | Clic derecho (3+ vértices) |
| Cancelar | Clic derecho (1 punto pendiente) / Escape |
| Seleccionar | Clic con herramienta Seleccionar |
| Deseleccionar | Clic en vacío |
| Orbitar 3D | Botón derecho + arrastrar |
| Deshacer / Rehacer | Ctrl+Z / Ctrl+Y |
| Eliminar objeto | Suprimir (con objeto seleccionado) |
| Paleta de comandos | Ctrl+K |
| Herramientas 2D | F1 Seleccionar / F2 Punto / F3 Recta / F4 Círculo / F5 Polígono / F6 Función |
| Herramientas 3D | F7 Punto 3D / F8 Esfera 3D / F9 Cubo 3D |
| Análisis | R Raíz / E Extremo / N Inflexión / Ctrl+Y Intersección Y / Ctrl+A Analizar |
| Grilla y ejes | Shift+L (log), Shift+K (lineal), Shift+J (cuadrícula), G (snap a grilla) |
| Tema claro/oscuro | Ctrl+T |
| Abrir / Guardar | Ctrl+O / Ctrl+S |

---

## Desarrollo

### Verificar todo

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

### Perfilar frames

```bash
cargo run -p grafito-app --features profile -- --profile
# Conectar puffin_viewer a 127.0.0.1:8585
```

### Compilar para Windows desde Linux

```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
# Las dependencias C (GMP/MPFR) requieren compilación cruzada:
# Usá el GitHub Actions workflow 'build-windows.yml' como alternativa
```

---

## Contribuir

1. Hacé un fork
2. Creá tu rama (`git checkout -b feature/nueva-funcionalidad`)
3. Escribí tests, ejecutá `cargo fmt --all && cargo clippy --workspace -- -D warnings && cargo test --workspace`
4. Commiteá con [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`)
5. Abrí un Pull Request

Ver [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](.github/CONTRIBUTING.md) y [SECURITY.md](.github/SECURITY.md) para más detalles.

---

## Licencia

GNU General Public License v3.0 o posterior. Ver [LICENSE](LICENSE).

```
Grafito — Geometría interactiva, álgebra y cálculo acelerados por GPU
Copyright (C) 2025-2026  Diez111

Este programa es software libre: puede redistribuirlo y/o modificarlo
bajo los términos de la GNU General Public License publicada por
la Free Software Foundation, versión 3 o (a su elección) cualquier
versión posterior.
```
