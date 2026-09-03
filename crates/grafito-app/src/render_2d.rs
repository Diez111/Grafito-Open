use crate::GrafitoApp;
use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2};
use glam::Vec2 as GlamVec2;
use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_core::parametric_sampling;
use grafito_core::vector_field_sampling;
use grafito_core::{GeoObject, ImplicitCurveObj, ObjectId, RelationOperator};
use grafito_geometry::expr::{
    eval_batch_1d, eval_function_with_vars, eval_integral_batch, prepare_function_ast,
};
use grafito_geometry::{Color, Point2, ViewTransform};
use grafito_ui::theme::current_theme;
use rayon::prelude::*;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

// ── F3-Render caches: fractal / phase / ordered_visible keyed por document.version ──
thread_local! {
    static FRACTAL_RENDER_CACHE: RefCell<HashMap<u64, Vec<grafito_geometry::fractals::FractalPixel>>> =
        RefCell::new(HashMap::new());
    static PHASE_PORTRAIT_CACHE: RefCell<HashMap<u64, Vec<(Point2, Point2)>>> =
        RefCell::new(HashMap::new());
    static ORDERED_VISIBLE_CACHE: RefCell<Option<(u64, Vec<ObjectId>)>> =
        const { RefCell::new(None) };
    /// Cache de ASTs complejos parseados (ComplexGrid/ComplexMapping) keyed por
    /// la expresión. Evita re-parsear `complex_expr` en cada frame (H10).
    static COMPLEX_EXPR_CACHE: RefCell<HashMap<String, grafito_complex::ComplexExpr>> =
        RefCell::new(HashMap::new());
    /// Última `document.version` en la que se ejecutó `prune_fill_texture_cache`.
    /// Permite saltar el write lock + barrido LRU cuando el documento no cambió.
    static LAST_FILL_PRUNE_DOC_VERSION: RefCell<Option<u64>> = const { RefCell::new(None) };
}
const FRACTAL_RENDER_CACHE_CAP: usize = 8;
const PHASE_RENDER_CACHE_CAP: usize = 32;
const COMPLEX_EXPR_CACHE_CAP: usize = 16;

fn fractal_render_cache_key(document_version: u64, fr: &grafito_core::Fractal2DObj) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    document_version.hash(&mut h);
    fr.id.hash(&mut h);
    fr.fractal_type.hash(&mut h);
    fr.x_min.to_bits().hash(&mut h);
    fr.x_max.to_bits().hash(&mut h);
    fr.y_min.to_bits().hash(&mut h);
    fr.y_max.to_bits().hash(&mut h);
    fr.resolution.hash(&mut h);
    fr.max_iter.hash(&mut h);
    fr.params.len().hash(&mut h);
    for p in &fr.params {
        p.to_bits().hash(&mut h);
    }
    h.finish()
}

fn phase_render_cache_key(
    document_version: u64,
    portrait: &grafito_core::PhasePortraitObj,
    variables: &HashMap<String, f64>,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    document_version.hash(&mut h);
    portrait.id.hash(&mut h);
    portrait.expr_dx.hash(&mut h);
    portrait.expr_dy.hash(&mut h);
    portrait.x_min.to_bits().hash(&mut h);
    portrait.x_max.to_bits().hash(&mut h);
    portrait.y_min.to_bits().hash(&mut h);
    portrait.y_max.to_bits().hash(&mut h);
    portrait.density.hash(&mut h);
    let mut vars: Vec<_> = variables.iter().collect();
    vars.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (k, v) in vars {
        k.hash(&mut h);
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Cachea `try_compute_fractal` keyed por `document.version` para evitar recomputar
/// el mismo fractal en cada frame (hasta 160k píxeles, trabajo pesado).
fn cached_try_compute_fractal(
    fr: &grafito_core::Fractal2DObj,
    document_version: u64,
) -> Option<Vec<grafito_geometry::fractals::FractalPixel>> {
    let key = fractal_render_cache_key(document_version, fr);
    if let Some(cached) = FRACTAL_RENDER_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Some(cached);
    }
    let fractal_type = match fr.fractal_type.as_str() {
        "julia" if fr.params.len() >= 2 => grafito_geometry::fractals::FractalType::Julia {
            cr: fr.params[0],
            ci: fr.params[1],
            max_iter: fr.max_iter,
        },
        "burning_ship" => grafito_geometry::fractals::FractalType::BurningShip {
            max_iter: fr.max_iter,
        },
        "tricorn" => grafito_geometry::fractals::FractalType::Tricorn {
            max_iter: fr.max_iter,
        },
        _ => grafito_geometry::fractals::FractalType::Mandelbrot {
            max_iter: fr.max_iter,
        },
    };
    let pixels = grafito_geometry::fractals::try_compute_fractal(
        &fractal_type,
        fr.x_min,
        fr.x_max,
        fr.y_min,
        fr.y_max,
        fr.resolution,
        fr.resolution,
    )
    .ok()?;
    FRACTAL_RENDER_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= FRACTAL_RENDER_CACHE_CAP {
            if let Some(k) = cache.keys().next().copied() {
                cache.remove(&k);
            }
        }
        cache.insert(key, pixels.clone());
    });
    Some(pixels)
}

/// Cachea `sample_phase_portrait` keyed por `document.version` para evitar
/// re-evaluar la malla vectorial (density² evaluaciones) cada frame.
fn cached_sample_phase_portrait(
    portrait: &grafito_core::PhasePortraitObj,
    variables: &HashMap<String, f64>,
    document_version: u64,
) -> Vec<(Point2, Point2)> {
    let key = phase_render_cache_key(document_version, portrait, variables);
    if let Some(cached) = PHASE_PORTRAIT_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return cached;
    }
    let segments = grafito_render::sample_phase_portrait(portrait, variables);
    PHASE_PORTRAIT_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= PHASE_RENDER_CACHE_CAP {
            if let Some(k) = cache.keys().next().copied() {
                cache.remove(&k);
            }
        }
        cache.insert(key, segments.clone());
    });
    segments
}

/// Refina discontinuidades (transiciones finito/no-finito) en muestras de
/// función mediante bisección. Consume un iterador de muestras para no copiar
/// el guard del RwLock de `samples_or_compute` (hasta 10k muestras por frame
/// en cache caliente).
fn refine_function_samples(
    samples: impl IntoIterator<Item = (f64, Option<f64>)>,
    expr: &str,
    variables: &HashMap<String, f64>,
) -> Vec<(f64, Option<f64>)> {
    let mut iter = samples.into_iter().peekable();
    let mut refined = Vec::new();
    while let Some(current) = iter.next() {
        refined.push(current);
        if let Some(&next) = iter.peek() {
            let (x1, y1_opt) = current;
            let (x2, y2_opt) = next;
            if y1_opt.is_some() != y2_opt.is_some() {
                let mut good_x = if y1_opt.is_some() { x1 } else { x2 };
                let mut bad_x = if y1_opt.is_some() { x2 } else { x1 };
                let mut best_y = if let Some(y1) = y1_opt {
                    y1
                } else {
                    y2_opt.unwrap_or(0.0)
                };
                for _ in 0..24 {
                    let mid = (good_x + bad_x) * 0.5;
                    if let Ok(y) = eval_function_with_vars(expr, mid, variables) {
                        if y.is_finite() {
                            good_x = mid;
                            best_y = y;
                        } else {
                            bad_x = mid;
                        }
                    } else {
                        bad_x = mid;
                    }
                }
                refined.push((good_x, Some(best_y)));
            }
        }
    }
    refined
}

/// Devuelve el AST complejo parseado de `expr`, cacheado por string de
/// expresión (LRU acotado). Evita re-parsear `complex_expr` en cada frame
/// (H10): el parseo solo ocurre en cache miss.
fn cached_complex_expr(expr: &str) -> Option<grafito_complex::ComplexExpr> {
    if let Some(cached) = COMPLEX_EXPR_CACHE.with(|c| c.borrow().get(expr).cloned()) {
        return Some(cached);
    }
    let parsed = grafito_complex::complex_expr::parse(expr).ok()?;
    COMPLEX_EXPR_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() >= COMPLEX_EXPR_CACHE_CAP {
            if let Some(k) = cache.keys().next().cloned() {
                cache.remove(&k);
            }
        }
        cache.insert(expr.to_string(), parsed.clone());
    });
    Some(parsed)
}

/// Cachea `ordered_visible_2d_objects` keyed por `document.version`.
/// Evita re-ordenar (layer+ObjectId) y re-filtrar cada vez que se pintan
/// múltiples pasadas (grid, fills, mappings) en el mismo frame.
fn cached_ordered_visible_ids(document: &grafito_core::Document) -> Vec<ObjectId> {
    let version = document.version;
    if let Some((cached_version, ids)) = ORDERED_VISIBLE_CACHE.with(|c| c.borrow().clone()) {
        if cached_version == version {
            return ids;
        }
    }
    let ids: Vec<ObjectId> = grafito_render::ordered_visible_2d_objects(document)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    ORDERED_VISIBLE_CACHE.with(|c| *c.borrow_mut() = Some((version, ids.clone())));
    ids
}

fn cached_ordered_visible_2d_objects(
    document: &grafito_core::Document,
) -> Vec<(ObjectId, &GeoObject)> {
    // Usa ids cacheados (keyed por version) y reconstruye refs sin re-ordenar.
    let ids = cached_ordered_visible_ids(document);
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(obj) = document.get_object(id) {
            out.push((id, obj));
        }
    }
    out
}

/// Read-lock optimizado con fast-path `try_read` y manejo de poison sin `unwrap`.
#[inline]
fn read_lock_optimized<T>(lock: &std::sync::RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    if let Ok(guard) = lock.try_read() {
        return guard;
    }
    match lock.read() {
        Ok(g) => g,
        Err(poisoned) => {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            poisoned.into_inner()
        }
    }
}

fn to_color32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c.r * 255.0).clamp(0.0, 255.0) as u8,
        (c.g * 255.0).clamp(0.0, 255.0) as u8,
        (c.b * 255.0).clamp(0.0, 255.0) as u8,
        (c.a * 255.0).clamp(0.0, 255.0) as u8,
    )
}

fn should_connect_screen_points(a: Pos2, b: Pos2, canvas_rect: Rect) -> bool {
    let d = a.distance(b);
    d.is_finite() && d <= canvas_rect.width().max(canvas_rect.height()) * 0.5
}

fn split_continuous_screen_runs(points: &[Option<Pos2>], canvas_rect: Rect) -> Vec<Vec<Pos2>> {
    let mut runs = Vec::new();
    let mut current = Vec::new();
    for point in points {
        let Some(point) = point.filter(|point| point.is_finite()) else {
            if !current.is_empty() {
                runs.push(std::mem::take(&mut current));
            }
            continue;
        };
        if current
            .last()
            .is_some_and(|previous| !should_connect_screen_points(*previous, point, canvas_rect))
        {
            runs.push(std::mem::take(&mut current));
        }
        current.push(point);
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

fn function_screen_point(view: &ViewTransform, canvas_rect: Rect, x: f64, y: f64) -> Option<Pos2> {
    if !x.is_finite()
        || !y.is_finite()
        || !canvas_rect.min.is_finite()
        || !canvas_rect.max.is_finite()
    {
        return None;
    }

    let screen = view.world_to_screen(Point2::new(x, y));
    if !screen.is_finite() {
        return None;
    }

    let position = canvas_rect.min + Vec2::new(screen.x, screen.y);
    let draw_bounds = canvas_rect.expand(1.0);
    (position.is_finite() && draw_bounds.contains(position)).then_some(position)
}

fn paint_render_geometry(
    painter: &egui::Painter,
    canvas_rect: Rect,
    vertices: &[grafito_render::Vertex],
    indices: &[u32],
    style: Option<StyleOverride>,
) {
    let mut mesh = egui::Mesh::default();
    let mut remapped = vec![None; vertices.len()];
    for (index, vertex) in vertices.iter().enumerate() {
        let position = Pos2::new(
            canvas_rect.min.x + vertex.position[0],
            canvas_rect.min.y + vertex.position[1],
        );
        if !position.is_finite() {
            continue;
        }
        let color = get_color(
            Color::new(
                vertex.color[0],
                vertex.color[1],
                vertex.color[2],
                vertex.color[3],
            ),
            style,
        );
        let Ok(new_index) = u32::try_from(mesh.vertices.len()) else {
            return;
        };
        remapped[index] = Some(new_index);
        mesh.vertices.push(egui::epaint::Vertex {
            pos: position,
            uv: Pos2::ZERO,
            color: to_color32(color),
        });
    }
    for triangle in indices.chunks_exact(3) {
        let mapped = [triangle[0], triangle[1], triangle[2]].map(|index| {
            usize::try_from(index)
                .ok()
                .and_then(|index| remapped.get(index).copied().flatten())
        });
        if let [Some(a), Some(b), Some(c)] = mapped {
            mesh.indices.extend([a, b, c]);
        }
    }
    if !mesh.indices.is_empty() {
        painter.add(Shape::Mesh(mesh));
    }
}

fn is_gpu_base_geometry(document: &grafito_core::Document, obj: &GeoObject) -> bool {
    grafito_render::gpu_2d_base_owns(document, obj)
}

fn gpu_overlay_keeps_cpu_decorations(obj: &GeoObject) -> bool {
    matches!(
        obj,
        GeoObject::Function(_) | GeoObject::PolarCurve(_) | GeoObject::PhasePortrait(_)
    )
}

fn gpu_base_needs_cpu_backfill(obj: &GeoObject) -> bool {
    matches!(obj, GeoObject::Function(_) | GeoObject::PolarCurve(_))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BasePaint2D {
    Cpu(ObjectId),
    Gpu(ObjectId),
}

fn base_scene_paint_plan(
    document: &grafito_core::Document,
    gpu_base_active: bool,
) -> Vec<BasePaint2D> {
    cached_ordered_visible_2d_objects(document)
        .into_iter()
        .map(|(id, object)| {
            if gpu_base_active && is_gpu_base_geometry(document, object) {
                BasePaint2D::Gpu(id)
            } else {
                BasePaint2D::Cpu(id)
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuObjectPass {
    Full,
    Supplement,
    Skip,
}

fn cpu_object_pass(
    document: &grafito_core::Document,
    object: &GeoObject,
    gpu_base_active: bool,
) -> CpuObjectPass {
    if !gpu_base_active || !is_gpu_base_geometry(document, object) {
        CpuObjectPass::Full
    } else if gpu_overlay_keeps_cpu_decorations(object) {
        CpuObjectPass::Supplement
    } else {
        CpuObjectPass::Skip
    }
}

/// Una sola pasada de dibujo no debe sincronizar más trabajo de curvas
/// implícitas, incluida la rasterización de fills, que el límite de marching-squares.
const MAX_CPU_IMPLICIT_FRAME_WORK_UNITS: usize =
    grafito_core::implicit_curve::MAX_MARCHING_SQUARES_WORK_UNITS;

fn implicit_curve_grid_size(canvas_rect: Rect, quality: grafito_core::RenderQuality) -> usize {
    grafito_core::implicit_curve::recommended_grid_size_for_quality(
        canvas_rect.width(),
        canvas_rect.height(),
        quality,
    )
}

fn complex_grid_cpu_resolution(density: usize, quality: grafito_core::RenderQuality) -> usize {
    let base = density.clamp(50, 500);
    if quality == grafito_core::RenderQuality::Preview {
        base.min(64)
    } else {
        base
    }
}

fn implicit_curve_view_bounds(view: ViewTransform, canvas_rect: Rect) -> (f64, f64, f64, f64) {
    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
    let world_br = view.screen_to_world(glam::Vec2::new(canvas_rect.width(), canvas_rect.height()));
    (
        world_tl.x.min(world_br.x),
        world_tl.x.max(world_br.x),
        world_br.y.min(world_tl.y),
        world_br.y.max(world_tl.y),
    )
}

fn implicit_curve_cache_matches_request(
    curve: &ImplicitCurveObj,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    variables: &HashMap<String, f64>,
    quality: grafito_core::RenderQuality,
) -> bool {
    let grid_size = match quality {
        grafito_core::RenderQuality::Preview => grid_size.min(128),
        grafito_core::RenderQuality::Normal => grid_size.min(512),
        grafito_core::RenderQuality::High => {
            grid_size.min(grafito_core::implicit_curve::MAX_IMPLICIT_GRID_SIZE)
        }
    };
    let padded_bounds = grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);
    let requested_key = curve.cache_key(padded_bounds, grid_size, variables);
    // Read-lock optimizado: fast-path `try_read` evita bloqueo si no hay escritor.
    let cached_key = read_lock_optimized(&curve.cached_key);
    if cached_key.as_ref() != Some(&requested_key) {
        return false;
    }
    let cached_region = read_lock_optimized(&curve.cached_region);
    cached_region.is_some_and(|(x_min, x_max, y_min, y_max)| {
        view_bounds.0 >= x_min
            && view_bounds.1 <= x_max
            && view_bounds.2 >= y_min
            && view_bounds.3 <= y_max
    })
}

fn implicit_curve_fill_work(
    curve: &ImplicitCurveObj,
    view_bounds: (f64, f64, f64, f64),
    canvas_rect: Rect,
    quality: grafito_core::RenderQuality,
) -> Option<usize> {
    if curve.fill_color.is_none() || matches!(curve.operator, RelationOperator::Eq) {
        return Some(0);
    }

    let padded_bounds = grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);
    let canvas_size = (
        canvas_rect.width().max(1.0) as u32,
        canvas_rect.height().max(1.0) as u32,
    );
    let (mut texture_width, mut texture_height) =
        fill_cache_texture_size(view_bounds, padded_bounds, canvas_size);
    if quality == grafito_core::RenderQuality::Preview {
        texture_width = (texture_width as f64 * 0.25).ceil().max(1.0) as u32;
        texture_height = (texture_height as f64 * 0.25).ceil().max(1.0) as u32;
    }
    (texture_width as usize).checked_mul(texture_height as usize)
}

fn implicit_curve_cache_miss_work(
    curve: &ImplicitCurveObj,
    grid_size: usize,
    view_bounds: (f64, f64, f64, f64),
    canvas_rect: Rect,
    quality: grafito_core::RenderQuality,
) -> Option<usize> {
    let level_count = curve
        .contour_levels
        .as_ref()
        .filter(|levels| !levels.is_empty())
        .map_or(1, |levels| levels.len())
        .min(grafito_core::validation::MAX_CONTOUR_LEVELS);
    let samples_per_axis = grid_size.checked_add(1)?;
    let field_samples = samples_per_axis.checked_mul(samples_per_axis)?;
    let marching_cells = grid_size.checked_mul(grid_size)?.checked_mul(level_count)?;
    field_samples
        .checked_add(marching_cells)?
        .checked_add(implicit_curve_fill_work(
            curve,
            view_bounds,
            canvas_rect,
            quality,
        )?)
}

fn visible_implicit_cache_plan(
    document: &grafito_core::Document,
    view_bounds: (f64, f64, f64, f64),
    grid_size: usize,
    canvas_rect: Rect,
) -> BTreeSet<grafito_core::ObjectId> {
    let quality = document.render_quality;
    let objects = cached_ordered_visible_2d_objects(document);

    let mut remaining_work = MAX_CPU_IMPLICIT_FRAME_WORK_UNITS;
    let mut admitted = BTreeSet::new();
    for (id, object) in objects {
        let GeoObject::ImplicitCurve(curve) = object else {
            continue;
        };
        let cache_matches = implicit_curve_cache_matches_request(
            curve,
            view_bounds,
            grid_size,
            &document.variables,
            quality,
        );
        let Some(work) = (if cache_matches {
            implicit_curve_fill_work(curve, view_bounds, canvas_rect, quality)
        } else {
            implicit_curve_cache_miss_work(curve, grid_size, view_bounds, canvas_rect, quality)
        }) else {
            continue;
        };
        let Some(remaining) = remaining_work.checked_sub(work) else {
            continue;
        };
        remaining_work = remaining;
        admitted.insert(id);
    }
    admitted
}

fn precompute_visible_implicit_curve_caches(
    document: &grafito_core::Document,
    canvas_rect: Rect,
) -> BTreeSet<grafito_core::ObjectId> {
    let view = *document.view();
    let view_bounds = implicit_curve_view_bounds(view, canvas_rect);
    let quality = document.render_quality;
    let grid_size = implicit_curve_grid_size(canvas_rect, quality);
    let admitted = visible_implicit_cache_plan(document, view_bounds, grid_size, canvas_rect);

    for id in &admitted {
        let Some(GeoObject::ImplicitCurve(curve)) = document.get_object(*id) else {
            continue;
        };
        let _segments = grafito_core::implicit_curve::segments_or_compute(
            curve,
            view_bounds,
            grid_size,
            &document.variables,
            quality,
        );
    }
    admitted
}

#[cfg(test)]
mod overlay_layer_tests {
    use super::*;
    use grafito_core::{
        CircleObj, ComplexGridObj, ComplexMappingObj, Document, EllipseObj, Fractal2DObj,
        FunctionObj, GeoObject, HyperbolaObj, ImplicitCurveObj, LineObj, ParabolaObj,
        ParametricCurve2DObj, PencilObj, PhasePortraitObj, PointObj, PolarCurveObj,
        RegressionLineObj, RelationOperator, TransformedObj, VectorField2DObj,
    };

    #[test]
    fn complex_grid_preview_caps_cpu_sampling_while_high_quality_keeps_the_requested_density() {
        assert_eq!(
            complex_grid_cpu_resolution(500, grafito_core::RenderQuality::Preview),
            64
        );
        assert_eq!(
            complex_grid_cpu_resolution(500, grafito_core::RenderQuality::High),
            500
        );
    }

    #[test]
    fn mixed_cpu_gpu_scene_keeps_global_layer_order() {
        let mut document = Document::new();
        let function = document.add_object(GeoObject::Function(FunctionObj::new("0")));
        let circle = document.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(0.0, 0.0),
            2.0,
        )));

        assert_eq!(
            base_scene_paint_plan(&document, true),
            vec![BasePaint2D::Cpu(circle), BasePaint2D::Gpu(function)]
        );
    }

    #[test]
    fn mixed_cpu_gpu_objects_in_one_layer_keep_object_id_order() {
        let mut document = Document::new();
        let function = document.add_object(GeoObject::Function(FunctionObj::new("0")));
        let line = document.add_object(GeoObject::Line(LineObj::new(
            Point2::new(-1.0, 0.0),
            Point2::new(1.0, 0.0),
        )));
        let mut expected = vec![BasePaint2D::Gpu(function), BasePaint2D::Cpu(line)];
        expected.sort_unstable_by_key(|paint| match paint {
            BasePaint2D::Cpu(id) | BasePaint2D::Gpu(id) => *id,
        });

        assert_eq!(base_scene_paint_plan(&document, true), expected);
    }

    #[test]
    fn gpu_overlay_keeps_vector_field_details_on_cpu() {
        let document = Document::new();
        assert!(is_gpu_base_geometry(
            &document,
            &GeoObject::Function(FunctionObj::new("sin(x)")),
        ));
        assert!(
            !is_gpu_base_geometry(
                &document,
                &GeoObject::VectorField2D(VectorField2DObj::new("x", "y")),
            ),
            "the CPU overlay owns vector arrowheads and RK4 streamlines"
        );
        assert!(is_gpu_base_geometry(
            &document,
            &GeoObject::Fractal2D(Fractal2DObj::mandelbrot()),
        ));
        assert!(is_gpu_base_geometry(
            &document,
            &GeoObject::ParametricCurve2D(ParametricCurve2DObj::new("t", "t^2", 0.0, 1.0)),
        ));
        assert!(is_gpu_base_geometry(
            &document,
            &GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, 1.0)),
        ));
        assert!(!is_gpu_base_geometry(
            &document,
            &GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))),
        ));
    }

    #[test]
    fn gpu_overlay_keeps_function_and_polar_cpu_decorations() {
        let function = GeoObject::Function(FunctionObj::new("sin(x)"));
        let polar = GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, 1.0));
        let parametric =
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new("t", "t^2", 0.0, 1.0));

        assert!(gpu_overlay_keeps_cpu_decorations(&function));
        assert!(gpu_overlay_keeps_cpu_decorations(&polar));
        assert!(!gpu_overlay_keeps_cpu_decorations(&parametric));
    }

    #[test]
    fn gpu_overlay_never_redraws_a_gpu_owned_family_in_full() {
        let mut document = Document::new();
        let point = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
        let objects = [
            GeoObject::Function(FunctionObj::new("sin(x)")),
            GeoObject::Fractal2D(Fractal2DObj::mandelbrot()),
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new("t", "t^2", 0.0, 1.0)),
            GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, 1.0)),
            GeoObject::PhasePortrait(PhasePortraitObj::new("y", "-x", -1.0, 1.0, -1.0, 1.0)),
            GeoObject::ComplexGrid(ComplexGridObj::new("z", -1.0, 1.0, -1.0, 1.0)),
            GeoObject::ComplexMapping(ComplexMappingObj::new("1/z", point)),
            GeoObject::Transformed(TransformedObj::new(
                GeoObject::Line(LineObj::new(Point2::new(0.0, 0.0), Point2::new(1.0, 1.0))),
                "z^2",
            )),
        ];

        for object in objects {
            assert!(grafito_render::gpu_2d_base_owns(&document, &object));
            assert_ne!(
                cpu_object_pass(&document, &object, true),
                CpuObjectPass::Full,
                "{} must not have two base-scene owners",
                object.name()
            );
        }
    }

    #[test]
    fn gpu_overlay_keeps_cpu_only_families_in_full() {
        let document = Document::new();
        let objects = [
            GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))),
            GeoObject::Ellipse(EllipseObj::new(Point2::new(0.0, 0.0), 2.0, 1.0)),
            GeoObject::ImplicitCurve(ImplicitCurveObj::new("x^2+y^2", "1", RelationOperator::Eq)),
            GeoObject::VectorField2D(VectorField2DObj::new("x", "y")),
        ];

        for object in objects {
            assert!(!grafito_render::gpu_2d_base_owns(&document, &object));
            assert_eq!(
                cpu_object_pass(&document, &object, true),
                CpuObjectPass::Full
            );
        }
    }

    #[test]
    fn polar_fill_runs_split_at_the_same_nonfinite_and_jump_gaps_as_strokes() {
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let points = [
            Some(Pos2::new(0.0, 0.0)),
            Some(Pos2::new(10.0, 0.0)),
            Some(Pos2::new(700.0, 0.0)),
            Some(Pos2::new(710.0, 0.0)),
            None,
            Some(Pos2::new(20.0, 0.0)),
            Some(Pos2::new(30.0, 0.0)),
        ];

        let runs = split_continuous_screen_runs(&points, canvas);

        assert_eq!(
            runs,
            vec![
                vec![Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0)],
                vec![Pos2::new(700.0, 0.0), Pos2::new(710.0, 0.0)],
                vec![Pos2::new(20.0, 0.0), Pos2::new(30.0, 0.0)],
            ]
        );
    }

    #[test]
    fn gpu_overlay_routes_unrecognized_complex_mappings_to_cpu() {
        let mut document = Document::new();
        let targets = [
            document.add_object(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                "t", "t^2", 0.0, 1.0,
            ))),
            document.add_object(GeoObject::PolarCurve(PolarCurveObj::new("1", 0.0, 1.0))),
            document.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                "x^2 + y^2",
                "1",
                RelationOperator::Eq,
            ))),
            document.add_object(GeoObject::VectorField2D(VectorField2DObj::new("x", "y"))),
        ];
        for target in targets {
            assert!(!is_gpu_base_geometry(
                &document,
                &GeoObject::ComplexMapping(ComplexMappingObj::new("gamma(z)", target)),
            ));
        }

        let point = document.add_object(GeoObject::Point(PointObj::new(Point2::new(1.0, 0.0))));
        assert!(is_gpu_base_geometry(
            &document,
            &GeoObject::ComplexMapping(ComplexMappingObj::new("1/z", point)),
        ));
    }

    #[test]
    fn complex_mapping_circle_and_pencil_routes_depend_on_map_support() {
        let mut document = Document::new();
        let targets = [
            document.add_object(GeoObject::Circle(CircleObj::new(
                Point2::new(2.0, 0.0),
                0.5,
            ))),
            document.add_object(GeoObject::Pencil(PencilObj::new(vec![
                Point2::new(1.0, 0.0),
                Point2::new(2.0, 1.0),
            ]))),
        ];

        for target in targets {
            assert!(!is_gpu_base_geometry(
                &document,
                &GeoObject::ComplexMapping(ComplexMappingObj::new("gamma(z)", target)),
            ));
            assert!(is_gpu_base_geometry(
                &document,
                &GeoObject::ComplexMapping(ComplexMappingObj::new("1/z", target)),
            ));
        }
    }

    #[test]
    fn complex_mapping_analytic_curves_route_to_gpu_only_when_recognized() {
        let mut document = Document::new();
        let targets = [
            document.add_object(GeoObject::Ellipse(EllipseObj::new(
                Point2::new(0.0, 0.0),
                2.0,
                1.0,
            ))),
            document.add_object(GeoObject::Parabola(ParabolaObj::new(
                Point2::new(0.0, 0.0),
                1.0,
            ))),
            document.add_object(GeoObject::Hyperbola(HyperbolaObj::new(
                Point2::new(0.0, 0.0),
                1.0,
                0.5,
            ))),
            document.add_object(GeoObject::RegressionLine(RegressionLineObj::linear(
                vec![-1.0, 0.0, 1.0],
                vec![-1.0, 0.0, 1.0],
                1.0,
                0.0,
                1.0,
            ))),
        ];

        for target in targets {
            assert!(is_gpu_base_geometry(
                &document,
                &GeoObject::ComplexMapping(ComplexMappingObj::new("1/z", target)),
            ));
            assert!(!is_gpu_base_geometry(
                &document,
                &GeoObject::ComplexMapping(ComplexMappingObj::new("gamma(z)", target)),
            ));
        }
    }

    #[test]
    fn function_projection_rejects_unrepresentable_samples_and_keeps_visible_large_values() {
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let mut view = ViewTransform::new(800.0, 600.0);
        let samples = [(-1.0, Some(1_000_000.0)), (0.0, Some(1e100))];

        assert!(!view
            .world_to_screen(Point2::new(samples[1].0, samples[1].1.unwrap()))
            .is_finite());
        let projected: Vec<_> = samples
            .iter()
            .map(|&(x, y)| y.and_then(|y| function_screen_point(&view, canvas, x, y)))
            .collect();
        assert!(projected.iter().all(|point| point.is_none()));

        view.scale = 0.0001;
        let screen = function_screen_point(&view, canvas, samples[0].0, samples[0].1.unwrap())
            .expect("a finite million-valued constant in view should remain renderable");
        assert!(screen.is_finite());
        assert!(canvas.expand(1.0).contains(screen));
    }

    fn fixed_object_id(value: u128) -> grafito_core::ObjectId {
        let hex = format!("{value:032x}");
        let uuid = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        serde_json::from_str(&format!("\"{uuid}\"")).expect("valid fixed object id")
    }

    #[test]
    fn fill_texture_cache_evicts_least_recent_entry_with_entry_and_byte_budgets() {
        let first = fixed_object_id(1);
        let second = fixed_object_id(2);
        let third = fixed_object_id(3);
        let mut cache = FillTextureCacheStore::with_limits(2, 128);

        cache.insert(first, FillTextureCache::without_texture((4, 4)));
        cache.insert(second, FillTextureCache::without_texture((4, 4)));
        assert!(
            cache.get(first).is_some(),
            "the first entry becomes most recent"
        );
        cache.insert(third, FillTextureCache::without_texture((4, 4)));

        assert!(cache.contains_key(first));
        assert!(
            !cache.contains_key(second),
            "the least-recent entry is evicted"
        );
        assert!(cache.contains_key(third));
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.total_bytes(), 128);
    }

    #[test]
    fn fill_texture_cache_has_a_global_limit_for_five_thousand_filled_objects() {
        let mut cache = FillTextureCacheStore::default();

        for value in 1..=5_000 {
            cache.insert(
                fixed_object_id(value),
                FillTextureCache::without_texture((4_096, 4_096)),
            );
        }

        assert!(cache.len() <= MAX_FILL_TEXTURE_CACHE_ENTRIES);
        assert!(cache.total_bytes() <= MAX_FILL_TEXTURE_CACHE_BYTES);
    }

    #[test]
    fn fill_texture_cache_removes_deleted_and_hidden_fill_owners() {
        let mut document = Document::new();
        let visible = document.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x",
            "0",
            RelationOperator::Less,
        )));
        let hidden = document.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "y",
            "0",
            RelationOperator::Less,
        )));
        let deleted = document.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
            "x + y",
            "0",
            RelationOperator::Less,
        )));
        document
            .get_object_mut(hidden)
            .expect("hidden curve exists")
            .set_visible(false);
        assert!(document.remove_object(deleted).is_some());

        let mut cache = FillTextureCacheStore::default();
        for id in [visible, hidden, deleted] {
            cache.insert(id, FillTextureCache::without_texture((1, 1)));
        }
        cache.retain_visible_fill_owners(&document);

        assert!(cache.contains_key(visible));
        assert!(!cache.contains_key(hidden));
        assert!(!cache.contains_key(deleted));
    }

    #[test]
    fn implicit_cache_misses_share_a_deterministic_frame_budget() {
        let mut document = Document::new();
        document.set_view(ViewTransform::new(800.0, 600.0));
        let mut ids = Vec::new();
        for value in 1..=4 {
            let mut curve = ImplicitCurveObj::new("x", "0", RelationOperator::Less);
            curve.id = fixed_object_id(value);
            curve.contour_levels = Some(vec![-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]);
            ids.push(document.add_object(GeoObject::ImplicitCurve(curve)));
        }
        ids.sort_unstable();

        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0));
        let evaluated = super::precompute_visible_implicit_curve_caches(&document, canvas);

        assert_eq!(evaluated.len(), 2);
        assert!(ids[..2].iter().all(|id| evaluated.contains(id)));
        assert!(ids[2..].iter().all(|id| !evaluated.contains(id)));
        assert!(ids[..2].iter().all(|id| {
            let GeoObject::ImplicitCurve(curve) = document.get_object(*id).unwrap() else {
                panic!("expected implicit curve");
            };
            curve
                .cached_key
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .is_some()
        }));
        let GeoObject::ImplicitCurve(deferred) = document.get_object(ids[2]).unwrap() else {
            panic!("expected deferred implicit curve");
        };
        assert!(
            deferred
                .cached_key
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .is_none(),
            "a deferred cache miss must not overwrite or mark a cache as ready"
        );
    }
}

fn draw_dashed_line(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    stroke: Stroke,
    dash_len: f32,
    gap_len: f32,
) {
    let delta = b - a;
    let len = delta.length();
    if !len.is_finite() || len <= 0.0 {
        return;
    }
    let dir = delta / len;
    let mut dist = 0.0;
    while dist < len {
        let start = a + dir * dist;
        let end = a + dir * (dist + dash_len).min(len);
        painter.line_segment([start, end], stroke);
        dist += dash_len + gap_len;
    }
}

fn trig_asymptotes(function: u8, x_min: f64, x_max: f64) -> Vec<f64> {
    let (offset, step) = match function as usize {
        2 | 4 => (std::f64::consts::FRAC_PI_2, std::f64::consts::PI),
        3 | 5 => (0.0, std::f64::consts::PI),
        _ => return Vec::new(),
    };
    let span = (x_max - x_min).abs();
    // En zoom-out extremo, demasiadas asíntotas saturen la vista.
    let max_asymptotes = if span > 80.0 {
        8
    } else if span > 40.0 {
        16
    } else {
        64
    };
    let mut xs = Vec::new();
    let k_min = ((x_min - offset) / step).floor() as i64 - 1;
    let k_max = ((x_max - offset) / step).ceil() as i64 + 1;
    for k in k_min..=k_max {
        let x = offset + k as f64 * step;
        if x >= x_min && x <= x_max {
            xs.push(x);
            if xs.len() >= max_asymptotes {
                break;
            }
        }
    }
    xs
}

/// Devuelve una cantidad de muestras acotada para el gráfico trigonométrico.
///
/// En zoom extremo el tope puede ser menor que el mínimo habitual de calidad
/// alta; el mínimo efectivo se ajusta al tope para evitar rangos inválidos.
pub(crate) fn trig_sample_count(
    width: usize,
    x_span: f64,
    quality: grafito_core::RenderQuality,
) -> usize {
    let zoom_cap = if x_span.abs() > 80.0 {
        200
    } else if x_span.abs() > 40.0 {
        320
    } else {
        760
    };

    match quality {
        grafito_core::RenderQuality::Preview => width.clamp(120, zoom_cap.min(280)),
        grafito_core::RenderQuality::Normal => width.clamp(200, zoom_cap.min(520)),
        grafito_core::RenderQuality::High => width.clamp(280.min(zoom_cap), zoom_cap),
    }
}

/// Caché de textura para el relleno de curvas implícitas.
///
/// Almacena la textura egui resultante de rasterizar el fill de una
/// `ImplicitCurveObj` para una región (padded/snapped) y tamaño de canvas
/// dados. Evita re-ejecutar el scanline fill cada frame (que llamaba
/// `eval_2d` ~2M veces/frame).
pub struct FillTextureCache {
    /// Textura egui subida a GPU. `None` si aún no se ha rasterizado.
    pub texture: Option<egui::TextureHandle>,
    /// Hash de (expr_lhs, expr_rhs, operator, padded_bounds, canvas_w,
    /// canvas_h, variables, fill_color).
    pub cache_key: u64,
    /// Tamaño de la textura en píxeles cuando se rasterizó.
    pub canvas_size: (u32, u32),
    /// Región world-space padded/snapped que cubre la textura. Si los
    /// `view_bounds` actuales caen dentro de esta región, se puede reusar.
    pub cached_region: (f64, f64, f64, f64),
    byte_size: usize,
    last_used: u64,
}

impl FillTextureCache {
    fn new(
        texture: Option<egui::TextureHandle>,
        cache_key: u64,
        canvas_size: (u32, u32),
        cached_region: (f64, f64, f64, f64),
    ) -> Self {
        Self {
            texture,
            cache_key,
            canvas_size,
            cached_region,
            byte_size: fill_texture_byte_size(canvas_size),
            last_used: 0,
        }
    }

    #[cfg(test)]
    fn without_texture(canvas_size: (u32, u32)) -> Self {
        Self::new(None, 0, canvas_size, (0.0, 1.0, 0.0, 1.0))
    }
}

/// Máximo global para texturas RGBA8 de relleno. El límite de bytes impide que
/// documentos grandes con regiones visibles simultáneamente retengan memoria
/// GPU proporcional al número de objetos. Reducido a 64 MB / 8 entradas para iGPU.
pub const MAX_FILL_TEXTURE_CACHE_ENTRIES: usize = 8;
pub const MAX_FILL_TEXTURE_CACHE_BYTES: usize = 64 * 1024 * 1024;

fn fill_texture_byte_size(canvas_size: (u32, u32)) -> usize {
    let pixels = usize::try_from(canvas_size.0).ok().and_then(|width| {
        usize::try_from(canvas_size.1)
            .ok()
            .and_then(|height| width.checked_mul(height))
    });
    pixels
        .and_then(|pixels| pixels.checked_mul(std::mem::size_of::<Color32>()))
        .unwrap_or(usize::MAX)
}

/// Caché global LRU de texturas de relleno compartida por curvas implícitas y
/// mapeos complejos. Cada entrada representa una textura RGBA8 completa.
pub struct FillTextureCacheStore {
    entries: HashMap<grafito_core::ObjectId, FillTextureCache>,
    total_bytes: usize,
    access_epoch: u64,
    max_entries: usize,
    max_bytes: usize,
}

impl Default for FillTextureCacheStore {
    fn default() -> Self {
        Self::with_limits(MAX_FILL_TEXTURE_CACHE_ENTRIES, MAX_FILL_TEXTURE_CACHE_BYTES)
    }
}

impl FillTextureCacheStore {
    fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            access_epoch: 0,
            max_entries,
            max_bytes,
        }
    }

    fn next_access_epoch(&mut self) -> u64 {
        if self.access_epoch == u64::MAX {
            let mut ids: Vec<_> = self.entries.keys().copied().collect();
            ids.sort_unstable_by(|left, right| {
                self.entries[left]
                    .last_used
                    .cmp(&self.entries[right].last_used)
                    .then_with(|| left.cmp(right))
            });
            for (index, id) in ids.into_iter().enumerate() {
                if let Some(entry) = self.entries.get_mut(&id) {
                    entry.last_used = index as u64 + 1;
                }
            }
            self.access_epoch = self.entries.len() as u64;
        }
        self.access_epoch += 1;
        self.access_epoch
    }

    fn get(&mut self, object_id: grafito_core::ObjectId) -> Option<&FillTextureCache> {
        let epoch = self.next_access_epoch();
        let entry = self.entries.get_mut(&object_id)?;
        entry.last_used = epoch;
        Some(entry)
    }

    #[cfg(test)]
    fn insert(&mut self, object_id: grafito_core::ObjectId, mut entry: FillTextureCache) {
        if entry.byte_size > self.max_bytes || self.max_entries == 0 {
            return;
        }
        self.remove(object_id);
        entry.last_used = self.next_access_epoch();
        self.total_bytes = self.total_bytes.saturating_add(entry.byte_size);
        self.entries.insert(object_id, entry);
        self.evict_to_budget();
    }

    fn insert_with_ctx(
        &mut self,
        object_id: grafito_core::ObjectId,
        mut entry: FillTextureCache,
        ctx: &egui::Context,
    ) {
        if entry.byte_size > self.max_bytes || self.max_entries == 0 {
            // Evita retener texturas que exceden el presupuesto de iGPU (64 MB / 8 entradas).
            if let Some(texture) = entry.texture.take() {
                ctx.forget_image(&format!("grafito_fill_{object_id}"));
                drop(texture);
            }
            return;
        }
        self.remove_with_ctx(object_id, Some(ctx));
        entry.last_used = self.next_access_epoch();
        self.total_bytes = self.total_bytes.saturating_add(entry.byte_size);
        self.entries.insert(object_id, entry);
        self.evict_to_budget_with_ctx(Some(ctx));
    }

    #[cfg(test)]
    fn remove(&mut self, object_id: grafito_core::ObjectId) {
        if let Some(entry) = self.entries.remove(&object_id) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.byte_size);
        }
    }

    fn remove_with_ctx(&mut self, object_id: grafito_core::ObjectId, ctx: Option<&egui::Context>) {
        if let Some(entry) = self.entries.remove(&object_id) {
            self.total_bytes = self.total_bytes.saturating_sub(entry.byte_size);
            if let (Some(ctx), Some(_)) = (ctx, entry.texture) {
                // Libera la textura GPU asociada al LRU evicted. Necesario en iGPU con 64 MB.
                ctx.forget_image(&format!("grafito_fill_{object_id}"));
                ctx.forget_image(&format!("grafito_fill_complex_{object_id}"));
                // Compatibilidad con URIs legacy fijas usadas antes de per-object URIs.
                ctx.forget_image("implicit_fill");
                ctx.forget_image("complex_mapping_fill");
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.total_bytes = 0;
    }

    #[allow(dead_code)] // TODO P2: remover cuando clear_with_ctx se active en teardown GPU (usado en tests de presupuesto)
    pub(crate) fn clear_with_ctx(&mut self, ctx: &egui::Context) {
        for (id, entry) in self.entries.drain() {
            self.total_bytes = self.total_bytes.saturating_sub(entry.byte_size);
            if entry.texture.is_some() {
                ctx.forget_image(&format!("grafito_fill_{id}"));
                ctx.forget_image(&format!("grafito_fill_complex_{id}"));
            }
        }
        self.entries.clear();
        self.total_bytes = 0;
        ctx.forget_image("implicit_fill");
        ctx.forget_image("complex_mapping_fill");
    }

    fn retain_visible_fill_owners(&mut self, document: &grafito_core::Document) {
        self.entries.retain(|id, _| match document.get_object(*id) {
            Some(GeoObject::ImplicitCurve(curve)) => curve.visible,
            Some(GeoObject::ComplexMapping(mapping)) => mapping.visible,
            _ => false,
        });
        self.total_bytes = self
            .entries
            .values()
            .fold(0usize, |total, entry| total.saturating_add(entry.byte_size));
    }

    #[allow(dead_code)] // TODO P2: remover cuando retain_visible_fill_owners_with_ctx se use en evicción LRU (usado en tests)
    fn retain_visible_fill_owners_with_ctx(
        &mut self,
        document: &grafito_core::Document,
        ctx: &egui::Context,
    ) {
        let mut evicted_ids = Vec::new();
        self.entries.retain(|id, _| {
            let keep = match document.get_object(*id) {
                Some(GeoObject::ImplicitCurve(curve)) => curve.visible,
                Some(GeoObject::ComplexMapping(mapping)) => mapping.visible,
                _ => false,
            };
            if !keep {
                evicted_ids.push(*id);
            }
            keep
        });
        for id in evicted_ids {
            ctx.forget_image(&format!("grafito_fill_{id}"));
            ctx.forget_image(&format!("grafito_fill_complex_{id}"));
        }
        self.total_bytes = self
            .entries
            .values()
            .fold(0usize, |total, entry| total.saturating_add(entry.byte_size));
    }

    #[cfg(test)]
    fn evict_to_budget(&mut self) {
        self.evict_to_budget_with_ctx(None);
    }

    fn evict_to_budget_with_ctx(&mut self, ctx: Option<&egui::Context>) {
        while self.entries.len() > self.max_entries || self.total_bytes > self.max_bytes {
            let Some(id) = self
                .entries
                .iter()
                .min_by(|(left_id, left), (right_id, right)| {
                    left.last_used
                        .cmp(&right.last_used)
                        .then_with(|| left_id.cmp(right_id))
                })
                .map(|(id, _)| *id)
            else {
                break;
            };
            self.remove_with_ctx(id, ctx);
        }
    }

    #[cfg(test)]
    fn contains_key(&self, object_id: grafito_core::ObjectId) -> bool {
        self.entries.contains_key(&object_id)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn total_bytes(&self) -> usize {
        self.total_bytes
    }
}

/// Calcula la cache key del fill de una curva implícita.
///
/// La key depende de (expr_lhs, expr_rhs, operator, padded_bounds,
/// canvas_w, canvas_h, variables, fill_color). Dos llamadas con la misma
/// key indican que el caché puede reusarse sin recalcular la rasterización.
pub fn compute_fill_cache_key(
    ic: &ImplicitCurveObj,
    padded_bounds: (f64, f64, f64, f64),
    canvas_size: (u32, u32),
    variables: &std::collections::HashMap<String, f64>,
    fill_color: Color,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    ic.expr_lhs.hash(&mut hasher);
    ic.expr_rhs.hash(&mut hasher);
    std::mem::discriminant(&ic.operator).hash(&mut hasher);
    padded_bounds.0.to_bits().hash(&mut hasher);
    padded_bounds.1.to_bits().hash(&mut hasher);
    padded_bounds.2.to_bits().hash(&mut hasher);
    padded_bounds.3.to_bits().hash(&mut hasher);
    canvas_size.0.hash(&mut hasher);
    canvas_size.1.hash(&mut hasher);
    let mut variables: Vec<_> = variables.iter().collect();
    variables.sort_unstable_by_key(|(name, _)| *name);
    for (k, v) in variables {
        k.hash(&mut hasher);
        v.to_bits().hash(&mut hasher);
    }
    fill_color.r.to_bits().hash(&mut hasher);
    fill_color.g.to_bits().hash(&mut hasher);
    fill_color.b.to_bits().hash(&mut hasher);
    fill_color.a.to_bits().hash(&mut hasher);
    hasher.finish()
}

/// Cache key para el fill de un `ComplexMapping`. Extiende la key del
/// ImplicitCurve con la identidad del `ConformalMap` (su representación
/// `Debug`), para que cambiar la expresión (p.ej. `1/z` → `z^2`) invalide
/// el caché aunque el target sea la misma ImplicitCurve.
pub fn compute_complex_fill_cache_key(
    ic: &ImplicitCurveObj,
    map: &ConformalMap,
    padded_bounds: (f64, f64, f64, f64),
    canvas_size: (u32, u32),
    variables: &std::collections::HashMap<String, f64>,
    fill_color: Color,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    // Reusar la key base del ImplicitCurve (todavía aplica por los ASTs
    // lhs/rhs, operator, bounds, canvas, variables, fill_color).
    let base = compute_fill_cache_key(ic, padded_bounds, canvas_size, variables, fill_color);
    base.hash(&mut hasher);
    // Sumar la identidad del mapeo conforme.
    format!("{:?}", map).hash(&mut hasher);
    hasher.finish()
}

/// Evalúa si un punto del plano de salida `w=(output_x, output_y)` cae dentro
/// de la región transformada por un `ComplexMapping` de una curva implícita.
///
/// La prueba se hace en el plano original: `z = map.inverse_apply(w)`, y luego
/// se evalúa la desigualdad original sobre `(x,y)=(Re z, Im z)`.
pub fn complex_mapping_region_contains(
    ic: &ImplicitCurveObj,
    map: ConformalMap,
    variables: &std::collections::HashMap<String, f64>,
    output_x: f64,
    output_y: f64,
) -> Option<bool> {
    use num_complex::Complex64;

    if matches!(ic.operator, RelationOperator::Eq) {
        return None;
    }
    let z = map.inverse_apply(Complex64::new(output_x, output_y))?;
    let (eval_lhs, eval_rhs) = ic.get_cached_asts(variables, &["x", "y"])?;
    let lhs = eval_lhs.eval_2d("x", z.re, "y", z.im);
    let rhs = eval_rhs.eval_2d("x", z.re, "y", z.im);
    if !lhs.is_finite() || !rhs.is_finite() {
        return None;
    }
    Some(match ic.operator {
        RelationOperator::Less | RelationOperator::LessEq => lhs - rhs <= 0.0,
        RelationOperator::Greater | RelationOperator::GreaterEq => rhs - lhs <= 0.0,
        RelationOperator::Eq => return None,
    })
}

/// Calcula el sub-rect UV que corresponde a `view_bounds` dentro de una textura
/// que representa `cached_region` completa.
pub fn fill_cache_view_uv(
    cached_region: (f64, f64, f64, f64),
    view_bounds: (f64, f64, f64, f64),
) -> Option<(f32, f32, f32, f32)> {
    let (rx_min, rx_max, ry_min, ry_max) = cached_region;
    let (vx_min, vx_max, vy_min, vy_max) = view_bounds;
    if !(vx_min >= rx_min && vx_max <= rx_max && vy_min >= ry_min && vy_max <= ry_max) {
        return None;
    }
    let rw = rx_max - rx_min;
    let rh = ry_max - ry_min;
    if rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let u_min = ((vx_min - rx_min) / rw) as f32;
    let u_max = ((vx_max - rx_min) / rw) as f32;
    // Texture coordinates grow downward; world y grows upward.
    let v_min = ((ry_max - vy_max) / rh) as f32;
    let v_max = ((ry_max - vy_min) / rh) as f32;
    Some((u_min, u_max, v_min, v_max))
}

/// Tamaño de textura necesario para rasterizar `cached_region` a la misma
/// densidad aproximada que el viewport actual. Para `pad_factor=2`, suele ser
/// ~2x el canvas en cada eje; se limita para evitar texturas enormes.
pub fn fill_cache_texture_size(
    view_bounds: (f64, f64, f64, f64),
    cached_region: (f64, f64, f64, f64),
    canvas_size: (u32, u32),
) -> (u32, u32) {
    let view_width = (view_bounds.1 - view_bounds.0).max(f64::EPSILON);
    let view_height = (view_bounds.3 - view_bounds.2).max(f64::EPSILON);
    let region_width = (cached_region.1 - cached_region.0).max(f64::EPSILON);
    let region_height = (cached_region.3 - cached_region.2).max(f64::EPSILON);
    let scale_cap = 4096.0;
    let texture_w = ((canvas_size.0 as f64 * region_width / view_width)
        .ceil()
        .clamp(1.0, scale_cap)) as u32;
    let texture_h = ((canvas_size.1 as f64 * region_height / view_height)
        .ceil()
        .clamp(1.0, scale_cap)) as u32;
    (texture_w, texture_h)
}

/// Transforma segmentos independientes por un mapa conforme y devuelve strokes
/// finitos sin conectar un segmento de marching-squares con el siguiente.
pub fn complex_mapping_segment_strokes(
    map: ConformalMap,
    segments: &[(Point2, Point2)],
    subdivisions: usize,
) -> Vec<(Point2, Point2)> {
    complex_mapping_segment_strokes_at(map, segments, subdivisions, 1.0)
}

fn complex_mapping_segment_strokes_at(
    map: ConformalMap,
    segments: &[(Point2, Point2)],
    subdivisions: usize,
    homotopy_factor: f64,
) -> Vec<(Point2, Point2)> {
    grafito_render::transform_complex_mapping_segments(map, segments, subdivisions, homotopy_factor)
}

#[derive(Clone, Copy, Default)]
pub struct StyleOverride {
    pub color: Option<Color>,
    pub color_alpha_multiplier: Option<f32>,
    pub width: Option<f32>,
    pub width_scale: Option<f32>,
    pub size_scale: Option<f32>,
    pub hide_label: bool,
    pub clear_fill_color: bool,
    pub skip_stroke: bool,
}

fn get_color(base: Color, style: Option<StyleOverride>) -> Color {
    let mut c = base;
    if let Some(s) = style {
        if let Some(over) = s.color {
            c = over;
        }
        if let Some(mult) = s.color_alpha_multiplier {
            c.a = (c.a * mult).max(0.05);
        }
    }
    c
}

fn get_fill_color(base: Option<Color>, style: Option<StyleOverride>) -> Option<Color> {
    if style.is_some_and(|s| s.clear_fill_color) {
        return None;
    }
    base.map(|c| get_color(c, style))
}

fn get_width(base: f32, style: Option<StyleOverride>) -> f32 {
    let mut w = base;
    if let Some(s) = style {
        if let Some(over) = s.width {
            w = over;
        }
        if let Some(scale) = s.width_scale {
            w *= scale;
        }
    }
    w
}

fn get_size(base: f32, style: Option<StyleOverride>) -> f32 {
    let mut size = base;
    if let Some(s) = style {
        if let Some(scale) = s.size_scale {
            size *= scale;
        }
    }
    size
}

fn get_label(base: &str, style: Option<StyleOverride>) -> &str {
    if let Some(s) = style {
        if s.hide_label {
            return "";
        }
    }
    base
}

/// HSL to RGB conversion for domain coloring
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let hue_to_rgb = |p: f64, q: f64, mut t: f64| -> f64 {
        while t < 0.0 {
            t += 1.0;
        }
        while t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

/// Thermal colormap for heat maps: blue → cyan → green → yellow → red
fn thermal_colormap(t: f64) -> (f64, f64, f64) {
    let t = t.clamp(0.0, 1.0);
    let r = (t * 3.0 - 1.5).clamp(0.0, 1.0).min(1.0);
    let g = (1.5 - (t * 3.0 - 1.5).abs()).clamp(0.0, 1.0);
    let b = (1.5 - t * 3.0).clamp(0.0, 1.0);
    (r, g, b)
}

/// Convert integer exponent to Unicode superscript (e.g. 3 → "³", -2 → "⁻²")
fn superscript(exp: i32) -> String {
    let digits: Vec<char> = exp.to_string().chars().collect();
    let mut result = String::new();
    for &c in &digits {
        result.push(match c {
            '-' => '⁻',
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            _ => c,
        });
    }
    result
}

impl GrafitoApp {
    pub(crate) fn draw_grid(&self, painter: &egui::Painter, canvas_rect: Rect) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_grid");
        if !self.show_grid {
            return;
        }
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let grid_color = current_theme(painter.ctx()).grid_line;
        let grid_stroke = Stroke::new(1.0, grid_color);
        let minor_stroke = Stroke::new(0.5, current_theme(painter.ctx()).grid_minor);

        // Vertical grid lines
        if view.x_log {
            let x_min = world_tl.x.min(world_br.x);
            let x_max = world_tl.x.max(world_br.x);
            if x_max > 0.0 {
                let positive_min = x_min.max(1e-12);
                let min_pow = positive_min.log10().floor() as i32 - 1;
                let max_pow = x_max.log10().ceil() as i32 + 1;
                for pow in min_pow..=max_pow {
                    let x = 10_f64.powf(pow as f64);
                    if x < positive_min || x > x_max {
                        continue;
                    }
                    let a = view.world_to_screen(Point2::new(x, world_br.y));
                    let b = view.world_to_screen(Point2::new(x, world_tl.y));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(a.x, a.y),
                            canvas_rect.min + Vec2::new(b.x, b.y),
                        ],
                        grid_stroke,
                    );
                    // Minor grid at 2..9 * 10^pow
                    if pow < max_pow {
                        for k in 2..=9 {
                            let xm = k as f64 * 10_f64.powf(pow as f64);
                            if xm < positive_min || xm > x_max {
                                continue;
                            }
                            let am = view.world_to_screen(Point2::new(xm, world_br.y));
                            let bm = view.world_to_screen(Point2::new(xm, world_tl.y));
                            painter.line_segment(
                                [
                                    canvas_rect.min + Vec2::new(am.x, am.y),
                                    canvas_rect.min + Vec2::new(bm.x, bm.y),
                                ],
                                minor_stroke,
                            );
                        }
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
            let magnitude = target_world_step.log10().floor();
            let base = 10f64.powf(magnitude);
            let factor = target_world_step / base;
            let major_step = if factor < 2.0 {
                1.0 * base
            } else if factor < 5.0 {
                2.0 * base
            } else {
                5.0 * base
            };
            let mut min_x = (world_tl.x / major_step).floor() as i64 - 1;
            let mut max_x = (world_br.x / major_step).ceil() as i64 + 1;
            if max_x.saturating_sub(min_x) > 500 {
                let center = (min_x + max_x) / 2;
                min_x = center - 250;
                max_x = center + 250;
            }
            for xi in min_x..=max_x {
                let x = xi as f64 * major_step;
                let a = view.world_to_screen(Point2::new(x, world_br.y.min(world_tl.y)));
                let b = view.world_to_screen(Point2::new(x, world_br.y.max(world_tl.y)));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
            }
        }

        // Horizontal grid lines
        if view.y_log {
            let y_min = world_tl.y.min(world_br.y);
            let y_max = world_tl.y.max(world_br.y);
            if y_max > 0.0 {
                let positive_min = y_min.max(1e-12);
                let min_pow = positive_min.log10().floor() as i32 - 1;
                let max_pow = y_max.log10().ceil() as i32 + 1;
                for pow in min_pow..=max_pow {
                    let y = 10_f64.powf(pow as f64);
                    if y < positive_min || y > y_max {
                        continue;
                    }
                    let a = view.world_to_screen(Point2::new(world_tl.x, y));
                    let b = view.world_to_screen(Point2::new(world_br.x, y));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(a.x, a.y),
                            canvas_rect.min + Vec2::new(b.x, b.y),
                        ],
                        grid_stroke,
                    );
                    // Minor grid at 2..9 * 10^pow
                    if pow < max_pow {
                        for k in 2..=9 {
                            let ym = k as f64 * 10_f64.powf(pow as f64);
                            if ym < positive_min || ym > y_max {
                                continue;
                            }
                            let am = view.world_to_screen(Point2::new(world_tl.x, ym));
                            let bm = view.world_to_screen(Point2::new(world_br.x, ym));
                            painter.line_segment(
                                [
                                    canvas_rect.min + Vec2::new(am.x, am.y),
                                    canvas_rect.min + Vec2::new(bm.x, bm.y),
                                ],
                                minor_stroke,
                            );
                        }
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
            let magnitude = target_world_step.log10().floor();
            let base = 10f64.powf(magnitude);
            let factor = target_world_step / base;
            let major_step = if factor < 2.0 {
                1.0 * base
            } else if factor < 5.0 {
                2.0 * base
            } else {
                5.0 * base
            };
            let mut min_y = (world_br.y / major_step).floor() as i64 - 1;
            let mut max_y = (world_tl.y / major_step).ceil() as i64 + 1;
            if max_y.saturating_sub(min_y) > 500 {
                let center = (min_y + max_y) / 2;
                min_y = center - 250;
                max_y = center + 250;
            }
            for yi in min_y..=max_y {
                let y = yi as f64 * major_step;
                let a = view.world_to_screen(Point2::new(world_tl.x, y));
                let b = view.world_to_screen(Point2::new(world_br.x, y));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(a.x, a.y),
                        canvas_rect.min + Vec2::new(b.x, b.y),
                    ],
                    grid_stroke,
                );
            }
        }
    }

    pub(crate) fn draw_axes(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        show_numeric_ticks: bool,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_axes");
        let view = self.document.view();
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let stroke = Stroke::new(1.0, current_theme(painter.ctx()).grid_axis);

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        painter.line_segment(
            [
                canvas_rect.min + Vec2::new(x_axis_a.x, x_axis_a.y),
                canvas_rect.min + Vec2::new(x_axis_b.x, x_axis_b.y),
            ],
            stroke,
        );

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        painter.line_segment(
            [
                canvas_rect.min + Vec2::new(y_axis_a.x, y_axis_a.y),
                canvas_rect.min + Vec2::new(y_axis_b.x, y_axis_b.y),
            ],
            stroke,
        );

        if !show_numeric_ticks {
            return;
        }

        // Tick marks and labels — log-appropriate or linear
        let text_color = current_theme(painter.ctx()).axis_label;
        let font = egui::FontId::proportional(12.0);
        let minor_tick = Stroke::new(0.5, text_color);

        // X-axis ticks
        if view.x_log {
            let min_pow = world_tl.x.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_br.x.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let x = 10_f64.powf(pow as f64);
                let s = view.world_to_screen(Point2::new(x, x_axis_y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(0.0, -4.0), pos + Vec2::new(0.0, 4.0)],
                    stroke,
                );
                let label = if pow == 0 {
                    "1".into()
                } else if pow == 1 {
                    "10".into()
                } else if pow == -1 {
                    "10⁻¹".into()
                } else {
                    format!("10{}", superscript(pow))
                };
                painter.text(
                    pos + Vec2::new(0.0, 6.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    font.clone(),
                    text_color,
                );
                // Minor ticks at 2..9 * 10^pow
                if pow < max_pow {
                    for k in 2..=9 {
                        let xm = k as f64 * 10_f64.powf(pow as f64);
                        let sm = view.world_to_screen(Point2::new(xm, x_axis_y));
                        let posm = canvas_rect.min + Vec2::new(sm.x, sm.y);
                        painter.line_segment(
                            [posm + Vec2::new(0.0, -2.0), posm + Vec2::new(0.0, 2.0)],
                            minor_tick,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
            let magnitude = target_world_step.log10().floor();
            let base = 10f64.powf(magnitude);
            let factor = target_world_step / base;
            let major_step = if factor < 2.0 {
                1.0 * base
            } else if factor < 5.0 {
                2.0 * base
            } else {
                5.0 * base
            };
            let mut min_x = (world_tl.x / major_step).floor() as i64 - 1;
            let mut max_x = (world_br.x / major_step).ceil() as i64 + 1;
            if max_x.saturating_sub(min_x) > 500 {
                let center = (min_x + max_x) / 2;
                min_x = center - 250;
                max_x = center + 250;
            }
            for xi in min_x..=max_x {
                let x = xi as f64 * major_step;
                if x.abs() < 1e-9 {
                    continue;
                }
                let s = view.world_to_screen(Point2::new(x, x_axis_y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(0.0, -3.0), pos + Vec2::new(0.0, 3.0)],
                    stroke,
                );
                // Format nicely
                let label = if (x.fract()).abs() < 1e-9 {
                    format!("{}", x as i64)
                } else {
                    format!("{:.2}", x)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                painter.text(
                    pos + Vec2::new(0.0, 6.0),
                    egui::Align2::CENTER_TOP,
                    label,
                    font.clone(),
                    text_color,
                );
            }
        }

        // Y-axis ticks
        if view.y_log {
            let min_pow = world_br.y.max(1e-300).log10().floor() as i32 - 1;
            let max_pow = world_tl.y.max(1e-300).log10().ceil() as i32 + 1;
            for pow in min_pow..=max_pow {
                let y = 10_f64.powf(pow as f64);
                let s = view.world_to_screen(Point2::new(y_axis_x, y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(-4.0, 0.0), pos + Vec2::new(4.0, 0.0)],
                    stroke,
                );
                let label = if pow == 0 {
                    "1".into()
                } else if pow == 1 {
                    "10".into()
                } else if pow == -1 {
                    "10⁻¹".into()
                } else {
                    format!("10{}", superscript(pow))
                };
                painter.text(
                    pos + Vec2::new(-6.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    font.clone(),
                    text_color,
                );
                if pow < max_pow {
                    for k in 2..=9 {
                        let ym = k as f64 * 10_f64.powf(pow as f64);
                        let sm = view.world_to_screen(Point2::new(y_axis_x, ym));
                        let posm = canvas_rect.min + Vec2::new(sm.x, sm.y);
                        painter.line_segment(
                            [posm + Vec2::new(-2.0, 0.0), posm + Vec2::new(2.0, 0.0)],
                            minor_tick,
                        );
                    }
                }
            }
        } else {
            let pixels_per_unit = view.scale;
            let target_world_step = 80.0 / pixels_per_unit.max(1e-50);
            let magnitude = target_world_step.log10().floor();
            let base = 10f64.powf(magnitude);
            let factor = target_world_step / base;
            let major_step = if factor < 2.0 {
                1.0 * base
            } else if factor < 5.0 {
                2.0 * base
            } else {
                5.0 * base
            };
            let mut min_y = (world_br.y / major_step).floor() as i64 - 1;
            let mut max_y = (world_tl.y / major_step).ceil() as i64 + 1;
            if max_y.saturating_sub(min_y) > 500 {
                let center = (min_y + max_y) / 2;
                min_y = center - 250;
                max_y = center + 250;
            }
            for yi in min_y..=max_y {
                let y = yi as f64 * major_step;
                if y.abs() < 1e-9 {
                    continue;
                }
                let s = view.world_to_screen(Point2::new(y_axis_x, y));
                let pos = canvas_rect.min + Vec2::new(s.x, s.y);
                painter.line_segment(
                    [pos + Vec2::new(-3.0, 0.0), pos + Vec2::new(3.0, 0.0)],
                    stroke,
                );
                let label = if (y.fract()).abs() < 1e-9 {
                    format!("{}", y as i64)
                } else {
                    format!("{:.2}", y)
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                };
                painter.text(
                    pos + Vec2::new(-6.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    font.clone(),
                    text_color,
                );
            }
        }

        let origin = view.world_to_screen(Point2::new(0.0, 0.0));
        let origin_pos = canvas_rect.min + Vec2::new(origin.x, origin.y);
        painter.text(
            origin_pos + Vec2::new(-6.0, 6.0),
            egui::Align2::RIGHT_TOP,
            "0",
            font,
            text_color,
        );
    }

    pub(crate) fn draw_trig_canvas_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_trig_canvas_overlay");
        if !self.show_trig_animation {
            return;
        }

        if self.perspective == crate::Perspective::Complex {
            self.draw_complex_animation_overlay(painter, canvas_rect);
            return;
        }

        let view = self.document.view();
        let theme = current_theme(painter.ctx());
        let spec = Self::trig_spec(self.trig_function);
        let accent = to_color32(spec.color);
        let marker = Color32::from_rgb(255, 84, 84);
        let projection_x = Color32::from_rgb(88, 180, 105);
        let projection_y = Color32::from_rgb(80, 120, 240);

        let to_pos = |p: Point2| {
            let screen = view.world_to_screen(p);
            canvas_rect.min + Vec2::new(screen.x, screen.y)
        };

        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
        let x_min = world_tl.x.min(world_br.x);
        let x_max = world_tl.x.max(world_br.x);
        let y_min = world_tl.y.min(world_br.y);
        let y_max = world_tl.y.max(world_br.y);
        let (segments, asymptotes) =
            self.trig_graph_segments(x_min, x_max, y_min, y_max, canvas_rect.width());
        for x in asymptotes {
            let a = to_pos(Point2::new(x, y_min));
            let b = to_pos(Point2::new(x, y_max));
            draw_dashed_line(
                painter,
                a,
                b,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 84, 84, 115)),
                8.0,
                7.0,
            );
        }
        for (a, b) in segments {
            painter.line_segment([to_pos(a), to_pos(b)], Stroke::new(2.0, accent));
        }

        let t = self.trig_angle;
        let value = Self::trig_value(self.trig_function, t);
        if value.is_finite() && value >= y_min && value <= y_max && t >= x_min && t <= x_max {
            let p = to_pos(Point2::new(t, value));
            draw_dashed_line(
                painter,
                to_pos(Point2::new(t, y_min)),
                to_pos(Point2::new(t, y_max)),
                Stroke::new(1.2, marker),
                10.0,
                6.0,
            );
            painter.line_segment(
                [to_pos(Point2::new(t, y_min)), to_pos(Point2::new(t, y_max))],
                Stroke::new(0.4, Color32::from_rgba_unmultiplied(255, 84, 84, 90)),
            );
            painter.circle_filled(p, 4.0, marker);
            painter.text(
                p + Vec2::new(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{}({:.2})", spec.name, t),
                egui::FontId::proportional(11.0),
                theme.text_primary,
            );
        }

        match self.trig_view_mode {
            crate::app::TrigViewMode::Didactic => {
                self.draw_trig_unit_card(
                    painter,
                    canvas_rect,
                    accent,
                    marker,
                    projection_x,
                    projection_y,
                );
            }
            crate::app::TrigViewMode::Grid => {
                let center = to_pos(Point2::new(0.0, 0.0));
                let unit_x = to_pos(Point2::new(1.0, 0.0));
                let radius_px = center.distance(unit_x);
                if radius_px.is_finite() && radius_px > 6.0 && radius_px < 5000.0 {
                    painter.circle_stroke(center, radius_px, Stroke::new(1.6, accent));

                    let t = self.trig_angle;
                    let point_world = Point2::new(t.cos(), t.sin());
                    let point = to_pos(point_world);
                    let foot_x = to_pos(Point2::new(point_world.x, 0.0));
                    let foot_y = to_pos(Point2::new(0.0, point_world.y));

                    painter.line_segment([center, point], Stroke::new(2.0, marker));
                    painter.line_segment([point, foot_x], Stroke::new(1.0, projection_x));
                    painter.line_segment([point, foot_y], Stroke::new(1.0, projection_y));
                    painter.circle_filled(point, 4.5, marker);

                    painter.text(
                        unit_x + Vec2::new(8.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        "cos",
                        egui::FontId::proportional(11.0),
                        theme.text_secondary,
                    );
                    painter.text(
                        to_pos(Point2::new(0.0, 1.0)) + Vec2::new(0.0, -8.0),
                        egui::Align2::CENTER_BOTTOM,
                        "sin",
                        egui::FontId::proportional(11.0),
                        theme.text_secondary,
                    );
                }
            }
        }
    }

    fn draw_trig_unit_card(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        accent: Color32,
        marker: Color32,
        projection_x: Color32,
        projection_y: Color32,
    ) {
        let theme = current_theme(painter.ctx());
        let card_size = Vec2::new(210.0, 190.0);
        let card = Rect::from_min_size(canvas_rect.min + Vec2::new(14.0, 14.0), card_size);
        let bg = theme.panel_bg;
        painter.rect_filled(card, 10.0, bg);
        painter.rect_stroke(card, 10.0, Stroke::new(1.0, theme.separator));

        let center = card.center() + Vec2::new(0.0, 8.0);
        let radius = 58.0;
        let t = self.trig_angle;
        let cos_t = t.cos() as f32;
        let sin_t = t.sin() as f32;
        let point = center + Vec2::new(cos_t * radius, -sin_t * radius);
        let foot_x = center + Vec2::new(cos_t * radius, 0.0);
        let foot_y = center + Vec2::new(0.0, -sin_t * radius);

        painter.text(
            card.min + Vec2::new(12.0, 10.0),
            egui::Align2::LEFT_TOP,
            "Círculo unitario",
            egui::FontId::proportional(13.0),
            theme.text_primary,
        );
        painter.circle_stroke(center, radius, Stroke::new(1.5, accent));
        painter.line_segment(
            [
                center + Vec2::new(-radius - 8.0, 0.0),
                center + Vec2::new(radius + 8.0, 0.0),
            ],
            Stroke::new(1.0, theme.separator),
        );
        painter.line_segment(
            [
                center + Vec2::new(0.0, -radius - 8.0),
                center + Vec2::new(0.0, radius + 8.0),
            ],
            Stroke::new(1.0, theme.separator),
        );
        painter.line_segment([center, point], Stroke::new(2.0, marker));
        painter.line_segment([point, foot_x], Stroke::new(1.2, projection_y));
        painter.line_segment([point, foot_y], Stroke::new(1.2, projection_x));
        painter.circle_filled(point, 4.5, marker);
        painter.text(
            card.left_bottom() + Vec2::new(12.0, -28.0),
            egui::Align2::LEFT_BOTTOM,
            format!("cos θ = {:.3}   sin θ = {:.3}", t.cos(), t.sin()),
            egui::FontId::proportional(11.0),
            theme.text_secondary,
        );
    }

    fn trig_graph_segments(
        &self,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
        width: f32,
    ) -> (Vec<(Point2, Point2)>, Vec<f64>) {
        let key = (
            self.trig_function,
            x_min.to_bits(),
            x_max.to_bits(),
            y_min.to_bits(),
            y_max.to_bits(),
            width.max(0.0).round() as u32,
            self.document.render_quality,
        );
        if let Ok(cache) = self.trig_graph_cache.read() {
            if let Some(cache) = cache.as_ref() {
                if cache.function == key.0
                    && cache.x_min_bits == key.1
                    && cache.x_max_bits == key.2
                    && cache.y_min_bits == key.3
                    && cache.y_max_bits == key.4
                    && cache.width_px == key.5
                    && cache.quality == key.6
                {
                    return (cache.segments.clone(), cache.asymptotes.clone());
                }
            }
        }

        let y_span = (y_max - y_min).abs().max(1.0);
        let x_span = (x_max - x_min).abs();
        let samples = trig_sample_count(width as usize, x_span, self.document.render_quality);
        let mut segments = Vec::with_capacity(samples);
        let mut prev: Option<Point2> = None;
        let asymptotes = trig_asymptotes(self.trig_function, x_min, x_max);
        let dx = ((x_max - x_min).abs() / samples.max(1) as f64).max(1e-6);
        let asymptote_pad = dx * 1.75;
        for i in 0..=samples {
            let x = x_min + (x_max - x_min) * i as f64 / samples as f64;
            let y = Self::trig_value(self.trig_function, x);
            let near_asymptote = asymptotes.iter().any(|a| (x - *a).abs() <= asymptote_pad);
            let visible = !near_asymptote && y.is_finite() && y >= y_min && y <= y_max;
            if visible {
                let world = Point2::new(x, y);
                if let Some(prev_world) = prev {
                    let crosses_asymptote = asymptotes
                        .iter()
                        .any(|a| (prev_world.x - *a) * (world.x - *a) <= 0.0);
                    if !crosses_asymptote && (world.y - prev_world.y).abs() <= y_span * 0.30 {
                        segments.push((prev_world, world));
                    }
                }
                prev = Some(world);
            } else {
                prev = None;
            }
        }

        if let Ok(mut cache) = self.trig_graph_cache.write() {
            *cache = Some(crate::app::TrigGraphCache {
                function: key.0,
                x_min_bits: key.1,
                x_max_bits: key.2,
                y_min_bits: key.3,
                y_max_bits: key.4,
                width_px: key.5,
                quality: key.6,
                segments: segments.clone(),
                asymptotes: asymptotes.clone(),
            });
        }
        (segments, asymptotes)
    }

    fn draw_complex_animation_overlay(&self, painter: &egui::Painter, canvas_rect: Rect) {
        use num_complex::Complex64;
        use std::collections::HashMap;

        let view = self.document.view();
        let theme = current_theme(painter.ctx());
        let source_color = Color32::from_rgb(255, 84, 84);
        let image_color = Color32::from_rgb(150, 70, 255);
        let unit_color = Color32::from_rgb(80, 150, 240);
        let to_pos = |p: Point2| {
            let screen = view.world_to_screen(p);
            canvas_rect.min + Vec2::new(screen.x, screen.y)
        };

        let (expr, label) = self.active_complex_animation_expr();
        let parsed = grafito_complex::complex_expr::parse(&expr).ok();
        let eval_at = |z: Complex64| -> Option<Complex64> {
            let expr = parsed.as_ref()?;
            let mut vars: HashMap<String, Complex64> = HashMap::new();
            vars.insert(self.document.complex_base_symbol.clone(), z);
            for (name, value) in &self.document.variables {
                vars.insert(name.clone(), Complex64::new(*value, 0.0));
            }
            expr.eval(&vars)
                .ok()
                .filter(|w| w.re.is_finite() && w.im.is_finite())
        };

        let mut prev_source: Option<Pos2> = None;
        let mut prev_image: Option<Pos2> = None;
        for i in 0..=128 {
            let a = std::f64::consts::TAU * i as f64 / 128.0;
            let z = Complex64::new(a.cos(), a.sin());
            let source_pos = to_pos(Point2::new(z.re, z.im));
            if let Some(prev) = prev_source {
                painter.line_segment([prev, source_pos], Stroke::new(1.3, unit_color));
            }
            prev_source = Some(source_pos);

            if let Some(w) = eval_at(z) {
                let image_pos = to_pos(Point2::new(w.re, w.im));
                if let Some(prev) = prev_image {
                    if prev.distance(image_pos)
                        < canvas_rect.width().max(canvas_rect.height()) * 0.75
                    {
                        painter.line_segment([prev, image_pos], Stroke::new(2.0, image_color));
                    }
                }
                prev_image = Some(image_pos);
            } else {
                prev_image = None;
            }
        }

        let t = self.trig_angle;
        let z = Complex64::new(t.cos(), t.sin());
        let z_pos = to_pos(Point2::new(z.re, z.im));
        painter.circle_filled(z_pos, 5.0, source_color);
        painter.text(
            z_pos + Vec2::new(7.0, -7.0),
            egui::Align2::LEFT_BOTTOM,
            "z=e^(it)",
            egui::FontId::proportional(11.0),
            theme.text_primary,
        );

        if let Some(w) = eval_at(z) {
            let w_pos = to_pos(Point2::new(w.re, w.im));
            painter.line_segment(
                [z_pos, w_pos],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 120, 120, 120)),
            );
            painter.circle_filled(w_pos, 5.0, image_color);
            painter.text(
                w_pos + Vec2::new(7.0, -7.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{}({})", label, self.document.complex_base_symbol),
                egui::FontId::proportional(11.0),
                theme.text_primary,
            );
        }

        let card = Rect::from_min_size(
            canvas_rect.min + Vec2::new(14.0, 14.0),
            Vec2::new(250.0, 86.0),
        );
        let bg = theme.panel_bg;
        painter.rect_filled(card, 10.0, bg);
        painter.rect_stroke(card, 10.0, Stroke::new(1.0, theme.separator));
        painter.text(
            card.min + Vec2::new(12.0, 10.0),
            egui::Align2::LEFT_TOP,
            "Animación compleja",
            egui::FontId::proportional(13.0),
            theme.text_primary,
        );
        painter.text(
            card.min + Vec2::new(12.0, 34.0),
            egui::Align2::LEFT_TOP,
            format!("z(t) = {:.3} {:+.3}i", z.re, z.im),
            egui::FontId::proportional(11.0),
            theme.text_secondary,
        );
        painter.text(
            card.min + Vec2::new(12.0, 56.0),
            egui::Align2::LEFT_TOP,
            format!("Imagen activa: {}", expr),
            egui::FontId::proportional(11.0),
            image_color,
        );
    }

    fn active_complex_animation_expr(&self) -> (String, &'static str) {
        for (_, obj) in cached_ordered_visible_2d_objects(&self.document) {
            if let GeoObject::ComplexMapping(cm) = obj {
                if cm.visible {
                    return (cm.expr.clone(), "f");
                }
            }
        }
        for (_, obj) in cached_ordered_visible_2d_objects(&self.document) {
            if let GeoObject::ComplexGrid(cg) = obj {
                if cg.visible {
                    return (cg.expr.clone(), "f");
                }
            }
        }
        (self.document.complex_base_symbol.clone(), "id")
    }

    pub(crate) fn draw_objects(
        &mut self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        gpu_base_active: bool,
        mut paint_gpu_object: impl FnMut(ObjectId),
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_objects");
        self.prune_fill_texture_cache();

        // Cache misses and fill rasterization are admitted in ObjectId order
        // under one frame budget. Deferred curves keep their old cache untouched.
        precompute_visible_implicit_curve_caches(&self.document, canvas_rect);

        let mut hovered_object_for_tool = None;
        if matches!(
            self.current_tool,
            grafito_ui::Tool::Root
                | grafito_ui::Tool::Extremum
                | grafito_ui::Tool::Inflection
                | grafito_ui::Tool::YIntercept
                | grafito_ui::Tool::XIntercept
                | grafito_ui::Tool::Analyze
                | grafito_ui::Tool::Intersect
                | grafito_ui::Tool::Distance
                | grafito_ui::Tool::Angle
                | grafito_ui::Tool::Area
                | grafito_ui::Tool::Slope
        ) {
            if let Some(pos) = self.last_mouse_pos {
                let view = *self.document.view();
                let local = pos - canvas_rect.min;
                let world = view.screen_to_world(GlamVec2::new(local.x, local.y));
                let tolerance = 10.0 / view.scale;
                hovered_object_for_tool = self.document.pick_object(world, tolerance);
            }
        }

        for paint in base_scene_paint_plan(&self.document, gpu_base_active) {
            let id = match paint {
                BasePaint2D::Cpu(id) | BasePaint2D::Gpu(id) => id,
            };
            let Some(obj) = self.document.get_object(id) else {
                continue;
            };
            let is_tool_hovered = hovered_object_for_tool == Some(id);
            let is_driver = self.tool_state.driver == Some(id);

            let style = if is_tool_hovered || is_driver {
                Some(StyleOverride {
                    width_scale: Some(1.5),
                    color: Some(Color {
                        r: 0.8,
                        g: 0.8,
                        b: 0.0,
                        a: 1.0,
                    }),
                    ..Default::default()
                })
            } else {
                None
            };
            match paint {
                BasePaint2D::Cpu(_) => {
                    self.draw_object_styled(painter, canvas_rect, obj, style, false);
                }
                BasePaint2D::Gpu(_) => {
                    self.draw_gpu_object_backfill(painter, canvas_rect, obj);
                    paint_gpu_object(id);
                    self.draw_object_styled(painter, canvas_rect, obj, style, true);
                }
            }
        }

        if let Some(preview) = &self.preview_object {
            let style = StyleOverride {
                color: Some(Color {
                    r: 0.4,
                    g: 0.4,
                    b: 0.4,
                    a: 0.8,
                }),
                hide_label: true,
                width: if matches!(preview, GeoObject::Function(_)) {
                    Some(2.5)
                } else {
                    None
                },
                ..Default::default()
            };
            self.draw_object_styled(painter, canvas_rect, preview, Some(style), false);
        }

        // Draw hover analytics
        if let Some(hover) = &self.hovered_analysis {
            let view = *self.document.view();
            let screen_pos = view.world_to_screen(hover.point);
            let pos = canvas_rect.min + egui::Vec2::new(screen_pos.x, screen_pos.y);

            let color = Self::hovered_analysis_color(hover.is_snap, hover.feature, hover.snap_kind);
            let radius = if hover.is_snap { 6.0 } else { 4.0 };
            painter.circle_filled(pos, radius, color);
            painter.circle_stroke(
                pos,
                radius + 1.0,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
            );

            let font = egui::FontId::proportional(14.0);
            painter.text(
                pos + egui::Vec2::new(10.0, -10.0),
                egui::Align2::LEFT_BOTTOM,
                &hover.label,
                font,
                color,
            );
        }
    }

    fn draw_gpu_object_backfill(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        object: &GeoObject,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_gpu_object_backfill");
        if gpu_base_needs_cpu_backfill(object) {
            self.draw_object_styled(
                painter,
                canvas_rect,
                object,
                Some(StyleOverride {
                    hide_label: true,
                    skip_stroke: true,
                    ..Default::default()
                }),
                false,
            );
        }

        let GeoObject::ComplexMapping(cm) = object else {
            return;
        };
        let view = self.document.view();
        let admitted = visible_implicit_cache_plan(
            &self.document,
            implicit_curve_view_bounds(*view, canvas_rect),
            implicit_curve_grid_size(canvas_rect, self.document.render_quality),
            canvas_rect,
        );
        let Some(map) = cm.conformal_map(self.document.complex_base_symbol.as_str()) else {
            return;
        };
        let Some(GeoObject::ImplicitCurve(ic)) = self.document.get_object(cm.target) else {
            return;
        };
        if !admitted.contains(&cm.target) || matches!(ic.operator, RelationOperator::Eq) {
            return;
        }
        let Some(fill_color) = ic.fill_color else {
            return;
        };
        let homotopy_factor = grafito_render::complex_mapping_homotopy_factor(
            cm.animate_homotopy,
            cm.homotopy_speed,
            self.transient_render_state.homotopy_time(),
        );
        if homotopy_factor >= 1.0 - f64::EPSILON {
            self.draw_complex_mapping_fill(painter, canvas_rect, view, ic, cm.id, map, fill_color);
        }
    }

    fn prune_fill_texture_cache(&self) {
        let version = self.document.version;
        // PERF: solo re-prunea si el documento cambió desde la última pasada.
        // Evita tomar el write lock + barrer el LRU completo en cada frame idle.
        let already_pruned =
            LAST_FILL_PRUNE_DOC_VERSION.with(|last| last.borrow().as_ref() == Some(&version));
        if already_pruned {
            return;
        }
        let mut cache = self.fill_textures.write().unwrap_or_else(|poisoned| {
            log::warn!("Fill texture cache write lock poisoned; recovering");
            poisoned.into_inner()
        });
        cache.retain_visible_fill_owners(&self.document);
        LAST_FILL_PRUNE_DOC_VERSION.with(|last| *last.borrow_mut() = Some(version));
    }

    fn hovered_analysis_color(
        is_snap: bool,
        feature: Option<grafito_geometry::analysis::AnalysisFeature>,
        snap_kind: Option<crate::snap::SnapKind>,
    ) -> egui::Color32 {
        use crate::snap::SnapKind;
        use egui::Color32;
        use grafito_geometry::analysis::AnalysisFeature;
        if is_snap {
            match feature {
                Some(AnalysisFeature::Root) | Some(AnalysisFeature::XIntercept) => {
                    Color32::from_rgb(255, 100, 100)
                }
                Some(AnalysisFeature::YIntercept) => Color32::from_rgb(100, 180, 255),
                Some(AnalysisFeature::LocalMaximum) => Color32::from_rgb(74, 222, 128),
                Some(AnalysisFeature::LocalMinimum) => Color32::from_rgb(34, 211, 238),
                Some(AnalysisFeature::Inflection) => Color32::from_rgb(251, 146, 60),
                Some(AnalysisFeature::Centroid) => Color32::from_rgb(120, 220, 120),
                Some(AnalysisFeature::Intersection) | Some(AnalysisFeature::Equilibrium) => {
                    Color32::from_rgb(220, 100, 220)
                }
                _ => Color32::from_rgb(255, 100, 100),
            }
        } else {
            // El snap no es a una característica, pero el cursor está cerca
            // de algo: diferenciamos por `SnapKind` para que el usuario vea
            // visualmente qué tipo de snap aplicó.
            match snap_kind {
                Some(SnapKind::Axis) => Color32::from_rgb(180, 180, 255),
                Some(SnapKind::Grid) => Color32::from_rgb(180, 180, 180),
                Some(SnapKind::Object) => Color32::from_rgb(200, 200, 120),
                Some(SnapKind::Curve) => Color32::from_rgb(160, 220, 200),
                _ => Color32::from_rgb(160, 160, 160),
            }
        }
    }

    fn draw_arrowhead(painter: &egui::Painter, from: Pos2, to: Pos2, width: f32, color: Color32) {
        let dir = to - from;
        let len = dir.length();
        if len < 1e-3 {
            return;
        }
        let dir = dir / len;
        let normal = Vec2::new(-dir.y, dir.x);
        let arrow_len = (width * 4.0).max(6.0).min(len * 0.5);
        let arrow_width = arrow_len * 0.5;

        let tip_back = to - dir * arrow_len;
        let left = tip_back + normal * arrow_width;
        let right = tip_back - normal * arrow_width;

        painter.line_segment([to, left], Stroke::new(width, color));
        painter.line_segment([to, right], Stroke::new(width, color));
    }

    pub(crate) fn draw_tool_ghost(&self, painter: &egui::Painter, canvas_rect: Rect) {
        if let Some(ghost) = &self.tool_ghost {
            let mut style = StyleOverride {
                color_alpha_multiplier: Some(0.3),
                ..Default::default()
            };
            match ghost {
                GeoObject::Point(_) => {
                    style.size_scale = Some(1.3);
                }
                GeoObject::Line(_) => {
                    style.width_scale = Some(0.7);
                }
                GeoObject::Circle(_) => {
                    style.width_scale = Some(0.7);
                    style.clear_fill_color = true;
                }
                GeoObject::Polygon(_) => {
                    style.width_scale = Some(0.7);
                    style.color_alpha_multiplier = Some(0.2);
                }
                _ => {}
            }
            self.draw_object_styled(painter, canvas_rect, ghost, Some(style), false);
        }
    }

    pub(crate) fn draw_object(&self, painter: &egui::Painter, canvas_rect: Rect, obj: &GeoObject) {
        self.draw_object_styled(painter, canvas_rect, obj, None, false);
    }

    /// Rellena el área interior de la imagen transformada de un
    /// `ImplicitCurve` por un `ComplexMapping`. La técnica:
    ///
    /// 1. Para cada fila de píxeles del canvas, muestrear el campo en el
    ///    plano de output (`w`).
    /// 2. Para cada muestra, aplicar `map.inverse_apply(w)` para obtener
    ///    el `z` correspondiente en el plano original.
    /// 3. Evaluar la curva original `f(z) = lhs - rhs` (o `rhs - lhs`
    ///    para `Greater/GreaterEq`) en ese `z`.
    /// 4. Aplicar **scanline fill** alternando "fuera"/"dentro" en cada
    ///    cruce de cero y rellenando entre pares de cruces con la regla
    ///    par-impar. Esto garantiza que el relleno no excede el contorno
    ///    (al contrario del cell-fill anterior).
    #[allow(clippy::too_many_arguments)]
    fn draw_complex_mapping_fill(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        view: &ViewTransform,
        ic: &ImplicitCurveObj,
        cm_id: grafito_core::ObjectId,
        map: ConformalMap,
        fill_color: Color,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_complex_mapping_fill");
        // 1) Operador: Eq no tiene región rellenable.
        if matches!(ic.operator, RelationOperator::Eq) {
            return;
        }
        let (eval_lhs, eval_rhs) = match ic.get_cached_asts(&self.document.variables, &["x", "y"]) {
            Some(t) => t,
            None => return,
        };
        let swap = matches!(
            ic.operator,
            RelationOperator::Greater | RelationOperator::GreaterEq
        );

        let fill_32 = to_color32(fill_color);

        // 2) Dimensiones del canvas en píxeles.
        let w = canvas_rect.width().max(1.0) as u32;
        let h = canvas_rect.height().max(1.0) as u32;

        // 3) Calcular view_bounds y padded/snapped bounds para el caché.
        //    El cache SOLO es válido si los view_bounds actuales caen dentro
        //    de la región padded cacheada → pan/zoom chico no recalcula.
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
        let view_bounds = (
            world_tl.x.min(world_br.x),
            world_tl.x.max(world_br.x),
            world_br.y.min(world_tl.y),
            world_br.y.max(world_tl.y),
        );
        let padded_bounds =
            grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);

        let (mut texture_w, mut texture_h) =
            fill_cache_texture_size(view_bounds, padded_bounds, (w, h));

        if self.document.render_quality == grafito_core::RenderQuality::Preview {
            texture_w = (texture_w as f64 * 0.25).ceil() as u32;
            texture_h = (texture_h as f64 * 0.25).ceil() as u32;
            texture_w = texture_w.max(1);
            texture_h = texture_h.max(1);
        }

        // 4) Cache key: (expr lhs/rhs, operator, padded_bounds, texture_size,
        //    variables, fill_color, conformal_map). El conformal_map se hashea
        //    via Debug para que cambiar `1/z` → `z^2` invalide aunque el
        //    ImplicitCurve target quede igual.
        let cache_key = compute_complex_fill_cache_key(
            ic,
            &map,
            padded_bounds,
            (texture_w, texture_h),
            &self.document.variables,
            fill_color,
        );

        // 5) Verificar caché por el ObjectId del ComplexMapping (no del
        //    ImplicitCurve, para no pisarlo con su propia cache de fill).
        {
            let mut cache = self.fill_textures.write().unwrap_or_else(|poisoned| {
                log::warn!("Fill texture cache write lock poisoned; recovering");
                poisoned.into_inner()
            });
            if let Some(entry) = cache.get(cm_id) {
                if entry.cache_key == cache_key
                    && entry.canvas_size == (texture_w, texture_h)
                    && entry.texture.is_some()
                {
                    if let Some((u_min, u_max, v_min, v_max)) =
                        fill_cache_view_uv(entry.cached_region, view_bounds)
                    {
                        if let Some(ref texture) = entry.texture {
                            let texture_id = texture.id();
                            let uv = Rect::from_min_max(
                                Pos2::new(u_min, v_min),
                                Pos2::new(u_max, v_max),
                            );
                            painter.image(texture_id, canvas_rect, uv, Color32::WHITE);
                            return;
                        }
                    }
                }
            }
        }

        // 6) Cache miss: rasterizar la región padded COMPLETA, no el viewport
        //    actual. En cache hit se dibuja el sub-rect UV correspondiente al
        //    viewport; esto evita estirar una textura del viewport antiguo.
        //
        //    Precomputar world_xs (evita screen_to_world redundante en filas).
        let (rx_min, rx_max, ry_min, ry_max) = padded_bounds;
        let world_xs: Vec<f64> = (0..texture_w)
            .map(|x_pixel| rx_min + (x_pixel as f64 + 0.5) / texture_w as f64 * (rx_max - rx_min))
            .collect();

        let rows: Vec<Vec<Color32>> = (0..texture_h)
            .into_par_iter()
            .map(|y_pixel| {
                let wy = ry_max - (y_pixel as f64 + 0.5) / texture_h as f64 * (ry_max - ry_min);
                let mut row = vec![Color32::TRANSPARENT; texture_w as usize];
                for x_pixel in 0..texture_w {
                    let wx = world_xs[x_pixel as usize];
                    let Some(z) = map.inverse_apply(num_complex::Complex64::new(wx, wy)) else {
                        continue;
                    };
                    let lhs = eval_lhs.eval_2d("x", z.re, "y", z.im);
                    let rhs = eval_rhs.eval_2d("x", z.re, "y", z.im);
                    let f = if !lhs.is_finite() || !rhs.is_finite() {
                        f64::NAN
                    } else if swap {
                        rhs - lhs
                    } else {
                        lhs - rhs
                    };
                    if f.is_finite() && f <= 0.0 {
                        row[x_pixel as usize] = fill_32;
                    }
                }
                row
            })
            .collect();

        // 8) Subir textura a GPU.
        let image = egui::ColorImage {
            size: [texture_w as usize, texture_h as usize],
            pixels: rows.into_iter().flatten().collect(),
        };
        let texture = painter.ctx().load_texture(
            format!("grafito_fill_complex_{cm_id}"),
            image,
            egui::TextureOptions::LINEAR,
        );

        // 9) Almacenar en cache y blit.
        {
            let mut cache = self.fill_textures.write().unwrap_or_else(|poisoned| {
                log::warn!("Fill texture cache write lock poisoned; recovering");
                poisoned.into_inner()
            });
            cache.insert_with_ctx(
                cm_id,
                FillTextureCache::new(
                    Some(texture.clone()),
                    cache_key,
                    (texture_w, texture_h),
                    padded_bounds,
                ),
                painter.ctx(),
            );
        }
        let texture_id = texture.id();
        let Some((u_min, u_max, v_min, v_max)) = fill_cache_view_uv(padded_bounds, view_bounds)
        else {
            return;
        };
        let uv = Rect::from_min_max(Pos2::new(u_min, v_min), Pos2::new(u_max, v_max));
        painter.image(texture_id, canvas_rect, uv, Color32::WHITE);
    }

    /// Rellena el **interior** de una región `ImplicitCurve` (operator
    /// `Less/LessEq/Greater/GreaterEq`). Para `x^2 + y^2 <= 1` produce
    /// un disco violeta translúcido cuyo borde coincide con la curva;
    /// para `x^2 + y^2 = 1` no rellena nada (es solo el contorno).
    ///
    /// **Caché de textura**: el relleno se rasteriza una sola vez a una
    /// `egui::TextureHandle` (RGBA, resolución full del canvas) y se
    /// cachea por `ObjectId`. En frames subsiguientes solo se hace un
    /// `painter.image` (BLIT, ~0.5ms) en lugar de re-ejecutar el scanline
    /// fill por frame (que llamaba `eval_2d` ~2M veces/frame en 1920×1080).
    /// La invalidación usa `padded_snapped_bounds` (pad 2.0, snap 64) para
    /// que pequeños pans no invaliden el caché. La rasterización inicial
    /// se paraleliza por filas con rayon.
    fn draw_implicit_curve_fill(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        view: &ViewTransform,
        ic: &ImplicitCurveObj,
        fill_color: Color,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_implicit_curve_fill");
        // 1) Parsear lhs/rhs usando el cache del ImplicitCurveObj para evitar
        //    reparsear y re-simplificar el AST en cada frame.
        let (eval_lhs, eval_rhs) = match ic.get_cached_asts(&self.document.variables, &["x", "y"]) {
            Some(t) => t,
            None => return,
        };

        // 2) Operador -> convención "interior es f <= 0".
        //    Less / LessEq     : f = lhs - rhs
        //    Greater / GreaterEq: f = rhs - lhs
        //    Eq                 : sin relleno (es solo contorno).
        if matches!(ic.operator, RelationOperator::Eq) {
            return;
        }
        let swap = matches!(
            ic.operator,
            RelationOperator::Greater | RelationOperator::GreaterEq
        );

        let fill_32 = to_color32(fill_color);

        // 3) Dimensiones del canvas en píxeles.
        let w = canvas_rect.width().max(1.0) as u32;
        let h = canvas_rect.height().max(1.0) as u32;

        // 4) Calcular view_bounds y padded/snapped bounds para el caché.
        let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
        let world_br =
            view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
        let view_bounds = (
            world_tl.x.min(world_br.x),
            world_tl.x.max(world_br.x),
            world_br.y.min(world_tl.y),
            world_br.y.max(world_tl.y),
        );
        let padded_bounds =
            grafito_core::implicit_curve::padded_snapped_bounds(view_bounds, 2.0, 64);
        let (mut texture_w, mut texture_h) =
            fill_cache_texture_size(view_bounds, padded_bounds, (w, h));

        if self.document.render_quality == grafito_core::RenderQuality::Preview {
            texture_w = (texture_w as f64 * 0.25).ceil() as u32;
            texture_h = (texture_h as f64 * 0.25).ceil() as u32;
            texture_w = texture_w.max(1);
            texture_h = texture_h.max(1);
        }

        // 5) Calcular hash de la cache key: (expr_lhs, expr_rhs, operator,
        //    padded_bounds, texture_w, texture_h, variables, fill_color).
        let cache_key = compute_fill_cache_key(
            ic,
            padded_bounds,
            (texture_w, texture_h),
            &self.document.variables,
            fill_color,
        );

        // 6) Verificar caché: si la key coincide y los view_bounds están
        //    dentro de la región cacheada, blitear la textura y retornar.
        let object_id = ic.id;
        {
            let mut cache = self.fill_textures.write().unwrap_or_else(|poisoned| {
                log::warn!("Fill texture cache write lock poisoned; recovering");
                poisoned.into_inner()
            });
            if let Some(entry) = cache.get(object_id) {
                if entry.cache_key == cache_key
                    && entry.canvas_size == (texture_w, texture_h)
                    && entry.texture.is_some()
                {
                    if let Some((u_min, u_max, v_min, v_max)) =
                        fill_cache_view_uv(entry.cached_region, view_bounds)
                    {
                        if let Some(ref texture) = entry.texture {
                            let texture_id = texture.id();
                            let uv = Rect::from_min_max(
                                Pos2::new(u_min, v_min),
                                Pos2::new(u_max, v_max),
                            );
                            painter.image(texture_id, canvas_rect, uv, Color32::WHITE);
                            return;
                        }
                    }
                }
            }
        }

        // 7) Cache miss: rasterizar la región padded completa a una textura.
        //    En frames siguientes se dibuja el sub-rect UV del viewport actual.

        // Precomputar world_x por columna (evita llamadas redundantes a
        // screen_to_world dentro del loop paralelo de filas).
        let (rx_min, rx_max, ry_min, ry_max) = padded_bounds;
        let world_xs: Vec<f64> = (0..texture_w)
            .map(|x_pixel| rx_min + (x_pixel as f64 + 0.5) / texture_w as f64 * (rx_max - rx_min))
            .collect();

        // Rasterizar filas en paralelo con rayon.
        // Cada fila produce un Vec<Color32> de w píxeles.
        let rows: Vec<Vec<Color32>> = (0..texture_h)
            .into_par_iter()
            .map(|y_pixel| {
                let wy = ry_max - (y_pixel as f64 + 0.5) / texture_h as f64 * (ry_max - ry_min);
                let mut row = vec![Color32::TRANSPARENT; texture_w as usize];
                for x_pixel in 0..texture_w {
                    let wx = world_xs[x_pixel as usize];
                    let lhs = eval_lhs.eval_2d("x", wx, "y", wy);
                    let rhs = eval_rhs.eval_2d("x", wx, "y", wy);
                    let f = if !lhs.is_finite() || !rhs.is_finite() {
                        f64::NAN
                    } else if swap {
                        rhs - lhs
                    } else {
                        lhs - rhs
                    };
                    if f.is_finite() && f <= 0.0 {
                        row[x_pixel as usize] = fill_32;
                    }
                }
                row
            })
            .collect();

        // Construir ColorImage directamente desde los Color32 (sin
        // conversión a bytes intermedia) y subir como TextureHandle.
        let image = egui::ColorImage {
            size: [texture_w as usize, texture_h as usize],
            pixels: rows.into_iter().flatten().collect(),
        };
        let texture = painter.ctx().load_texture(
            format!("grafito_fill_{object_id}"),
            image,
            egui::TextureOptions::LINEAR,
        );

        // Almacenar en caché.
        {
            let mut cache = self.fill_textures.write().unwrap_or_else(|poisoned| {
                log::warn!("Fill texture cache write lock poisoned; recovering");
                poisoned.into_inner()
            });
            cache.insert_with_ctx(
                object_id,
                FillTextureCache::new(
                    Some(texture.clone()),
                    cache_key,
                    (texture_w, texture_h),
                    padded_bounds,
                ),
                painter.ctx(),
            );
        }

        // Dibujar la textura (BLIT, ~0.5ms).
        let texture_id = texture.id();
        let Some((u_min, u_max, v_min, v_max)) = fill_cache_view_uv(padded_bounds, view_bounds)
        else {
            return;
        };
        let uv = Rect::from_min_max(Pos2::new(u_min, v_min), Pos2::new(u_max, v_max));
        painter.image(texture_id, canvas_rect, uv, Color32::WHITE);
    }

    pub(crate) fn draw_object_styled(
        &self,
        painter: &egui::Painter,
        canvas_rect: Rect,
        obj: &GeoObject,
        style: Option<StyleOverride>,
        overlay_only: bool,
    ) {
        #[cfg(feature = "profile")]
        puffin::profile_scope!("draw_object_styled");
        let overlay_only = match cpu_object_pass(&self.document, obj, overlay_only) {
            CpuObjectPass::Full => false,
            CpuObjectPass::Supplement => true,
            CpuObjectPass::Skip => return,
        };

        let view = self.document.view();
        let label_color = current_theme(painter.ctx()).object_label;
        match obj {
            GeoObject::Point(p) => {
                let screen = view.world_to_screen(p.position);
                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                let size = get_size(p.size, style).max(1.0);
                let color = to_color32(get_color(p.color, style));
                let label = get_label(&p.label, style);
                painter.circle_filled(pos, size, color);
                // Marcas de eje para puntos de intercepto (cerca de x=0 o y=0).
                let axis_tol = 1e-6f64.max(2.0 / view.scale);
                let tick_world = 6.0 / view.scale;
                if p.position.x.abs() <= axis_tol {
                    let a = view.world_to_screen(Point2::new(-tick_world, p.position.y));
                    let b = view.world_to_screen(Point2::new(tick_world, p.position.y));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(a.x, a.y),
                            canvas_rect.min + Vec2::new(b.x, b.y),
                        ],
                        Stroke::new(1.5, color),
                    );
                }
                if p.position.y.abs() <= axis_tol {
                    let a = view.world_to_screen(Point2::new(p.position.x, -tick_world));
                    let b = view.world_to_screen(Point2::new(p.position.x, tick_world));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(a.x, a.y),
                            canvas_rect.min + Vec2::new(b.x, b.y),
                        ],
                        Stroke::new(1.5, color),
                    );
                }
                if !label.is_empty() {
                    painter.text(
                        pos + Vec2::new(size + 2.0, -size - 2.0),
                        egui::Align2::LEFT_BOTTOM,
                        label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Line(l) => {
                let width = get_width(l.width, style);
                let color = get_color(l.color, style);
                let label = get_label(&l.label, style);
                let start = Point2::new(
                    self.document.resolve_expr(&l.start_x_expr, l.start.x),
                    self.document.resolve_expr(&l.start_y_expr, l.start.y),
                );
                let end = Point2::new(
                    self.document.resolve_expr(&l.end_x_expr, l.end.x),
                    self.document.resolve_expr(&l.end_y_expr, l.end.y),
                );

                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );

                let stroke = Stroke::new(width, to_color32(color));
                let clipped = match l.kind {
                    grafito_core::LineKind::Segment => {
                        grafito_geometry::clip_segment_to_rect(start, end, view_bounds)
                    }
                    grafito_core::LineKind::Ray => {
                        grafito_geometry::clip_ray_to_rect(start, end, view_bounds)
                    }
                    grafito_core::LineKind::Line => {
                        grafito_geometry::clip_line_to_rect(start, end, view_bounds)
                    }
                };
                if let Some((clip_start, clip_end)) = clipped {
                    let a = view.world_to_screen(clip_start);
                    let b = view.world_to_screen(clip_end);
                    let pa = canvas_rect.min + Vec2::new(a.x, a.y);
                    let pb = canvas_rect.min + Vec2::new(b.x, b.y);
                    if !overlay_only {
                        painter.line_segment([pa, pb], stroke);
                    }

                    // Arrowhead for vectors at the forward (t=1) end.
                    let is_vector = label == "v";
                    if is_vector && !overlay_only {
                        Self::draw_arrowhead(painter, pa, pb, width, to_color32(color));
                    }
                }
                if !label.is_empty() {
                    let mid = if l.kind == grafito_core::LineKind::Segment {
                        let a = view.world_to_screen(start);
                        let b = view.world_to_screen(end);
                        (a + b) * 0.5
                    } else {
                        // Place label near the start for rays/lines.
                        view.world_to_screen(start)
                    };
                    painter.text(
                        canvas_rect.min + Vec2::new(mid.x, mid.y) + Vec2::new(0.0, -8.0),
                        egui::Align2::CENTER_BOTTOM,
                        label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let radius = (c.radius * view.scale) as f32;
                let radius = radius.clamp(0.5, 50000.0);
                let pos = canvas_rect.min + Vec2::new(center.x, center.y);
                let width = get_width(c.width, style);
                let color = get_color(c.color, style);
                let fill_color = get_fill_color(c.fill_color, style);
                let label = get_label(&c.label, style);
                let stroke = Stroke::new(width, to_color32(color));
                if !overlay_only {
                    if let Some(fill) = fill_color {
                        painter.circle_filled(pos, radius, to_color32(fill));
                    }
                    painter.circle_stroke(pos, radius, stroke);
                }
                if !label.is_empty() {
                    painter.text(
                        pos + Vec2::new(radius + 2.0, -radius - 2.0),
                        egui::Align2::LEFT_BOTTOM,
                        label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let points: Vec<_> = poly
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let x = self
                            .document
                            .resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = self
                            .document
                            .resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        let s = view.world_to_screen(Point2::new(x, y));
                        canvas_rect.min + Vec2::new(s.x, s.y)
                    })
                    .collect();
                let width = get_width(poly.width, style);
                let color = get_color(poly.color, style);
                let fill_color = get_fill_color(poly.fill_color, style);
                let label = get_label(&poly.label, style);
                let stroke = Stroke::new(width, to_color32(color));
                let fill = fill_color.map(to_color32).unwrap_or(Color32::TRANSPARENT);
                if !overlay_only {
                    painter.add(Shape::convex_polygon(points.clone(), fill, stroke));
                }
                if !label.is_empty() {
                    let cx: f32 = points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32;
                    let cy: f32 = points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32;
                    painter.text(
                        Pos2::new(cx, cy),
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Pencil(pencil) if pencil.points.len() >= 2 || pencil.is_dynamic_locus() => {
                // Polilínea: dibuja cada par consecutivo como segmento.
                let width = get_width(pencil.width, style);
                let color = to_color32(get_color(pencil.color, style));
                let stroke = Stroke::new(width, color);
                for w in pencil.points.windows(2) {
                    let a = view.world_to_screen(w[0]);
                    let b = view.world_to_screen(w[1]);
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(a.x, a.y),
                            canvas_rect.min + Vec2::new(b.x, b.y),
                        ],
                        stroke,
                    );
                }
                if pencil.is_dynamic_locus() {
                    if let Some(last) = pencil.points.last().copied() {
                        let screen = view.world_to_screen(last);
                        let end = canvas_rect.min + Vec2::new(screen.x, screen.y);
                        painter.circle_stroke(end, (width * 1.6).max(3.0), Stroke::new(1.0, color));
                        let label = get_label(&pencil.label, style);
                        if !label.is_empty() {
                            painter.text(
                                end + Vec2::new(6.0, -6.0),
                                egui::Align2::LEFT_BOTTOM,
                                label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
            }
            GeoObject::Function(fun) => {
                let width = get_width(fun.width, style);
                let color = get_color(fun.color, style);
                let fill_color = get_fill_color(fun.fill_color, style);
                let label = get_label(&fun.label, style);
                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let min_x = self
                    .document
                    .resolve_expr(&fun.domain_min_expr, fun.domain_min.unwrap_or(world_tl.x));
                let max_x = self
                    .document
                    .resolve_expr(&fun.domain_max_expr, fun.domain_max.unwrap_or(world_br.x));

                let variables = &self.document.variables;
                let samples: Vec<(f64, Option<f64>)> = if fun.is_integral {
                    let screen_width = canvas_rect.width() as f64;
                    let world_width = max_x - min_x;

                    let mut steps = screen_width.clamp(400.0, 4000.0) as usize;
                    let max_world_step = 0.01;
                    let mut step = world_width / steps as f64;
                    if step > max_world_step {
                        steps = (world_width / max_world_step).ceil() as usize;
                    }
                    steps = steps.min(500_000);
                    step = world_width / steps as f64;

                    let xs = (0..=steps).map(|i| min_x + i as f64 * step);

                    let mut s: Vec<(f64, Option<f64>)> = Vec::with_capacity(steps + 1);
                    let batch_results = eval_integral_batch(
                        &fun.expr,
                        &fun.integral_var,
                        fun.integral_lower,
                        xs.clone(),
                        variables,
                    );

                    for (x, y_opt) in
                        xs.zip(batch_results.into_iter().chain(std::iter::repeat(None)))
                    {
                        if let Some(y) = y_opt {
                            if y.is_finite() {
                                s.push((x, Some(y)));
                                continue;
                            }
                        }
                        let eps = 1e-9;
                        if let (Ok(y1), Ok(y2)) = (
                            eval_function_with_vars(&fun.expr, x - eps, variables),
                            eval_function_with_vars(&fun.expr, x + eps, variables),
                        ) {
                            if y1.is_finite() && y2.is_finite() && (y1 - y2).abs() < 1.0 {
                                s.push((x, Some((y1 + y2) * 0.5)));
                                continue;
                            }
                        }
                        if let Ok(y) = eval_function_with_vars(&fun.expr, x, variables) {
                            if y.is_finite() {
                                s.push((x, Some(y)));
                                continue;
                            }
                        }
                        s.push((x, None));
                    }
                    refine_function_samples(s, &fun.expr, variables)
                } else {
                    let domain = (min_x, max_x);
                    let grid_size =
                        grafito_core::function_sampling::recommended_grid_size_for_quality(
                            canvas_rect.width(),
                            self.document.render_quality,
                        );
                    // PERF (H1): iteramos el guard del RwLock sin clonar (evita
                    // copiar hasta 10k muestras por frame en cache caliente). Si
                    // `samples_or_compute` devolviera `Arc<FunctionSamples>`,
                    // podríamos retener `&[Sample]` sin copia alguna.
                    let guard = grafito_core::function_sampling::samples_or_compute(
                        fun, domain, grid_size, variables,
                    );
                    refine_function_samples(guard.iter().copied(), &fun.expr, variables)
                };

                let projected_samples: Vec<Option<Pos2>> = samples
                    .iter()
                    .map(|&(x, y)| y.and_then(|y| function_screen_point(view, canvas_rect, x, y)))
                    .collect();
                let draw_bounds = canvas_rect.expand(1.0);

                // Fill area under curve if fill_color is set
                if !overlay_only {
                    if let Some(fill) = fill_color {
                        let fill_rgba = to_color32(fill);
                        let mut run: Vec<(Pos2, Pos2)> = Vec::new();
                        let flush_run = |run: &mut Vec<(Pos2, Pos2)>| {
                            if run.len() < 2 {
                                run.clear();
                                return;
                            }
                            let mut fill_pts: Vec<Pos2> =
                                run.iter().map(|(curve, _)| *curve).collect();
                            fill_pts.extend(run.iter().rev().map(|(_, baseline)| *baseline));
                            if fill_pts.len() >= 3 {
                                painter.add(Shape::Path(egui::epaint::PathShape {
                                    points: fill_pts,
                                    closed: true,
                                    fill: fill_rgba,
                                    stroke: Stroke::new(0.5, fill_rgba).into(),
                                }));
                            }
                            run.clear();
                        };

                        for ((x, _), curve) in samples.iter().zip(&projected_samples) {
                            if let (Some(curve), Some(baseline)) =
                                (curve, function_screen_point(view, canvas_rect, *x, 0.0))
                            {
                                if let Some((prev, _)) = run.last() {
                                    if !should_connect_screen_points(*prev, *curve, canvas_rect) {
                                        flush_run(&mut run);
                                    }
                                }
                                run.push((*curve, baseline));
                            } else {
                                flush_run(&mut run);
                            }
                        }
                        flush_run(&mut run);
                    }
                }

                if !overlay_only && !style.is_some_and(|style| style.skip_stroke) {
                    let stroke = Stroke::new(width, to_color32(color));
                    let mut optimized_points = Vec::new();
                    let mut i = 0;
                    while i < projected_samples.len() {
                        if let Some(p) = projected_samples[i] {
                            let px = p.x.round().clamp(draw_bounds.min.x, draw_bounds.max.x);
                            let first_y = p.y;
                            let mut min_y = p.y;
                            let mut max_y = p.y;
                            let mut min_j = i;
                            let mut max_j = i;
                            let mut last_y = p.y;

                            let mut j = i + 1;
                            while j < projected_samples.len() {
                                if let Some(p2) = projected_samples[j] {
                                    if p2.x.round().clamp(draw_bounds.min.x, draw_bounds.max.x)
                                        == px
                                    {
                                        if p2.y < min_y {
                                            min_y = p2.y;
                                            min_j = j;
                                        }
                                        if p2.y > max_y {
                                            max_y = p2.y;
                                            max_j = j;
                                        }
                                        last_y = p2.y;
                                        j += 1;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            optimized_points.push(Pos2::new(px, first_y));
                            if (min_y - first_y).abs() > 1.0 || (max_y - first_y).abs() > 1.0 {
                                if min_j < max_j {
                                    optimized_points.push(Pos2::new(px, min_y));
                                    optimized_points.push(Pos2::new(px, max_y));
                                } else {
                                    optimized_points.push(Pos2::new(px, max_y));
                                    optimized_points.push(Pos2::new(px, min_y));
                                }
                            }
                            optimized_points.push(Pos2::new(px, last_y));
                            i = j;
                        } else {
                            if !optimized_points.is_empty() {
                                painter.add(Shape::line(
                                    std::mem::take(&mut optimized_points),
                                    stroke,
                                ));
                            }
                            i += 1;
                        }
                    }
                    if !optimized_points.is_empty() {
                        painter.add(Shape::line(optimized_points, stroke));
                    }
                }

                if !label.is_empty() {
                    let mid_x = (min_x + max_x) * 0.5;
                    if let Ok(y) = grafito_geometry::expr::eval_function_with_vars(
                        &fun.expr,
                        mid_x,
                        &self.document.variables,
                    ) {
                        if let Some(position) = function_screen_point(view, canvas_rect, mid_x, y) {
                            let label_position = position + Vec2::new(0.0, 14.0);
                            if draw_bounds.contains(label_position) {
                                painter.text(
                                    label_position,
                                    egui::Align2::CENTER_TOP,
                                    label,
                                    egui::FontId::proportional(12.0),
                                    label_color,
                                );
                            }
                        }
                    }
                }
            }
            GeoObject::Ellipse(el) => {
                let stroke = Stroke::new(el.width, to_color32(el.color));
                let n = 64;
                let mut pts = Vec::with_capacity(n);
                for i in 0..n {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU;
                    let x = el.center.x + el.rx * t.cos() * el.angle.cos()
                        - el.ry * t.sin() * el.angle.sin();
                    let y = el.center.y
                        + el.rx * t.cos() * el.angle.sin()
                        + el.ry * t.sin() * el.angle.cos();
                    let s = view.world_to_screen(Point2::new(x, y));
                    pts.push(canvas_rect.min + Vec2::new(s.x, s.y));
                }
                if let Some(fill) = el.fill_color {
                    painter.add(Shape::convex_polygon(
                        pts.clone(),
                        to_color32(fill),
                        Stroke::NONE,
                    ));
                }
                for i in 0..n {
                    let j = (i + 1) % n;
                    painter.line_segment([pts[i], pts[j]], stroke);
                }
                if !el.label.is_empty() {
                    let s = view.world_to_screen(el.center);
                    painter.text(
                        canvas_rect.min
                            + Vec2::new(s.x, s.y + el.ry as f32 * view.scale as f32 + 14.0),
                        egui::Align2::CENTER_TOP,
                        &el.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Parabola(pb) => {
                if !pb.p.is_finite() || pb.p.abs() < 1e-12 {
                    return;
                }
                let stroke = Stroke::new(pb.width, to_color32(pb.color));
                let steps = 128;
                let range = (20.0 / view.scale).clamp(0.1, 500.0);
                let cos_a = pb.angle.cos();
                let sin_a = pb.angle.sin();
                let mut prev: Option<Pos2> = None;
                for i in 0..=steps {
                    let t = -range + 2.0 * range * i as f64 / steps as f64;
                    let lx = t;
                    let ly = t * t / (4.0 * pb.p);
                    let wx = pb.vertex.x + lx * cos_a - ly * sin_a;
                    let wy = pb.vertex.y + lx * sin_a + ly * cos_a;
                    let s = view.world_to_screen(Point2::new(wx, wy));
                    let p = canvas_rect.min + Vec2::new(s.x, s.y);
                    if wx.is_finite() && wy.is_finite() {
                        if let Some(prev_p) = prev {
                            if (p.x - prev_p.x).abs() < 300.0 {
                                painter.line_segment([prev_p, p], stroke);
                            }
                        }
                        prev = Some(p);
                    }
                }
                if !pb.label.is_empty() {
                    let s = view.world_to_screen(Point2::new(pb.vertex.x, pb.vertex.y - 1.0));
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y - 8.0),
                        egui::Align2::CENTER_BOTTOM,
                        &pb.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Hyperbola(hb) => {
                let stroke = Stroke::new(hb.width, to_color32(hb.color));
                let n = 64;
                let epsilon = 0.05;
                let cos_a = hb.angle.cos();
                let sin_a = hb.angle.sin();
                for branch in 0..2 {
                    let t_start = -std::f64::consts::FRAC_PI_2
                        + epsilon
                        + branch as f64 * std::f64::consts::PI;
                    let t_end = std::f64::consts::FRAC_PI_2 - epsilon
                        + branch as f64 * std::f64::consts::PI;
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=n {
                        let t = t_start + (t_end - t_start) * i as f64 / n as f64;
                        let sec = 1.0 / t.cos();
                        let tan = t.tan();
                        let (lx, ly) = if hb.horizontal {
                            (hb.a * sec, hb.b * tan)
                        } else {
                            (hb.b * tan, hb.a * sec)
                        };
                        let wx = hb.center.x + lx * cos_a - ly * sin_a;
                        let wy = hb.center.y + lx * sin_a + ly * cos_a;
                        if wx.is_finite() && wy.is_finite() {
                            let s = view.world_to_screen(Point2::new(wx, wy));
                            let p = canvas_rect.min + Vec2::new(s.x, s.y);
                            if let Some(prev_p) = prev {
                                if (p.x - prev_p.x).abs() < 300.0 {
                                    painter.line_segment([prev_p, p], stroke);
                                }
                            }
                            prev = Some(p);
                        }
                    }
                }
                if !hb.label.is_empty() {
                    let s =
                        view.world_to_screen(Point2::new(hb.center.x, hb.center.y + hb.b + 0.5));
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y),
                        egui::Align2::CENTER_BOTTOM,
                        &hb.label,
                        egui::FontId::proportional(12.0),
                        label_color,
                    );
                }
            }
            GeoObject::Text(txt) => {
                let s = view.world_to_screen(txt.position);
                painter.text(
                    canvas_rect.min + Vec2::new(s.x, s.y),
                    egui::Align2::LEFT_CENTER,
                    &txt.content,
                    egui::FontId::proportional(txt.font_size.max(8.0)),
                    to_color32(txt.color),
                );
            }
            GeoObject::Histogram(h) => {
                let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                if max_count <= 0.0 {
                    return;
                }
                let stroke = Stroke::new(h.width, to_color32(h.color));
                let fill = h
                    .fill_color
                    .map(to_color32)
                    .unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 100));
                let y_scale = (h.y_max - h.y_min) / max_count;
                for (left, right, count) in &bins {
                    let bl = view.world_to_screen(Point2::new(*left, h.y_min));
                    let tr = view.world_to_screen(Point2::new(*right, h.y_min + count * y_scale));
                    let rect = Rect::from_min_max(
                        canvas_rect.min + Vec2::new(tr.x, bl.y),
                        canvas_rect.min + Vec2::new(bl.x, tr.y),
                    );
                    painter.rect_filled(rect, 0.0, fill);
                    painter.rect_stroke(rect, 0.0, stroke);
                }
            }
            GeoObject::ScatterPlot(sp) => {
                let color = to_color32(sp.color);
                let r = sp.point_size.max(1.0);
                for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                    let s = view.world_to_screen(Point2::new(*x, *y));
                    painter.circle_filled(canvas_rect.min + Vec2::new(s.x, s.y), r, color);
                }
            }
            GeoObject::BoxPlot(bp) => {
                if let Some((wl, q1, med, q3, wh, outliers)) =
                    grafito_geometry::statistics::boxplot_stats(&bp.data)
                {
                    let stroke = Stroke::new(bp.width, to_color32(bp.color));
                    let fill = bp
                        .fill_color
                        .map(to_color32)
                        .unwrap_or(Color32::from_rgba_premultiplied(50, 120, 220, 80));
                    let half_w = bp.width_box * 0.5;
                    let bx_min = bp.position - half_w;
                    let bx_max = bp.position + half_w;
                    let s_q1 = view.world_to_screen(Point2::new(bx_min, q1));
                    let s_q3 = view.world_to_screen(Point2::new(bx_max, q3));
                    let box_rect = Rect::from_min_max(
                        canvas_rect.min + Vec2::new(s_q3.x, s_q3.y),
                        canvas_rect.min + Vec2::new(s_q1.x, s_q1.y),
                    );
                    painter.rect_filled(box_rect, 0.0, fill);
                    painter.rect_stroke(box_rect, 0.0, stroke);
                    let s_med_l = view.world_to_screen(Point2::new(bx_min, med));
                    let s_med_r = view.world_to_screen(Point2::new(bx_max, med));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_med_l.x, s_med_l.y),
                            canvas_rect.min + Vec2::new(s_med_r.x, s_med_r.y),
                        ],
                        Stroke::new(bp.width * 2.0, to_color32(bp.color)),
                    );
                    let s_wl = view.world_to_screen(Point2::new(bp.position, wl));
                    let s_q1c = view.world_to_screen(Point2::new(bp.position, q1));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wl.x, s_wl.y),
                            canvas_rect.min + Vec2::new(s_q1c.x, s_q1c.y),
                        ],
                        stroke,
                    );
                    let s_wh = view.world_to_screen(Point2::new(bp.position, wh));
                    let s_q3c = view.world_to_screen(Point2::new(bp.position, q3));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wh.x, s_wh.y),
                            canvas_rect.min + Vec2::new(s_q3c.x, s_q3c.y),
                        ],
                        stroke,
                    );
                    let wl_half = half_w * 0.4;
                    let s_wl_l = view.world_to_screen(Point2::new(bp.position - wl_half, wl));
                    let s_wl_r = view.world_to_screen(Point2::new(bp.position + wl_half, wl));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wl_l.x, s_wl_l.y),
                            canvas_rect.min + Vec2::new(s_wl_r.x, s_wl_r.y),
                        ],
                        stroke,
                    );
                    let s_wh_l = view.world_to_screen(Point2::new(bp.position - wl_half, wh));
                    let s_wh_r = view.world_to_screen(Point2::new(bp.position + wl_half, wh));
                    painter.line_segment(
                        [
                            canvas_rect.min + Vec2::new(s_wh_l.x, s_wh_l.y),
                            canvas_rect.min + Vec2::new(s_wh_r.x, s_wh_r.y),
                        ],
                        stroke,
                    );
                    for &o in &outliers {
                        let s_o = view.world_to_screen(Point2::new(bp.position, o));
                        painter.circle_stroke(
                            canvas_rect.min + Vec2::new(s_o.x, s_o.y),
                            3.0,
                            stroke,
                        );
                    }
                }
            }
            GeoObject::RegressionLine(rl) => {
                let stroke = Stroke::new(rl.width, to_color32(rl.color));
                let x0 = rl.x_min;
                let x1 = rl.x_max;
                let y0 = rl.slope * x0 + rl.intercept;
                let y1 = rl.slope * x1 + rl.intercept;
                let s0 = view.world_to_screen(Point2::new(x0, y0));
                let s1 = view.world_to_screen(Point2::new(x1, y1));
                painter.line_segment(
                    [
                        canvas_rect.min + Vec2::new(s0.x, s0.y),
                        canvas_rect.min + Vec2::new(s1.x, s1.y),
                    ],
                    stroke,
                );
                let pt_color = to_color32(rl.color);
                for (x, y) in rl.xs.iter().zip(rl.ys.iter()) {
                    let s = view.world_to_screen(Point2::new(*x, *y));
                    painter.circle_filled(canvas_rect.min + Vec2::new(s.x, s.y), 4.0, pt_color);
                }
                if !rl.label.is_empty() {
                    let s = view.world_to_screen(Point2::new(
                        (x0 + x1) * 0.5,
                        rl.slope * (x0 + x1) * 0.5 + rl.intercept,
                    ));
                    painter.text(
                        canvas_rect.min + Vec2::new(s.x, s.y - 12.0),
                        egui::Align2::CENTER_BOTTOM,
                        &rl.label,
                        egui::FontId::proportional(11.0),
                        to_color32(rl.color),
                    );
                }
            }
            GeoObject::Fractal2D(fr) => {
                use grafito_geometry::fractals::fractal_color_hsv;
                // Cache keyed por document.version: evita recomputar 160k píxeles cada frame.
                let Some(pixels) = cached_try_compute_fractal(fr, self.document.version) else {
                    return;
                };
                let res = fr.resolution;
                if res == 0 {
                    return;
                }
                let dx = (fr.x_max - fr.x_min) / res as f64;
                let dy = (fr.y_max - fr.y_min) / res as f64;
                for px in &pixels {
                    let (r, g, b, a) = fractal_color_hsv(px.iter, px.max_iter, px.smooth_value);
                    let bl = view.world_to_screen(Point2::new(px.x, px.y));
                    let tr = view.world_to_screen(Point2::new(px.x + dx, px.y + dy));
                    let rect = Rect::from_min_max(
                        canvas_rect.min + Vec2::new(bl.x, tr.y),
                        canvas_rect.min + Vec2::new(tr.x, bl.y),
                    );
                    painter.rect_filled(
                        rect,
                        0.0,
                        Color32::from_rgba_premultiplied(
                            (r * 255.0) as u8,
                            (g * 255.0) as u8,
                            (b * 255.0) as u8,
                            (a * 255.0) as u8,
                        ),
                    );
                }
            }
            GeoObject::ParametricCurve2D(pc) => {
                let steps = 4000;
                let samples = parametric_sampling::samples_or_compute_curve_2d(
                    pc,
                    steps,
                    &self.document.variables,
                );
                let mut prev: Option<Pos2> = None;
                for &(x, y) in samples.iter() {
                    if x.is_finite() && y.is_finite() {
                        let screen = view.world_to_screen(Point2::new(x, y));
                        let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                        if let Some(prev_pos) = prev {
                            if !overlay_only
                                && !style.is_some_and(|style| style.skip_stroke)
                                && should_connect_screen_points(prev_pos, pos, canvas_rect)
                            {
                                painter.line_segment(
                                    [prev_pos, pos],
                                    Stroke::new(pc.width, to_color32(pc.color)),
                                );
                            }
                        }
                        prev = Some(pos);
                    } else {
                        prev = None;
                    }
                }
            }
            GeoObject::PolarCurve(pol) => {
                let steps = 4000;
                let samples = parametric_sampling::samples_or_compute_polar(
                    pol,
                    steps,
                    &self.document.variables,
                );
                let projected: Vec<_> = samples
                    .iter()
                    .map(|&(x, y)| {
                        (x.is_finite() && y.is_finite()).then(|| {
                            let screen = view.world_to_screen(Point2::new(x, y));
                            canvas_rect.min + Vec2::new(screen.x, screen.y)
                        })
                    })
                    .collect();
                let runs = split_continuous_screen_runs(&projected, canvas_rect);
                if !overlay_only && !style.is_some_and(|style| style.skip_stroke) {
                    for run in &runs {
                        for points in run.windows(2) {
                            painter.line_segment(
                                [points[0], points[1]],
                                Stroke::new(pol.width, to_color32(pol.color)),
                            );
                        }
                    }
                }
                // Fill from origin
                if !overlay_only {
                    if let Some(fill) = get_fill_color(pol.fill_color, style) {
                        let origin = view.world_to_screen(Point2::new(0.0, 0.0));
                        let origin_pos = canvas_rect.min + Vec2::new(origin.x, origin.y);
                        for run in runs.iter().filter(|run| run.len() >= 2) {
                            let mut fill_pts = run.clone();
                            fill_pts.push(origin_pos);
                            fill_pts.push(run[0]);
                            painter.add(Shape::Path(egui::epaint::PathShape {
                                points: fill_pts,
                                closed: true,
                                fill: to_color32(fill),
                                stroke: Stroke::NONE.into(),
                            }));
                        }
                    }
                }
                let all_pts: Vec<_> = runs.iter().flatten().copied().collect();
                let label = get_label(&pol.label, style);
                if !label.is_empty() {
                    if let Some(position) = all_pts.get(all_pts.len() / 2) {
                        painter.text(
                            *position + Vec2::new(0.0, 14.0),
                            egui::Align2::CENTER_TOP,
                            label,
                            egui::FontId::proportional(12.0),
                            label_color,
                        );
                    }
                }
            }
            GeoObject::VectorField2D(vf) => {
                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let view_bounds = (
                    world_tl.x.min(world_br.x),
                    world_tl.x.max(world_br.x),
                    world_br.y.min(world_tl.y),
                    world_br.y.max(world_tl.y),
                );
                let grid_size = vf.density.clamp(5, 80);
                let dx = (view_bounds.1 - view_bounds.0).abs() / grid_size as f64;
                let dy = (view_bounds.3 - view_bounds.2).abs() / grid_size as f64;
                let arrow_length = dx.min(dy) * 0.8;

                // Evita `samples.clone()` iterando directamente sobre el guard del RwLock.
                // `samples_or_compute` devuelve `RwLockReadGuard<Vec<...>>`; iterar sin clonar
                // evita duplicar ~16k tuplas por frame y respeta el cache RwLock.
                let samples = vector_field_sampling::samples_or_compute(
                    vf,
                    view_bounds,
                    grid_size,
                    &self.document.variables,
                );
                for (x, y, u, v) in samples.iter().copied() {
                    if x < view_bounds.0 - dx
                        || x > view_bounds.1 + dx
                        || y < view_bounds.2 - dy
                        || y > view_bounds.3 + dy
                    {
                        continue;
                    }
                    if u.is_finite() && v.is_finite() {
                        let mag = (u * u + v * v).sqrt();
                        if mag > 1e-10 {
                            let nu = u / mag * arrow_length;
                            let nv = v / mag * arrow_length;

                            let start = view.world_to_screen(Point2::new(x, y));
                            let end = view.world_to_screen(Point2::new(x + nu, y + nv));
                            let start_pos = canvas_rect.min + Vec2::new(start.x, start.y);
                            let end_pos = canvas_rect.min + Vec2::new(end.x, end.y);

                            painter.line_segment(
                                [start_pos, end_pos],
                                Stroke::new(1.5, to_color32(vf.color)),
                            );

                            // Arrow head
                            let angle = (nv as f32).atan2(nu as f32);
                            let head_len = arrow_length as f32 * 0.3;
                            let head1 = end_pos
                                + Vec2::new(
                                    -head_len * (angle - 0.4).cos(),
                                    -head_len * (angle - 0.4).sin(),
                                );
                            let head2 = end_pos
                                + Vec2::new(
                                    -head_len * (angle + 0.4).cos(),
                                    -head_len * (angle + 0.4).sin(),
                                );
                            painter.line_segment(
                                [end_pos, head1],
                                Stroke::new(1.5, to_color32(vf.color)),
                            );
                            painter.line_segment(
                                [end_pos, head2],
                                Stroke::new(1.5, to_color32(vf.color)),
                            );
                        }
                    }
                }
                // Streamlines: trace from seed points using RK4
                let sl_steps = 200;
                let sl_dt = 0.05;
                let sl_color = Color32::from_rgba_unmultiplied(180, 100, 200, 180);
                let sl_stroke = Stroke::new(1.2, sl_color);
                let prepared_u =
                    prepare_function_ast(&vf.expr_u, &self.document.variables, &["x", "y"]).ok();
                let prepared_v =
                    prepare_function_ast(&vf.expr_v, &self.document.variables, &["x", "y"]).ok();
                // Evita `base_environment.clone()` en cada evaluación RK4 usando un vec mutable
                // con slots fijos para "x" e "y" (20k clones por frame → 0).
                let mut environment: Vec<(String, f64)> = self
                    .document
                    .variables
                    .iter()
                    .filter(|(name, _)| name.as_str() != "x" && name.as_str() != "y")
                    .map(|(name, value)| (name.clone(), *value))
                    .collect();
                environment.sort_unstable_by(|left, right| left.0.cmp(&right.0));
                environment.push(("x".to_string(), 0.0));
                environment.push(("y".to_string(), 0.0));
                let x_idx = environment.len() - 2;
                let y_idx = environment.len() - 1;
                let mut evaluate_field = |x: f64, y: f64| {
                    environment[x_idx].1 = x;
                    environment[y_idx].1 = y;
                    let u = prepared_u
                        .as_ref()
                        .map(|ast| ast.eval_2d("x", x, "y", y))
                        .filter(|value| value.is_finite())
                        .or_else(|| {
                            grafito_geometry::expr::evaluate(&vf.expr_u, &environment)
                                .ok()
                                .filter(|value| value.is_finite())
                        })?;
                    let v = prepared_v
                        .as_ref()
                        .map(|ast| ast.eval_2d("x", x, "y", y))
                        .filter(|value| value.is_finite())
                        .or_else(|| {
                            grafito_geometry::expr::evaluate(&vf.expr_v, &environment)
                                .ok()
                                .filter(|value| value.is_finite())
                        })?;
                    Some((u, v))
                };
                // Distribute seeds uniformly
                let seeds_x = 5;
                let seeds_y = 5;
                let sx = (world_br.x - world_tl.x) / (seeds_x + 1) as f64;
                let sy = (world_br.y - world_tl.y) / (seeds_y + 1) as f64;
                for si in 1..=seeds_x {
                    for sj in 1..=seeds_y {
                        let mut x = world_tl.x + si as f64 * sx;
                        let mut y = world_tl.y + sj as f64 * sy;
                        let mut prev: Option<Pos2> = None;
                        for _ in 0..sl_steps {
                            let Some((k1x, k1y)) = evaluate_field(x, y) else {
                                break;
                            };
                            let half_dt = sl_dt * 0.5;
                            let Some((k2x, k2y)) =
                                evaluate_field(x + half_dt * k1x, y + half_dt * k1y)
                            else {
                                break;
                            };
                            let Some((k3x, k3y)) =
                                evaluate_field(x + half_dt * k2x, y + half_dt * k2y)
                            else {
                                break;
                            };
                            let Some((k4x, k4y)) = evaluate_field(x + sl_dt * k3x, y + sl_dt * k3y)
                            else {
                                break;
                            };
                            x += sl_dt / 6.0 * (k1x + 2.0 * k2x + 2.0 * k3x + k4x);
                            y += sl_dt / 6.0 * (k1y + 2.0 * k2y + 2.0 * k3y + k4y);
                            let screen = view.world_to_screen(Point2::new(x, y));
                            let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                            if x < world_tl.x - dx
                                || x > world_br.x + dx
                                || y < world_tl.y - dy
                                || y > world_br.y + dy
                            {
                                break;
                            }
                            if let Some(prev_pos) = prev {
                                painter.line_segment([prev_pos, pos], sl_stroke);
                            }
                            prev = Some(pos);
                        }
                    }
                }
            }
            GeoObject::PhasePortrait(portrait) => {
                // Cache keyed por document.version: evita re-muestrear la grilla cada frame.
                let segments = cached_sample_phase_portrait(
                    portrait,
                    &self.document.variables,
                    self.document.version,
                );
                if !overlay_only && !style.is_some_and(|style| style.skip_stroke) {
                    let stroke = Stroke::new(1.5, to_color32(portrait.color));
                    for (start, end) in &segments {
                        let start = view.world_to_screen(*start);
                        let end = view.world_to_screen(*end);
                        if start.is_finite() && end.is_finite() {
                            painter.line_segment(
                                [
                                    canvas_rect.min + Vec2::new(start.x, start.y),
                                    canvas_rect.min + Vec2::new(end.x, end.y),
                                ],
                                stroke,
                            );
                        }
                    }
                }
                let label = get_label(&portrait.label, style);
                if !label.is_empty() {
                    if let Some((start, _)) = segments.get(segments.len() / 2) {
                        let start = view.world_to_screen(*start);
                        if start.is_finite() {
                            painter.text(
                                canvas_rect.min + Vec2::new(start.x, start.y - 8.0),
                                egui::Align2::CENTER_BOTTOM,
                                label,
                                egui::FontId::proportional(12.0),
                                label_color,
                            );
                        }
                    }
                }
            }
            GeoObject::ImplicitCurve(ic) => {
                let quality = self.document.render_quality;
                if !implicit_curve_cache_matches_request(
                    ic,
                    implicit_curve_view_bounds(*view, canvas_rect),
                    implicit_curve_grid_size(canvas_rect, quality),
                    &self.document.variables,
                    quality,
                ) {
                    return;
                }
                // 0) Relleno del interior (solo para regiones con `<=`, `>=`, `<`, `>`).
                //    Para curvas con `=`, no hay interior que rellenar.
                if !overlay_only {
                    if let Some(fill_color) = ic.fill_color {
                        if matches!(
                            ic.operator,
                            RelationOperator::Less
                                | RelationOperator::LessEq
                                | RelationOperator::Greater
                                | RelationOperator::GreaterEq
                        ) {
                            self.draw_implicit_curve_fill(
                                painter,
                                canvas_rect,
                                view,
                                ic,
                                fill_color,
                            );
                        }
                    }
                }

                // 1) Contorno: dibujar los segmentos del marching squares.
                // Read-lock optimizado + clon breve para no retener el lock durante el pintado.
                let levels = {
                    let guard = read_lock_optimized(&ic.cached_segments);
                    guard.clone()
                };
                if !levels.is_empty() {
                    let use_contour_colors = ic.contour_levels.is_some();
                    let contour_count = levels.len();
                    let palette = ic.contour_colors.as_deref().unwrap_or(&[]);
                    for (idx, (_level, segs)) in levels.iter().enumerate() {
                        let color = if use_contour_colors {
                            palette.get(idx).cloned().unwrap_or_else(|| {
                                let t = idx as f64 / contour_count.max(1) as f64;
                                Color::new(
                                    (0.5 + t * 0.5) as f32,
                                    (0.2 + (1.0 - t) * 0.6) as f32,
                                    0.2,
                                    1.0,
                                )
                            })
                        } else {
                            ic.color
                        };
                        let stroke = Stroke::new(ic.width, to_color32(color));
                        for (a, b) in segs {
                            let p1 = view.world_to_screen(*a);
                            let p2 = view.world_to_screen(*b);
                            let pos1 = canvas_rect.min + Vec2::new(p1.x, p1.y);
                            let pos2 = canvas_rect.min + Vec2::new(p2.x, p2.y);
                            painter.line_segment([pos1, pos2], stroke);
                        }
                    }
                }
            }
            GeoObject::ComplexGrid(cg) => {
                use num_complex::Complex64;
                use std::collections::HashMap;
                // Recorta al canvas: el domain coloring y la rejilla deformada
                // pueden emitir primitivas fuera del área de dibujo.
                let painter = painter.with_clip_rect(canvas_rect);

                if cg.render_mode == 1 || cg.render_mode == 2 {
                    // Domain coloring (complex f(z)) or Heat map (real f(x,y))
                    let res = complex_grid_cpu_resolution(cg.density, self.document.render_quality);
                    let dx = (cg.x_max - cg.x_min) / res as f64;
                    let dy = (cg.y_max - cg.y_min) / res as f64;

                    let is_heatmap = cg.render_mode == 2;

                    if is_heatmap {
                        // Heat map: evaluate f(x,y) using real AST
                        let prepared =
                            prepare_function_ast(&cg.expr, &self.document.variables, &["x", "y"]);

                        if let Ok(ast) = prepared {
                            for j in 0..res {
                                let y = cg.y_min + (res - 1 - j) as f64 * dy;
                                for i in 0..res {
                                    let x = cg.x_min + i as f64 * dx;
                                    let val = ast.eval_2d("x", x, "y", y);
                                    if val.is_finite() {
                                        // Thermal colormap: blue(cold) through green to red(hot)
                                        let t = (val.atan() / std::f64::consts::FRAC_PI_2)
                                            .clamp(-1.0, 1.0);
                                        let t = (t + 1.0) * 0.5; // [0, 1]
                                        let (r, g, b) = thermal_colormap(t);
                                        let sp1 = view.world_to_screen(Point2::new(
                                            cg.x_min + i as f64 * dx,
                                            cg.y_min + (res - 1 - j) as f64 * dy,
                                        ));
                                        let sp2 = view.world_to_screen(Point2::new(
                                            cg.x_min + (i + 1) as f64 * dx,
                                            cg.y_min + (res - j) as f64 * dy,
                                        ));
                                        let min = canvas_rect.min + Vec2::new(sp1.x, sp2.y);
                                        let max = canvas_rect.min + Vec2::new(sp2.x, sp1.y);
                                        let c = Color32::from_rgb(
                                            (r * 255.0) as u8,
                                            (g * 255.0) as u8,
                                            (b * 255.0) as u8,
                                        );
                                        painter.rect_filled(Rect::from_min_max(min, max), 0.0, c);
                                    }
                                }
                            }
                        }
                    } else {
                        // Domain coloring: evaluate complex f(z)
                        let expr = match grafito_complex::complex_expr::parse(&cg.expr) {
                            Ok(e) => e,
                            Err(_) => return,
                        };
                        let mut vars: HashMap<String, Complex64> = HashMap::new();
                        for (name, val) in &self.document.variables {
                            vars.insert(name.clone(), Complex64::new(*val, 0.0));
                        }
                        // PERF (H9): cachear `complex_base_symbol` como `&str` y
                        // actualizar la clave con `get_mut` evita 250k `String`
                        // allocations por frame (una por píxel en domain coloring).
                        let base_symbol = self.document.complex_base_symbol.as_str();
                        vars.insert(base_symbol.to_string(), Complex64::new(0.0, 0.0));
                        let dc_mode = cg.domain_coloring_mode;
                        // Umbral: si |f(z)| < MAG_ZERO se considera cero y se pinta negro.
                        // Evita que arg(~0) dé ruido aleatorio en retratos de fase.
                        const MAG_ZERO: f64 = 1e-6;
                        for j in 0..res {
                            let y = cg.y_min + (res - 1 - j) as f64 * dy;
                            for i in 0..res {
                                let x = cg.x_min + i as f64 * dx;
                                if let Some(z) = vars.get_mut(base_symbol) {
                                    *z = Complex64::new(x, y);
                                }
                                if let Ok(fz) = expr.eval(&vars) {
                                    if fz.re.is_finite() && fz.im.is_finite() {
                                        let mag = fz.norm();
                                        // Función identicamente nula → negro (evita ruido de arg(0))
                                        if mag < MAG_ZERO {
                                            let sp1 = view.world_to_screen(Point2::new(
                                                cg.x_min + i as f64 * dx,
                                                cg.y_min + (res - 1 - j) as f64 * dy,
                                            ));
                                            let sp2 = view.world_to_screen(Point2::new(
                                                cg.x_min + (i + 1) as f64 * dx,
                                                cg.y_min + (res - j) as f64 * dy,
                                            ));
                                            let min = canvas_rect.min + Vec2::new(sp1.x, sp2.y);
                                            let max = canvas_rect.min + Vec2::new(sp2.x, sp1.y);
                                            painter.rect_filled(
                                                Rect::from_min_max(min, max),
                                                0.0,
                                                Color32::BLACK,
                                            );
                                            continue;
                                        }
                                        let arg = fz.arg();
                                        let hue = (arg + std::f64::consts::PI)
                                            / (2.0 * std::f64::consts::PI);
                                        // Calcula lightness y saturation según el modo de coloración
                                        let (lightness, saturation) = match dc_mode {
                                            // 0: HSL Clásico — lightness varía con módulo
                                            0 => {
                                                let l = (mag.max(1e-10).ln().atan()
                                                    / std::f64::consts::FRAC_PI_2)
                                                    * 0.5
                                                    + 0.5;
                                                (l.clamp(0.0, 1.0), 0.85)
                                            }
                                            // 1: Retrato de Fase Puro — lightness=0.5 constante, sat=1
                                            1 => (0.5, 1.0),
                                            // 2: Rejilla Polar Conforme — como HSL + damping por rejilla polar
                                            2 => {
                                                let l = (mag.max(1e-10).ln().atan()
                                                    / std::f64::consts::FRAC_PI_2)
                                                    * 0.5
                                                    + 0.5;
                                                (l.clamp(0.0, 1.0), 0.85)
                                            }
                                            // 3: Rejilla Cartesiana — como HSL
                                            3 => {
                                                let l = (mag.max(1e-10).ln().atan()
                                                    / std::f64::consts::FRAC_PI_2)
                                                    * 0.5
                                                    + 0.5;
                                                (l.clamp(0.0, 1.0), 0.85)
                                            }
                                            _ => {
                                                let l = (mag.max(1e-10).ln().atan()
                                                    / std::f64::consts::FRAC_PI_2)
                                                    * 0.5
                                                    + 0.5;
                                                (l.clamp(0.0, 1.0), 0.85)
                                            }
                                        };
                                        let (mut r, mut g, mut b) =
                                            hsl_to_rgb(hue, saturation, lightness);
                                        // Overlay de rejillas conformes (modos 2 y 3)
                                        if dc_mode == 2 {
                                            let log_mag = mag.max(1e-5).ln();
                                            let mag_grid =
                                                (log_mag * std::f64::consts::PI * 2.0).sin().abs();
                                            let arg_grid = (arg * 10.0).sin().abs();
                                            let shading = 0.5
                                                + 0.5
                                                    * mag_grid.max(0.0).powf(0.15)
                                                    * arg_grid.max(0.0).powf(0.15);
                                            r *= shading;
                                            g *= shading;
                                            b *= shading;
                                        } else if dc_mode == 3 {
                                            let grid_re =
                                                (fz.re * std::f64::consts::PI * 2.0).sin().abs();
                                            let grid_im =
                                                (fz.im * std::f64::consts::PI * 2.0).sin().abs();
                                            let shading = 0.5
                                                + 0.5
                                                    * grid_re.max(0.0).powf(0.15)
                                                    * grid_im.max(0.0).powf(0.15);
                                            r *= shading;
                                            g *= shading;
                                            b *= shading;
                                        }
                                        let sp1 = view.world_to_screen(Point2::new(
                                            cg.x_min + i as f64 * dx,
                                            cg.y_min + (res - 1 - j) as f64 * dy,
                                        ));
                                        let sp2 = view.world_to_screen(Point2::new(
                                            cg.x_min + (i + 1) as f64 * dx,
                                            cg.y_min + (res - j) as f64 * dy,
                                        ));
                                        let min = canvas_rect.min + Vec2::new(sp1.x, sp2.y);
                                        let max = canvas_rect.min + Vec2::new(sp2.x, sp1.y);
                                        let c = Color32::from_rgb(
                                            (r * 255.0) as u8,
                                            (g * 255.0) as u8,
                                            (b * 255.0) as u8,
                                        );
                                        painter.rect_filled(Rect::from_min_max(min, max), 0.0, c);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }

                // Original: Draw deformed grid under complex mapping
                let grid_lines = cg.density;
                let dx = (cg.x_max - cg.x_min) / grid_lines as f64;
                let dy = (cg.y_max - cg.y_min) / grid_lines as f64;

                let expr = match grafito_complex::complex_expr::parse(&cg.expr) {
                    Ok(e) => e,
                    Err(_) => return,
                };

                let mut vars: HashMap<String, Complex64> = HashMap::new();
                for (name, val) in &self.document.variables {
                    vars.insert(name.clone(), Complex64::new(*val, 0.0));
                }
                // PERF (H9): misma técnica que domain coloring — cachear el
                // símbolo base como `&str` y actualizar con `get_mut` (sin
                // `String` alloc por punto de la rejilla deformada).
                let base_symbol = self.document.complex_base_symbol.as_str();
                vars.insert(base_symbol.to_string(), Complex64::new(0.0, 0.0));

                // Draw horizontal lines (constant imaginary part)
                for j in 0..=grid_lines {
                    let y = cg.y_min + j as f64 * dy;
                    let mut prev: Option<Pos2> = None;
                    for i in 0..=grid_lines * 4 {
                        let x = cg.x_min + i as f64 * dx / 4.0;
                        if let Some(z) = vars.get_mut(base_symbol) {
                            *z = Complex64::new(x, y);
                        }

                        if let Ok(result) = expr.eval(&vars) {
                            if result.re.is_finite()
                                && result.im.is_finite()
                                && result.re.abs() < 1e6
                                && result.im.abs() < 1e6
                            {
                                let screen =
                                    view.world_to_screen(Point2::new(result.re, result.im));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment(
                                        [prev_pos, pos],
                                        Stroke::new(1.0, to_color32(cg.color)),
                                    );
                                }
                                prev = Some(pos);
                            } else {
                                prev = None;
                            }
                        } else {
                            prev = None;
                        }
                    }
                }

                // Draw vertical lines (constant real part)
                for i in 0..=grid_lines {
                    let x = cg.x_min + i as f64 * dx;
                    let mut prev: Option<Pos2> = None;
                    for j in 0..=grid_lines * 4 {
                        let y = cg.y_min + j as f64 * dy / 4.0;
                        if let Some(z) = vars.get_mut(base_symbol) {
                            *z = Complex64::new(x, y);
                        }

                        if let Ok(result) = expr.eval(&vars) {
                            if result.re.is_finite()
                                && result.im.is_finite()
                                && result.re.abs() < 1e6
                                && result.im.abs() < 1e6
                            {
                                let screen =
                                    view.world_to_screen(Point2::new(result.re, result.im));
                                let pos = canvas_rect.min + Vec2::new(screen.x, screen.y);
                                if let Some(prev_pos) = prev {
                                    painter.line_segment(
                                        [prev_pos, pos],
                                        Stroke::new(1.0, to_color32(cg.color)),
                                    );
                                }
                                prev = Some(pos);
                            } else {
                                prev = None;
                            }
                        } else {
                            prev = None;
                        }
                    }
                }
            }
            GeoObject::ComplexMapping(cm) => {
                use num_complex::Complex64;
                use std::collections::HashMap;
                // Recorta al canvas: trazos transformados (asíntotas, segmentos
                // largos) pueden salir del área de dibujo.
                let painter = painter.with_clip_rect(canvas_rect);

                // 1) Validar que la expresión compleja parsea. Si falla,
                //    skip (es comportamiento lazy: el objeto queda creado pero
                //    no se dibuja hasta que la expresión sea válida).
                // PERF (H10): parsear una sola vez y cachear el AST por string
                // de expresión (`cached_complex_expr`); el parseo solo ocurre
                // en cache miss, no en cada frame. El AST se reutiliza en el
                // batch eval de abajo (evita el doble parse por frame).
                let parsed_expr = match cached_complex_expr(&cm.expr) {
                    Some(expr) => expr,
                    None => return,
                };

                // 2) Resolver el target. Si no existe o el tipo no está
                //    soportado, no dibujamos nada.
                let target = match self.document.get_object(cm.target) {
                    Some(t) => t,
                    None => return,
                };

                // 3) Extraer el dominio visible para sampling de Function y
                //    cotas de muestreo para ParametricCurve2D / PolarCurve.
                let world_tl = view.screen_to_world(GlamVec2::new(0.0, 0.0));
                let world_br =
                    view.screen_to_world(GlamVec2::new(canvas_rect.width(), canvas_rect.height()));
                let (xmin, xmax) = (world_tl.x.min(world_br.x), world_tl.x.max(world_br.x));
                let (ymin, ymax) = (world_br.y.min(world_tl.y), world_br.y.max(world_tl.y));

                let conformal_map = cm.conformal_map(self.document.complex_base_symbol.as_str());
                let homotopy_factor = grafito_render::complex_mapping_homotopy_factor(
                    cm.animate_homotopy,
                    cm.homotopy_speed,
                    self.transient_render_state.homotopy_time(),
                );

                if let (GeoObject::ImplicitCurve(ic), Some(map)) = (target, conformal_map) {
                    if !implicit_curve_cache_matches_request(
                        ic,
                        (xmin, xmax, ymin, ymax),
                        implicit_curve_grid_size(canvas_rect, self.document.render_quality),
                        &self.document.variables,
                        self.document.render_quality,
                    ) {
                        return;
                    }
                    if homotopy_factor >= 1.0 - f64::EPSILON {
                        if let Some(fill_color) = ic.fill_color {
                            self.draw_complex_mapping_fill(
                                &painter,
                                canvas_rect,
                                view,
                                ic,
                                cm.id,
                                map,
                                fill_color,
                            );
                        }
                    }

                    let mut source_segments = Vec::new();
                    for (_level, segments) in self.document.implicit_curve_segments(cm.target) {
                        for (a, b) in segments {
                            let len = (a.x - b.x).hypot(a.y - b.y);
                            if len >= 1e-3 {
                                source_segments.push((a, b));
                            }
                        }
                    }

                    let stroke = Stroke::new(2.0, to_color32(cm.color));
                    for (a, b) in complex_mapping_segment_strokes_at(
                        map,
                        &source_segments,
                        16,
                        homotopy_factor,
                    ) {
                        let p1 = view.world_to_screen(a);
                        let p2 = view.world_to_screen(b);
                        if (p2.x - p1.x).abs() > 300.0 || (p2.y - p1.y).abs() > 300.0 {
                            continue;
                        }
                        let pos1 = canvas_rect.min + Vec2::new(p1.x, p1.y);
                        let pos2 = canvas_rect.min + Vec2::new(p2.x, p2.y);
                        painter.line_segment([pos1, pos2], stroke);
                    }
                    return;
                }

                // 4) Generar la lista de puntos complejos z que vamos a
                //    transformar. Cada target emite un Vec<Complex64> en
                //    orden (puntos densos para curvas, vértices para
                //    polígonos, grid para Function, etc.).
                let z_samples: Vec<Complex64> = match target {
                    GeoObject::Polygon(poly) => poly
                        .vertices
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            let x = self
                                .document
                                .resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                            let y = self
                                .document
                                .resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                            Complex64::new(x, y)
                        })
                        .collect(),
                    GeoObject::Point(point) => vec![Complex64::new(
                        self.document.resolve_expr(&point.x_expr, point.position.x),
                        self.document.resolve_expr(&point.y_expr, point.position.y),
                    )],
                    GeoObject::Line(line) => {
                        let start = Point2::new(
                            self.document.resolve_expr(&line.start_x_expr, line.start.x),
                            self.document.resolve_expr(&line.start_y_expr, line.start.y),
                        );
                        let end = Point2::new(
                            self.document.resolve_expr(&line.end_x_expr, line.end.x),
                            self.document.resolve_expr(&line.end_y_expr, line.end.y),
                        );
                        let steps = 50;
                        (0..=steps)
                            .map(|i| {
                                let t = i as f64 / steps as f64;
                                Complex64::new(
                                    start.x + t * (end.x - start.x),
                                    start.y + t * (end.y - start.y),
                                )
                            })
                            .collect()
                    }
                    GeoObject::Circle(circle) => {
                        let radius = self
                            .document
                            .resolve_expr(&circle.radius_expr, circle.radius);
                        if !circle.center.x.is_finite()
                            || !circle.center.y.is_finite()
                            || !radius.is_finite()
                            || radius <= 0.0
                        {
                            Vec::new()
                        } else {
                            let samples = 128;
                            (0..=samples)
                                .map(|index| {
                                    let angle =
                                        index as f64 * std::f64::consts::TAU / samples as f64;
                                    Complex64::new(
                                        circle.center.x + radius * angle.cos(),
                                        circle.center.y + radius * angle.sin(),
                                    )
                                })
                                .collect()
                        }
                    }
                    GeoObject::Pencil(pencil) => pencil
                        .points
                        .iter()
                        .map(|point| Complex64::new(point.x, point.y))
                        .collect(),
                    GeoObject::Function(f) => {
                        let n = 400;
                        (0..=n)
                            .map(|i| {
                                let t = i as f64 / n as f64;
                                let x = xmin + t * (xmax - xmin);
                                let y = grafito_geometry::expr::eval_function_with_vars(
                                    &f.expr,
                                    x,
                                    &self.document.variables,
                                )
                                .unwrap_or(f64::NAN);
                                Complex64::new(x, y)
                            })
                            .collect()
                    }
                    GeoObject::ImplicitCurve(ic) => {
                        if !implicit_curve_cache_matches_request(
                            ic,
                            (xmin, xmax, ymin, ymax),
                            implicit_curve_grid_size(canvas_rect, self.document.render_quality),
                            &self.document.variables,
                            self.document.render_quality,
                        ) {
                            Vec::new()
                        } else {
                            // Para implícitas usamos el helper de muestreo del
                            // crate core (marching squares + polilínea cerrada).
                            //
                            // Filtro de segmentos degenerados: marching squares
                            // puede emitir segmentos muy cortos (de longitud menor
                            // a 1e-3) en celdas donde la interpolación es
                            // inestable.
                            let mut samples = Vec::new();
                            for (level, segments) in
                                self.document.implicit_curve_segments(cm.target)
                            {
                                for (a, b) in segments {
                                    let len = (a.x - b.x).hypot(a.y - b.y);
                                    if len < 1e-3 {
                                        continue;
                                    }
                                    let n = 16;
                                    for i in 0..=n {
                                        let t = i as f64 / n as f64;
                                        samples.push(Complex64::new(
                                            a.x + t * (b.x - a.x),
                                            a.y + t * (b.y - a.y),
                                        ));
                                    }
                                    let _ = level;
                                }
                            }
                            samples
                        }
                    }
                    GeoObject::ParametricCurve2D(c) => {
                        let n = 200;
                        (0..=n)
                            .map(|i| {
                                let t = c.t_min + (i as f64 / n as f64) * (c.t_max - c.t_min);
                                let x = eval_batch_1d(
                                    &c.expr_x,
                                    "t",
                                    std::iter::once(t),
                                    &self.document.variables,
                                )
                                .ok()
                                .and_then(|mut v| v.pop().flatten())
                                .unwrap_or(f64::NAN);
                                let y = eval_batch_1d(
                                    &c.expr_y,
                                    "t",
                                    std::iter::once(t),
                                    &self.document.variables,
                                )
                                .ok()
                                .and_then(|mut v| v.pop().flatten())
                                .unwrap_or(f64::NAN);
                                Complex64::new(x, y)
                            })
                            .collect()
                    }
                    GeoObject::PolarCurve(c) => {
                        let n = 200;
                        (0..=n)
                            .map(|i| {
                                let t = c.t_min + (i as f64 / n as f64) * (c.t_max - c.t_min);
                                let r = eval_batch_1d(
                                    &c.expr_r,
                                    "t",
                                    std::iter::once(t),
                                    &self.document.variables,
                                )
                                .ok()
                                .and_then(|mut v| v.pop().flatten())
                                .unwrap_or(f64::NAN);
                                Complex64::new(r * t.cos(), r * t.sin())
                            })
                            .collect()
                    }
                    GeoObject::Ellipse(el) => {
                        let n = 128;
                        let cos_a = el.angle.cos();
                        let sin_a = el.angle.sin();
                        (0..=n)
                            .map(|i| {
                                let t = i as f64 * std::f64::consts::TAU / n as f64;
                                Complex64::new(
                                    el.center.x + el.rx * t.cos() * cos_a - el.ry * t.sin() * sin_a,
                                    el.center.y + el.rx * t.cos() * sin_a + el.ry * t.sin() * cos_a,
                                )
                            })
                            .collect()
                    }
                    GeoObject::Parabola(pb) if pb.p.is_finite() && pb.p.abs() >= 1e-12 => {
                        let n = 128;
                        let range = (20.0 / view.scale).clamp(0.1, 500.0);
                        let cos_a = pb.angle.cos();
                        let sin_a = pb.angle.sin();
                        (0..=n)
                            .map(|i| {
                                let t = -range + 2.0 * range * i as f64 / n as f64;
                                Complex64::new(
                                    pb.vertex.x + t * cos_a - (t * t / (4.0 * pb.p)) * sin_a,
                                    pb.vertex.y + t * sin_a + (t * t / (4.0 * pb.p)) * cos_a,
                                )
                            })
                            .collect()
                    }
                    GeoObject::Hyperbola(hb)
                        if hb.a.is_finite() && hb.b.is_finite() && hb.a > 0.0 && hb.b > 0.0 =>
                    {
                        let n = 64;
                        let epsilon = 0.05;
                        let cos_a = hb.angle.cos();
                        let sin_a = hb.angle.sin();
                        let mut samples = Vec::with_capacity((n + 2) * 2);
                        for branch in 0..2 {
                            let start = -std::f64::consts::FRAC_PI_2
                                + epsilon
                                + branch as f64 * std::f64::consts::PI;
                            let end = std::f64::consts::FRAC_PI_2 - epsilon
                                + branch as f64 * std::f64::consts::PI;
                            for i in 0..=n {
                                let t = start + (end - start) * i as f64 / n as f64;
                                let (local_x, local_y) = if hb.horizontal {
                                    (hb.a / t.cos(), hb.b * t.tan())
                                } else {
                                    (hb.b * t.tan(), hb.a / t.cos())
                                };
                                samples.push(Complex64::new(
                                    hb.center.x + local_x * cos_a - local_y * sin_a,
                                    hb.center.y + local_x * sin_a + local_y * cos_a,
                                ));
                            }
                            samples.push(Complex64::new(f64::NAN, f64::NAN));
                        }
                        samples
                    }
                    GeoObject::RegressionLine(rl)
                        if rl.x_min.is_finite()
                            && rl.x_max.is_finite()
                            && rl.slope.is_finite()
                            && rl.intercept.is_finite()
                            && rl.x_min < rl.x_max =>
                    {
                        vec![
                            Complex64::new(rl.x_min, rl.slope * rl.x_min + rl.intercept),
                            Complex64::new(rl.x_max, rl.slope * rl.x_max + rl.intercept),
                        ]
                    }
                    GeoObject::VectorField2D(vf) => {
                        let grid_size = vf.density.clamp(5, 80);
                        let cell_width = (xmax - xmin).abs() / grid_size as f64;
                        let cell_height = (ymax - ymin).abs() / grid_size as f64;
                        let arrow_length = cell_width.min(cell_height) * 0.8;
                        if !arrow_length.is_finite() || arrow_length <= 0.0 {
                            return;
                        }
                        let samples = vector_field_sampling::samples_or_compute(
                            vf,
                            (xmin, xmax, ymin, ymax),
                            grid_size,
                            &self.document.variables,
                        );
                        let mut points = Vec::with_capacity(samples.len() * 2);
                        for (x, y, u, v) in samples.iter() {
                            if !x.is_finite() || !y.is_finite() || !u.is_finite() || !v.is_finite()
                            {
                                continue;
                            }
                            let magnitude = (*u).hypot(*v);
                            if !magnitude.is_finite() || magnitude <= 1e-10 {
                                continue;
                            }
                            points.push(Complex64::new(*x, *y));
                            points.push(Complex64::new(
                                *x + *u / magnitude * arrow_length,
                                *y + *v / magnitude * arrow_length,
                            ));
                        }
                        points
                    }
                    _ => return,
                };

                if z_samples.is_empty() {
                    return;
                }

                // 5) Batch-eval de la expresión.
                //
                //    Camino rápido: si la expresión fue reconocida por
                //    `ConformalMap::from_expr_string` y cacheada en
                //    `cm.conformal_cache`, evaluamos con la fórmula
                //    algebraica cerrada. Esto evita el bug del parser
                //    que tokenizaba `1/z` como `1*z`.
                //
                //    Camino lento: `eval_complex_batch` parsea el AST
                //    una vez y evalúa cada punto.
                let results: Vec<Option<Complex64>> = if let Some(map) = conformal_map {
                    z_samples.iter().map(|z| map.apply(*z)).collect()
                } else {
                    // PERF (H10): reutiliza el AST ya parseado/cacheado en lugar
                    // de `eval_complex_batch`, que re-parsea la expresión en cada
                    // frame. `get_mut` evita `String` alloc por punto muestreado.
                    let mut cmap: HashMap<String, Complex64> = self
                        .document
                        .variables
                        .iter()
                        .map(|(name, val)| (name.clone(), Complex64::new(*val, 0.0)))
                        .collect();
                    let base_symbol = self.document.complex_base_symbol.as_str();
                    cmap.insert(base_symbol.to_string(), Complex64::new(0.0, 0.0));
                    z_samples
                        .iter()
                        .map(|z| {
                            if let Some(slot) = cmap.get_mut(base_symbol) {
                                *slot = *z;
                            }
                            match parsed_expr.eval(&cmap) {
                                Ok(val) if val.re.is_finite() && val.im.is_finite() => Some(val),
                                _ => None,
                            }
                        })
                        .collect()
                };

                // 5b) **Relleno de área** para el caso del `ImplicitCurve` como
                //     target. El render de líneas arriba solo dibuja el
                //     contorno; para que el usuario vea un área rellena
                //     cuando hace `ComplexMapping[1/z, I]`, escaneamos
                //     el plano de output con el conformal map inverso y
                //     evaluamos la curva original en cada celda.
                if let GeoObject::ImplicitCurve(ic) = target {
                    if let (Some(fill_color), Some(map)) = (ic.fill_color, conformal_map) {
                        self.draw_complex_mapping_fill(
                            &painter,
                            canvas_rect,
                            view,
                            ic,
                            cm.id,
                            map,
                            fill_color,
                        );
                    }
                }
                // Re-construimos un vec paralelo para evitar manejar Option<Point2>:
                // usamos Point2 con NaN para representar "no-finito".
                let mut transformed: Vec<(Point2, bool)> = Vec::with_capacity(results.len());
                for (z_in, w_out) in z_samples.iter().zip(results.iter()) {
                    match w_out {
                        Some(w) if w.re.is_finite() && w.im.is_finite() => {
                            transformed.push((
                                grafito_render::interpolate_complex_mapping_point(
                                    Point2::new(z_in.re, z_in.im),
                                    Point2::new(w.re, w.im),
                                    homotopy_factor,
                                ),
                                true,
                            ));
                        }
                        _ => {
                            // Guardamos el z original (no el resultado) como
                            // "último punto conocido antes de la singularidad"
                            // para dibujar la asíntota desde donde el trazo
                            // sale del plano visible.
                            let _ = z_in;
                            transformed.push((Point2::new(f64::NAN, f64::NAN), false));
                        }
                    }
                }

                if let GeoObject::Point(point) = target {
                    let source = Point2::new(
                        self.document.resolve_expr(&point.x_expr, point.position.x),
                        self.document.resolve_expr(&point.y_expr, point.position.y),
                    );
                    let marker = transformed
                        .first()
                        .and_then(|(point, finite)| finite.then_some(*point))
                        // A non-finite image is not drawable. Retain a source marker so the
                        // user sees the singular mapping instead of an absent object.
                        .unwrap_or(source);
                    let screen = view.world_to_screen(marker);
                    if screen.is_finite() {
                        let position = canvas_rect.min + Vec2::new(screen.x, screen.y);
                        painter.circle_filled(position, point.size.max(6.0), to_color32(cm.color));
                    }
                    return;
                }

                // 6) Render: dibujar segmentos sólidos entre puntos finitos
                //    consecutivos, y asíntotas punteadas en los huecos no
                //    finitos. La asíntota se traza desde el último punto
                //    finito en la dirección del último delta (en world), lo
                //    que aproxima la tangente de la curva justo antes de la
                //    singularidad.
                let stroke = Stroke::new(2.0, to_color32(cm.color));
                let to_screen = |world: Point2| -> Pos2 {
                    let s = view.world_to_screen(world);
                    canvas_rect.min + Vec2::new(s.x, s.y)
                };
                let to_screen_dir = |dx: f64, dy: f64| -> Vec2 {
                    let s1 = view.world_to_screen(Point2::new(0.0, 0.0));
                    let s2 = view.world_to_screen(Point2::new(dx, dy));
                    Vec2::new(s2.x - s1.x, s2.y - s1.y)
                };
                let dashed_stroke = Stroke::new(1.0, to_color32(cm.color).gamma_multiply(0.7));

                let mut prev: Option<(Point2, Pos2, Vec2)> = None;
                // (world_pos, screen_pos, screen_dir_of_tangent)

                let vector_target = matches!(target, GeoObject::VectorField2D(_));
                for (index, (world_pt, is_finite)) in transformed.iter().enumerate() {
                    if vector_target && index % 2 == 0 {
                        prev = None;
                    }
                    if *is_finite {
                        let screen_pt = to_screen(*world_pt);
                        if let Some((prev_world, prev_screen, _)) = prev {
                            // Trazo sólido.
                            painter.line_segment([prev_screen, screen_pt], stroke);
                            // Actualizar dirección tangente.
                            let dx = world_pt.x - prev_world.x;
                            let dy = world_pt.y - prev_world.y;
                            if dx.hypot(dy) > 1e-9 {
                                let dir_screen = to_screen_dir(dx, dy);
                                prev = Some((*world_pt, screen_pt, dir_screen));
                            }
                        } else {
                            // Primer punto finito después de una
                            // singularidad. Si la dirección tangente no
                            // existe aún, la dejamos en (0,1) (hacia
                            // abajo) como placeholder.
                            prev = Some((*world_pt, screen_pt, Vec2::new(0.0, 1.0)));
                        }
                    } else {
                        // Punto no finito: dibujar la asíntota desde el
                        // último punto finito en la dirección de la
                        // tangente, en pasos pequeños hasta que aparezca
                        // un nuevo punto finito o agotemos N pasos.
                        if let Some((_, prev_screen, dir_screen)) = prev {
                            if dir_screen.length() > 0.5 {
                                // Normalizar y dibujar ~30 píxeles de
                                // asíntota en esa dirección.
                                let step_px = 6.0_f32;
                                let n_dashes = 6;
                                let dir_norm = dir_screen.normalized();
                                let mut last = prev_screen;
                                for i in 1..=n_dashes {
                                    let next = Pos2::new(
                                        last.x + dir_norm.x * step_px,
                                        last.y + dir_norm.y * step_px,
                                    );
                                    // Patrón: dibujar 1, saltar 1, dibujar
                                    // 1, ... para que parezca punteado.
                                    if i % 2 == 1 {
                                        painter.line_segment([last, next], dashed_stroke);
                                    }
                                    last = next;
                                }
                            } else {
                                // Sin dirección: marcar con una X roja
                                // en el último punto finito conocido.
                                painter.line_segment(
                                    [
                                        Pos2::new(prev_screen.x - 6.0, prev_screen.y - 6.0),
                                        Pos2::new(prev_screen.x + 6.0, prev_screen.y + 6.0),
                                    ],
                                    Stroke::new(1.5, Color32::from_rgb(220, 30, 30)),
                                );
                                painter.line_segment(
                                    [
                                        Pos2::new(prev_screen.x - 6.0, prev_screen.y + 6.0),
                                        Pos2::new(prev_screen.x + 6.0, prev_screen.y - 6.0),
                                    ],
                                    Stroke::new(1.5, Color32::from_rgb(220, 30, 30)),
                                );
                            }
                        }
                        prev = None;
                    }
                }
            }
            GeoObject::ComplexIntegral(ci) => {
                if let Some(target) = self.document.get_object(ci.target) {
                    self.draw_object_styled(
                        painter,
                        canvas_rect,
                        target,
                        Some(StyleOverride {
                            color: Some(ci.color),
                            width_scale: Some(1.4),
                            ..Default::default()
                        }),
                        overlay_only,
                    );
                }
            }
            GeoObject::Transformed(t) => {
                if overlay_only {
                    return;
                }
                let (vertices, indices) =
                    grafito_render::Renderer::build_transformed_geometry_static(
                        &self.document,
                        t,
                        view,
                        self.dark_mode,
                    );
                paint_render_geometry(painter, canvas_rect, &vertices, &indices, style);
            }
            _ => {}
        }
    }
}
