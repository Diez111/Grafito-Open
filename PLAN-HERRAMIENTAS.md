# Plan: Herramientas Profesionales + Hover de Intersecciones + Toolbar Fix

## Fase 1: Fix del Toolbar (crítico, 21 herramientas rotas)
- Arreglar mapeos rotos (Segmento→Segment, Polígono regular→RegularPolygon, etc.)
- Agregar grupos nuevos: ANALYSIS, CONSTRAINT, BOOLEAN
- Agregar herramientas faltantes: Ray, Vector, ImplicitCurve, VectorField2D, ConicByFivePoints
- Eliminar entradas rotas: Texto, 3D shapes inexistentes, duplicados

## Fase 2: Hover de Intersecciones
- snap_to_features: computar intersecciones entre pares de objetos cercanos al cursor
- Mostrar "Intersección: (x, y)" al hover sobre line-circle, circle-circle, etc.

## Fase 3: Fix Curve Snap para Círculos y Líneas
- snap_to_curve: retornar SnapResult en lugar de descartar con `let _ = c;`

## Fase 4: Tangente en Punto de Función
- Nuevo comando TangentAt[función, x] → crea línea tangente
- Nuevo comando NormalAt[función, x] → crea línea normal

## Fase 5: Longitud de Arco
- Nuevo comando ArcLength[función, a, b] → ∫√(1+f'²)dx

## Fase 6: Curvatura
- Función curvature_at(expr, x) → κ = |f''| / (1+f'²)^(3/2)

## Fase 7: Volumen de Revolución
- VolumeOfRevolution[función, a, b] → π∫f²dx
- SurfaceOfRevolution[función, a, b] → 2π∫f√(1+f'²)dx
