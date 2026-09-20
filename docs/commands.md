# Referencia de Comandos de Grafito

<!-- Generated from crates/grafito-command/src/command_registry.rs; do not edit manually. -->

Esta referencia se genera desde el registro de comandos estable. El parser y sus fallbacks siguen en `commands.rs`; el registro documenta sus metadatos, no reemplaza el despacho.

## Crear

- `Point[(x, y)]`: Crea un punto libre. Mutacion: crea objetos. Riesgo: bajo. Alias: `punto`.
- `ToPoint[complejo]`: Crea el punto (a, b) desde un complejo: ToPoint["a+bi"]. Mutacion: crea objetos. Riesgo: bajo. Alias: `to_punto`, `a_punto`.
- `Circle[centro, radio]`: Crea una circunferencia. Mutacion: crea objetos. Riesgo: bajo. Alias: `circulo`.
- `Polygon[(x1, y1), ...]`: Crea un poligono cerrado. Mutacion: crea objetos. Riesgo: bajo. Alias: `poligono`.
- `Polyline[P1, P2, ...]`: Crea una polilinea abierta: cadena de segmentos sin cierre ni relleno (minimo 2 puntos, maximo 8192). Mutacion: crea objetos. Riesgo: bajo. Alias: `polilinea`.
- `Function[expr]`: Grafica una funcion explicita. Mutacion: crea objetos. Riesgo: bajo. Alias: `func`, `funcion`.
## Dinámica

- `Animate[]`: Anima un parametro local; sin argumentos crea una fase ciclica. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `Animate[variable]`, `Animate[variable, minimo, maximo, velocidad]`. Alias: `animar`.
## Animaciones

- `GenerateAnimation[template, concepto]`: Genera una animación didáctica (vista previa nativa o Manim) para el concepto dado. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `GenerateAnimation[template]`, `GenerateAnimation[]`.
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
## Dinámica

- `Locus[driver, target]`: Crea un lugar geometrico persistente: registra el objetivo despues de cada actualizacion local valida del driver, sin eventos de puntero ni tiempo. Mutacion: agrega restricciones. Riesgo: medio. Alias: `lugar`.
- `LocusEquation[locus]`: Aproximación por regresión (no exacta) a partir de muestreo de locus; no es eliminación Groebner exacta; genera curva implícita presupuestada con RMSE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `LocusEquation[locus, grado]`. Alias: `locus_equation`, `ecuacionlocus`, `ecuacion_locus`.
## Crear

- `ParametricCurve2D[x(t), y(t), t0, t1]`: Crea una curva parametrica 2D. Mutacion: crea objetos. Riesgo: medio. Alias: `parametric_curve_2d`, `param2d`, `Curve`.
- `PolarCurve[r(t), t0, t1]`: Crea una curva polar. Mutacion: crea objetos. Riesgo: medio. Alias: `polar_curve`, `polar`.
- `ImplicitCurve[f(x, y) = c]`: Crea una curva implicita. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `ImplicitCurve[lhs, rhs, relacion]`. Alias: `ImplicitRegion`.
- `VectorField2D[u(x, y), v(x, y)]`: Crea un campo vectorial 2D. Mutacion: crea objetos. Riesgo: alto. Alias: `vector_field_2d`, `vf2d`.
## Construir

- `Perpendicular[punto, recta]`: Crea una recta perpendicular. Mutacion: crea objetos. Riesgo: bajo.
- `Parallel[punto, recta]`: Crea una recta paralela. Mutacion: crea objetos. Riesgo: bajo. Alias: `paralela`, `paralelo`.
- `Tangent[obj1, obj2]`: Construye o restringe una tangencia segun los argumentos. Mutacion: agrega restricciones. Riesgo: medio. Formas alternativas: `Tangent[centro, radio, punto]`. Alias: `tangente`.
- `PerpendicularBisector[(x1, y1), (x2, y2)]`: Crea la mediatriz de dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `AngleBisector[p1, vertice, p2]`: Crea la bisectriz de un angulo. Mutacion: crea objetos. Riesgo: bajo.
- `Midpoint[A, B]`: Crea el punto medio. Mutacion: crea objetos. Riesgo: bajo. Alias: `punto_medio`, `puntomedio`.
- `Line[(x1, y1), (x2, y2)]`: Crea una recta por dos puntos. Mutacion: crea objetos. Riesgo: bajo. Alias: `linea`.
- `Segment[(x1, y1), (x2, y2)]`: Crea un segmento por dos puntos. Mutacion: crea objetos. Riesgo: bajo. Alias: `segmento`.
- `Vector[(x1, y1), (x2, y2)]`: Crea un vector por dos puntos. Mutacion: crea objetos. Riesgo: bajo.
- `Ray[(x1, y1), (x2, y2)]`: Crea una semirrecta por dos puntos. Mutacion: crea objetos. Riesgo: bajo. Alias: `semirrecta`, `rayo`.
- `MeasureDistance[A, B]`: Crea un texto vivo con la distancia entre dos puntos. Mutacion: crea objetos. Riesgo: bajo. Alias: `medirdistancia`, `distancia`.
## Transformar

- `Translate[punto, (dx, dy)]`: Traslada un objeto. Mutacion: transforma objetos. Riesgo: medio.
- `Rotate[punto, centro, angulo]`: Rota un objeto. Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Rotate[punto, angulo]`.
- `Dilate[punto, factor, centro]`: Aplica una homotecia. Mutacion: transforma objetos. Riesgo: medio.
- `Reflect[obj, punto_a, punto_b]`: [exacto] Refleja un objeto respecto a un eje (linea) o a un circulo (inversion exacta punto/linea/circulo/poligono; circulo que pasa por el centro invierte a recta; [no-soportado] resto de tipos con error honesto). Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Reflect[obj, circulo]`. Alias: `mirror`.
- `Shear[objeto, angulo, eje]`: [exacto] Aplica cizallamiento afin punto/linea/poligono (x' = x + k*y, k = tan(angulo)). [no-soportado] circulo (la imagen real es una elipse, no un circulo) y resto de tipos, con error honesto, sin objeto sustituto. Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Shear[objeto, angulo]`. Alias: `cizalla`, `trasquilacion`.
- `Stretch[objeto, factor, eje]`: Aplica estiramiento afin: x' = factor*x (o y' = factor*y segun eje). Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Stretch[objeto, factor]`. Alias: `estirar`, `estiramiento`.
## Crear

- `FractionText[valor]`: Crea texto con valor fraccionario: FractionText[0.5] -> "1/2". Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `FractionText[valor, punto]`. Alias: `fraccion`, `fraction`.
- `SurdText[valor]`: Crea texto con surd: SurdText[1.414] -> "√2". Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `SurdText[valor, punto]`. Alias: `surd`, `raiztexto`.
## Estadística

- `FillColumn[col, valor]`: Rellena una columna de la hoja iterando filas y escribiendo valor; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillColumn[col, inicio, fin, valor]`. Alias: `fill_column`, `fillcol`.
- `FillCells[rango, valor]`: Rellena un rango rectangular de celdas con un valor; respeta presupuestos de spreadsheet. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillCells[a1, b2, valor]`. Alias: `fill_cells`, `rellenar`.
- `CellRange[a1, b2]`: Resuelve un rango A1:B2 a array de valores evaluados; soporta A1:B2 o A1,B2. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CellRange[rango]`. Alias: `cell_range`, `rango`.
- `FillRow[fila, valor]`: Rellena una fila de la hoja iterando columnas y escribiendo valor; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillRow[fila, inicio, fin, valor]`. Alias: `fill_row`.
- `FillSeries[rango, inicio, paso]`: Autorrelleno con serie lineal (inicio+paso·i) o geométrica (inicio·paso^i) sobre un rango 1D; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillSeries[rango, inicio, paso, modo]`. Alias: `fill_series`, `serie`, `rellenar_serie`.
## Restricciones

- `Distance[A, B, valor]`: Impone una distancia entre objetos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `dist`.
- `Angle[l1, l2, grados]`: Impone un angulo entre objetos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `angulo`.
- `Coincident[A, B]`: Hace coincidir dos puntos. Mutacion: agrega restricciones. Riesgo: medio.
- `Horizontal[obj]`: Fuerza una orientacion horizontal. Mutacion: agrega restricciones. Riesgo: medio.
- `Vertical[obj]`: Fuerza una orientacion vertical. Mutacion: agrega restricciones. Riesgo: medio.
- `EqualLength[s1, s2]`: Iguala longitudes. Mutacion: agrega restricciones. Riesgo: medio. Alias: `equal_length`, `eqlength`.
- `Symmetry[P, Q, eje]`: Impone simetria respecto a un eje. Mutacion: agrega restricciones. Riesgo: medio.
## Cónicas

- `EllipseByFoci[F1, F2, P]`: Construye una elipse por focos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `ellipse_by_foci`.
- `ParabolaByFocusDirectrix[F, d]`: Construye una parabola por foco y directriz. Mutacion: agrega restricciones. Riesgo: medio. Alias: `parabola_by_focus_directrix`.
- `HyperbolaByFoci[F1, F2, P]`: Construye una hiperbola por focos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `hyperbola_by_foci`.
- `ConicByFivePoints[A, B, C, D, E]`: Ajusta una conica por cinco puntos. Mutacion: agrega restricciones. Riesgo: alto. Alias: `conic_by_five_points`.
## Booleanas

- `PolygonUnion[poly1, poly2]`: Une dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polyunion`.
- `PolygonIntersection[poly1, poly2]`: Interseca dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polyintersection`.
- `PolygonDifference[poly1, poly2]`: Resta dos poligonos. Mutacion: crea objetos. Riesgo: alto. Alias: `polydifference`, `Difference`.
- `PolygonXor[poly1, poly2]`: Calcula la diferencia simetrica. Mutacion: crea objetos. Riesgo: alto. Alias: `polyxor`.
## Expresiones

- `PointExpr[x_expr, y_expr]`: Crea un punto ligado a expresiones. Mutacion: crea objetos. Riesgo: bajo.
- `CircleExpr[centro, radius_expr]`: Crea un circulo con radio ligado a una expresion. Mutacion: crea objetos. Riesgo: bajo.
## CAS

- `Derivative[expr, variable]`: Deriva simbolicamente una expresion. Mutacion: crea objetos. Riesgo: bajo. Alias: `derivada`, `deriv`, `diff`.
- `Integral[expr]`: Calcula una integral simbolica o definida. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Integral[expr, variable]`, `Integral[expr, a, b]`, `Integral[expr, variable, a, b]`. Alias: `integrar`, `int`.
- `Solve[expr, variable]`: Resuelve una ecuacion en la variable indicada. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Solve[expr, variable, minimo, maximo]`. Alias: `resolver`, `PlotSolve`.
- `NSolve[expr, variable, minimo, maximo]`: Aproxima una sola raíz numérica en el intervalo dado (1 raíz). Mutacion: crea objetos. Riesgo: medio.
- `SolveNlSystem[eq1, eq2, var1, var2]`: Resuelve un sistema polinómico 2x2 por eliminación (puntos verificados). Mutacion: crea objetos. Riesgo: medio. Alias: `sistema_nolineal`.
- `Limit[expr, variable, punto]`: Estima un limite bilateral finito. Mutacion: solo consulta. Riesgo: medio. Alias: `limite`, `lim`.
- `LimitAbove[expr, variable, punto]`: Estima un límite lateral por la derecha (x→a⁺). Mutacion: solo consulta. Riesgo: medio. Alias: `limite_superior`, `limite_derecho`.
- `LimitBelow[expr, variable, punto]`: Estima un límite lateral por la izquierda (x→a⁻). Mutacion: solo consulta. Riesgo: medio. Alias: `limite_inferior`, `limite_izquierdo`.
- `ParametricDerivative[x(t), y(t), variable]`: Deriva paramétrica dy/dx = (dy/dt)/(dx/dt) simbólicamente. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ParametricDerivative[x(t), y(t)]`. Alias: `derivada_parametrica`, `derivadaParametrica`.
- `Asymptote[expr]`: Calcula asíntota oblicua y = m·x + b con m = lim f/x, b = lim f−m·x. Mutacion: solo consulta. Riesgo: medio. Formas alternativas: `Asymptote[expr, variable]`. Alias: `asintota`, `asíntota`.
- `Groebner[polinomios]`: Base de Groebner por Buchberger acotado (hasta 12 polinomios en 6 variables, 384 S-polinomios; orden por defecto como GroebnerBasis): Groebner[polinomios, variables]. Fuera de cota da error honesto que deriva a Eliminate. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Groebner[polinomios, variables]`.
- `GroebnerLex[polinomios]`: Base de Groebner en orden lexicográfico por Buchberger acotado (hasta 12 polinomios en 6 variables, 384 S-polinomios): GroebnerLex[polinomios, variables]. Fuera de cota da error honesto que deriva a Eliminate. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `GroebnerLex[polinomios, variables]`.
- `GroebnerDegRevLex[polinomios]`: Base de Groebner en orden grevlex por Buchberger acotado (hasta 12 polinomios en 6 variables, 384 S-polinomios): GroebnerDegRevLex[polinomios, variables]. Fuera de cota da error honesto que deriva a Eliminate. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `GroebnerDegRevLex[polinomios, variables]`.
- `Factor[expr, variable]`: Factoriza polinomios por raíces racionales y Kronecker acotado (enteros hasta grado 6); irreducible sobre Q se devuelve tal cual. Mutacion: solo consulta. Riesgo: bajo. Alias: `factorizar`.
- `Expand[expr]`: Expande productos y potencias algebraicas. Mutacion: solo consulta. Riesgo: bajo. Alias: `expandir`.
- `Simplify[expr]`: Simplifica una expresion mediante reglas seguras. Mutacion: solo consulta. Riesgo: bajo. Alias: `simplificar`.
- `TrigExpand[expr]`: Expande sin/cos de suma o resta, doble ángulo 2·u y potencias sin²/cos² a (1±cos(2u))/2; tan, potencias ≠2 y resto exigen motor general. Mutacion: solo consulta. Riesgo: bajo. Alias: `expandirTrig`.
- `TrigCombine[expr]`: Combina productos sin·sin, sin·cos y cos·cos en suma o diferencia (factor constante opcional); fuera de eso informa el límite. Mutacion: solo consulta. Riesgo: bajo. Alias: `combinarTrig`.
- `TrigSimplify[expr]`: Simplifica con pitagóricas (sin²+cos²→1, 1+tan²→sec²) más expansión y combinación en loop acotado de 8 pasos con mejor forma; si oscila, corta por cota. Mutacion: solo consulta. Riesgo: bajo. Alias: `simplificarTrig`.
- `Rationalize[expr]`: Quita radicales del denominador: 1/sqrt(d), a/(k·sqrt(c)) y a/(b±sqrt(c)) por conjugada; cbrt y resto exigen motor general. Mutacion: solo consulta. Riesgo: bajo. Alias: `racionalizar`.
- `Taylor[expr, variable, centro, orden]`: Construye una serie de Taylor finita. Mutacion: crea objetos. Riesgo: medio.
- `CompleteSquare[expr, variable]`: Completa cuadrado: convierte a*x^2+b*x+c a a*(x+b/2a)^2 + (c - b^2/4a). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CompleteSquare[expr]`. Alias: `complete_square`, `completarCuadrado`, `completar_cuadrado`.
- `PrimeFactors[n]`: Factoriza un entero n (2 <= n <= 1e12) en primos por trial division. Mutacion: solo consulta. Riesgo: bajo. Alias: `prime_factors`, `factoresPrimos`, `factores_primos`.
- `IFactor[expr]`: Factorización entera: si es entero usa PrimeFactors, si es polinomio extrae contenido entero y lo factoriza. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `IFactor[expr, variable]`. Alias: `ifactorizar`, `factorEntero`, `factor_entero`.
- `CFactor[expr]`: Factorización compleja: lineales/cuadráticas con raíces complejas conjugadas + raíces racionales grado>2. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CFactor[expr, variable]`. Alias: `cfactorizar`, `factorComplejo`, `factor_complejo`.
- `CIFactor[expr]`: Factorización gaussiana: contenido entero vía PrimeFactors + resto en complejos vía CFactor. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CIFactor[expr, variable]`. Alias: `cifactorizar`, `factorGaussiano`, `factor_gaussiano`.
- `PartialFractions[expr]`: Fracciones parciales: denominador factorizable en lineales + cuadráticas irreducibles, grado 2..=6, fracción propia. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `PartialFractions[expr, variable]`. Alias: `fracciones_parciales`, `fraccionesParciales`, `partial_fractions`.
- `Assume[predicado]`: Almacena hipótesis como x>0 (positive), x!=0 (nonzero), x real/integer; guarda en Document.variables_assumptions. Mutacion: solo consulta. Riesgo: bajo. Alias: `asumir`, `suponer`, `supone`.
## Análisis

- `Root[f]`: Busca raices de una funcion. Mutacion: crea objetos. Riesgo: medio. Alias: `raiz`, `raices`.
- `Extremum[f]`: Busca extremos locales. Mutacion: crea objetos. Riesgo: medio. Alias: `extremos`, `max`, `min`.
- `Inflection[f]`: Busca puntos de inflexion. Mutacion: crea objetos. Riesgo: medio. Alias: `inflexion`.
- `YIntercept[f]`: Calcula el intercepto con el eje Y. Mutacion: crea objetos. Riesgo: bajo. Alias: `interceptoy`, `intercepto_y`.
- `XIntercept[f]`: Calcula los interceptos con el eje X. Mutacion: crea objetos. Riesgo: medio. Alias: `interceptox`, `intercepto_x`.
- `Intersect[a, b]`: [exacto] Calcula intersecciones entre curvas 2D. [no-soportado] pares 3D sin solver (esfera-cubo, recta-cubo, resto de poliedros) con error honesto UnsupportedIntersection, sin objeto sustituto. Mutacion: crea objetos. Riesgo: medio. Alias: `interseccion`.
- `Analyze[f]`: Ejecuta el analisis disponible de una funcion. Mutacion: crea objetos. Riesgo: medio. Alias: `analizar`, `analisis`.
- `FunctionStudy[f]`: Recorrido visual de f: ceros, extremos, AV y tabla de signos; marca puntos en el canvas. Mutacion: crea objetos. Riesgo: medio. Alias: `estudiofuncion`, `estudio`.
## Complejos

- `ComplexMapping[expr_compleja, target]`: Aplica un mapeo complejo a un objetivo (sin target usa el disco unidad I, creado si falta; con target, el objeto debe existir). Mutacion: crea objetos. Riesgo: alto. Alias: `complex_mapping`, `mapeocomplejo`.
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

- `Determinant[[a, b], [c, d]]`: Calcula el determinante; con entradas decimales exactas y hasta 32×32 usa aritmética racional exacta. Mutacion: solo consulta. Riesgo: medio. Alias: `det`.
- `Inverse[[a, b], [c, d]]`: Calcula la inversa; con entradas decimales exactas y hasta 32×32 usa aritmética racional exacta. Mutacion: solo consulta. Riesgo: medio. Alias: `inversa`.
- `SolveSystem[A, b]`: Resuelve un sistema lineal. Mutacion: solo consulta. Riesgo: medio. Alias: `linearsolve`, `linsolve`, `sistema`.
- `GaussJordan[A]`: Reduce una matriz por Gauss-Jordan. Mutacion: solo consulta. Riesgo: medio.
- `Cramer[A, b]`: Resuelve un sistema por Cramer. Mutacion: solo consulta. Riesgo: medio.
- `ChangeOfBasis[v, B_from, B_to]`: Cambia coordenadas entre bases. Mutacion: solo consulta. Riesgo: medio.
- `Diagonalization[A]`: Intenta diagonalizar una matriz. Mutacion: solo consulta. Riesgo: alto.
- `Eigenvalues[A]`: Autovalores (reales y complejos) vía SymmetricEigen/complex_eigenvalues; matriz cuadrada. Mutacion: solo consulta. Riesgo: bajo. Alias: `autovalores`, `eigen_valores`.
- `Eigenvectors[A]`: Autovectores reales (simétrica) u honestos si el par complejo no admite vector real; matriz cuadrada. Mutacion: solo consulta. Riesgo: bajo. Alias: `autovectores`, `eigen_vectores`.
## Probabilidad

- `Normal[mu, sigma]`: Evalua o crea una distribucion normal. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Normal[mu, sigma, x]`. Alias: `normaldist`.
- `Binomial[n, p, k]`: Evalua una distribucion binomial. Mutacion: solo consulta. Riesgo: bajo. Alias: `binomialdist`.
- `Poisson[lambda, k]`: Evalua una distribucion de Poisson. Mutacion: solo consulta. Riesgo: bajo.
- `Uniform[a, b]`: Uniforme U(a,b): PDF 1/(b-a) y CDF; con 2 args evalúa en x=(a+b)/2. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Uniform[a, b, x]`. Alias: `uniforme`, `uniform_distribution`.
- `Exponential[lambda]`: Exponencial Exp(λ): PDF λ·exp(-λx) y CDF 1-exp(-λx) para x≥0, λ>0. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Exponential[lambda, x]`. Alias: `exponencial`, `exponential_distribution`.
- `ChiSquared[df]`: Chi-cuadrado χ²(k): PDF y CDF vía gamma regularizada; k>0. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ChiSquared[df, x]`. Alias: `chi_cuadrado_dist`, `dist_chi2`.
## Estadística

- `Histogram[{data}, bins]`: Crea un histograma. Mutacion: crea objetos. Riesgo: medio. Alias: `histograma`.
- `BarChart[{data}]`: Crea un gráfico de barras por categoría (una barra por dato) desde lista o rango de planilla DataTable (tabla usa la columna y; tabla.xs/.ys elige columna). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `BarChart[tabla]`. Alias: `barras`, `bar`.
- `PieChart[{data}]`: Crea un gráfico de torta proporcional (valores no negativos con total positivo) desde lista o rango de planilla DataTable (tabla usa la columna y; tabla.xs/.ys elige columna). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `PieChart[tabla]`. Alias: `torta`, `pie`.
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
- `FitLogistic[tabla]`: Ajusta a/(1+b*exp(-c*x)) con Gauss-Newton acotado MAX_ITER 100 y tolerancia 1e-6; genera función y métricas RMSE/R². Mutacion: crea objetos. Riesgo: medio. Alias: `fit_logistic`, `logistica`, `ajuste logistico`.
- `FitGrowth[tabla]`: Ajusta a*exp(b*x) con Gauss-Newton acotado MAX_ITER 100 y tolerancia 1e-6. Mutacion: crea objetos. Riesgo: medio. Alias: `fit_growth`, `crecimiento`, `ajuste crecimiento`.
- `FitImplicit[tabla, expr]`: Ajuste implícito genérico Gauss-Newton: FitImplicit[tabla, exprConParams, a0, b0, ...] minimiza y - expr(x; params). Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `FitImplicit[tabla, expr, a0, b0, c0]`. Alias: `fit_implicit`, `implicit_fit`, `ajuste implicito`.
- `Mean[{data}]`: Calcula la media. Mutacion: solo consulta. Riesgo: bajo. Alias: `media`.
- `Median[{data}]`: Calcula la mediana. Mutacion: solo consulta. Riesgo: bajo. Alias: `mediana`.
- `StdDev[{data}]`: Calcula el desvio estandar. Mutacion: solo consulta. Riesgo: bajo. Alias: `desviacion`, `SampleSD`, `desvio_muestral`.
- `Correlation[{xs}, {ys}]`: Calcula una correlacion. Mutacion: solo consulta. Riesgo: bajo. Alias: `correlacion`, `CorrelationCoefficient`.
## Probabilidad

- `InverseNormal[p]`: Cuantil normal: InverseNormal[p, mu, sigma] (p en (0,1), sigma>0); con un arg usa N(0,1). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `InverseNormal[p, mu, sigma]`. Alias: `inverse_normal`, `cuantilnormal`, `cuantil_normal`.
- `InverseT[p, df]`: Cuantil t-Student: InverseT[p, df] (p en (0,1), df>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_t`, `cuantilt`, `cuantil_t`, `InverseTDistribution`.
- `InverseChiSquared[p, df]`: Cuantil chi-cuadrado: InverseChiSquared[p, df] (p en (0,1), df>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_chi_squared`, `inversachicuadrado`, `cuantilchicuadrado`.
- `InverseF[p, df1, df2]`: Cuantil F de Fisher: InverseF[p, df1, df2] (p en (0,1), df1>0, df2>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_f`, `cuantilf`, `cuantil_f`, `InverseFDistribution`.
- `InverseExponential[p, lambda]`: Cuantil exponencial cerrado: -ln(1-p)/λ (p en (0,1), λ>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_exponential`, `cuantilexponencial`, `cuantil_exponencial`.
- `InverseUniform[p, a, b]`: Cuantil uniforme cerrado: a+p·(b-a) (p en [0,1], a<b). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_uniform`, `cuantiluniforme`, `cuantil_uniforme`.
## Estadística

- `FrequencyTable[{datos}]`: Tabla de frecuencias: FrequencyTable[{datos}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `frequency_table`, `frecuencia`, `tabl frecuencias`.
- `StemPlot[{datos}]`: Diagrama tallo-hoja: StemPlot[{datos}] texto. Mutacion: solo consulta. Riesgo: bajo. Alias: `stem_plot`, `stemleaf`, `tallo_hoja`, `diagrama_tallo`.
- `ResidualPlot[{xs}, {ys}]`: Residuos de regresión lineal: ResidualPlot[{xs}, {ys}] o ResidualPlot[tabla]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ResidualPlot[tabla]`. Alias: `residual_plot`, `grafico_residuos`.
- `StepGraph[{xs}, {ys}]`: Gráfico escalonado de datos: StepGraph[{xs}, {ys}] o StepGraph[tabla] o StepGraph[{ys}]. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `StepGraph[datos]`. Alias: `grafico_escalon`, `escalon`.
- `StickGraph[{xs}, {ys}]`: Bastones verticales desde y=0: StickGraph[{xs}, {ys}] o StickGraph[tabla] o StickGraph[{ys}]. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `StickGraph[datos]`. Alias: `grafico_bastones`, `bastones`.
- `LineGraph[{xs}, {ys}]`: Poligonal por los puntos ordenados: LineGraph[{xs}, {ys}] o LineGraph[tabla] o LineGraph[{ys}]. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `LineGraph[datos]`. Alias: `grafico_lineas`, `poligonal`.
- `NormalQuantilePlot[datos]`: QQ-plot normal: cuantiles teóricos N(0,1) vs datos ordenados. NormalQuantilePlot[datos]. Mutacion: crea objetos. Riesgo: bajo. Alias: `qqplot`, `qq_plot`, `grafico_cuantil`.
- `TTest[{datos}, mu0]`: Prueba t de una muestra: TTest[{datos}, mu0]. Mutacion: solo consulta. Riesgo: bajo. Alias: `t_test`, `prueba_t`.
- `TTest2[{a}, {b}]`: Prueba t de dos muestras independientes: TTest2[{a}, {b}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `t_test2`, `prueba_t2`.
- `TTestPaired[{a}, {b}]`: Prueba t pareada: TTestPaired[{antes}, {despues}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `ttest_paired`, `t_paired`, `prueba_t_pareada`, `ttestpareado`.
- `ZTest[{datos}, mu0, sigma]`: Prueba z de una muestra con sigma conocido: ZTest[{datos}, mu0, sigma]. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_test`, `prueba_z`.
- `ZTest2[{a}, {b}, sigma1, sigma2]`: Prueba z de dos muestras con sigmas poblacionales conocidos: ZTest2[{a}, {b}, sigma1, sigma2]. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_test2`, `prueba_z2`.
- `FTest[{a}, {b}]`: Prueba F de igualdad de varianzas (bilateral): FTest[{a}, {b}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `f_test`, `prueba_f`.
- `ChiSqTest[{obs}, {esp}]`: Prueba chi-cuadrado de bondad de ajuste: ChiSqTest[{obs}, {esp}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `chi2test`, `prueba_chi2`, `chi_cuadrado`, `ChiSquaredTest`.
- `ANOVA[{g1}, {g2}]`: ANOVA de un factor: ANOVA[{g1}, {g2}, ...]. Mutacion: solo consulta. Riesgo: bajo. Alias: `anova_oneway`.
## Financiera

- `Rate[nper, pmt, pv, fv]`: Calcula la tasa periodica (tipo 0=anual) resolviendo TVM con exp/log; 4-5 args. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Rate[nper, pmt, pv, fv, tipo]`. Alias: `tasa`, `tipo`.
- `Nper[rate, pmt, pv, fv]`: Calcula numero de periodos via TVM con exp/log; usa log((pmt*(1+r*tipo)-fv*r)/(pmt*(1+r*tipo)+pv*r))/log(1+r). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Nper[rate, pmt, pv, fv, tipo]`. Alias: `n_per`, `periodos`, `plazo`.
- `Pmt[rate, nper, pv, fv]`: Calcula el pago periodico TVM; 4-5 args con tipo 0/1. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Pmt[rate, nper, pv, fv, tipo]`. Alias: `pago`, `cuota`, `Payment`.
- `PV[rate, nper, pmt, fv]`: Calcula valor presente TVM; usa exp/log para (1+rate)^nper. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `PV[rate, nper, pmt, fv, tipo]`. Alias: `va`, `valoractual`, `presentvalue`.
- `FV[rate, nper, pmt, pv]`: Calcula valor futuro TVM; usa exp/log para (1+rate)^nper. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `FV[rate, nper, pmt, pv, tipo]`. Alias: `vf`, `valorfuturo`, `futurevalue`.
## Atractores

- `Lorenz[sigma, rho, beta]`: Crea el atractor de Lorenz. Mutacion: crea objetos. Riesgo: alto.
- `Rossler[a, b, c]`: Crea el atractor de Rossler. Mutacion: crea objetos. Riesgo: alto.
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
- `Dodecahedron[x, y, z, edge]`: Dodecaedro regular centrado en (x,y,z) con arista dada; malla platonic_mesh. Mutacion: crea objetos. Riesgo: medio. Alias: `dodecaedro`.
- `Icosahedron[x, y, z, edge]`: Icosaedro regular centrado en (x,y,z) con arista dada; malla platonic_mesh. Mutacion: crea objetos. Riesgo: medio. Alias: `icosaedro`.
- `Octahedron[x, y, z, edge]`: Octaedro regular centrado en (x,y,z) con arista dada; 6 vértices axiales. Mutacion: crea objetos. Riesgo: medio. Alias: `octaedro`.
- `InfiniteCone[ax, ay, az, dx, dy, dz, angle_deg]`: Cono infinito por ápice y dirección, semiángulo en grados; render clipado honesto ±50. Mutacion: crea objetos. Riesgo: medio. Alias: `cono_infinito`, `conoinfinito`.
- `InfiniteCylinder[x, y, z, dx, dy, dz, radius]`: Cilindro infinito por punto base y dirección con radio; render clipado honesto ±50. Mutacion: crea objetos. Riesgo: medio. Alias: `cilindro_infinito`, `cilindroinfinito`.
- `Pyramid[x, y, z, base_size, height]`: Crea una piramide 3D de base cuadrada (base en (x,y,z), apice en (x,y+h,z)). Mutacion: crea objetos. Riesgo: medio. Alias: `piramide`.
- `Torus[x, y, z, major_radius, minor_radius]`: Crea un toro 3D. Mutacion: crea objetos. Riesgo: alto.
- `Moebius[radius, width]`: Crea una banda de Moebius 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `mobius`.
- `Curve3D[(x(t), y(t), z(t)), t, tmin, tmax]`: Crea una curva parametrica 3D. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Curve3D[(x(t), y(t), z(t)), tmin, tmax]`.
- `Surface3D[f(x, y), xmin, xmax, ymin, ymax]`: Crea una superficie 3D parametrica o explicita. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]`, `Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]`.
- `ComplexSurface[expr, xmin, xmax, ymin, ymax, resolution]`: Grafica el modulo de una funcion compleja como superficie 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `complex_surface`, `csurface`.
- `Extrude[polygon_label, height]`: Extruye un poligono a un solido. Mutacion: crea objetos. Riesgo: alto.
- `VectorField3D[u, v, w]`: Crea un campo vectorial 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `vectorfield`.
- `Prism[poligono, altura]`: Crea un prisma extruyendo un polígono base por un vector (altura en Z o dx,dy,dz). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Prism[poligono, dx, dy, dz]`. Alias: `prisma`.
- `Net[poliedro]`: Genera el desarrollo 2D de un poliedro (Cube/Tetrahedron/Pyramid/Prism vía PolyhedronNet::unfold; persiste una cara = un polígono 2D). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Net[poliedro, escala]`. Alias: `desarrollo`, `desplegado`, `unwrap`.
- `Quadric[a, b, c, d, e, f, g, h, i, j]`: Crea una cuádrica general a*x²+b*y²+c*z²+d*xy+e*yz+f*zx+g*x+h*y+i*z+j=0. Mutacion: crea objetos. Riesgo: medio. Alias: `cuadrica`, `cuádrica`.
- `ImplicitSurface[expr, x0, x1, y0, y1, z0, z1, res]`: Crea una superficie implícita F(x,y,z)=0 en la caja dada (marching-tetra, res 8..=32, 16 por defecto). Mutacion: crea objetos. Riesgo: medio. Alias: `superficieimplicita`, `implicitsurface3d`.
- `Intersection3D[a, b]`: [exacto] Intersecciones 3D: Plano-Plano, Recta-Plano, Recta-Recta, Plano-Esfera (curva parametrica real) o Plano-Cubo (poligono ortografico real). [no-soportado] resto (esfera-cubo, recta-cubo, demas poliedros) con error honesto UnsupportedIntersection, sin objeto sustituto. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Intersection3D[a, b, c]`. Alias: `intersect3d`, `interseccion3d`, `intersección3d`.
- `Vista3D[vista]`: Pide la vista del canvas 3D: perspectiva (orbital) o alzado/planta/perfil (ortográficas vía OrthoView del cerebro). Solo consulta: no crea ni muta objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `vista`, `view3d`.
## Crear

- `Arc[centro, radio, inicio, fin]`: Crea un arco por centro/radio/ángulos o por tres puntos. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Arc[P1, P2, P3]`. Alias: `arco`.
- `Sector[centro, radio, angulo]`: Crea un sector circular con relleno. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Sector[centro, radio, inicio, fin]`.
- `Semicircle[centro, radio]`: Crea un semicírculo por centro/radio o por tres puntos. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Semicircle[P1, P2, P3]`. Alias: `semicirculo`.
- `BezierCurve[P1, P2, ...]`: Crea una curva de Bézier por 2..64 puntos de control. Mutacion: crea objetos. Riesgo: medio. Alias: `bezier`, `bezier_curve`.
- `Spline[P1, P2, ...]`: Crea una spline Catmull-Rom por 2..64 puntos. Mutacion: crea objetos. Riesgo: medio.
## Construir

- `Compasses[centro, punto]`: Traza un círculo con compás: centro y punto o radio. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Compasses[centro, radio]`. Alias: `compass`, `compas`.
- `Incircle[A, B, C]`: Crea el incírculo de un triángulo ABC. Mutacion: crea objetos. Riesgo: medio. Alias: `incirculo`.
- `Circumcircle[A, B, C]`: Crea el circuncírculo de un triángulo ABC. Mutacion: crea objetos. Riesgo: medio. Alias: `circuncirculo`.
## Discreta

- `ConvexHull[puntos]`: Calcula la envolvente convexa de un conjunto de puntos con monotone chain; respeta MAX_POLYGON_VERTICES 8192 y MAX_DISCRETE_COUNT 10000. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `ConvexHull[{p1, p2, ...}]`. Alias: `convex_hull`, `envolventeconvexa`, `envolvente`.
- `DelaunayTriangulation[puntos]`: Triangulación de Delaunay real con predicados exactos (círculo vacío); crea un polígono por triángulo. Hasta 8192 puntos; duplicados o colineales dan error honesto. Mutacion: crea objetos. Riesgo: medio. Alias: `delaunay`, `triangulaciondelaunay`.
- `Voronoi[puntos]`: Diagrama de Voronoi dual de la Delaunay real, con celdas recortadas a la envolvente de los sitios; crea un polígono por celda. Hasta 8192 puntos; duplicados o colineales dan error honesto. Mutacion: crea objetos. Riesgo: medio. Alias: `cellsvoronoi`, `diagramaVoronoi`.
- `MinimumSpanningTree[puntos]`: Árbol de expansión mínima por Prim euclídeo O(n²); crea segmentos entre puntos. Mutacion: crea objetos. Riesgo: medio. Alias: `mst`, `arbolminimo`, `kruskal`.
- `TravelingSalesman[puntos]`: Tour del viajante aproximado por vecino más cercano (greedy) empezando en el primer punto. Mutacion: crea objetos. Riesgo: medio. Alias: `tsp`, `viajante`, `travellingsalesman`.
- `ShortestDistance[punto, objeto]`: Distancia euclídea mínima entre un punto y un objeto (punto/segmento/círculo/polígono). Valida finitud y límites. Mutacion: solo consulta. Riesgo: bajo. Alias: `distanciaminima`, `closestdistance`, `distanciamínima`.
- `UnitPairs[puntos]`: Cuenta pares a distancia 1 en un conjunto de puntos (TOPP 39 f2); informa el conteo sin crear objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `unit_pairs`, `paresunitarios`.
- `DistinctDistances[puntos]`: Cuenta distancias distintas en un conjunto de puntos (TOPP 39 g2); informa el conteo sin crear objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `distinct_distances`, `distanciasdistintas`.
- `UnitGraphEdges[puntos]`: Lista cuántas aristas tiene el grafo unit-distance del conjunto (TOPP 57); informa el conteo sin crear objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `unit_graph_edges`, `aristasunitarias`.
- `ChromaticCheck[puntos, k]`: Verifica por backtracking si el grafo unit-distance es k-coloreable (n<=24); si es más grande indica exportar DIMACS. Mutacion: solo consulta. Riesgo: bajo. Alias: `chromatic_check`, `chequeocromatico`.
- `HalvingEdges[puntos]`: Cuenta halving edges no dirigidas de un conjunto de puntos (TOPP 7, n par); informa el conteo sin crear objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `halving_edges`, `aristashalving`.
- `EmptyTriangle[puntos]`: Informa si el conjunto contiene un triángulo vacío (sin puntos dentro); informa sí/no sin crear objetos. Mutacion: solo consulta. Riesgo: bajo. Alias: `empty_triangle`, `triangulovacio`.
- `Topp39Scan[semilla, n]`: Corrida reproducible TOPP 39: genera n puntos con semilla, mide pares unitarios y distancias distintas, devuelve el registro con hash. Mutacion: solo consulta. Riesgo: bajo. Alias: `topp39_scan`, `barridotopp39`.
## Lista

- `List[elementos]`: Crea una lista persistible con etiqueta: List[{1, 2, {3}}]. Referenciable por Element/Zip/Sort y el resto de la familia. Mutacion: crea objetos. Riesgo: bajo. Alias: `lista`.
- `Sequence[expr, var, start, end]`: Genera lista {expr(var=start)...expr(var=end)} evaluando expr con var entera; valida MAX_ARRAY_LENGTH 200k y MAX_DISCRETE_COUNT 10k. Mutacion: solo consulta. Riesgo: bajo. Alias: `seq`, `secuencia`.
- `SequenceLive[expr, var, start, end]`: Secuencia viva: crea DataTable con binding variable_meta y re-evalúa automáticamente al cambiar variables (dependencia registrada). Mutacion: crea objetos. Riesgo: bajo. Alias: `secuenciaviva`, `seqviva`, `viva`.
- `Zip[list1, list2]`: Empareja dos listas en lista de pares {{a1,b1},…}; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `emparejar`, `cremallera`.
- `Flatten[list]`: Aplana un nivel de anidamiento {{1,2},{3,4}}→{1,2,3,4}; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `aplanar`, `aplanado`.
- `Sort[list]`: Ordena ascendentemente una lista plana numérica; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `ordenar`, `orden`.
- `Reverse[list]`: Invierte el orden de una lista; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `invertir`, `reversa`.
- `Join[list1, list2]`: Concatena dos listas; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `unir`, `concat`, `concatenar`.
- `Append[list, elem]`: Añade un elemento al final de la lista; valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `anexar`, `agregar`.
- `First[list]`: Primer elemento de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `primero`, `head`.
- `Last[list]`: Último elemento de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `ultimo`, `último`, `tail`.
- `Take[list, n]`: Primeros n elementos de la lista; valida 0≤n≤len y MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `tomar`, `coger`.
- `KeepIf[list, predicado]`: Filtra con predicado simple sobre x (ej x>2); valida MAX_ARRAY_LENGTH. Mutacion: solo consulta. Riesgo: bajo. Alias: `keep_if`, `filtrar`, `selectif`, `filter`.
- `CountIf[list, predicado]`: Cuenta elementos que cumplen predicado simple sobre x; valida longitud. Mutacion: solo consulta. Riesgo: bajo. Alias: `count_if`, `contarsi`, `contar_si`.
- `Element[lista, n]`: Elemento n-ésimo (1-based) de una lista; error honesto si está fuera de rango. Mutacion: solo consulta. Riesgo: bajo. Alias: `elemento`.
- `Unique[lista]`: Únicos ordenados de una lista numérica (orden total determinista). Mutacion: solo consulta. Riesgo: bajo. Alias: `unico`.
- `IterationList[f, var, semilla, n]`: Itera f sobre var desde la semilla n veces: {f(s), f(f(s)), …}. Mutacion: solo consulta. Riesgo: bajo. Alias: `iteracion`.
- `Union[a, b]`: Unión de dos listas como conjuntos, orden determinista. Mutacion: solo consulta. Riesgo: bajo. Alias: `unir_listas`.
- `Intersection[a, b]`: Intersección de dos listas como conjuntos, orden determinista. Mutacion: solo consulta. Riesgo: bajo. Alias: `intersecar`.
- `Insert[lista, pos, valor]`: Inserta un valor en la posición 1-based (1..=len+1). Mutacion: solo consulta. Riesgo: bajo. Alias: `insertar`.
- `Remove[lista, pos]`: Quita el elemento en la posición 1-based. Mutacion: solo consulta. Riesgo: bajo. Alias: `quitar`.
- `IndexOf[lista, valor]`: Primera posición 1-based de un valor (igualdad exacta). Mutacion: solo consulta. Riesgo: bajo. Alias: `indice`.
- `Map[f, lista]`: Aplica la expresión con x ligada a cada elemento. Mutacion: solo consulta. Riesgo: bajo. Alias: `mapear`.
- `Shuffle[lista]`: Permutación determinista (semilla = hash de versión+args). Mutacion: solo consulta. Riesgo: bajo. Alias: `mezclar`.
- `Sample[lista, k]`: Muestra k elementos sin reposición, determinista. Mutacion: solo consulta. Riesgo: bajo. Alias: `muestrear`.
- `RandomElement[lista]`: Un elemento uniforme de la lista, determinista. Mutacion: solo consulta. Riesgo: bajo. Alias: `elemento_azar`.
- `RandomDiscrete[min, max]`: Entero uniforme en [min, max], determinista. Mutacion: solo consulta. Riesgo: bajo. Alias: `entero_azar`.
- `ListMin[lista]`: Mínimo de una lista numérica. Mutacion: solo consulta. Riesgo: bajo. Alias: `list_min`, `min_lista`.
- `ListMax[lista]`: Máximo de una lista numérica. Mutacion: solo consulta. Riesgo: bajo. Alias: `list_max`, `max_lista`.
- `Sum[lista]`: Suma de una lista numérica. Mutacion: solo consulta. Riesgo: bajo. Alias: `suma`.
- `Product[lista]`: Producto de una lista numérica. Mutacion: solo consulta. Riesgo: bajo. Alias: `producto`.
## Estadística

- `Covariance[xs, ys]`: Covarianza muestral (÷n−1) de dos listas del mismo largo. Mutacion: solo consulta. Riesgo: bajo. Alias: `covarianza`.
- `RSquare[xs, ys]`: R² de la regresión lineal de ys sobre xs. Mutacion: solo consulta. Riesgo: bajo. Alias: `r_cuadrado`.
- `Spearman[xs, ys]`: Correlación de rangos de Spearman (empates promediados). Mutacion: solo consulta. Riesgo: bajo. Alias: `coef_spearman`.
- `TiedRank[lista]`: Rangos promedio 1-based (empates promediados). Mutacion: solo consulta. Riesgo: bajo. Alias: `rangos_empatados`.
- `OrdinalRank[lista]`: Rangos 1..n (empates por orden de aparición). Mutacion: solo consulta. Riesgo: bajo. Alias: `rangos_ordinales`.
- `MAD[lista]`: Desviación absoluta mediana: mediana(|x−mediana|). Mutacion: solo consulta. Riesgo: bajo. Alias: `desv_mediana`.
- `Quartile1[lista]`: Primer cuartil (interpolación lineal). Mutacion: solo consulta. Riesgo: bajo. Alias: `cuartil1`.
- `Quartile3[lista]`: Tercer cuartil (interpolación lineal). Mutacion: solo consulta. Riesgo: bajo. Alias: `cuartil3`.
- `Percentile[lista, p]`: Percentil p en [0,100] (interpola como quantile). Mutacion: solo consulta. Riesgo: bajo. Alias: `percentil`.
- `SDX[xs]`: Desvío poblacional (÷n) de las abscisas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SDX[xs, ys]`. Alias: `desv_pob_x`.
- `SDY[ys]`: Desvío poblacional (÷n) de las ordenadas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SDY[xs, ys]`. Alias: `desv_pob_y`.
- `SampleSDX[xs]`: Desvío muestral (÷n−1) de las abscisas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SampleSDX[xs, ys]`. Alias: `desv_muestral_x`.
- `SampleSDY[ys]`: Desvío muestral (÷n−1) de las ordenadas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SampleSDY[xs, ys]`. Alias: `desv_muestral_y`.
- `MeanX[xs]`: Media de las abscisas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `MeanX[xs, ys]`. Alias: `media_x`.
- `MeanY[ys]`: Media de las ordenadas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `MeanY[xs, ys]`. Alias: `media_y`.
- `SigmaXX[xs]`: Suma Σx² de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `suma_cuadrados_x`.
- `SigmaXY[xs, ys]`: Suma Σxy de dos listas del mismo largo. Mutacion: solo consulta. Riesgo: bajo. Alias: `suma_productos_xy`.
- `SigmaYY[ys]`: Suma Σy² de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `suma_cuadrados_y`.
- `Sxx[xs]`: Suma Σ(x−x̄)² de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `sc_x`.
- `Sxy[xs, ys]`: Suma Σ(x−x̄)(y−ȳ) de dos listas. Mutacion: solo consulta. Riesgo: bajo. Alias: `sc_xy`.
- `Syy[ys]`: Suma Σ(y−ȳ)² de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `sc_y`.
- `GeometricMean[lista]`: Media geométrica (exige valores finitos > 0). Mutacion: solo consulta. Riesgo: bajo. Alias: `media_geometrica`.
- `HarmonicMean[lista]`: Media armónica (exige valores finitos ≠ 0). Mutacion: solo consulta. Riesgo: bajo. Alias: `media_armonica`.
- `Mode[lista]`: Moda (valor más frecuente) de la lista. Mutacion: solo consulta. Riesgo: bajo. Alias: `moda`.
- `RootMeanSquare[lista]`: Media cuadrática √(Σx²/n). Mutacion: solo consulta. Riesgo: bajo. Alias: `rms`.
- `SumSquaredErrors[lista]`: Suma de errores cuadráticos Σ(x−x̄)². Mutacion: solo consulta. Riesgo: bajo. Alias: `sse`.
- `ZMeanEstimate[lista, sigma, conf]`: Intervalo Z de la media con sigma conocida. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_media_estim`.
- `ZMean2Estimate[l1, s1, l2, s2, conf]`: Intervalo Z de μ₁−μ₂ con sigmas conocidas. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_media2_estim`.
- `ZMeanTest[lista, mu0, sigma]`: Prueba Z bilateral de la media (z, p). Mutacion: solo consulta. Riesgo: bajo. Alias: `z_media_test`.
- `ZMean2Test[l1, s1, l2, s2]`: Prueba Z bilateral de μ₁−μ₂ (z, p). Mutacion: solo consulta. Riesgo: bajo. Alias: `z_media2_test`.
- `ZProportionEstimate[exitos, n, conf]`: Intervalo Z (Wald) de una proporción. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_prop_estim`.
- `ZProportion2Estimate[x1, n1, x2, n2, conf]`: Intervalo Z de p₁−p₂ (no agrupado). Mutacion: solo consulta. Riesgo: bajo. Alias: `z_prop2_estim`.
- `ZProportionTest[exitos, n, p0]`: Prueba Z bilateral de una proporción contra p0. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_prop_test`.
- `ZProportion2Test[x1, n1, x2, n2]`: Prueba Z bilateral de p₁−p₂ (agrupada). Mutacion: solo consulta. Riesgo: bajo. Alias: `z_prop2_test`.
- `TMeanEstimate[lista, conf]`: Intervalo t de la media (sigma desconocida). Mutacion: solo consulta. Riesgo: bajo. Alias: `t_media_estim`.
- `TMean2Estimate[l1, l2, conf]`: Intervalo t de Welch de μ₁−μ₂. Mutacion: solo consulta. Riesgo: bajo. Alias: `t_media2_estim`.
- `ContingencyTable[obs, ncols]`: Chi² de independencia de una tabla plana filas×ncols. Mutacion: solo consulta. Riesgo: bajo. Alias: `contingencia`.
- `Class[lista, k, i]`: i-ésima clase de k clases de igual ancho. Mutacion: solo consulta. Riesgo: bajo. Alias: `clase`.
- `Classes[lista, k]`: Fronteras de k clases de igual ancho. Mutacion: solo consulta. Riesgo: bajo. Alias: `clases`.
- `DotPlot[lista]`: Diagrama de puntos (crea ScatterPlot con bastones). Mutacion: crea objetos. Riesgo: medio. Alias: `diagrama_puntos`.
- `FrequencyPolygon[lista]`: Polígono de frecuencias (crea Polyline de puntos medios). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FrequencyPolygon[lista, k]`. Alias: `poligono_frecuencias`.
## Probabilidad

- `Erlang[k, lambda]`: Erlang(k, λ): PDF y CDF (k entero ≥ 1). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Erlang[k, lambda, x]`. Alias: `erlang_dist`.
- `FDistribution[d1, d2]`: F de Fisher-Snedecor: PDF y CDF. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `FDistribution[d1, d2, x]`. Alias: `dist_f`.
- `Gamma[x]`: Gamma(α, β): PDF y CDF por gamma incompleta. Gamma[x] sigue siendo la función Γ. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Gamma[alpha, beta]`, `Gamma[alpha, beta, x]`. Alias: `dist_gamma`.
- `HyperGeometric[N, K, n]`: Hipergeométrica(N,K,n): PMF y CDF por suma acotada. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `HyperGeometric[N, K, n, k]`. Alias: `hipergeometrica`.
- `LogNormal[mu, sigma]`: Log-normal(μ, σ): PDF y CDF vía la normal. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `LogNormal[mu, sigma, x]`. Alias: `lognormal_dist`.
- `Logistic[mu, s]`: Logística(μ, s): PDF y CDF cerradas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Logistic[mu, s, x]`. Alias: `logistica_dist`.
- `Pascal[r, p]`: Pascal r,p (fallos antes del r-ésimo éxito): PMF y CDF. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Pascal[r, p, k]`. Alias: `pascal_dist`.
- `Triangular[a, b, c]`: Triangular(a,b,c): PDF y CDF cerradas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Triangular[a, b, c, x]`. Alias: `triangular_dist`.
- `Weibull[k, lambda]`: Weibull(k, λ): PDF y CDF cerradas. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Weibull[k, lambda, x]`. Alias: `weibull_dist`.
- `Zipf[s, N]`: Zipf(s, N≤100000): PMF y CDF por suma acotada. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Zipf[s, N, k]`. Alias: `zipf_dist`.
- `Bernoulli[p]`: Bernoulli(p): PMF y CDF escalonada. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Bernoulli[p, k]`. Alias: `bernoulli_dist`.
- `TDistribution[df]`: t de Student: PDF y CDF. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `TDistribution[df, x]`. Alias: `dist_t`.
- `InverseBeta[p, alpha, beta]`: Cuantil Beta por Newton-bisección (tol 1e-12, cap 200). Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_beta`.
- `InverseBinomial[p, n, ps]`: Cuantil binomial por barrido acotado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_binomial`.
- `InverseBinomialMinimumTrials[p, k, ps]`: Menor n con P(X≥k) ≥ p para X~Binomial(n,ps). Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_binomial_ensayos`.
- `InverseCauchy[p, x0, gamma]`: Cuantil Cauchy cerrado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_cauchy`.
- `InverseGamma[p, alpha, beta]`: Cuantil Gamma por Newton-bisección. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_gamma`.
- `InverseHyperGeometric[p, N, K, n]`: Cuantil hipergeométrico por barrido acotado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_hipergeometrica`.
- `InverseLogNormal[p, mu, sigma]`: Cuantil Log-normal cerrado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_lognormal`.
- `InverseLogistic[p, mu, s]`: Cuantil logístico cerrado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_logistica`.
- `InversePascal[p, r, ps]`: Cuantil Pascal por barrido acotado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_pascal`.
- `InversePoisson[p, lambda]`: Cuantil Poisson por barrido acotado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_poisson`.
- `InverseWeibull[p, k, lambda]`: Cuantil Weibull cerrado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_weibull`.
- `InverseZipf[p, s, N]`: Cuantil Zipf por barrido acotado. Mutacion: solo consulta. Riesgo: bajo. Alias: `inv_zipf`.
## Cónicas

- `Focus[conica]`: Devuelve el/los focos de una cónica (elipse, hipérbola, parábola) usando grafito-geometry::exact. Mutacion: solo consulta. Riesgo: bajo. Alias: `Foco`, `focos`.
- `Directrix[conica]`: Devuelve la directriz de una parábola como recta (dos puntos) usando exact::parabola. Mutacion: solo consulta. Riesgo: bajo. Alias: `Directriz`.
- `Center[conica]`: Devuelve el centro (elipse/hipérbola/círculo) o vértice (parábola) usando exact::center. Mutacion: solo consulta. Riesgo: bajo. Alias: `Centro`.
- `Eccentricity[conica]`: Devuelve la excentricidad e de una cónica (0 círculo, 0<e<1 elipse, e=1 parábola, e>1 hipérbola). Mutacion: solo consulta. Riesgo: bajo. Alias: `Excentricidad`, `ecc`.
- `Axes[conica]`: Devuelve los semiejes (a,b) de elipse/hipérbola o parámetro p de parábola usando exact::axes. Mutacion: solo consulta. Riesgo: bajo. Alias: `Ejes`, `semiejes`.
- `IsTangent[recta, conica]`: Predicado exacto IsTangent[recta, elipse] usando exact::is_tangent_to_ellipse (discriminante). Mutacion: solo consulta. Riesgo: bajo. Alias: `EsTangente`.
## Construir

- `AreCollinear[A, B, C]`: Predicado numérico AreCollinear[A,B,C]: |AB×AC| ≤ 1e-9·(1+|AB|+|AC|). Mutacion: solo consulta. Riesgo: bajo. Alias: `son_colineales`, `colineales`.
- `AreConcurrent[l, m, n]`: Predicado numérico AreConcurrent[l,m,n]: intersección l∩m a ≤1e-9 de n; paralelas ⇒ false. Mutacion: solo consulta. Riesgo: bajo. Alias: `son_concurrentes`, `concurrentes`.
- `AreConcyclic[A, B, C, D]`: Predicado numérico AreConcyclic[A,B,C,D]: |D-O|≈R del círculo ABC con tol 1e-9·(1+R). Mutacion: solo consulta. Riesgo: bajo. Alias: `son_conciclicos`, `conciclicos`.
- `AreParallel[l, m]`: Predicado numérico AreParallel[l,m]: |dir_l×dir_m| ≤ 1e-9·|l|·|m|. Mutacion: solo consulta. Riesgo: bajo. Alias: `son_paralelas`, `paralelas`.
- `ArePerpendicular[l, m]`: Predicado numérico ArePerpendicular[l,m]: |dir_l·dir_m| ≤ 1e-9·|l|·|m|. Mutacion: solo consulta. Riesgo: bajo. Alias: `son_perpendiculares`, `perpendiculares`.
## Texto

- `TableText[funcion, min, max, paso]`: Genera tabla LaTeX-like texto desde función+rango+step; salida string pura sin mutar documento. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `TableText[expr, min, max, paso]`. Alias: `TablaTexto`.
## Dinámica

- `Slider[variable, min, max, paso, modo]`: Crea VariableMeta Slider[a, min, max, step, mode] con modo PingPong/Loop y velocity (animation_speed). Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Slider[variable, min, max, paso]`. Alias: `Deslizador`.
- `Rastro[objeto]`: Activa/desactiva el rastro de un objeto: al arrastrarlo deja una estela con fade. Rastro[etiqueta] alterna; Rastro[etiqueta, true|false] fija el estado. (Trace con matriz sigue siendo traza matricial.) Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `Rastro[objeto, estado]`. Alias: `Estela`, `SetTrace`.
- `Button[rotulo, guion]`: Crea un botón (action object sobre texto) con guion del subset GGBScript; el click lo ejecuta la UI. Mutacion: crea objetos. Riesgo: bajo. Alias: `Boton`.
- `Checkbox[rotulo, variable]`: Crea un checkbox ligado a una variable (1 activado, 0 desactivado). Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Checkbox[rotulo, variable, inicial]`. Alias: `Casilla`.
- `InputBox[rotulo, variable]`: Crea una caja de entrada ligada a una variable numérica. Mutacion: crea objetos. Riesgo: bajo. Alias: `CajaEntrada`.
- `TextField[rotulo, variable]`: Crea un campo de texto ligado a una variable (variante de InputBox). Mutacion: crea objetos. Riesgo: bajo. Alias: `CampoTexto`.
- `Show[objeto]`: Hace visibles de uno a cuatro objetos por etiqueta. Mutacion: transforma objetos. Riesgo: bajo. Alias: `Mostrar`.
- `Hide[objeto]`: Oculta de uno a cuatro objetos por etiqueta. Mutacion: transforma objetos. Riesgo: bajo. Alias: `Ocultar`.
- `ZoomIn[]`: Acerca la vista 2D (factor 1.25 por defecto, máximo 4 por invocación). Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `ZoomIn[factor]`. Alias: `Acercar`.
- `ZoomOut[]`: Aleja la vista 2D (factor 1.25 por defecto, máximo 4 por invocación). Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `ZoomOut[factor]`. Alias: `Alejar`.
- `PlayPause[]`: Alterna la animación de una variable o de todas si no se indica. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `PlayPause[variable]`. Alias: `AlternarAnimacion`.
- `If[condicion, guion_si]`: Ejecuta un guion del subset si la condición numérica es cierta, con rama opcional. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `If[condicion, guion_si, guion_no]`. Alias: `Si`.
- `Repeat[n, guion]`: Repite un guion del subset de 1 a 1000 veces con presupuesto total de 1000 pasos. Mutacion: transforma objetos. Riesgo: medio. Alias: `Repetir`.
- `DefineTool[nombre, pasos]`: Define una custom tool desde una secuencia y devuelve su JSON .ggt versionado. Mutacion: solo consulta. Riesgo: bajo. Alias: `DefinirHerramienta`.
- `LoadTool[json]`: Valida un JSON .ggt (versión, nombre, cotas, allowlist) y lo describe sin ejecutar. Mutacion: solo consulta. Riesgo: bajo. Alias: `CargarHerramienta`.
- `Execute[guion]`: Ejecuta un guion del subset GGBScript con presupuesto y rollback atómico: Execute[guion]. Comparte la cota de 1000 pasos con el anidado. Mutacion: transforma objetos. Riesgo: bajo. Alias: `Ejecutar`.
- `CenterView[x, y]`: Centra la vista 2D en (x, y): CenterView[x, y]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `centrar_vista`.
- `Pan[dx, dy]`: Desplaza la vista en píxeles de pantalla: Pan[dx, dy]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `desplazar_vista`, `paneo`.
- `OnClick[etiqueta, guion]`: Guarda un guion que se ejecuta al hacer click sobre el objeto: OnClick[etiqueta, guion]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `al_click`, `al_hacer_click`.
- `OnUpdate[etiqueta, guion]`: Guarda un guion OnUpdate (ejecución en P3c: requiere tracking de cambios). Mutacion: transforma objetos. Riesgo: bajo. Alias: `al_actualizar`.
- `OnLoad[guion]`: Guarda el guion de apertura del documento (ejecución en P3c). Mutacion: transforma objetos. Riesgo: bajo. Alias: `al_cargar`.
- `Turtle[programa]`: Tortuga Logo: FD/BK/LT/RT/PU/PD/REPEAT a polilíneas. Turtle[programa]. Mutacion: crea objetos. Riesgo: bajo. Alias: `tortuga`.
- `UpdateConstruction[]`: Recalcula secuencias vivas y vinculados: UpdateConstruction[]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `actualizar_construccion`.
- `SetFilling[objeto, alfa]`: Alfa de relleno de polígono 0..=1: SetFilling[objeto, alfa]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `relleno`, `transparencia_relleno`.
- `SetLineThickness[objeto, grosor]`: Grosor de línea 0.5..=20 en línea/círculo/polígono/polilínea/elipse: SetLineThickness[objeto, grosor]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `grosor_linea`.
- `ShowLayer[n]`: Hace visibles los objetos de la capa n: ShowLayer[n]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `mostrar_capa`.
- `HideLayer[n]`: Oculta los objetos de la capa n: HideLayer[n]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `ocultar_capa`.
- `StartAnimation[]`: Pone en marcha la animación de una variable o de todas si no se indica (semántica set: repetir no alterna, a diferencia de PlayPause). Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `StartAnimation[variable]`. Alias: `IniciarAnimacion`.
- `StopAnimation[]`: Pausa la animación de una variable o de todas si no se indica (idempotente: pausar lo pausado es no-op honesto). Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `StopAnimation[variable]`. Alias: `DetenerAnimacion`.
- `Delete[objeto]`: Borra el objeto con la etiqueta dada (nombre GeoGebra de Erase[etiqueta]). Mutacion: transforma objetos. Riesgo: bajo. Alias: `Eliminar`, `Borrar`.
- `Rename[objeto, nuevo_nombre]`: Renombra la etiqueta de un objeto: Rename[objeto, nuevo_nombre] valida (no vacío, ≤64, sin saltos, sin colisión) y aplica con undo transaccional. Mutacion: transforma objetos. Riesgo: bajo. Alias: `Renombrar`.
## Análisis

- `TangentAt[expr, x0]`: Recta tangente a y=f(x) en x0: TangentAt[expr, x0] crea una recta por (x0,f(x0)) con pendiente f'(x0). Mutacion: crea objetos. Riesgo: bajo. Alias: `TangenteEn`.
- `NormalAt[expr, x0]`: Recta normal a y=f(x) en x0: NormalAt[expr, x0] crea una recta perpendicular a la tangente en (x0,f(x0)). Mutacion: crea objetos. Riesgo: bajo. Alias: `NormalEn`.
- `ArcLength[expr, a, b]`: Longitud de arco de y=f(x) entre a y b: ArcLength[expr, a, b] integra sqrt(1+f'(x)^2). Mutacion: solo consulta. Riesgo: medio. Alias: `LongitudArco`.
- `CurvatureAt[expr, x0]`: Curvatura de y=f(x) en x0: CurvatureAt[expr, x0] calcula κ = |f''|/(1+f'^2)^{3/2}. Mutacion: solo consulta. Riesgo: medio. Alias: `CurvaturaEn`.
- `VolumeOfRevolution[expr, a, b]`: Volumen de revolución de y=f(x) alrededor del eje X entre a y b: VolumeOfRevolution[expr, a, b] = π∫f(x)^2 dx. Mutacion: solo consulta. Riesgo: medio. Alias: `VolumenRevolucion`, `volumen_revolucion`.
- `SurfaceOfRevolution[expr, a, b]`: Superficie de revolución de y=f(x) entre a y b: SurfaceOfRevolution[expr, a, b] = 2π∫f(x)sqrt(1+f'(x)^2) dx. Mutacion: solo consulta. Riesgo: medio. Alias: `SuperficieRevolucion`, `superficie_revolucion`.
## CAS

- `ODE[expr, t0, y0, t_end]`: Resuelve EDO y'=f(t,y): ODE[expr, t0, y0, t_end, steps, metodo, tolerancia] con metodos euler/rk4/rk45/backward; genera PencilObj. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `ODE[expr, t0, y0, t_end, steps]`, `ODE[expr, t0, y0, t_end, steps, metodo]`, `ODE[expr, t0, y0, t_end, steps, metodo, tolerancia]`. Alias: `EDO`.
- `ODESystem[expr1, expr2, t0, x0, y0]`: Resuelve sistema 2D x'=f(t,x,y), y'=g(t,x,y): ODESystem[expr1, expr2, t0, x0, y0, t_end, steps, metodo, tolerancia]. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `ODESystem[expr1, expr2, t0, x0, y0, t_end]`, `ODESystem[expr1, expr2, t0, x0, y0, t_end, steps]`, `ODESystem[expr1, expr2, t0, x0, y0, t_end, steps, metodo]`, `ODESystem[expr1, expr2, t0, x0, y0, t_end, steps, metodo, tolerancia]`. Alias: `SistemaEDO`, `sistema_edo`.
- `SolveODE2[a, b, c, rhs]`: Resolvé EDO lineal de 2do orden a·y''+b·y'+c·y=rhs con a, b, c constantes (a≠0): SolveODE2[a, b, c, rhs] o SolveODE2[a, b, c, rhs, variable]. Orden ≥3 o coeficientes variables quedan fuera del subset y dan error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SolveODE2[a, b, c, rhs, variable]`. Alias: `edo2`, `edo_2`.
- `ODESystem2[a11, a12, a21, a22]`: Resolvé sistema lineal 2x2 constante x'=A·x por autovalores: ODESystem2[a11, a12, a21, a22] o ODESystem2[a11, a12, a21, a22, t]. No lineal o no constante queda fuera del subset y da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ODESystem2[a11, a12, a21, a22, t]`. Alias: `sistemaedo2`, `odesys2`.
- `LaplaceT[expr]`: Calculá la transformada de Laplace directa del subset F3c (1, t^n con n≤20, exp, sin/cos y combinaciones lineales): LaplaceT[expr] o LaplaceT[expr, t, s]. El resto da error honesto, no inventa. Laplace es distribución. ¿Buscabas LaplaceT[expr]? Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `LaplaceT[expr, t]`, `LaplaceT[expr, t, s]`. Alias: `transformadalaplace`, `laplace_t`.
- `InvLaplaceT[expr]`: Calculá la Laplace inversa de racionales propios con denominador de grado ≤2: InvLaplaceT[expr] o InvLaplaceT[expr, s, t]. Grado ≥3, impropias o retardos quedan fuera del subset y dan error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `InvLaplaceT[expr, s]`, `InvLaplaceT[expr, s, t]`. Alias: `laplaceinversa`, `invlaplace_t`, `InverseLaplace`.
- `RischInt[expr]`: Integrá por Risch-Norman (polinomios, exponenciales, logaritmos): RischInt[expr], RischInt[expr, variable] o definida RischInt[expr, variable, a, b] por FTC. Sin primitiva en el subset (p. ej. exp(x^2)) da error honesto que deriva a cuadratura. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `RischInt[expr, variable]`, `RischInt[expr, variable, a, b]`. Alias: `risch`, `risch_int`.
- `GroebnerBasis[polinomios, variables]`: Calculá la base de Groebner por Buchberger acotado (hasta 12 polinomios en 6 variables, 384 S-polinomios; 3x3 lineal verificado): GroebnerBasis[polinomios, variables]. Fuera de cota o no polinómico da error honesto que deriva a Eliminate. Mutacion: solo consulta. Riesgo: bajo. Alias: `groebner_basis`, `basegroebner`.
- `SolveODEN[coeficientes, rhs]`: Resolvé EDO lineal de orden n≤8 con coeficientes constantes por anulador + resonancia: SolveODEN[{a2,a1,a0}, rhs] o SolveODEN[{a2,a1,a0}, rhs, x]. Fuera del subset da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `SolveODEN[coeficientes, rhs, variable]`. Alias: `edo_n`, `solveode`.
- `EulerODE[a, b, rhs]`: Resolvé Euler x²·y''+a·x·y'+b·y=rhs vía x=e^t (x>0): EulerODE[a, b, rhs] o EulerODE[a, b, rhs, x]. Fuera del subset da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `EulerODE[a, b, rhs, variable]`. Alias: `edoeuler`, `euleredo`.
- `FrobeniusSeries[p, q]`: Serie de Frobenius en punto ordinario (términos≤9): FrobeniusSeries[p, q] o FrobeniusSeries[p, q, x, x0, terminos]. Punto singular da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `FrobeniusSeries[p, q, x, x0, terminos]`. Alias: `frobenius`, `seriefrobenius`.
- `LaplaceDeriv[n, y]`: Laplace de derivada L{y⁽ⁿ⁾} con iniciales (n≤8): LaplaceDeriv[n, y] o LaplaceDeriv[n, y, t, s, {y0, y1}]. Fuera del subset da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `LaplaceDeriv[n, y, t, s, iniciales]`. Alias: `derivadalaplace`.
- `LaplaceInt[f]`: Laplace de integral L{∫₀ᵗ f} = L{f}/s: LaplaceInt[f] o LaplaceInt[f, t, s]. Fuera del subset da error honesto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `LaplaceInt[f, t, s]`. Alias: `integrallaplace`.
- `GroebnerOrdered[polinomios, variables, orden]`: Base de Groebner con orden monomial explícito (mismas cotas que GroebnerBasis): GroebnerOrdered[polinomios, variables, orden] con orden lex|grlex|grevlex. Útil para eliminación. Mutacion: solo consulta. Riesgo: bajo. Alias: `groebnerorden`, `baseordenada`.
- `GroebnerLexDeg[polinomios, variables]`: Base de Groebner en orden grado-lexicográfico (grlex), apto para eliminación: GroebnerLexDeg[polinomios, variables]. Equivalentes: GroebnerOrdered[..., "grlex"]. Mutacion: solo consulta. Riesgo: bajo. Alias: `groebner_gradolex`.
- `Eliminate[polinomios, variables, eliminar]`: Elimina variables por Groebner lexicográfico (intersecciones): Eliminate[polinomios, variables, eliminar]. Fuera de cota da ResourceLimit honesto. Mutacion: solo consulta. Riesgo: bajo. Alias: `elimina`, `eliminacion`.
## Estadística

- `FitLine[tabla]`: Ajusta una recta a una tabla local (alias de FitLinear con RMSE y R²). Mutacion: crea objetos. Riesgo: medio.
- `Fit[tabla]`: Ajusta una recta a una tabla local (alias corto de FitLinear). Mutacion: crea objetos. Riesgo: medio.
- `FitLineX[tabla]`: Ajusta una recta a una tabla local (alias de FitLinear). Mutacion: crea objetos. Riesgo: medio.
## CAS

- `IntegralBetween[expr, a, b]`: Calcula una integral definida entre límites (alias de Integral). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `IntegralBetween[expr, variable, a, b]`.
- `IntegralSymbolic[expr]`: Integra por Risch-Norman (alias de RischInt con formas indefinida y definida). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `IntegralSymbolic[expr, variable]`, `IntegralSymbolic[expr, variable, a, b]`.
- `TaylorPolynomial[expr, variable, centro, orden]`: Construye una serie de Taylor finita (alias de Taylor). Mutacion: crea objetos. Riesgo: medio.
## Financiera

- `Periods[rate, pmt, pv, fv]`: Calcula número de periodos TVM (alias de Nper con exp/log). Mutacion: solo consulta. Riesgo: bajo.
## Matrices

- `Transpose[[a, b], [c, d]]`: Calcula la transpuesta de una matriz. Mutacion: solo consulta. Riesgo: medio. Alias: `transpuesta`.
- `Invert[[a, b], [c, d]]`: Calcula una matriz inversa (alias de Inverse). Mutacion: solo consulta. Riesgo: medio.
- `NInvert[[a, b], [c, d]]`: Calcula una matriz inversa numérica (alias de Inverse). Mutacion: solo consulta. Riesgo: medio.
## CAS

- `SolveCubic[expr, variable]`: Resuelve una ecuación cúbrica en la variable indicada (vía motor Solve). Mutacion: crea objetos. Riesgo: medio.
- `SolveQuartic[expr, variable]`: Resuelve una ecuación cuártica en la variable indicada (vía motor Solve). Mutacion: crea objetos. Riesgo: medio.
## Análisis

- `Curvature[expr, x0]`: Calcula la curvatura de y=f(x) en x0 (alias de CurvatureAt). Mutacion: solo consulta. Riesgo: medio.
## CAS

- `NIntegral[expr, a, b]`: Calcula una integral definida numérica (alias de Integral con límites). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `NIntegral[expr, variable, a, b]`.
## Matrices

- `Dot[u, v]`: Calcula el producto punto de dos vectores de igual dimensión. Mutacion: solo consulta. Riesgo: bajo.
- `Cross[u, v]`: Calcula el producto cruz de dos vectores 3D. Mutacion: solo consulta. Riesgo: bajo.
- `UnitVector[v]`: Normaliza un vector no nulo a longitud unitaria. Mutacion: solo consulta. Riesgo: bajo.
- `ApplyMatrix[M, v]`: Aplica una matriz a un vector o matriz compatible vía multiplicación. Mutacion: solo consulta. Riesgo: medio.
## CAS

- `NDerivative[expr, variable, x0]`: Deriva numéricamente por diferencias centrales en un punto. Mutacion: solo consulta. Riesgo: bajo.
- `IsDefined[nombre]`: Indica si un nombre es variable u objeto definido en el documento. Mutacion: solo consulta. Riesgo: bajo.
- `IsInteger[valor]`: Indica si un valor numérico finito es entero. Mutacion: solo consulta. Riesgo: bajo.
- `IsPrime[n]`: Indica si un entero entre 2 y 1e12 es primo (vía PrimeFactors). Mutacion: solo consulta. Riesgo: bajo.
- `IsInRegion[punto, region]`: Indica si un punto está dentro de un círculo o polígono del documento. Mutacion: solo consulta. Riesgo: bajo.
- `CSolve[expr, variable?]`: Raíces en ℂ de un polinomio: grados 1–2 exactos, 3–16 por Durand–Kerner acotado (100 iteraciones). CSolve[expr] o CSolve[expr, variable]. Mutacion: solo consulta. Riesgo: bajo. Alias: `csol`, `resolver_complejo`.
- `CSolutions[expr, variable?]`: Como CSolve pero verificando cada raíz por sustitución e informando el residuo máximo |p(z)|. Mutacion: solo consulta. Riesgo: bajo. Alias: `csoluciones`.
- `NSolutions[expr, variable?]`: Todas las raíces reales por aislamiento Sturm + bisección + Newton (solo mensaje, sin objetos). Mutacion: solo consulta. Riesgo: bajo. Alias: `nsoluciones`, `soluciones_numericas`.
- `Solutions[ecuación, variable?]`: Conjunto solución de una ecuación `lhs = rhs` (solo mensaje, sin objetos; Solve además grafica). Mutacion: solo consulta. Riesgo: bajo. Alias: `soluciones`.
- `ToComplex[a, b]`: Forma canónica a+bi desde partes o literal: ToComplex[a, b] o ToComplex["a+bi"]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ToComplex[complejo]`. Alias: `to_complejo`, `complejo`.
- `ToPolar[a, b]`: Forma polar (r; θ) con θ en radianes: ToPolar[a, b] o ToPolar["a+bi"]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ToPolar[complejo]`. Alias: `to_polar`, `a_polar`.
- `Prove[conclusión, hipótesis?, variables?]`: Demuestra una conclusión bajo hipótesis por Rabinowitsch + Gröbner acotado: Prove[conclusión] o Prove[conclusión, {hipótesis}] o Prove[conclusión, {hipótesis}, {variables}]. Fuera de cotas responde indefinido, jamás falso positivo. Mutacion: solo consulta. Riesgo: bajo. Alias: `demostrar`, `probar`.
- `ProveDetails[conclusión, hipótesis?, variables?]`: Como Prove pero mostrando la base de Gröbner certificante (hasta 8 polinomios). Mutacion: solo consulta. Riesgo: bajo. Alias: `detalles_demostracion`.
- `Relation[expr1, expr2]`: Conjetura expr1 = expr2: simbólico exacto si el motor decide; si no, muestreo numérico que refuta con contraejemplo o sugiere (usa Prove para demostrar). Mutacion: solo consulta. Riesgo: bajo. Alias: `relacion`, `conjetura`.
## Texto

- `FormulaText[expr]`: Crea un texto con la fórmula literal dada. Mutacion: crea objetos. Riesgo: bajo.
- `ScientificText[valor]`: Crea un texto con el valor en notación científica. Mutacion: crea objetos. Riesgo: bajo.
- `MixedNumber[valor]`: Crea un texto con número mixto: MixedNumber[2.5] -> 2 1/2. Mutacion: crea objetos. Riesgo: bajo.
- `Ordinal[n]`: Crea un texto con ordinal español: Ordinal[1] -> 1.º. Mutacion: crea objetos. Riesgo: bajo.
## Dinámica

- `SetColor[objeto, color]`: Cambia el color de trazo de un objeto existente y lo refleja en el canvas. Mutacion: transforma objetos. Riesgo: bajo.
- `SetCoords[objeto, coords]`: Mueve un punto libre a nuevas coordenadas y lo refleja en el canvas. Mutacion: transforma objetos. Riesgo: bajo.
- `SetVisible[objeto, visible]`: Cambia la visibilidad de un objeto (true/false) y lo refleja en el canvas. Mutacion: transforma objetos. Riesgo: bajo.
- `SetCaption[objeto, rotulo]`: Pone el rótulo visible del objeto (caption = etiqueta mostrada en álgebra y canvas). Mutacion: transforma objetos. Riesgo: bajo. Alias: `poner_rotulo`.
- `SetLineStyle[objeto, estilo]`: Cambia el trazo de un objeto con línea (solid/dashed/dotted) y lo refleja en el canvas. Mutacion: transforma objetos. Riesgo: bajo. Alias: `estilo_linea`.
- `SetPointStyle[objeto, estilo]`: Cambia la forma del marcador de un punto (dot/circle/cross/plus) y la refleja en el canvas. Mutacion: transforma objetos. Riesgo: bajo. Alias: `estilo_punto`.
- `SetLayer[objeto, capa]`: Mueve un objeto a una capa de dibujo 0..=255 (0 = fondo; >255 cae a 255 avisando). Mutacion: transforma objetos. Riesgo: bajo. Alias: `poner_capa`.
## 3D

- `Surface[objeto]`: Muestra el área exacta de un sólido 3D del documento. Mutacion: solo consulta. Riesgo: bajo.
- `Volume[objeto]`: Muestra el volumen exacto de un sólido 3D del documento. Mutacion: solo consulta. Riesgo: bajo.
## Análisis

- `OsculatingCircle[expr, x0]`: Crea el círculo osculador a y=f(x) en x0 desde la curvatura existente. Mutacion: crea objetos. Riesgo: medio.
## Estadística

- `Cell[celda]`: Lee una celda de la planilla por etiqueta A1. Mutacion: solo consulta. Riesgo: bajo.
- `Column[col]`: Lee una columna de la planilla por letra o índice. Mutacion: solo consulta. Riesgo: bajo.
- `Row[fila]`: Lee una fila de la planilla por número. Mutacion: solo consulta. Riesgo: bajo.
## CAS

- `GCD[a, b]`: Máximo común divisor de dos enteros (Euclides exacto en i128). Mutacion: solo consulta. Riesgo: bajo. Alias: `mcd`, `maximo_comun_divisor`.
- `LCM[a, b]`: Mínimo común múltiplo de dos enteros (exacto en i128, error si desborda). Mutacion: solo consulta. Riesgo: bajo. Alias: `mcm`, `minimo_comun_multiplo`.
- `ExtendedGCD[a, b]`: Euclides extendido: devuelve (mcd, x, y) con a·x+b·y=mcd, exacto en i128. Mutacion: solo consulta. Riesgo: bajo. Alias: `extended_gcd`, `mcd_extendido`, `bezout`.
- `Divisors[n]`: Lista ordenada de divisores positivos de n (1 <= n <= 1e12). Mutacion: solo consulta. Riesgo: bajo. Alias: `divisores`, `DivisorsList`, `divisors_list`, `lista_divisores`.
- `DivisorsSum[n]`: Suma de divisores σ(n) exacta en i128 (1 <= n <= 1e12). Mutacion: solo consulta. Riesgo: bajo. Alias: `divisors_sum`, `suma_divisores`, `sigma_divisores`.
- `NextPrime[n]`: Menor primo mayor que n (Miller-Rabin determinista, cota 1e12). Mutacion: solo consulta. Riesgo: bajo. Alias: `next_prime`, `siguiente_primo`, `proximo_primo`.
- `PreviousPrime[n]`: Mayor primo menor que n (Miller-Rabin determinista, cota 1e12). Mutacion: solo consulta. Riesgo: bajo. Alias: `previous_prime`, `primo_anterior`.
- `ModularExponent[base, exp, mod]`: Potencia modular base^exp mod m (exponente >= 0, m >= 1, exacta). Mutacion: solo consulta. Riesgo: bajo. Alias: `modular_exponent`, `potencia_modular`, `modexp`.
- `Mod[a, n]`: Resto euclídeo no negativo de a dividido b (b ≠ 0). Mutacion: solo consulta. Riesgo: bajo. Alias: `modulo_entero`, `resto_euclideo`.
- `Div[a, b]`: Cociente entero euclídeo de a dividido b (b ≠ 0). Mutacion: solo consulta. Riesgo: bajo. Alias: `div_entero`, `cociente_entero`, `division_entera`.
- `ToBase[n, base]`: Convierte un entero a base 2..=36 (dígitos 0-9a-z). Mutacion: solo consulta. Riesgo: bajo. Alias: `to_base`, `a_base`.
- `FromBase[texto, base]`: Lee un entero escrito en base 2..=36 (exacto en i128). Mutacion: solo consulta. Riesgo: bajo. Alias: `from_base`, `desde_base`.
- `ContinuedFraction[x]`: Fracción continua simple de un número finito (hasta 64 términos). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ContinuedFraction[x, n]`. Alias: `continued_fraction`, `fraccion_continua`.
- `Substitute[expr, var, valor]`: Sustituye una variable por un valor en una expresión (vía AST). Mutacion: solo consulta. Riesgo: bajo. Alias: `sustituir`, `sustituye`.
- `Polynomial[coefs]`: Construye la forma canónica a_n·x^n+… desde coeficientes ascendentes. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Polynomial[coefs, variable]`. Alias: `polinomio`.
- `Coefficients[expr]`: Lista de coeficientes por grado del polinomio expandido. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Coefficients[expr, variable]`. Alias: `coeficientes`.
- `Degree[expr]`: Grado total del polinomio (con variable: grado en ella). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Degree[expr, variable]`. Alias: `grado_polinomio`.
- `Roots[expr]`: Raíces reales del polinomio en una variable (grado ≤ 16). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Roots[expr, variable]`. Alias: `raices_polinomio`.
- `RootList[expr]`: Raíces reales en formato lista {r1, r2, …} (grado ≤ 16). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `RootList[expr, variable]`. Alias: `root_list`, `lista_raices`.
- `ComplexRoot[expr]`: Raíces en ℂ del polinomio como pares (re, im) (grado ≤ 20). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ComplexRoot[expr, variable]`. Alias: `complex_root`.
- `Numerator[expr]`: Numerador reducido de una fracción constante exacta. Mutacion: solo consulta. Riesgo: bajo. Alias: `numerador`.
- `Denominator[expr]`: Denominador reducido y positivo de una fracción constante exacta. Mutacion: solo consulta. Riesgo: bajo. Alias: `denominador`.
- `CommonDenominator[a, b]`: Mínimo común denominador de dos fracciones constantes exactas. Mutacion: solo consulta. Riesgo: bajo. Alias: `common_denominator`, `denominador_comun`.
- `Division[p, q]`: División polinómica con cociente y resto (grados ≤ 1024). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Division[p, q, variable]`. Alias: `division_polinomica`, `cociente_resto`.
- `IsFactored[expr]`: Indica si la forma ya es producto o potencia de factores no constantes. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `IsFactored[expr, variable]`. Alias: `is_factored`, `esta_factorizado`.
- `IsVertexForm[expr]`: Indica si la forma es a·(x−h)²+k (a y k opcionales). Mutacion: solo consulta. Riesgo: bajo. Alias: `is_vertex_form`, `es_forma_vertice`.
- `MinimalPolynomial[expr]`: Polinomio minimal exacto de radicales cuadráticos (grado ≤ 8). Mutacion: solo consulta. Riesgo: bajo. Alias: `minimal_polynomial`, `polinomio_minimo`.
## Texto

- `LetterToUnicode[letra]`: Código U+HHHH (y decimal) del carácter dado. Mutacion: solo consulta. Riesgo: bajo. Alias: `letter_to_unicode`, `letra_a_unicode`.
- `UnicodeToLetter[codigo]`: Carácter desde decimal, U+HHHH o 0xHH (UTF-8 seguro). Mutacion: solo consulta. Riesgo: bajo. Alias: `unicode_to_letter`, `unicode_a_letra`.
- `TextToUnicode[texto]`: Lista de códigos decimales de cada carácter (UTF-8 seguro). Mutacion: solo consulta. Riesgo: bajo. Alias: `text_to_unicode`, `texto_a_unicode`.
- `UnicodeToText[codigos]`: Reconstruye el texto desde códigos separados por comas o espacios. Mutacion: solo consulta. Riesgo: bajo. Alias: `unicode_to_text`, `unicode_a_texto`.
## Matrices

- `CharacteristicPolynomial[matriz]`: Polinomio característico numérico por Faddeeva-LeVerrier (n ≤ 64). Mutacion: solo consulta. Riesgo: bajo. Alias: `characteristic_polynomial`, `polinomio_caracteristico`, `charpoly`.
- `ReducedRowEchelonForm[matriz]`: Forma escalonada reducida por filas; exacta si las entradas son decimales exactos (≤32×32), numérica si no. Mutacion: solo consulta. Riesgo: bajo. Alias: `reduced_row_echelon_form`, `forma_escalonada`, `rref`.
- `SVD[matriz]`: Descomposición en valores singulares U·Σ·Vᵀ (numérica, motor SVD). Mutacion: solo consulta. Riesgo: bajo. Alias: `descomposicion_svd`, `valores_singulares`.
- `LUDecomposition[matriz]`: Descomposición LU con pivoteo parcial (numérica, motor LU). Mutacion: solo consulta. Riesgo: bajo. Alias: `lu_decomposition`, `descomposicion_lu`.
- `QRDecomposition[matriz]`: Descomposición QR con Q ortogonal (numérica, motor QR). Mutacion: solo consulta. Riesgo: bajo. Alias: `qr_decomposition`, `descomposicion_qr`.
- `JordanDiagonalization[matriz]`: Diagonalización real P·D·P⁻¹ (numérica; error honesto si es defectiva o compleja). Mutacion: solo consulta. Riesgo: bajo. Alias: `jordan_diagonalization`, `diagonalizacion_jordan`, `jordan`.
## Construir

- `UnitPerpendicularVector[v]`: Calcula el perpendicular 2D unitario de un vector no nulo. Mutacion: solo consulta. Riesgo: bajo. Alias: `unit_perpendicular_vector`, `vector_perpendicular_unitario`.
- `Normalize[v]`: Calcula el vector unitario en la dirección dada (2D o 3D). Mutacion: solo consulta. Riesgo: bajo. Alias: `normalizar`, `normaliza_vector`.
- `PerpendicularVector[v]`: Calcula el perpendicular 2D (−y, x) de un vector no nulo. Mutacion: solo consulta. Riesgo: bajo. Alias: `perpendicular_vector`, `vector_perpendicular`.
## Análisis

- `CurvatureVector[expr, x0]`: Calcula el vector curvatura con signo de y=f(x) en x0 (numérico). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CurvatureVector[expr, variable, x0]`. Alias: `curvature_vector`, `vector_curvatura`.
- `Minimize[f, variable]`: Mínimo en [a,b] por grilla densa + sección áurea: Minimize[f, variable] o Minimize[f, variable, a, b]. Global no garantizado (se declara). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Minimize[f, variable, a, b]`. Alias: `minimizar`, `minimo`.
- `Maximize[f, variable]`: Máximo en [a,b] por grilla densa + sección áurea: Maximize[f, variable] o Maximize[f, variable, a, b]. Global no garantizado (se declara). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Maximize[f, variable, a, b]`. Alias: `maximizar`, `maximo`.
- `NSolveODE[campo, x0, y0, x1]`: Integra y'=f(x,y) por RK45 Dormand–Prince con paso adaptativo: NSolveODE[campo, x0, y0, x1] o NSolveODE[campo, x0, y0, x1, n]. Crea tabla + gráfico enlazados. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `NSolveODE[campo, x0, y0, x1, n]`. Alias: `edo_numerica`, `rk45`.
- `SlopeField[expr]`: Campo de pendientes de y'=f(x,y) como campo vectorial (1, f): SlopeField[expr]. Mutacion: crea objetos. Riesgo: bajo. Alias: `campo_pendientes`, `campo_direcciones`.
- `Area[objeto]`: Área de polígono, círculo o elipse: Area[objeto]. Mutacion: solo consulta. Riesgo: bajo. Alias: `superficie`.
- `Perimeter[objeto]`: Perímetro de polígono, círculo o elipse (Ramanujan): Perimeter[objeto]. Mutacion: solo consulta. Riesgo: bajo. Alias: `perimetro`, `perímetro`.
- `Length[objeto]`: Longitud de segmento, círculo, arco, polígono, polilínea o Bézier (Simpson adaptativo): Length[objeto]. Mutacion: solo consulta. Riesgo: bajo. Alias: `longitud`, `largo`.
- `Radius[objeto]`: Radio de círculo o arco: Radius[objeto]. Mutacion: solo consulta. Riesgo: bajo. Alias: `radio`.
- `Circumference[círculo]`: Longitud de circunferencia: Circumference[círculo]. Mutacion: solo consulta. Riesgo: bajo. Alias: `circunferencia`.
- `Height[A, B, C]`: Altura desde C a la recta AB: Height[A, B, C]. Mutacion: solo consulta. Riesgo: bajo. Alias: `altura`.
## Construir

- `TriangleCenter[triángulo, n]`: Centro de Kimberling 1–6 (centroide, circuncentro, incentro, ortocentro, nueve puntos, simmediano): TriangleCenter[triángulo, n] o TriangleCenter[P, Q, R, n]. Mutacion: crea objetos. Riesgo: bajo. Alias: `centro_triangulo`, `kimberling`.
- `Centroid[polígono]`: Centroide de área de un polígono: Centroid[polígono]. Mutacion: crea objetos. Riesgo: bajo. Alias: `centroide`.
- `Barycenter[P, Q, ...]`: Promedio de puntos: Barycenter[P, Q, ...]. Mutacion: crea objetos. Riesgo: bajo. Alias: `baricentro`.
- `Trilinear[a, b, c, P, Q, R]`: Punto de trilineales α:β:γ sobre el triángulo: Trilinear[a, b, c, P, Q, R]. Mutacion: crea objetos. Riesgo: bajo. Alias: `trilineal`.
- `TriangleCurve[P, Q, R, ecuación]`: Curva implícita desde ecuación baricéntrica en A, B, C: TriangleCurve[P, Q, R, ecuación]. Mutacion: crea objetos. Riesgo: bajo. Alias: `curva_triangulo`.
## Cónicas

- `ConjugateDiameter[cónica]`: Diámetro conjugado: 2·min(rx,ry) en elipse, 2b en hipérbola. Mutacion: solo consulta. Riesgo: bajo. Alias: `diametro_conjugado`.
- `MinorAxis[elipse]`: Eje menor de elipse: MinorAxis[elipse]. Mutacion: solo consulta. Riesgo: bajo. Alias: `eje_menor`.
- `MajorAxis[elipse]`: Eje mayor de elipse: MajorAxis[elipse]. Mutacion: solo consulta. Riesgo: bajo. Alias: `eje_mayor`.
- `SemiMajorAxisLength[elipse]`: Semieje mayor de elipse: SemiMajorAxisLength[elipse]. Mutacion: solo consulta. Riesgo: bajo. Alias: `semieje_mayor`.
- `SemiMinorAxisLength[elipse]`: Semieje menor de elipse: SemiMinorAxisLength[elipse]. Mutacion: solo consulta. Riesgo: bajo. Alias: `semieje_menor`.
- `LinearEccentricity[cónica]`: Excentricidad lineal: sqrt(|rx²−ry²|) en elipse, sqrt(a²+b²) en hipérbola. Mutacion: solo consulta. Riesgo: bajo. Alias: `excentricidad_lineal`.
## Construir

- `ClosestPoint[punto, objeto]`: Punto más cercano sobre polígono, polilínea, círculo o segmento: ClosestPoint[punto, objeto]. Mutacion: crea objetos. Riesgo: bajo. Alias: `punto_cercano`, `punto_mas_cercano`.
- `ClosestPointRegion[punto, polígono]`: Punto más cercano dentro de la región poligonal (adentro → el punto mismo): ClosestPointRegion[punto, polígono]. Mutacion: crea objetos. Riesgo: bajo. Alias: `punto_region`.
- `PointIn[punto, polígono]`: Pertenece el punto al polígono (borde cuenta adentro): PointIn[punto, polígono]. Mutacion: solo consulta. Riesgo: bajo. Alias: `punto_en`, `dentro`.
- `RandomPointIn[polígono]`: Punto aleatorio determinista dentro del polígono (misma semilla → mismo punto): RandomPointIn[polígono]. Mutacion: crea objetos. Riesgo: bajo. Alias: `punto_aleatorio`.
- `IntersectPath[recta, cónica]`: Intersección analítica recta × círculo/elipse (sin recorte de vista, a diferencia de Intersect): IntersectPath[recta, cónica]. Mutacion: crea objetos. Riesgo: bajo. Alias: `interseccion_trayectoria`.
- `Envelope[a, b, c]`: Envolvente numérica de familia a(t)x+b(t)y+c(t)=0 (200 muestras, derivadas centrales): Envelope[a, b, c] o Envelope[a, b, c, t0, t1]. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Envelope[a, b, c, t0, t1]`. Alias: `curva_envolvente`.
## 3D

- `PlaneBisector[A, B]`: Plano mediatriz de dos puntos 3D: PlaneBisector[A, B]. Mutacion: crea objetos. Riesgo: bajo. Alias: `plano_mediatriz`, `bisector`.
- `PerpendicularPlane[A, B, P]`: Plano por P con normal AB: PerpendicularPlane[A, B, P]. Mutacion: crea objetos. Riesgo: bajo. Alias: `plano_perpendicular`.
## CAS

- `ImplicitDerivative[f, x]`: Derivada implícita dy/dx de F(x,y)=0 o evaluada en un punto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ImplicitDerivative[f, x, y]`, `ImplicitDerivative[f, x, y, x0, y0]`.
- `Iteration[f, x, x0, n]`: Itera x_{k+1} = f(x_k) desde x0: Iteration[f, x, x0, n]. Mutacion: solo consulta. Riesgo: bajo.
- `Numeric[expr]`: Evalúa a decimal: Numeric[expr] o Numeric[expr, var, valor]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Numeric[expr, var, valor]`.
- `ToExponential[a, b]`: Complejo (a, b) a forma r·e^(iθ): ToExponential[a, b]. Mutacion: solo consulta. Riesgo: bajo.
- `LeftSide[ecuación]`: Lado izquierdo de una ecuación izq = der. Mutacion: solo consulta. Riesgo: bajo.
- `RightSide[ecuación]`: Lado derecho de una ecuación izq = der. Mutacion: solo consulta. Riesgo: bajo.
- `Factors[n]`: Factores primos con multiplicidad: Factors[n]. Mutacion: solo consulta. Riesgo: bajo.
- `AreEqual[a, b]`: Igualdad demostrable: verdadero, falso o indefinido honesto. Mutacion: solo consulta. Riesgo: bajo.
- `RemovableDiscontinuity[f, punto]`: Discontinuidad evitable de f en un punto: límite existe pero f difiere. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `RemovableDiscontinuity[f, x, punto]`.
- `InflectionPoint[f]`: Puntos de inflexión en [-10, 10] o en [a, b] dado. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `InflectionPoint[f, var, a, b]`.
- `Dimension[matriz]`: Dimensión (filas, columnas) de una matriz. Mutacion: solo consulta. Riesgo: bajo.
- `Identity[n]`: Matriz identidad n×n: Identity[n]. Mutacion: solo consulta. Riesgo: bajo.
- `MatrixRank[matriz]`: Rango de una matriz (Bareiss exacto o SVD). Mutacion: solo consulta. Riesgo: bajo.
- `RandomBetween[a, b]`: Entero uniforme en [a, b], determinista por versión del documento. Mutacion: solo consulta. Riesgo: bajo.
- `RandomPolynomial[grado]`: Polinomio aleatorio de grado dado, determinista por versión del documento. Mutacion: solo consulta. Riesgo: bajo.
- `LeftSum[f, var, a, b, n]`: Suma de Riemann por extremo izquierdo: LeftSum[f, var, a, b, n]. Mutacion: solo consulta. Riesgo: bajo.
- `LowerSum[f, var, a, b, n]`: Suma inferior estimada con 8 muestras por subintervalo, sin precisión exacta. Mutacion: solo consulta. Riesgo: bajo.
- `UpperSum[f, var, a, b, n]`: Suma superior estimada con 8 muestras por subintervalo, sin precisión exacta. Mutacion: solo consulta. Riesgo: bajo.
- `TrapezoidalSum[f, var, a, b, n]`: Suma trapezoidal: TrapezoidalSum[f, var, a, b, n]. Mutacion: solo consulta. Riesgo: bajo.
- `RectangleSum[f, var, a, b, n]`: Suma de rectángulos por punto medio: RectangleSum[f, var, a, b, n]. Mutacion: solo consulta. Riesgo: bajo.
## Lista

- `ColumnName[n]`: Nombre de columna 1-based (1→A, 27→AA): ColumnName[n]. Mutacion: solo consulta. Riesgo: bajo.
- `DataFunction[expr, xs]`: Evalúa expr sobre cada fila (x, y opcional, n): DataFunction[expr, xs]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `DataFunction[expr, xs, ys]`.
- `Frequency[lista]`: Conteo por valor único ordenado: Frequency[lista]. Mutacion: solo consulta. Riesgo: bajo.
## Crear

- `PointList[matriz]`: Crea puntos desde una matriz de 2 o 3 columnas uniformes. Mutacion: crea objetos. Riesgo: bajo.
## Lista

- `RemoveUndefined[lista]`: Filtra NaN e infinitos de una lista. Mutacion: solo consulta. Riesgo: bajo.
- `SelectedElement[lista]`: Elemento de la lista cuyo rótulo está seleccionado; sin selección da error honesto. Mutacion: solo consulta. Riesgo: bajo.
- `SelectedIndex[lista]`: Posición 1-based del primer rótulo seleccionado; sin selección da error honesto. Mutacion: solo consulta. Riesgo: bajo.
## Texto

- `ParseToFunction[texto, var]`: Valida texto como expresión en la variable dada. Mutacion: solo consulta. Riesgo: bajo.
- `ParseToNumber[texto]`: Interpreta texto como número (coma o punto decimal). Mutacion: solo consulta. Riesgo: bajo.
- `ReadText[etiqueta]`: Contenido de un objeto texto existente. Mutacion: solo consulta. Riesgo: bajo.
- `ReplaceAll[texto, buscar, reemplazo]`: Reemplazo literal de subcadenas: ReplaceAll[texto, buscar, reemplazo]. Mutacion: solo consulta. Riesgo: bajo.
## Crear

- `RotateText[texto, grados]`: Crea un texto rotado (grados a radianes en el brazo). Mutacion: crea objetos. Riesgo: bajo.
## Texto

- `Split[texto, delim]`: Divide texto por un separador literal. Mutacion: solo consulta. Riesgo: bajo.
## Crear

- `Text[texto]`: Crea un objeto texto con el contenido dado. Mutacion: crea objetos. Riesgo: bajo.
- `VerticalText[texto]`: Crea un texto apilado un carácter por línea. Mutacion: crea objetos. Riesgo: bajo.
## Análisis

- `AffineRatio[A, B, C]`: Razón afín de tres puntos colineales: AffineRatio[A, B, C]. Mutacion: solo consulta. Riesgo: bajo.
- `CrossRatio[A, B, C, D]`: Razón doble de cuatro puntos: CrossRatio[A, B, C, D]. Mutacion: solo consulta. Riesgo: bajo.
- `AreCongruent[obj1, obj2]`: Congruencia por longitud (segmentos) o SSS (polígonos); otros tipos dan error honesto. Mutacion: solo consulta. Riesgo: bajo.
## Construir

- `CircularArc[centro, r, a0, a1]`: Crea un arco por centro, radio y ángulos en radianes. Mutacion: crea objetos. Riesgo: bajo.
- `CircularSector[centro, r, a0, a1]`: Crea un sector por centro, radio y ángulos en radianes. Mutacion: crea objetos. Riesgo: bajo.
- `CircumcircularArc[A, B, C]`: Crea el arco circunscrito a tres puntos no colineales. Mutacion: crea objetos. Riesgo: bajo.
- `CircumcircularSector[A, B, C]`: Crea el sector circunscrito a tres puntos no colineales. Mutacion: crea objetos. Riesgo: bajo.
- `Cubic[P1, P2, P3, P4, P5, P6, P7, P8, P9]`: Crea la cúbica implícita por 9 puntos. Mutacion: crea objetos. Riesgo: bajo.
## Análisis

- `Direction[recta]`: Dirección de recta o normal de plano; mensaje con componentes, sin objeto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Direction[A, B]`.
## Construir

- `PerpendicularLine[P, recta]`: Crea la recta perpendicular por P a una recta dada. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `PerpendicularLine[P, A, B]`.
- `RigidPolygon[P1, P2, P3]`: Crea un polígono con validación de rigidez Laman; sin ligas automáticas. Mutacion: crea objetos. Riesgo: bajo.
## Cónicas

- `Conic[P1, P2, P3, P4, P5]`: Crea la cónica implícita por 5 puntos (ConicByFivePoints existe como restricción paramétrica). Mutacion: crea objetos. Riesgo: bajo.
- `Parameter[cónica]`: Parámetro focal de parábola, elipse o hipérbola. Mutacion: solo consulta. Riesgo: bajo.
## Análisis

- `PathParameter[P, camino]`: Parámetro de recorrido de un punto sobre polilínea o polígono. Mutacion: solo consulta. Riesgo: bajo.
- `Type[objeto]`: Nombre del tipo del objeto (GeoObject::name). Mutacion: solo consulta. Riesgo: bajo.
## Construir

- `Vertex[objeto]`: Crea puntos en los vértices de polígono o cónica. Mutacion: crea objetos. Riesgo: bajo.
## Análisis

- `InteriorAngles[polígono]`: Ángulos interiores de un polígono en radianes. Mutacion: solo consulta. Riesgo: bajo.
## 3D

- `Bottom[sólido]`: Crea el plano horizontal en la cota mínima del sólido (solo tipos con cotas reales). Mutacion: crea objetos. Riesgo: bajo.
- `Top[sólido]`: Crea el plano horizontal en la cota máxima del sólido (solo tipos con cotas reales). Mutacion: crea objetos. Riesgo: bajo.
- `Ends[sólido]`: Crea los 2 planos de tapa de cilindro o prisma. Mutacion: crea objetos. Riesgo: bajo.
## Análisis

- `Side[sólido]`: Área lateral de cilindro, cono o prisma; mensaje con el valor. Mutacion: solo consulta. Riesgo: bajo.
## 3D

- `IntersectConic[esfera, plano]`: [aproximado] Circulo de esfera por plano calculado exacto (centro+radio) pero solo como mensaje honesto; [no-soportado] objeto circulo-3D en esta version (no se crea sustituto). Mutacion: solo consulta. Riesgo: bajo.
## Estadística

- `Variance[lista]`: Varianza poblacional (÷n) de una lista. Mutacion: solo consulta. Riesgo: bajo.
- `HistogramRight[lista]`: Histograma con bins cerrados a derecha; solo mensaje, sin objeto. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `HistogramRight[lista, bins]`.
## Probabilidad

- `RandomUniform[a, b]`: Sorteo uniforme en [a, b), determinista por versión del documento. Mutacion: solo consulta. Riesgo: bajo.
- `RandomNormal[mu, sigma]`: Sorteo normal por Box-Muller, determinista por versión del documento. Mutacion: solo consulta. Riesgo: bajo.
- `RandomBinomial[n, p]`: Conteo binomial exacto hasta la cota, determinista por versión del documento. Mutacion: solo consulta. Riesgo: bajo.
- `RandomPoisson[lambda]`: Conteo Poisson exacto o aproximado según lambda, determinista. Mutacion: solo consulta. Riesgo: bajo.
- `Beta[a, b]`: Función beta B(a, b) (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo.
- `GammaDist[alpha, beta]`: Distribución gamma: PDF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `GammaDist[alpha, beta, x]`.
- `BetaDist[alpha, beta]`: Distribución beta: PDF por gamma incompleta (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `BetaDist[alpha, beta, x]`.
- `Cauchy[x0, gamma]`: Distribución Cauchy: PDF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Cauchy[x0, gamma, x]`.
- `Pareto[xm, alpha]`: Distribución Pareto: PDF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Pareto[xm, alpha, x]`.
- `Laplace[mu, b]`: Distribución Laplace: PDF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Laplace[mu, b, x]`.
- `Rayleigh[sigma]`: Distribución Rayleigh: PDF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Rayleigh[sigma, x]`.
- `NegBinomial[r, p]`: Binomial negativa: PMF y CDF (brazo huérfano previo, ahora visible). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `NegBinomial[r, p, k]`.
## Dinámica

- `RunClickScript[etiqueta]`: Ejecuta el guion OnClick guardado de un objeto. Mutacion: transforma objetos. Riesgo: bajo.
- `RunUpdateScript[etiqueta]`: Dispara el guion OnUpdate de un objeto; sin hooks automáticos. Mutacion: transforma objetos. Riesgo: bajo.
- `SelectObjects[etiquetas]`: Selecciona objetos por etiquetas: SelectObjects[{A, B}]. Mutacion: transforma objetos. Riesgo: bajo.
- `SetActiveView[perspectiva]`: Valida una de las 10 perspectivas; se aplica desde la UI. Mutacion: solo consulta. Riesgo: bajo.
- `SetPerspective[perspectiva]`: Valida una de las 10 perspectivas; se aplica desde la UI. Mutacion: solo consulta. Riesgo: bajo.
- `SetViewDirection[dirección]`: Valida la dirección de vista 3D; la cámara vive en la app. Mutacion: solo consulta. Riesgo: bajo.
- `SetAxesRatio[x, y]`: Guarda la razón de ejes en __view_axes_rx/ry; escala uniforme hoy, P3c. Mutacion: transforma objetos. Riesgo: bajo.
- `AxisStepX[paso]`: Fija el paso de grilla del eje X en el lienzo 2D: AxisStepX[paso]. Mutacion: transforma objetos. Riesgo: bajo.
- `AxisStepY[paso]`: Fija el paso de grilla del eje Y en el lienzo 2D: AxisStepY[paso]. Mutacion: transforma objetos. Riesgo: bajo.
- `ShowAxes[bool]`: Muestra u oculta los ejes del lienzo 2D: ShowAxes[bool]. Mutacion: transforma objetos. Riesgo: bajo.
- `ShowGrid[bool]`: Muestra u oculta la grilla del lienzo 2D: ShowGrid[bool] (sin flag vale el interruptor de la app). Mutacion: transforma objetos. Riesgo: bajo.
- `SetConditionToShowObject[etiqueta, condición]`: Guarda la condición de visibilidad; se evalúa en el render. Mutacion: transforma objetos. Riesgo: bajo.
- `SetDynamicColor[etiqueta, r, g, b]`: Guarda el color dinámico r, g, b; se evalúa por frame en el render. Mutacion: transforma objetos. Riesgo: bajo.
- `SetTooltipMode[etiqueta, modo]`: Guarda el modo de tooltip; flag-guardado, el hover al objeto llega en P3c. Mutacion: transforma objetos. Riesgo: bajo.
- `SetLabelMode[etiqueta, bool]`: Muestra u oculta la etiqueta del canvas; simplificado honesto true/false. Mutacion: transforma objetos. Riesgo: bajo.
- `ShowLabel[etiqueta, bool]`: Muestra u oculta la etiqueta del canvas vía hide_label en el render. Mutacion: transforma objetos. Riesgo: bajo.
- `SetFixed[etiqueta, bool]`: Fija un objeto y bloquea su arrastre en el canvas. Mutacion: transforma objetos. Riesgo: bajo.
- `SetDecoration[etiqueta, n]`: Guarda marcas de ángulo o segmento; flag-guardado sin punto limpio en el render. Mutacion: transforma objetos. Riesgo: bajo.
- `SetLevelOfDetail[etiqueta, n]`: Guarda el nivel de detalle 0..=2; flag-guardado con respeto mínimo en P3c. Mutacion: transforma objetos. Riesgo: bajo.
- `SetVisibleInView[etiqueta, vista]`: Error honesto: una sola vista por documento, sin vistas múltiples nombradas. Mutacion: solo consulta. Riesgo: bajo.
- `SetImage[etiqueta, ruta]`: Error honesto: sin pipeline de imágenes para objetos en esta versión. Mutacion: solo consulta. Riesgo: bajo.
- `ToolImage[etiqueta]`: Error honesto: Tool::Image no disponible en esta versión. Mutacion: solo consulta. Riesgo: bajo.
- `PlaySound[ruta]`: Error honesto: sin pipeline de reproducción de audio en la app. Mutacion: solo consulta. Riesgo: bajo.
- `StartRecord[]`: Error honesto: sin grabación de pantalla en la app. Mutacion: solo consulta. Riesgo: bajo.
- `SlowPlot[objeto]`: Error honesto: sin plantilla de trazado progresivo en el motor. Mutacion: solo consulta. Riesgo: bajo.
- `SetLineOpacity[etiqueta, opacidad]`: Cambia la opacidad de línea reescribiendo el alfa del color. Mutacion: transforma objetos. Riesgo: bajo.
- `SetPointSize[etiqueta, tamaño]`: Cambia el tamaño de punto 2D, 3D o nube. Mutacion: transforma objetos. Riesgo: bajo.
- `ExportImage[ruta]`: Valida la ruta PNG o SVG; la escritura la hace la UI sin I/O en comandos. Mutacion: solo consulta. Riesgo: bajo.
- `GetTime[]`: Hora actual del sistema como lista año, mes, día, hora, min, seg. Mutacion: solo consulta. Riesgo: bajo.
- `Name[etiqueta]`: Etiqueta existente verificada del objeto. Mutacion: solo consulta. Riesgo: bajo.
- `DynamicCoordinates[punto]`: Coordenadas vivas de un punto 2D. Mutacion: solo consulta. Riesgo: bajo.
- `Corner[n]`: Esquina visible de la vista en mundo: Corner[1..=4]. Mutacion: solo consulta. Riesgo: bajo.
- `ConstructionStep[n]`: Error honesto: el protocolo vive en la app y requiere cableado UI P3c. Mutacion: solo consulta. Riesgo: bajo.
- `SetConstructionStep[n]`: Error honesto: sin rebobinado del documento; requiere cableado UI P3c. Mutacion: solo consulta. Riesgo: bajo.
## Estadística

- `SD[lista]`: Desvío estándar poblacional (÷n) de una lista: SD[lista]. Mutacion: solo consulta. Riesgo: bajo. Alias: `desvio_poblacional`, `stdevp`.
- `SampleVariance[lista]`: Varianza muestral (÷n−1) de una lista: SampleVariance[lista]. Mutacion: solo consulta. Riesgo: bajo. Alias: `varianza_muestral`.
## Dinámica

- `SetSeed[semilla]`: Fija la semilla de los comandos aleatorios (RandomBetween, RandomPolynomial, Shuffle, Sample, Random*): SetSeed[entero]. Determinista: misma semilla → misma secuencia. Mutacion: transforma objetos. Riesgo: bajo. Alias: `semilla`, `fijar_semilla`.
- `CASLoaded[]`: Indica si el motor CAS está disponible: CASLoaded[] → verdadero/falso. Mutacion: solo consulta. Riesgo: bajo. Alias: `cas_cargado`.
- `CopyFreeObject[etiqueta]`: Crea una copia libre (sin dependencias) de un objeto existente: CopyFreeObject[etiqueta]. Mutacion: crea objetos. Riesgo: bajo. Alias: `copiar_libre`, `copia_libre`.
- `SetBackgroundColor[color]`: Fondo del lienzo 2D (color nombrado red/green/blue/black/white/gray o "r,g,b" 0..1): SetBackgroundColor[color]. Con objeto, tiñe su relleno si lo tiene. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `SetBackgroundColor[objeto, color]`. Alias: `color_fondo`.
- `SetSpinSpeed[grados]`: Velocidad de giro automático de la vista 3D (grados por segundo, 0 = quieto): SetSpinSpeed[grados]. Mutacion: transforma objetos. Riesgo: bajo. Alias: `velocidad_giro`.
- `AttachCopyToView[etiqueta, vista]`: Copia un objeto y la adjunta a una vista (0 = 2D, 1 = 3D): AttachCopyToView[etiqueta, vista]. Mutacion: crea objetos. Riesgo: bajo. Alias: `adjuntar_vista`.
- `Object[nombre]`: Etiqueta resuelta de un nombre dinámico: Object["A"] → A. Deprecado en GeoGebra; en Grafito devuelve la etiqueta existente o error honesto. Mutacion: solo consulta. Riesgo: bajo. Alias: `objeto_por_nombre`.
## Análisis

- `Slope[objeto]`: Pendiente de una recta o de una función en x=0 (derivada numérica): Slope[objeto]. Mutacion: solo consulta. Riesgo: bajo. Alias: `pendiente`.
## Dinámica

- `SetValue[nombre, valor]`: Asigna valor a una variable libre o mueve un punto libre: SetValue[nombre, valor] o SetValue[punto, (x, y)]. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `SetValue[punto, (x, y)]`. Alias: `fijar_valor`.
- `TurtleForward[n]`: Tortuga Logo: avanza n dibujando si el lápiz está abajo. Estado persistente del documento. Mutacion: crea objetos. Riesgo: bajo. Alias: `tortuga_avanza`.
- `TurtleBack[n]`: Tortuga Logo: retrocede n dibujando si el lápiz está abajo. Mutacion: crea objetos. Riesgo: bajo. Alias: `tortuga_retrocede`.
- `TurtleLeft[grados]`: Tortuga Logo: gira a la izquierda (antihorario) los grados dados. Mutacion: transforma objetos. Riesgo: bajo. Alias: `tortuga_izquierda`.
- `TurtleRight[grados]`: Tortuga Logo: gira a la derecha (horario) los grados dados. Mutacion: transforma objetos. Riesgo: bajo. Alias: `tortuga_derecha`.
- `TurtleUp[]`: Tortuga Logo: levanta el lápiz (deja de dibujar). Mutacion: transforma objetos. Riesgo: bajo. Alias: `tortuga_arriba`.
- `TurtleDown[]`: Tortuga Logo: baja el lápiz (vuelve a dibujar). Mutacion: transforma objetos. Riesgo: bajo. Alias: `tortuga_abajo`.
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
