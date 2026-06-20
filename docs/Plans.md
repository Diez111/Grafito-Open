# Grafito — Plan de Desarrollo

> **Versión:** 0.6.0-alpha | **24 commits** | **~9300 líneas Rust** | **33 tests**

## DoD (Definition of Done) por Feature

Cada feature se considera completa cuando:
1. Compila sin errores ni warnings (`cargo check`)
2. Tiene al menos 1 test unitario que verifica el comportamiento esperado
3. Está documentada en el README
4. No introduce `unwrap()` sin contexto ni `#[allow(dead_code)]`
5. El código está en el archivo correcto (no en main.rs si es genérico)

---

## Fase 0: Infraestructura ✅

- [x] Workspace con 5 crates
- [x] eframe 0.29 + wgpu backend
- [x] Compilación limpia
- [ ] CI/CD (GitHub Actions) ← PENDIENTE
- [ ] Tests unitarios ← PENDIENTE (0 tests)

## Fase 1: Canvas 2D ✅

- [x] Grid, ejes, pan/zoom
- [x] 5 herramientas (Select, Point, Line, Circle, Polygon)
- [x] Hit testing O(n) → [ ] Actualizar a O(log n) con spatial_index
- [x] Snap to grid toggle
- [x] Zoom-to-fit

## Fase 2: Geometría 2D ✅

- [x] Point, Line, Circle, Polygon, Ellipse, Parabola, Hyperbola
- [x] Function plotting con 1000 samples paralelos (rayon)
- [x] Asymptote detection via interval arithmetic
- [x] Text objects con rendering
- [x] 29 variantes de GeoObject

## Fase 3: Álgebra + Sliders ✅

- [x] Variables (`a=5`, `f(x)=a*x^2`)
- [x] Sliders con animación paramétrica
- [x] Panel de variables en sidebar

## Fase 4: Gráficos 3D ✅

- [x] Cámara orbital (rotate/zoom/pan)
- [x] 8 sólidos wireframe (Point3D, Segment3D, Sphere3D, Cube3D, Pyramid3D, Cone3D, Cylinder3D, Surface3D)
- [x] Ejes XYZ con labels y arrow tips
- [x] Curvas paramétricas 3D (`Curve3D[(t,sin(t),cos(t)), t, 0, 6.28]`)
- [x] Extrusión 2D→3D (`Extrude[polygon, height]`)
- [ ] Surface of revolution ← PENDIENTE

## Fase 5: CAS ✅

- [x] Derivada numérica (diferencias finitas)
- [x] Integral numérica (Simpson)
- [x] Newton root finding
- [x] Límites (Richardson extrapolation)
- [x] Symbolic (Factor, Expand, Simplify)
- [x] Comandos CAS: `Derivative[expr]`, `Integral[expr,a,b]`, `Solve[expr,a,b]`, `Limit[expr,x0]`
- [x] FunctionInspector (raíces + extremos)
- [ ] Integrar rug::Float en el pipeline de evaluación ← PENDIENTE (módulo creado, sin cablear)

## Fase 6: Avanzado ✅

- [x] Export SVG, PNG, TikZ/LaTeX
- [x] Save/Load documentos (.grafito JSON)
- [x] Undo/Redo (Ctrl+Z/Y) + Delete key
- [x] Spreadsheet View (grid A-H × 20 filas)
- [x] Status bar (object count, view mode, scale, CAS result)
- [x] Recent files (File > Recent submenu)
- [x] Right-click context menu (2D y 3D)
- [x] Measurements en properties panel
- [x] Transformaciones (Translate, Rotate, Dilate, Reflect)
- [x] Herramientas geométricas (Tangent, PerpBisector, AngleBisector, Midpoint, Vector, Ray, RegularPolygon, Locus)
- [x] Probabilidad (Normal, Binomial, Poisson)
- [x] Scripting básico (`Script[cmd1;cmd2]`, `SetValue[label,val]`)
- [x] Exam Mode (bloquea CAS/input/File)
- [x] Constraint Graph (módulo creado) → [ ] Cablear a add_object/update cascade

---

## Tech Debt (priorizado)

### P0 — Bloqueantes para beta
1. **0 tests** — Agregar tests unitarios para geometry/core, tests de integración para app
2. **main.rs: 2200+ líneas** — Modularizar en ~7 archivos
3. **Sin CI/CD** — Configurar GitHub Actions
4. **README desactualizado** — Actualizar con features reales
5. **3 módulos sin integrar**: spatial_index, constraint_graph, interval arithmetic

### P1 — Calidad de código
6. Match arms duplicados en GeoObject — Crear macro `geo_match!`
7. thiserror sin usar — Definir `GrafitoError`
8. anyhow sin usar — Eliminar o usar
9. Magic numbers sin nombrar — Constantes para FUNCTION_SAMPLES, MAX_GAP, etc.
10. SVG/TikZ/PNG export duplicados — Unificar en trait `Exportable`

### P2 — Features pendientes
11. Surface of revolution en 3D
12. rug::Float integrado en CAS pipeline
13. GPU compute shaders para evaluación masiva
14. Undo/Redo por comando (no snapshots)
15. Soporte para archivos .ggb (GeoGebra XML)

---

## Dependencias pendientes de instalar

```bash
# Para rug (MPFR precisión arbitraria)
sudo apt install libgmp-dev libmpfr-dev libmpc-dev m4

# Para symbolica (CAS simbólico completo)
# Requiere m4 instalado — ya disponible
```

## Comandos de verificación

```bash
cargo check                    # Compilación
cargo test                     # Tests (0 actualmente)
cargo clippy                   # Linting
cargo build --release          # Build optimizado
cargo doc --no-deps --open    # Documentación
```
