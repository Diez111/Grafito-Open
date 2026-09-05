# Referencia de Comandos de Grafito

<!-- Generated from crates/grafito-command/src/command_registry.rs; do not edit manually. -->

Esta referencia se genera desde el registro de comandos estable. El parser y sus fallbacks siguen en `commands.rs`; el registro documenta sus metadatos, no reemplaza el despacho.

## Crear

- `Point[(x, y)]`: Crea un punto libre. Mutacion: crea objetos. Riesgo: bajo.
- `Circle[centro, radio]`: Crea una circunferencia. Mutacion: crea objetos. Riesgo: bajo.
- `Polygon[(x1, y1), ...]`: Crea un poligono cerrado. Mutacion: crea objetos. Riesgo: bajo.
- `Function[expr]`: Grafica una funcion explicita. Mutacion: crea objetos. Riesgo: bajo. Alias: `func`.
## Dinámica

- `Animate[]`: Anima un parametro local; sin argumentos crea una fase ciclica. Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `Animate[variable]`, `Animate[variable, minimo, maximo, velocidad]`. Alias: `animar`.
## Animaciones

- `GenerateAnimation[template, concepto]`: Genera una animación didáctica (placeholder o Manim) para el concepto dado. Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `GenerateAnimation[template]`, `GenerateAnimation[]`.
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
- `LocusEquation[locus]`: Aproxima eliminación Groebner (mock) a partir de muestreo de locus + regresión simbólica; genera curva implícita presupuestada. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `LocusEquation[locus, grado]`. Alias: `locus_equation`, `ecuacionlocus`, `ecuacion_locus`.
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
- `Reflect[obj, punto_a, punto_b]`: Refleja un objeto respecto a un eje (linea) o a un circulo (inversion). Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Reflect[obj, circulo]`. Alias: `mirror`.
- `Shear[objeto, angulo, eje]`: Aplica cizallamiento afin: x' = x + k*y con k = tan(angulo). Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Shear[objeto, angulo]`. Alias: `cizalla`, `trasquilacion`.
- `Stretch[objeto, factor, eje]`: Aplica estiramiento afin: x' = factor*x (o y' = factor*y segun eje). Mutacion: transforma objetos. Riesgo: medio. Formas alternativas: `Stretch[objeto, factor]`. Alias: `estirar`, `estiramiento`.
## Crear

- `FractionText[valor]`: Crea texto con valor fraccionario: FractionText[0.5] -> "1/2". Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `FractionText[valor, punto]`. Alias: `fraccion`, `fraction`.
- `SurdText[valor]`: Crea texto con surd: SurdText[1.414] -> "√2". Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `SurdText[valor, punto]`. Alias: `surd`, `raiztexto`.
## Estadística

- `FillColumn[col, valor]`: Rellena una columna de la hoja iterando filas y escribiendo valor; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillColumn[col, inicio, fin, valor]`. Alias: `fill_column`, `fillcol`.
- `FillCells[rango, valor]`: Rellena un rango rectangular de celdas con un valor; respeta presupuestos de spreadsheet. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillCells[a1, b2, valor]`. Alias: `fill_cells`, `rellenar`.
- `CellRange[a1, b2]`: Resuelve un rango A1:B2 a array de valores evaluados; soporta A1:B2 o A1,B2. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CellRange[rango]`. Alias: `cell_range`, `rango`.
- `FillRow[fila, valor]`: Rellena una fila de la hoja iterando columnas y escribiendo valor; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE. Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `FillRow[fila, inicio, fin, valor]`. Alias: `fill_row`.
## Restricciones

- `Distance[A, B, valor]`: Impone una distancia entre objetos. Mutacion: agrega restricciones. Riesgo: medio. Alias: `dist`.
- `Angle[l1, l2, grados]`: Impone un angulo entre objetos. Mutacion: agrega restricciones. Riesgo: medio.
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
- `LimitAbove[expr, variable, punto]`: Estima un límite lateral por la derecha (x→a⁺). Mutacion: solo consulta. Riesgo: medio. Alias: `limite_superior`, `limite_derecho`.
- `LimitBelow[expr, variable, punto]`: Estima un límite lateral por la izquierda (x→a⁻). Mutacion: solo consulta. Riesgo: medio. Alias: `limite_inferior`, `limite_izquierdo`.
- `ParametricDerivative[x(t), y(t), variable]`: Deriva paramétrica dy/dx = (dy/dt)/(dx/dt) simbólicamente. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ParametricDerivative[x(t), y(t)]`. Alias: `derivada_parametrica`, `derivadaParametrica`.
- `Asymptote[expr]`: Calcula asíntota oblicua y = m·x + b con m = lim f/x, b = lim f−m·x. Mutacion: solo consulta. Riesgo: medio. Formas alternativas: `Asymptote[expr, variable]`. Alias: `asintota`, `asíntota`.
- `GroebnerDegRevLex[polinomios]`: Base de Groebner (stub: no implementado, use Eliminate). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `GroebnerDegRevLex[polinomios, variables]`. Alias: `groebner`, `groebnerbasis`, `groebnerlex`, `groebner_basis`.
- `Factor[expr, variable]`: Factoriza polinomios equivalentes. Mutacion: solo consulta. Riesgo: bajo. Alias: `factorizar`.
- `Expand[expr]`: Expande productos y potencias algebraicas. Mutacion: solo consulta. Riesgo: bajo. Alias: `expandir`.
- `Simplify[expr]`: Simplifica una expresion mediante reglas seguras. Mutacion: solo consulta. Riesgo: bajo. Alias: `simplificar`.
- `Taylor[expr, variable, centro, orden]`: Construye una serie de Taylor finita. Mutacion: crea objetos. Riesgo: medio.
- `CompleteSquare[expr, variable]`: Completa cuadrado: convierte a*x^2+b*x+c a a*(x+b/2a)^2 + (c - b^2/4a). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `CompleteSquare[expr]`. Alias: `complete_square`, `completarCuadrado`, `completar_cuadrado`.
- `PrimeFactors[n]`: Factoriza un entero n (2 <= n <= 1e12) en primos por trial division. Mutacion: solo consulta. Riesgo: bajo. Alias: `prime_factors`, `factoresPrimos`, `factores_primos`.
- `IFactor[expr]`: Factorización entera: si es entero usa PrimeFactors, si es polinomio extrae contenido entero y lo factoriza. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `IFactor[expr, variable]`. Alias: `ifactorizar`, `factorEntero`, `factor_entero`.
- `Assume[predicado]`: Almacena hipótesis como x>0 (positive), x!=0 (nonzero), x real/integer; guarda en Document.variables_assumptions. Mutacion: solo consulta. Riesgo: bajo. Alias: `asumir`, `suponer`, `supone`.
## Análisis

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
## Estadística

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
- `FitLogistic[tabla]`: Ajusta a/(1+b*exp(-c*x)) con Gauss-Newton acotado MAX_ITER 100 y tolerancia 1e-6; genera función y métricas RMSE/R². Mutacion: crea objetos. Riesgo: medio. Alias: `fit_logistic`, `logistica`, `ajuste logistico`.
- `FitGrowth[tabla]`: Ajusta a*exp(b*x) con Gauss-Newton acotado MAX_ITER 100 y tolerancia 1e-6. Mutacion: crea objetos. Riesgo: medio. Alias: `fit_growth`, `crecimiento`, `ajuste crecimiento`.
- `FitImplicit[tabla, expr]`: Ajuste implícito genérico Gauss-Newton: FitImplicit[tabla, exprConParams, a0, b0, ...] minimiza y - expr(x; params). Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `FitImplicit[tabla, expr, a0, b0, c0]`. Alias: `fit_implicit`, `implicit_fit`, `ajuste implicito`.
- `Mean[{data}]`: Calcula la media. Mutacion: solo consulta. Riesgo: bajo. Alias: `media`.
- `Median[{data}]`: Calcula la mediana. Mutacion: solo consulta. Riesgo: bajo. Alias: `mediana`.
- `StdDev[{data}]`: Calcula el desvio estandar. Mutacion: solo consulta. Riesgo: bajo. Alias: `desviacion`.
- `Correlation[{xs}, {ys}]`: Calcula una correlacion. Mutacion: solo consulta. Riesgo: bajo. Alias: `correlacion`.
## Probabilidad

- `InverseNormal[p]`: Cuantil normal: InverseNormal[p, mu, sigma] (p en (0,1), sigma>0); con un arg usa N(0,1). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `InverseNormal[p, mu, sigma]`. Alias: `inverse_normal`, `cuantilnormal`, `cuantil_normal`.
- `InverseT[p, df]`: Cuantil t-Student: InverseT[p, df] (p en (0,1), df>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_t`, `cuantilt`, `cuantil_t`.
- `InverseChiSquared[p, df]`: Cuantil chi-cuadrado: InverseChiSquared[p, df] (p en (0,1), df>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_chi_squared`, `inversachicuadrado`, `cuantilchicuadrado`.
- `InverseF[p, df1, df2]`: Cuantil F de Fisher: InverseF[p, df1, df2] (p en (0,1), df1>0, df2>0). Mutacion: solo consulta. Riesgo: bajo. Alias: `inverse_f`, `cuantilf`, `cuantil_f`.
## Estadística

- `FrequencyTable[{datos}]`: Tabla de frecuencias: FrequencyTable[{datos}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `frequency_table`, `frecuencia`, `tabl frecuencias`.
- `StemPlot[{datos}]`: Diagrama tallo-hoja: StemPlot[{datos}] texto. Mutacion: solo consulta. Riesgo: bajo. Alias: `stem_plot`, `stemleaf`, `tallo_hoja`, `diagrama_tallo`.
- `ResidualPlot[{xs}, {ys}]`: Residuos de regresión lineal: ResidualPlot[{xs}, {ys}] o ResidualPlot[tabla]. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `ResidualPlot[tabla]`. Alias: `residual_plot`, `grafico_residuos`.
- `TTest[{datos}, mu0]`: Prueba t de una muestra: TTest[{datos}, mu0]. Mutacion: solo consulta. Riesgo: bajo. Alias: `t_test`, `prueba_t`.
- `TTest2[{a}, {b}]`: Prueba t de dos muestras independientes: TTest2[{a}, {b}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `t_test2`, `prueba_t2`.
- `TTestPaired[{a}, {b}]`: Prueba t pareada: TTestPaired[{antes}, {despues}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `ttest_paired`, `t_paired`, `prueba_t_pareada`, `ttestpareado`.
- `ZTest[{datos}, mu0, sigma]`: Prueba z de una muestra con sigma conocido: ZTest[{datos}, mu0, sigma]. Mutacion: solo consulta. Riesgo: bajo. Alias: `z_test`, `prueba_z`.
- `ChiSqTest[{obs}, {esp}]`: Prueba chi-cuadrado de bondad de ajuste: ChiSqTest[{obs}, {esp}]. Mutacion: solo consulta. Riesgo: bajo. Alias: `chi2test`, `prueba_chi2`, `chi_cuadrado`.
- `ANOVA[{g1}, {g2}]`: ANOVA de un factor: ANOVA[{g1}, {g2}, ...]. Mutacion: solo consulta. Riesgo: bajo. Alias: `anova_oneway`.
## Financiera

- `Rate[nper, pmt, pv, fv]`: Calcula la tasa periodica (tipo 0=anual) resolviendo TVM con exp/log; 4-5 args. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Rate[nper, pmt, pv, fv, tipo]`. Alias: `tasa`, `tipo`.
- `Nper[rate, pmt, pv, fv]`: Calcula numero de periodos via TVM con exp/log; usa log((pmt*(1+r*tipo)-fv*r)/(pmt*(1+r*tipo)+pv*r))/log(1+r). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Nper[rate, pmt, pv, fv, tipo]`. Alias: `n_per`, `periodos`, `plazo`.
- `Pmt[rate, nper, pv, fv]`: Calcula el pago periodico TVM; 4-5 args con tipo 0/1. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Pmt[rate, nper, pv, fv, tipo]`. Alias: `pago`, `cuota`.
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
- `Torus[x, y, z, major_radius, minor_radius]`: Crea un toro 3D. Mutacion: crea objetos. Riesgo: alto.
- `Moebius[radius, width]`: Crea una banda de Moebius 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `mobius`.
- `Curve3D[(x(t), y(t), z(t)), t, tmin, tmax]`: Crea una curva parametrica 3D. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Curve3D[(x(t), y(t), z(t)), tmin, tmax]`.
- `Surface3D[f(x, y), xmin, xmax, ymin, ymax]`: Crea una superficie 3D parametrica o explicita. Mutacion: crea objetos. Riesgo: alto. Formas alternativas: `Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]`, `Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]`.
- `ComplexSurface[expr, xmin, xmax, ymin, ymax, resolution]`: Grafica el modulo de una funcion compleja como superficie 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `complex_surface`, `csurface`.
- `Extrude[polygon_label, height]`: Extruye un poligono a un solido. Mutacion: crea objetos. Riesgo: alto.
- `VectorField3D[u, v, w]`: Crea un campo vectorial 3D. Mutacion: crea objetos. Riesgo: alto. Alias: `vectorfield`.
- `Prism[poligono, altura]`: Crea un prisma extruyendo un polígono base por un vector (altura en Z o dx,dy,dz). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Prism[poligono, dx, dy, dz]`. Alias: `prisma`.
- `Net[poliedro]`: Genera el desarrollo 2D de un poliedro (stub: informa disponibilidad). Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `Net[poliedro, escala]`. Alias: `desarrollo`, `desplegado`, `unwrap`.
- `Quadric[a, b, c, d, e, f, g, h, i, j]`: Crea una cuádrica general a*x²+b*y²+c*z²+d*xy+e*yz+f*zx+g*x+h*y+i*z+j=0. Mutacion: crea objetos. Riesgo: medio. Alias: `cuadrica`, `cuádrica`.
- `Intersection3D[a, b]`: Calcula intersecciones 3D: Plano-Plano, Recta-Plano, Recta-Recta, Plano-Esfera (círculo) o Plano-Poliedro (stub). Mutacion: crea objetos. Riesgo: medio. Formas alternativas: `Intersection3D[a, b, c]`. Alias: `intersect3d`, `interseccion3d`, `intersección3d`.
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
- `DelaunayTriangulation[puntos]`: Triangulación Delaunay aproximada por abanico (fan) desde el primer punto; stub que no falla y respeta límites discretos. Mutacion: crea objetos. Riesgo: medio. Alias: `delaunay`, `triangulaciondelaunay`.
- `Voronoi[puntos]`: Diagrama de Voronoi aproximado: genera círculos stub en cada sitio cuando no hay motor exacto disponible. Mutacion: crea objetos. Riesgo: medio. Alias: `cellsvoronoi`, `diagramaVoronoi`.
- `MinimumSpanningTree[puntos]`: Árbol de expansión mínima por Prim euclídeo O(n²); crea segmentos entre puntos. Mutacion: crea objetos. Riesgo: medio. Alias: `mst`, `arbolminimo`, `kruskal`.
- `TravelingSalesman[puntos]`: Tour del viajante aproximado por vecino más cercano (greedy) empezando en el primer punto. Mutacion: crea objetos. Riesgo: medio. Alias: `tsp`, `viajante`, `travellingsalesman`.
- `ShortestDistance[punto, objeto]`: Distancia euclídea mínima entre un punto y un objeto (punto/segmento/círculo/polígono). Valida finitud y límites. Mutacion: solo consulta. Riesgo: bajo. Alias: `distanciaminima`, `closestdistance`, `distanciamínima`.
## Lista

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
## Cónicas

- `Focus[conica]`: Devuelve el/los focos de una cónica (elipse, hipérbola, parábola) usando grafito-geometry::exact. Mutacion: solo consulta. Riesgo: bajo. Alias: `Foco`, `focos`.
- `Directrix[conica]`: Devuelve la directriz de una parábola como recta (dos puntos) usando exact::parabola. Mutacion: solo consulta. Riesgo: bajo. Alias: `Directriz`.
- `Center[conica]`: Devuelve el centro (elipse/hipérbola/círculo) o vértice (parábola) usando exact::center. Mutacion: solo consulta. Riesgo: bajo. Alias: `Centro`.
- `Eccentricity[conica]`: Devuelve la excentricidad e de una cónica (0 círculo, 0<e<1 elipse, e=1 parábola, e>1 hipérbola). Mutacion: solo consulta. Riesgo: bajo. Alias: `Excentricidad`, `ecc`.
- `Axes[conica]`: Devuelve los semiejes (a,b) de elipse/hipérbola o parámetro p de parábola usando exact::axes. Mutacion: solo consulta. Riesgo: bajo. Alias: `Ejes`, `semiejes`.
- `IsTangent[recta, conica]`: Predicado exacto IsTangent[recta, elipse] usando exact::is_tangent_to_ellipse (discriminante). Mutacion: solo consulta. Riesgo: bajo. Alias: `EsTangente`.
## Texto

- `TableText[funcion, min, max, paso]`: Genera tabla LaTeX-like texto desde función+rango+step; salida string pura sin mutar documento. Mutacion: solo consulta. Riesgo: bajo. Formas alternativas: `TableText[expr, min, max, paso]`. Alias: `TablaTexto`.
## Dinámica

- `Slider[variable, min, max, paso, modo]`: Crea VariableMeta Slider[a, min, max, step, mode] con modo PingPong/Loop y velocity (animation_speed). Mutacion: crea objetos. Riesgo: bajo. Formas alternativas: `Slider[variable, min, max, paso]`. Alias: `Deslizador`.
- `Rastro[objeto]`: Activa/desactiva el rastro de un objeto: al arrastrarlo deja una estela con fade. Rastro[etiqueta] alterna; Rastro[etiqueta, true|false] fija el estado. (Trace con matriz sigue siendo traza matricial.) Mutacion: transforma objetos. Riesgo: bajo. Formas alternativas: `Rastro[objeto, estado]`. Alias: `Estela`.
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
