# Referencia de Comandos de Grafito

Esta hoja de referencia resume la sintaxis de texto de los comandos soportados por la barra de entrada y la paleta de comandos (`Ctrl+K`). Todos los ejemplos asumen un documento 2D vacío salvo que se indique lo contrario.

---

## Objetos básicos

### `Point[(x, y)]` o `A = (x, y)`
Crea un punto libre en las coordenadas indicadas.

```text
A = (1, 2)
B = (4, 6)
```
**Resultado:** dos puntos libres etiquetados `A` y `B`.

### `Line[(x1, y1), (x2, y2)]`
Crea una recta infinita que pasa por dos puntos.

```text
Line[(0, 0), (1, 1)]
```
**Resultado:** recta `l` con pendiente 1 que pasa por el origen.

### `Segment[(x1, y1), (x2, y2)]`
Crea un segmento de línea.

```text
Segment[(0, 0), (3, 0)]
```
**Resultado:** segmento `s` de longitud 3 sobre el eje X.

### `Circle[center, radio]`
Crea una circunferencia.

```text
Circle[(0, 0), 2]
```
**Resultado:** círculo `c` centrado en el origen con radio 2.

### `Polygon[(x1, y1), (x2, y2), ..., (xn, yn)]`
Crea un polígono cerrado a partir de sus vértices.

```text
Polygon[(0, 0), (2, 0), (1, 2)]
```
**Resultado:** triángulo `poly1` con área 2.

### `Function[expr]`
Grafica una función explícita `y = f(x)`.

```text
f(x) = sin(x)
Function[x^2 - 3x]
```
**Resultado:** curva `f` para el primer caso; parábola `g` para el segundo.

### `ParametricCurve2D[x(t), y(t), t_min, t_max]`
Curva paramétrica 2D.

```text
ParametricCurve2D[cos(t), sin(t), 0, 2pi]
```
**Resultado:** circunferencia paramétrica de radio 1.

### `Surface3D[x(u,v), y(u,v), z(u,v), u_min, u_max, v_min, v_max]`
Superficie paramétrica 3D.

```text
Surface3D[cos(u)sin(v), sin(u)sin(v), cos(v), 0, 2pi, 0, pi]
```
**Resultado:** esfera unitaria representada como malla 3D.

---

## Construcciones

### `Midpoint[A, B]`
Punto medio de dos puntos o extremos de un segmento.

```text
A = (0, 0)
B = (4, 0)
Midpoint[A, B]
```
**Resultado:** punto `M` en `(2, 0)`.

### `Perpendicular[ punto | recta, recta ]`
Crea una recta perpendicular.

```text
l = Line[(0, 0), (1, 0)]
A = (2, 2)
Perpendicular[A, l]
```
**Resultado:** recta vertical que pasa por `A`.

### `Parallel[ punto, recta ]`
Recta paralela a otra que pasa por un punto.

```text
l = Line[(0, 0), (1, 1)]
A = (0, 2)
Parallel[A, l]
```
**Resultado:** recta `y = x + 2`.

### `Intersect[obj1, obj2]`
Puntos de intersección de dos objetos.

```text
c = Circle[(0, 0), 2]
l = Line[(-3, 0), (3, 0)]
Intersect[c, l]
```
**Resultado:** dos puntos en `(-2, 0)` y `(2, 0)`.

### `Translate[obj, vector]`
Traslada un objeto por un vector.

```text
c = Circle[(0, 0), 1]
Translate[c, (3, 0)]
```
**Resultado:** círculo centrado en `(3, 0)`.

### `Rotate[obj, centro, ángulo]`
Rota un objeto alrededor de un centro.

```text
A = (1, 0)
Rotate[A, (0, 0), 90]
```
**Resultado:** punto en `(0, 1)`.

### `Reflect[obj, eje]`
Refleja un objeto respecto a una recta.

```text
A = (1, 2)
l = Line[(0, 0), (1, 0)]
Reflect[A, l]
```
**Resultado:** punto en `(1, -2)`.

### `Dilate[obj, centro, factor]`
Homotecia respecto a un centro.

```text
A = (1, 0)
Dilate[A, (0, 0), 3]
```
**Resultado:** punto en `(3, 0)`.

---

## Restricciones numéricas

### `Distance[A, B, valor]`
Fija o impone una distancia entre dos puntos.

```text
A = (0, 0)
B = (3, 0)
Distance[A, B, 5]
```
**Resultado:** se ajusta `B` para que `|AB| = 5`.

### `Angle[l1, l2, grados]`
Fija el ángulo entre dos rectas.

```text
l1 = Line[(0, 0), (1, 0)]
l2 = Line[(0, 0), (1, 1)]
Angle[l1, l2, 90]
```
**Resultado:** `l2` se ajusta para formar 90° con `l1`.

### `Tangent[obj1, obj2]`
Restricción de tangencia entre círculo/recta o círculo/círculo.

```text
c1 = Circle[(0, 0), 2]
c2 = Circle[(3, 0), 1]
Tangent[c1, c2]
```
**Resultado:** los círculos se ajustan hasta ser tangentes.

### `Coincident[A, B]`
Fuerza la coincidencia de dos puntos.

```text
A = (0, 0)
B = (1, 1)
Coincident[A, B]
```
**Resultado:** ambos puntos comparten la misma posición.

### `Horizontal[obj]`
Fuerza a que una recta o segmento sea horizontal.

```text
s = Segment[(0, 0), (1, 1)]
Horizontal[s]
```
**Resultado:** el segmento se alinea horizontalmente.

### `Vertical[obj]`
Fuerza a que una recta o segmento sea vertical.

```text
l = Line[(0, 0), (2, 3)]
Vertical[l]
```
**Resultado:** la recta se hace vertical.

### `EqualLength[s1, s2]`
Iguala las longitudes de dos segmentos.

```text
a = Segment[(0, 0), (2, 0)]
b = Segment[(0, 0), (1, 0)]
EqualLength[a, b]
```
**Resultado:** el segundo segmento se alarga hasta longitud 2.

### `Symmetry[P, Q, eje]`
Fuerza a que `P` y `Q` sean simétricos respecto a una recta.

```text
P = (1, 2)
Q = (0, 0)
l = Line[(0, 0), (1, 0)]
Symmetry[P, Q, l]
```
**Resultado:** `P` se ajusta para ser el reflejo de `Q` respecto a `l`.

---

## Cónicas avanzadas

### `EllipseByFoci[F1, F2, P]`
Elipse con focos `F1` y `F2` que pasa por `P`.

```text
F1 = (-3, 0)
F2 = (3, 0)
P = (5, 0)
EllipseByFoci[F1, F2, P]
```
**Resultado:** elipse con semieje mayor 5 y focos en `±3`.

### `ParabolaByFocusDirectrix[F, d]`
Parábola con foco `F` y directriz `d`.

```text
F = (0, 2)
d = Line[(0, -1), (1, -1)]
ParabolaByFocusDirectrix[F, d]
```
**Resultado:** parábola con vértice en `(0, 0.5)` y eje vertical.

### `HyperbolaByFoci[F1, F2, P]`
Hipérbola con focos `F1` y `F2` que pasa por `P`.

```text
F1 = (-5, 0)
F2 = (5, 0)
P = (3, 0)
HyperbolaByFoci[F1, F2, P]
```
**Resultado:** hipérbola horizontal que pasa por `(3, 0)`.

### `ConicByFivePoints[A, B, C, D, E]`
Cónica general que pasa por cinco puntos.

```text
A = (1, 0)
B = (0, 1)
C = (-1, 0)
D = (0, -1)
E = (0.5, 0.5)
ConicByFivePoints[A, B, C, D, E]
```
**Resultado:** cónica (en este caso una elipse) que interpola los cinco puntos.

---

## Operaciones booleanas 2D

### `PolygonUnion[poly1, poly2]`
Unión de dos polígonos.

```text
r1 = Polygon[(0,0), (2,0), (2,2), (0,2)]
r2 = Polygon[(1,1), (3,1), (3,3), (1,3)]
PolygonUnion[r1, r2]
```
**Resultado:** polígono `U` que cubre el área combinada.

### `PolygonIntersection[poly1, poly2]`
Intersección de dos polígonos.

```text
r1 = Polygon[(0,0), (2,0), (2,2), (0,2)]
r2 = Polygon[(1,1), (3,1), (3,3), (1,3)]
PolygonIntersection[r1, r2]
```
**Resultado:** cuadrado `I` de vértices `(1,1)` a `(2,2)`.

### `PolygonDifference[poly1, poly2]`
Diferencia `poly1 - poly2`.

```text
r1 = Polygon[(0,0), (3,0), (3,3), (0,3)]
r2 = Polygon[(1,1), (2,1), (2,2), (1,2)]
PolygonDifference[r1, r2]
```
**Resultado:** polígono `D` cuadrado grande con un agujero central.

### `PolygonXor[poly1, poly2]`
Diferencia simétrica.

```text
r1 = Polygon[(0,0), (2,0), (2,2), (0,2)]
r2 = Polygon[(1,1), (3,1), (3,3), (1,3)]
PolygonXor[r1, r2]
```
**Resultado:** polígono `X` con las áreas no superpuestas.

---

## Enlace de expresiones

### `PointExpr[x_expr, y_expr]`
Punto cuyas coordenadas dependen de expresiones.

```text
PointExpr[cos(t), sin(t)]
```
**Resultado:** punto que se mueve sobre la circunferencia unitaria cuando `t` cambia.

### `CircleExpr[centro, radius_expr]`
Círculo con radio ligado a una expresión.

```text
CircleExpr[(0, 0), 1 + 0.5*sin(t)]
```
**Resultado:** círculo cuyo radio oscila entre 0.5 y 1.5.

### `LineExpr[(x1_expr, y1_expr), (x2_expr, y2_expr)]`
Recta cuyos extremos dependen de expresiones.

```text
LineExpr[(t, 0), (t, 1)]
```
**Resultado:** segmento vertical que se desplaza horizontalmente con `t`.

### `PolygonExpr[(x1,y1), ..., (xn,yn)]`
Polígono con vértices expresados simbólicamente.

```text
PolygonExpr[(0, 0), (cos(t), sin(t)), (sin(t), cos(t))]
```
**Resultado:** triángulo cuyos vértices 2 y 3 rotan.

### `FunctionExpr[expr]`
Función cuya expresión puede contener variables.

```text
FunctionExpr[a*x^2 + b*x + c]
```
**Resultado:** familia de parábolas controlada por `a`, `b` y `c`.

### `ParametricExpr[x(t), y(t), t_min, t_max]`
Curva paramétrica con expresiones en `t`.

```text
ParametricExpr[t, a*sin(t), -2pi, 2pi]
```
**Resultado:** sinusoide cuya amplitud depende de `a`.

---

## Cálculo simbólico

### `Integral[expr, x, a, b]`

Calcula la integral definida de `expr` respecto a `x` entre `a` y `b`.

```text
Integral[x^2, x, 0, 1]
Integral[sin(x), x, 0, pi]
```

**Resultado:** valor numérico aproximado de la integral.

> **Nota de implementación:** si hay un evaluador GPU registrado y la
> expresión es compatible con el pipeline `function_compute`, la integral usa
> una ruta híbrida: el GPU evalúa `f(x)` en una grilla densa y el CPU aplica
> una regla de cuadratura compuesta (Simpson/trapecio). En caso contrario se
> resuelve con la cuadratura adaptativa de CPU existente.

### `Derivative[expr, x]`

Calcula la derivada simbólica de `expr` respecto a `x` y la grafica.

```text
Derivative[x^3 + 2x, x]
```

**Resultado:** expresión derivada graficada como una nueva función.

---

## Notas generales

- Las etiquetas de objeto se generan automáticamente (`A`, `B`, `l`, `c`, `poly1`, etc.).
- Los ángulos se expresan en grados salvo que una función trigonométrica indique radianes.
- Los comandos de restricción numérica requieren que los objetos de entrada ya existan en el documento.
- Las expresiones enlazadas se reevalúan automáticamente cuando cambian las variables del documento.
