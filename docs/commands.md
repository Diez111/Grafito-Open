# Referencia de Comandos de Grafito

<!-- Generated from crates/grafito-command/src/command_registry.rs; do not edit manually. -->

Esta referencia se genera desde el registro de comandos estable. El parser y sus fallbacks siguen en `commands.rs`; el registro documenta sus metadatos, no reemplaza el despacho.

## Crear

- `Point[(x, y)]`: Crea un punto libre. Mutacion: crea objetos. Riesgo: bajo.
- `Circle[centro, radio]`: Crea una circunferencia. Mutacion: crea objetos. Riesgo: bajo.
- `Polygon[(x1, y1), ...]`: Crea un poligono cerrado. Mutacion: crea objetos. Riesgo: bajo.
- `Function[expr]`: Grafica una funcion explicita. Mutacion: crea objetos. Riesgo: bajo. Alias: `func`.
## Dinamica

- `Animate[]`: Anima un parametro local; sin argumentos crea una fase ciclica. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `Animate[variable]`, `Animate[variable, minimo, maximo, velocidad]`. Alias: `animar`.
## Complejos

- `DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]`: Visualiza fase y módulo de una función compleja en el plano 2D; límites opcionales y una resolución que debe ser un entero literal entre 16 y 300 (200 por defecto). Mutacion: crea objetos. Riesgo: medio. Alias: `domain_coloring`, `dcolor`.
## Crear

- `Piecewise[condicion1, valor1, valor_por_defecto, ...]`: Crea una funcion definida por partes. Mutacion: crea objetos. Riesgo: medio. Alias: `pw`.
- `Contour[f(x, y), xmin, xmax, ymin, ymax, nivel, ...]`: Crea curvas de nivel 2D con uno a dieciseis niveles finitos. Mutacion: crea objetos. Riesgo: alto. Alias: `contourlines`, `contour_lines`.
- `PhasePortrait[dxdt, dydt]`: Crea un retrato de fase 2D. Mutacion: crea objetos. Riesgo: alto. Alias: `phase_portrait`, `phase`.
## Complejos

- `ComplexGrid[expr, xmin, xmax, ymin, ymax, density]`: Visualiza una rejilla compleja transformada; limites y densidad son opcionales. Mutacion: crea objetos. Riesgo: medio. Alias: `complex_grid`, `cgrid`.
## Crear

- `HeatMap[f(x, y), xmin, xmax, ymin, ymax, resolution]`: Crea un mapa de calor 2D; limites y resolucion son opcionales. Mutacion: crea objetos. Riesgo: alto. Alias: `heat_map`, `hmap`.
## Complejos

- `Quadrants[xmin, xmax, ymin, ymax]`: Muestra los cuadrantes del plano complejo con limites opcionales. Mutacion: crea objetos. Riesgo: bajo. Alias: `cuadrantes`.
## Crear

- `Ellipse[(cx, cy), rx, ry]`: Crea una elipse por centro y semiejes. Mutacion: crea objetos. Riesgo: bajo.
- `Parabola[(vx, vy), p]`: Crea una parabola por vertice y parametro. Mutacion: crea objetos. Riesgo: bajo.
- `Hyperbola[(cx, cy), a, b]`: Crea una hiperbola por centro y semiejes. Mutacion: crea objetos. Riesgo: bajo.
- `RegularPolygon[(cx, cy), n, r]`: Crea un poligono regular. Mutacion: crea objetos. Riesgo: bajo. Alias: `regular_polygon`.
- `SampledGraph[expr, range]`: Muestrea y=f(x) en 201 abscisas uniformes de [-range, range] y crea un poligono estatico cerrado con las muestras finitas; no es un lugar geometrico dinamico. Mutacion: crea objetos. Riesgo: medio.
## Dinamica

- `Locus[driver, target]`: Crea un lugar geometrico persistente: registra el objetivo despues de cada actualizacion local valida del driver, sin eventos de puntero ni tiempo. Mutacion: agrega restricciones. Riesgo: medio. Alias: `lugar`.
## Crear

- `ParametricCurve2D[x(t), y(t), t0, t1]`: Crea una curva parametrica 2D. Mutacion: crea objetos. Riesgo: medio. Alias: `parametric_curve_2d`, `param2d`.
- `PolarCurve[r(t), t0, t1]`: Crea una curva polar. Mutacion: crea objetos. Riesgo: medio. Alias: `polar_curve`, `polar`.
- `ImplicitCurve[f(x, y) = c]`: Crea una curva implicita. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `ImplicitCurve[lhs, rhs, relacion]`. Alias: `ImplicitRegion`.
- `VectorField2D[u(x, y), v(x, y)]`: Crea un campo vectorial 2D. Mutacion: crea objetos. Riesgo: alto. Alias: `vector_field_2d`, `vf2d`.
## Construir

- `Perpendicular[punto, recta]`: Crea una recta perpendicular. Mutacion: crea objetos. Riesgo: bajo.
- `Parallel[punto, recta]`: Crea una recta paralela. Mutacion: crea objetos. Riesgo: bajo.
- `Tangent[obj1, obj2]`: Construye o restringe una tangencia segun los argumentos. Mutacion: agrega restricciones. Riesgo: medio. Formas alternativas: `Tangent[centro, radio, punto]`.
- `PerpendicularBisector[(x1, y1), (x2, y2)]`: Crea la mediatriz de dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `AngleBisector[p1, vertice, p2]`: Crea la bisectriz de un angulo. Mutacion: crea objetos. Riesgo: bajo.
- `Midpoint[A, B]`: Crea el punto medio. Mutacion: crea objetos. Riesgo: bajo.
- `Line[(x1, y1), (x2, y2)]`: Crea una recta por dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `Segment[(x1, y1), (x2, y2)]`: Crea un segmento por dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `Vector[(x1, y1), (x2, y2)]`: Crea un vector por dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `Ray[(x1, y1), (x2, y2)]`: Crea una semirrecta por dos puntos. Mutacion: crea objetos. Riesgo: bajo.
## Transformar

- `Translate[punto, (dx, dy)]`: Traslada un objeto. Mutacion: transforma objetos. Riesgo: medio.
- `Rotate[punto, centro, angulo]`: Rota un objeto. Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Rotate[punto, angulo]`.
- `Dilate[punto, factor, centro]`: Aplica una homotecia. Mutacion: transforma objetos. Riesgo: medio.
- `Reflect[obj, punto_a, punto_b]`: Refleja un objeto respecto a un eje. Mutacion: transforma objetos. Riesgo: medio.
## Restricciones

- `Distance[A, B, valor]`: Impone una distancia entre objetos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `dist`.
- `Angle[l1, l2, grados]`: Impone un angulo entre objetos. Mutacion: agrega restricciones. Riesgo: medio.
- `Coincident[A, B]`: Hace coincidir dos puntos. Mutacion: agrega restricciones. Riesgo: medio.
- `Horizontal[obj]`: Fuerza una orientacion horizontal. Mutacion: agrega restricciones. Riesgo: medio.
- `Vertical[obj]`: Fuerza una orientacion vertical. Mutacion: agrega restricciones. Riesgo: medio.
- `EqualLength[s1, s2]`: Iguala longitudes. Mutacion: agrega restricciones. Riesgo: medio. Alias: `equal_length`, `eqlength`.
- `Symmetry[P, Q, eje]`: Impone simetria respecto a un eje. Mutacion: agrega restricciones. Riesgo: medio.
## Conicas

- `EllipseByFoci[F1, F2, P]`: Construye una elipse por focos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `ellipse_by_foci`.
- `ParabolaByFocusDirectrix[F, d]`: Construye una parabola por foco y directriz. Mutacion: agrega restricciones. Riesgo: medio. Alias: `parabola_by_focus_directrix`.
- `HyperbolaByFoci[F1, F2, P]`: Construye una hiperbola por focos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `hyperbola_by_foci`.
- `ConicByFivePoints[A, B, C, D, E]`: Ajusta una conica por cinco puntos. Mutacion: agrega restricciones. Riesgo: alto. Alias: `conic_by_five_points`.
## Booleanas

- `PolygonUnion[poly1, poly2]`: Une dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polyunion`.
- `PolygonIntersection[poly1, poly2]`: Interseca dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polyintersection`.
- `PolygonDifference[poly1, poly2]`: Resta dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polydifference`.
- `PolygonXor[poly1, poly2]`: Calcula la diferencia simetrica. Mutacion: crea objetos. Riesgo: alto. Alias: `polyxor`.
## Expresiones

- `PointExpr[x_expr, y_expr]`: Crea un punto ligado a expresiones. Mutacion: crea objetos. Riesgo: bajo.
- `CircleExpr[centro, radius_expr]`: Crea un circulo con radio ligado a una expresion. Mutacion: crea objetos. Riesgo: bajo.
## CAS

- `Derivative[expr, variable]`: Deriva simbolicamente una expresion. Mutacion: crea objetos. Riesgo: bajo. Alias: `derivada`, `deriv`, `diff`.
- `Integral[expr]`: Calcula una integral simbolica o definida. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Integral[expr, variable]`, `Integral[expr, a, b]`, `Integral[expr, variable, a, b]`. Alias: `integrar`, `int`.
- `Solve[expr, variable, minimo, maximo]`: Resuelve una ecuacion en la variable indicada. Mutacion: crea objetos. Riesgo: medio. Alias: `nsolve`, `resolver`.
- `Limit[expr, variable, punto]`: Estima un limite bilateral finito. Mutacion: solo consulta. Riesgo: medio. Alias: `limite`, `lim`.
- `Factor[expr, variable]`: Factoriza polinomios equivalentes. Mutacion: solo consulta. Riesgo: bajo. Alias: `factorizar`.
- `Expand[expr]`: Expande productos y potencias algebraicas. Mutacion: solo consulta. Riesgo: bajo. Alias: `expandir`.
- `Simplify[expr]`: Simplifica una expresion mediante reglas seguras. Mutacion: solo consulta. Riesgo: bajo. Alias: `simplificar`.
- `Taylor[expr, variable, centro, orden]`: Construye una serie de Taylor finita. Mutacion: crea objetos. Riesgo: medio.
## Analisis

- `Root[f]`: Busca raices de una funcion. Mutacion: crea objetos. Riesgo: medio. Alias: `raiz`, `raices`.
- `Extremum[f]`: Busca extremos locales. Mutacion: crea objetos. Riesgo: medio. Alias: `extremos`, `max`, `min`.
- `Inflection[f]`: Busca puntos de inflexion. Mutacion: crea objetos. Riesgo: medio. Alias: `inflexion`.
- `YIntercept[f]`: Calcula el intercepto con el eje Y. Mutacion: crea objetos. Riesgo: bajo. Alias: `interceptoy`, `intercepto_y`.
- `XIntercept[f]`: Calcula los interceptos con el eje X. Mutacion: crea objetos. Riesgo: medio. Alias: `interceptox`, `intercepto_x`.
- `Intersect[a, b]`: Calcula intersecciones entre curvas. Mutacion: crea objetos. Riesgo: medio. Alias: `interseccion`.
- `Analyze[f]`: Ejecuta el analisis disponible de una funcion. Mutacion: crea objetos. Riesgo: medio. Alias: `analizar`, `analisis`.
## Complejos

- `ComplexMapping[expr_compleja, target]`: Aplica un mapeo complejo a un objetivo. Mutacion: crea objetos. Riesgo: alto. Alias: `complex_mapping`, `mapeocomplejo`.
- `Gauss[expr_compleja, curva]`: Calcula una integral compleja por residuos. Mutacion: crea objetos. Riesgo: alto. Alias: `residuos`, `residue`.
- `ComplexIntegral[expr_compleja, curva]`: Calcula una integral compleja sobre una curva. Mutacion: crea objetos. Riesgo: alto. Alias: `integralcompleja`, `contourintegral`.
## AM1

- `RiemannSum[f, x, a, b, n, metodo]`: Calcula una suma de Riemann. Mutacion: solo consulta. Riesgo: medio.
- `BolzanoCheck[f, x, a, b]`: Verifica condiciones del teorema de Bolzano. Mutacion: solo consulta. Riesgo: medio.
- `LHopital[num, den, x, a, max_steps]`: Aplica pasos de la regla de L'Hopital. Mutacion: solo consulta. Riesgo: medio.
## AM2

- `JacobianMatrix[[f1, f2], [x, y]]`: Calcula una matriz Jacobiana. Mutacion: solo consulta. Riesgo: medio.
- `Hessian[f, [x, y]]`: Calcula una matriz Hessiana. Mutacion: solo consulta. Riesgo: medio.
- `LineIntegralVector[[P, Q], [x(t), y(t)], t, a, b, n]`: Calcula una integral de linea vectorial. Mutacion: solo consulta. Riesgo: alto.
- `TripleIntegral[f, x, a, b, y, c, d, z, e, f, n]`: Calcula una integral triple numerica. Mutacion: solo consulta. Riesgo: alto.
- `Flux[[P, Q, R], superficie, [u, v], u0, u1, v0, v1, n]`: Calcula el flujo de un campo vectorial. Mutacion: solo consulta. Riesgo: alto.
- `GreenTheorem[[P, Q], x, a, b, y, c, d, n]`: Calcula una verificacion del teorema de Green. Mutacion: solo consulta. Riesgo: alto.
- `GaussOstrogradski[[P, Q, R], x, a, b, y, c, d, z, e, f, n]`: Calcula una verificacion de Gauss-Ostrogradski. Mutacion: solo consulta. Riesgo: alto.
## Matrices

- `Determinant[[a, b], [c, d]]`: Calcula un determinante. Mutacion: solo consulta. Riesgo: medio. Alias: `det`.
- `Inverse[[a, b], [c, d]]`: Calcula una matriz inversa. Mutacion: solo consulta. Riesgo: medio. Alias: `inversa`.
- `SolveSystem[A, b]`: Resuelve un sistema lineal. Mutacion: solo consulta. Riesgo: medio. Alias: `linearsolve`, `linsolve`, `sistema`.
- `GaussJordan[A]`: Reduce una matriz por Gauss-Jordan. Mutacion: solo consulta. Riesgo: medio.
- `Cramer[A, b]`: Resuelve un sistema por Cramer. Mutacion: solo consulta. Riesgo: medio.
- `ChangeOfBasis[v, B_from, B_to]`: Cambia coordenadas entre bases. Mutacion: solo consulta. Riesgo: medio.
- `Diagonalization[A]`: Intenta diagonalizar una matriz. Mutacion: solo consulta. Riesgo: alto.
## Probabilidad

- `Normal[mu, sigma]`: Evalua o crea una distribucion normal. Mutacion: solo consulta. Riesgo: bajo.
- `Binomial[n, p, k]`: Evalua una distribucion binomial. Mutacion: solo consulta. Riesgo: bajo.
- `Poisson[lambda, k]`: Evalua una distribucion de Poisson. Mutacion: solo consulta. Riesgo: bajo.
## Estadistica

- `Histogram[{data}, bins]`: Crea un histograma. Mutacion: crea objetos. Riesgo: medio. Alias: `histograma`.
- `ScatterPlot[{xs}, {ys}]`: Crea un grafico de dispersion. Mutacion: crea objetos. Riesgo: medio. Alias: `scatter`.
- `BoxPlot[{data}]`: Crea un diagrama de caja. Mutacion: crea objetos. Riesgo: medio.
- `LinearRegression[{xs}, {ys}]`: Calcula una regresion lineal. Mutacion: crea objetos. Riesgo: medio. Alias: `regression`, `regresion`.
- `DataTable[{xs}, {ys}]`: Crea una tabla local de pares x/y y un gráfico de dispersión enlazado. Mutacion: crea objetos. Riesgo: medio. Alias: `datos`, `tabla`.
- `FitLinear[tabla]`: Ajusta una recta a una tabla local y muestra RMSE y R². Mutacion: crea objetos. Riesgo: medio. Alias: `ajuste lineal`.
- `FitPoly[tabla, grado]`: Ajusta un polinomio de grado elegido a una tabla local. Mutacion: crea objetos. Riesgo: medio. Alias: `ajuste polinomico`.
- `FitExp[tabla]`: Ajusta y = a exp(bx) a una tabla local con y positiva. Mutacion: crea objetos. Riesgo: medio. Alias: `ajuste exponencial`.
- `FitLog[tabla]`: Ajusta y = a ln(x) + b a una tabla local con x positiva. Mutacion: crea objetos. Riesgo: medio. Alias: `ajuste logaritmico`.
- `FitPow[tabla]`: Ajusta y = a x^b a una tabla local con x e y positivas. Mutacion: crea objetos. Riesgo: medio. Alias: `ajuste potencia`.
- `FitSin[tabla]`: Ajusta una senoide local con una búsqueda de frecuencia acotada. Mutacion: crea objetos. Riesgo: alto. Alias: `ajuste sinusoidal`.
- `Mean[{data}]`: Calcula la media. Mutacion: solo consulta. Riesgo: bajo. Alias: `media`.
- `Median[{data}]`: Calcula la mediana. Mutacion: solo consulta. Riesgo: bajo. Alias: `mediana`.
- `StdDev[{data}]`: Calcula el desvio estandar. Mutacion: solo consulta. Riesgo: bajo. Alias: `desviacion`.
- `Correlation[{xs}, {ys}]`: Calcula una correlacion. Mutacion: solo consulta. Riesgo: bajo. Alias: `correlacion`.
## Atractores

- `Lorenz[sigma, rho, beta]`: Crea el atractor de Lorenz. Mutacion: crea objetos. Riesgo: alto.
- `Rossler[a, b, c]`: Crea el atractor de Rossler. Mutacion: crea objetos. Riesgo: alto. Alias: `rossler`, `rossler`.
- `Thomas[pasos]`: Crea el atractor de Thomas. Mutacion: crea objetos. Riesgo: alto. Alias: `butterfly`.
- `Aizawa[a, b, c, d, e, f]`: Crea el atractor de Aizawa. Mutacion: crea objetos. Riesgo: alto.
- `Chen[a, b, c]`: Crea el atractor de Chen. Mutacion: crea objetos. Riesgo: alto.
- `Halvorsen[a, p2, p3, p4]`: Crea el atractor de Halvorsen. Mutacion: crea objetos. Riesgo: alto.
- `Dadras[p, q, r, s, e]`: Crea el atractor de Dadras. Mutacion: crea objetos. Riesgo: alto.
- `Chua[alpha, beta, m0, m1]`: Crea el atractor de Chua. Mutacion: crea objetos. Riesgo: alto.
## Fractales

- `Mandelbrot[max_iter]`: Crea el fractal de Mandelbrot. Mutacion: crea objetos. Riesgo: alto.
- `Julia[cr, ci, max_iter]`: Crea un fractal de Julia. Mutacion: crea objetos. Riesgo: alto.
- `BurningShip[]`: Crea el fractal Burning Ship. Mutacion: crea objetos. Riesgo: alto. Alias: `burning_ship`.
## 4D

- `Hypercube[a1, a2, a3]`: Crea una proyeccion de hipercubo. Mutacion: crea objetos. Riesgo: alto. Alias: `tesseract`.
- `Hypersphere[]`: Crea una proyeccion de hiperesfera. Mutacion: crea objetos. Riesgo: alto.
- `Pentachoron4D[]`: Crea el 5-celda regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Pentachoron4D[scale]`, `Pentachoron4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `fivecell4d`, `5cell4d`.
- `Tesseract4D[]`: Crea el hipercubo regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Tesseract4D[scale]`, `Tesseract4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `hypercube4d`.
- `SixteenCell4D[]`: Crea el 16-celda regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `SixteenCell4D[scale]`, `SixteenCell4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `16cell4d`.
- `TwentyFourCell4D[]`: Crea el 24-celda regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `TwentyFourCell4D[scale]`, `TwentyFourCell4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `24cell4d`.
- `OneTwentyCell4D[]`: Crea el 120-celda regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `OneTwentyCell4D[scale]`, `OneTwentyCell4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `120cell4d`.
- `SixHundredCell4D[]`: Crea el 600-celda regular 4D con escala y seis rotaciones opcionales. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `SixHundredCell4D[scale]`, `SixHundredCell4D[scale, {xy, xz, xw, yz, yw, zw}]`. Alias: `600cell4d`.
- `SimplexND[n]`: Crea un simplex regular en R^n para n entre 3 y 10. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `SimplexND[n, scale]`, `SimplexND[n, scale, {lexicographic-plane angles}]`. Alias: `simplex_nd`.
- `HypercubeND[n]`: Crea un hipercubo regular en R^n para n entre 3 y 10. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `HypercubeND[n, scale]`, `HypercubeND[n, scale, {lexicographic-plane angles}]`. Alias: `hypercube_nd`.
- `CrossPolytopeND[n]`: Crea un politopo cruzado regular en R^n para n entre 3 y 10. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `CrossPolytopeND[n, scale]`, `CrossPolytopeND[n, scale, {lexicographic-plane angles}]`. Alias: `cross_polytope_nd`.
## 3D

- `Point3D[x, y, z]`: Crea un punto 3D. Mutacion: crea objetos. Riesgo: bajo.
- `Segment3D[x1, y1, z1, x2, y2, z2]`: Crea un segmento 3D. Mutacion: crea objetos. Riesgo: bajo.
- `Line3D[x0, y0, z0, dx, dy, dz]`: Crea una recta 3D por punto y direccion o por dos puntos. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Line3D[p1, p2]`. Alias: `line3`, `recta3d`, `recta`.
- `Plane3D[a, b, c, d]`: Crea un plano 3D por ecuacion o por tres puntos. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Plane3D[p1, p2, p3]`. Alias: `plane`, `plano`, `plano3d`.
- `Sphere[x, y, z, radius]`: Crea una esfera 3D. Mutacion: crea objetos. Riesgo: medio.
- `Cube[x, y, z, size]`: Crea un cubo 3D. Mutacion: crea objetos. Riesgo: medio.
- `Tetrahedron[x, y, z, edge]`: Crea un tetraedro regular 3D sólido. Mutacion: crea objetos. Riesgo: medio.
- `Cylinder[x, y, z, radius, height]`: Crea un cilindro 3D vertical. Mutacion: crea objetos. Riesgo: medio.
- `Cone[x, y, z, radius, height]`: Crea un cono 3D vertical. Mutacion: crea objetos. Riesgo: medio.
- `Torus[x, y, z, major_radius, minor_radius]`: Crea un toro 3D. Mutacion: crea objetos. Riesgo: alto.
- `Moebius[radius, width]`: Crea una banda de Moebius 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `mobius`.
- `Curve3D[(x(t), y(t), z(t)), t, tmin, tmax]`: Crea una curva parametrica 3D. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Curve3D[(x(t), y(t), z(t)), tmin, tmax]`.
- `Surface3D[f(x, y), xmin, xmax, ymin, ymax]`: Crea una superficie 3D parametrica o explicita. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]`, `Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]`.
- `ComplexSurface[expr, xmin, xmax, ymin, ymax, resolution]`: Grafica el modulo de una funcion compleja como superficie 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `complexsurface`, `complex_surface`, `csurface`.
- `Extrude[polygon_label, height]`: Extruye un poligono a un solido. Mutacion: crea objetos. Riesgo: alto.
- `VectorField3D[u, v, w]`: Crea un campo vectorial 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `vectorfield`.
## Valores validos

Los comandos de grafica rechazan dominios degenerados, invertidos o no finitos para evitar objetos sin geometria visible.

La vista de Regresión también permite seleccionar explícitamente un CSV/TSV UTF-8 de dos columnas. Sólo se persisten encabezados y pares numéricos finitos; la ruta de origen no se guarda ni se transmite al asistente.

- `ParametricCurve2D[x(t), y(t), t0, t1]` y `PolarCurve[r(t), t0, t1]` requieren `t0 < t1`.
- `Curve3D[(x(t), y(t), z(t)), t0, t1]` y `Curve3D[(x(t), y(t), z(t)), t, t0, t1]` requieren limites finitos con `t0 < t1`.
- `Tetrahedron[x, y, z, edge]` requiere un centro finito y una arista finita estrictamente positiva.
- `Pentachoron4D`, `Tesseract4D`, `SixteenCell4D`, `TwentyFourCell4D`, `OneTwentyCell4D` y `SixHundredCell4D` aceptan `[]`, `[scale]` o `[scale,{xy,xz,xw,yz,yw,zw}]`; la escala predeterminada es 1, los seis angulos predeterminados son 0 y toda escala debe ser finita y estrictamente positiva.
- `SimplexND`, `HypercubeND` y `CrossPolytopeND` aceptan `[n]`, `[n,scale]` o `[n,scale,{angulos}]`; `n` debe ser un entero entre 3 y 10 y la lista contiene exactamente `n(n-1)/2` angulos finitos para los planos lexicograficos `(0,1),(0,2),...,(n-2,n-1)`.
- `ComplexGrid[expr, xmin, xmax, ymin, ymax, density]`, `DomainColoring[expr, xmin, xmax, ymin, ymax, density]` y `HeatMap[expr, xmin, xmax, ymin, ymax, density]` requieren `x_min < x_max` e `y_min < y_max`. Al omitir los limites se usan los valores predeterminados del comando.
- `Surface3D[z = f(x, y), xmin, xmax, ymin, ymax]` requiere `x_min < x_max` e `y_min < y_max`. Las formas paramétricas `Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]` y `Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]` requieren límites finitos ordenados y tres expresiones válidas. Las propuestas que usan `x,y` como parámetros se normalizan a `u,v`; no se pueden mezclar ambos pares en una misma superficie.
- `Contour[expr, xmin, xmax, ymin, ymax, level, ...]` requiere limites ordenados y niveles finitos.
- `SetValue[nombre, valor]` crea de forma explicita una variable ausente y confirma esa creacion con un mensaje visible.
