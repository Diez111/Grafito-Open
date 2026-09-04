#![allow(unknown_lints, float_literal_f32_fallback)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(deprecated)]
//! Grafito Render — Renderizador 2D/3D acelerado por GPU con wgpu.
//!
//! Este crate convierte un [`Document`] en geometría
//! lista para GPU, gestiona los pipelines de cómputo (`function_compute`,
//! `implicit_compute`, `parametric_compute`) y proporciona utilidades de
//! iluminación y vértices.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_render::Vertex;
//! use grafito_geometry::Color;
//!
//! let v = Vertex::new(0.0, 0.0, Color::BLACK);
//! assert_eq!(v.position, [0.0, 0.0, 0.0]);
//! ```
#![allow(
    clippy::too_many_arguments,
    clippy::needless_range_loop,
    clippy::manual_clamp
)]

use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_core::{
    ComplexGridObj, Document, GeoObject, ObjectId, PhasePortraitObj, RelationOperator,
    RenderQuality, TransformedObj, VectorField3DObj,
};
use grafito_geometry::{Camera3D, Color, Point2, Point3D, Tetrahedron3D, ViewTransform};
use lyon::{
    math::point,
    path::Path,
    tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers},
};
use wgpu::util::DeviceExt;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

pub mod complex_compute;
pub mod depth_3d;
pub mod domain_coloring_compute;
pub mod fill_compute;
pub mod function_compute;
pub mod gpu_timing;
pub mod implicit_compute;
pub mod parametric_compute;
pub mod vector_compute;

#[cfg(test)]
mod tests;

type TransformedCacheMap = HashMap<u64, (Vec<Vertex>, Vec<u32>)>;
thread_local! {
    #[allow(clippy::type_complexity)]
    static FILL_TESS: RefCell<FillTessellator> = RefCell::new(FillTessellator::new());
    #[allow(clippy::type_complexity)]
    static TRANSFORMED_CACHE: RefCell<TransformedCacheMap> = RefCell::new(HashMap::new());
}
const TRANSFORMED_CACHE_CAP: usize = 64;

/// Timeout for synchronous GPU readbacks. The caller already bounds this to
/// one attempt per frame via `MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE` in
/// canvas.rs, so a bounded poll here only guards against a hung GPU freezing
/// the prepare thread indefinitely.
const SYNC_GPU_READBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// Bounded synchronous readback: wraps `map_async` + `poll` in
/// `pollster::block_on` with a timeout. Uses `wgpu::Maintain::Poll` (non
/// blocking) in a loop instead of `Maintain::Wait`, so a stuck GPU cannot
/// block the prepare thread forever. Returns `true` if the buffer was mapped
/// before the deadline.
///
/// TODO P1: mover a `spawn_blocking` — el readback síncrono sigue bloqueando
/// el hilo de prepare (acotado a 1 intento por frame via
/// `MAX_SYNC_GPU_COMPUTE_ATTEMPTS_PER_PREPARE` en canvas.rs).
pub(crate) fn sync_readback_with_timeout(
    device: &wgpu::Device,
    map_ok: &std::sync::atomic::AtomicBool,
) -> bool {
    pollster::block_on(async {
        let deadline = std::time::Instant::now() + SYNC_GPU_READBACK_TIMEOUT;
        while !map_ok.load(std::sync::atomic::Ordering::SeqCst) {
            if std::time::Instant::now() >= deadline {
                log::warn!(
                    "GPU readback timed out after {:?}; falling back to CPU (1 intento por frame)",
                    SYNC_GPU_READBACK_TIMEOUT
                );
                return false;
            }
            device.poll(wgpu::Maintain::Poll);
            std::thread::yield_now();
        }
        true
    })
}

/// Tolerancia de `lyon` escalada inversamente a `view.scale` para mantener
/// calidad visual constante: zoom alto → tolerancia fina, zoom bajo → gruesa.
/// Base 0.1 a `scale = 50` (default), clamp para evitar extremos.
fn lyon_tolerance_for_view_scale(scale: f64) -> f32 {
    let base_scale = 50.0;
    let base_tol = 0.1_f32;
    let ratio = (base_scale / scale.max(1e-6)) as f32;
    (base_tol * ratio.clamp(0.25, 4.0)).clamp(0.01, 0.5)
}

fn transformed_cache_key(
    document: &Document,
    transformed: &TransformedObj,
    view: &ViewTransform,
    dark_mode: bool,
    depth: usize,
) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    document.version.hash(&mut hasher);
    view.scale.to_bits().hash(&mut hasher);
    view.offset.x.to_bits().hash(&mut hasher);
    view.offset.y.to_bits().hash(&mut hasher);
    view.screen_size.x.to_bits().hash(&mut hasher);
    view.screen_size.y.to_bits().hash(&mut hasher);
    view.x_log.hash(&mut hasher);
    view.y_log.hash(&mut hasher);
    dark_mode.hash(&mut hasher);
    depth.hash(&mut hasher);
    transformed.complex_expr.hash(&mut hasher);
    transformed.inner.id().hash(&mut hasher);
    format!("{:?}", transformed.inner).hash(&mut hasher);
    hasher.finish()
}

fn sample_environment(
    variables: &std::collections::HashMap<String, f64>,
    local_names: &[&str],
) -> Vec<(String, f64)> {
    let mut environment: Vec<_> = variables
        .iter()
        .filter(|(name, _)| !local_names.contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), *value))
        .collect();
    environment.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    environment
}

fn evaluate_prepared_2d(
    prepared: Option<&grafito_geometry::ast::Expr>,
    expression: &str,
    environment: &[(String, f64)],
    x: f64,
    y: f64,
) -> Option<f64> {
    prepared
        .map(|ast| ast.eval_2d("x", x, "y", y))
        .filter(|value| value.is_finite())
        .or_else(|| {
            grafito_geometry::expr::evaluate(expression, environment)
                .ok()
                .filter(|value| value.is_finite())
        })
}

fn evaluate_prepared_3d(
    prepared: Option<&grafito_geometry::ast::Expr>,
    expression: &str,
    environment: &[(String, f64)],
    x: f64,
    y: f64,
    z: f64,
) -> Option<f64> {
    prepared
        .map(|ast| ast.eval_3d("x", x, "y", y, "z", z))
        .filter(|value| value.is_finite())
        .or_else(|| {
            grafito_geometry::expr::evaluate(expression, environment)
                .ok()
                .filter(|value| value.is_finite())
        })
}

/// Muestrea un retrato de fase con las variables del documento y devuelve
/// segmentos en coordenadas matemáticas `(x, y)`.
pub fn sample_phase_portrait(
    portrait: &PhasePortraitObj,
    variables: &std::collections::HashMap<String, f64>,
) -> Vec<(Point2, Point2)> {
    if ![
        portrait.x_min,
        portrait.x_max,
        portrait.y_min,
        portrait.y_max,
    ]
    .into_iter()
    .all(f64::is_finite)
        || portrait.x_min >= portrait.x_max
        || portrait.y_min >= portrait.y_max
    {
        return Vec::new();
    }

    let density = portrait.density.clamp(5, 40);
    let dx = (portrait.x_max - portrait.x_min) / density as f64;
    let dy = (portrait.y_max - portrait.y_min) / density as f64;
    let prepared_dx =
        grafito_geometry::expr::prepare_function_ast(&portrait.expr_dx, variables, &["x", "y"])
            .ok();
    let prepared_dy =
        grafito_geometry::expr::prepare_function_ast(&portrait.expr_dy, variables, &["x", "y"])
            .ok();
    let mut environment = sample_environment(variables, &["x", "y"]);
    let x_index = environment.len();
    environment.push(("x".to_string(), 0.0));
    let y_index = environment.len();
    environment.push(("y".to_string(), 0.0));
    let mut segments = Vec::with_capacity((density + 1) * (density + 1));

    for i in 0..=density {
        let x = portrait.x_min + i as f64 * dx;
        environment[x_index].1 = x;
        for j in 0..=density {
            let y = portrait.y_min + j as f64 * dy;
            environment[y_index].1 = y;
            let (Some(u), Some(v)) = (
                evaluate_prepared_2d(prepared_dx.as_ref(), &portrait.expr_dx, &environment, x, y),
                evaluate_prepared_2d(prepared_dy.as_ref(), &portrait.expr_dy, &environment, x, y),
            ) else {
                continue;
            };
            let magnitude = u.hypot(v);
            if !magnitude.is_finite() || magnitude <= 0.001 {
                continue;
            }
            segments.push((
                Point2::new(x, y),
                Point2::new(x + u / magnitude * 0.5, y + v / magnitude * 0.5),
            ));
        }
    }
    segments
}

pub(crate) fn vector_field_3d_sample_count(field: &VectorField3DObj) -> Option<usize> {
    field.density.clamp(3, 15).checked_add(1)?.checked_pow(3)
}

/// Muestrea un campo vectorial 3D con un único contrato CPU/GPU. Los extremos
/// permanecen en coordenadas de documento `(x, y, z)` y las flechas ocupan el
/// 40 % de la menor celda del dominio.
pub fn sample_vector_field_3d(
    field: &VectorField3DObj,
    variables: &std::collections::HashMap<String, f64>,
) -> Vec<(Point3D, Point3D)> {
    if ![
        field.x_min,
        field.x_max,
        field.y_min,
        field.y_max,
        field.z_min,
        field.z_max,
    ]
    .into_iter()
    .all(f64::is_finite)
        || field.x_min >= field.x_max
        || field.y_min >= field.y_max
        || field.z_min >= field.z_max
    {
        return Vec::new();
    }

    let density = field.density.clamp(3, 15);
    let dx = (field.x_max - field.x_min) / density as f64;
    let dy = (field.y_max - field.y_min) / density as f64;
    let dz = (field.z_max - field.z_min) / density as f64;
    let arrow_scale = dx.abs().min(dy.abs()).min(dz.abs()) * 0.4;
    if !arrow_scale.is_finite() || arrow_scale <= 0.0 {
        return Vec::new();
    }

    let prepared_u =
        grafito_geometry::expr::prepare_function_ast(&field.expr_u, variables, &["x", "y", "z"])
            .ok();
    let prepared_v =
        grafito_geometry::expr::prepare_function_ast(&field.expr_v, variables, &["x", "y", "z"])
            .ok();
    let prepared_w =
        grafito_geometry::expr::prepare_function_ast(&field.expr_w, variables, &["x", "y", "z"])
            .ok();
    let mut environment = sample_environment(variables, &["x", "y", "z"]);
    let x_index = environment.len();
    environment.push(("x".to_string(), 0.0));
    let y_index = environment.len();
    environment.push(("y".to_string(), 0.0));
    let z_index = environment.len();
    environment.push(("z".to_string(), 0.0));
    let mut segments = Vec::with_capacity(vector_field_3d_sample_count(field).unwrap_or(0));

    for i in 0..=density {
        let x = field.x_min + i as f64 * dx;
        environment[x_index].1 = x;
        for j in 0..=density {
            let y = field.y_min + j as f64 * dy;
            environment[y_index].1 = y;
            for k in 0..=density {
                let z = field.z_min + k as f64 * dz;
                environment[z_index].1 = z;
                let (Some(u), Some(v), Some(w)) = (
                    evaluate_prepared_3d(prepared_u.as_ref(), &field.expr_u, &environment, x, y, z),
                    evaluate_prepared_3d(prepared_v.as_ref(), &field.expr_v, &environment, x, y, z),
                    evaluate_prepared_3d(prepared_w.as_ref(), &field.expr_w, &environment, x, y, z),
                ) else {
                    continue;
                };
                let magnitude = u.hypot(v).hypot(w);
                if !magnitude.is_finite() || magnitude <= 0.001 {
                    continue;
                }
                let start = Point3D::new(x, y, z);
                let end = Point3D::new(
                    x + u / magnitude * arrow_scale,
                    y + v / magnitude * arrow_scale,
                    z + w / magnitude * arrow_scale,
                );
                if end.x.is_finite() && end.y.is_finite() && end.z.is_finite() {
                    segments.push((start, end));
                }
            }
        }
    }
    segments
}

/// Transforma segmentos independientes por un mapa conforme sin crear líneas
/// puente entre segmentos consecutivos del marching squares.
pub fn transform_complex_mapping_segments(
    map: ConformalMap,
    segments: &[(Point2, Point2)],
    subdivisions: usize,
    t_val: f64,
) -> Vec<(Point2, Point2)> {
    let subdivisions = subdivisions.max(1);
    let mut strokes = Vec::new();
    for (a, b) in segments {
        let mut prev: Option<Point2> = None;
        for i in 0..=subdivisions {
            let t = i as f64 / subdivisions as f64;
            let z_orig = num_complex::Complex64::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let z_mapped = map.apply(z_orig);
            let current = match z_mapped {
                Some(w) if w.re.is_finite() && w.im.is_finite() => {
                    Some(interpolate_complex_mapping_point(
                        Point2::new(z_orig.re, z_orig.im),
                        Point2::new(w.re, w.im),
                        t_val,
                    ))
                }
                _ => None,
            };
            if let (Some(prev), Some(current)) = (prev, current) {
                strokes.push((prev, current));
            }
            prev = current;
        }
    }
    strokes
}

/// Interpola un punto con el mismo factor usado por los caminos CPU y GPU.
pub fn interpolate_complex_mapping_point(source: Point2, mapped: Point2, factor: f64) -> Point2 {
    Point2::new(
        source.x + (mapped.x - source.x) * factor,
        source.y + (mapped.y - source.y) * factor,
    )
}

/// Cálculo simple de iluminación para objetos 3D
pub fn calculate_lighting(base_color: Color, normal: glam::Vec3, light_dir: glam::Vec3) -> Color {
    let ambient = 0.45;
    let diffuse = 0.65;

    let normal = normal.normalize();
    let light_dir = light_dir.normalize();

    let dot = normal.dot(light_dir).max(0.0);
    let intensity = ambient + diffuse * dot;

    Color::new(
        (base_color.r * intensity).min(1.0),
        (base_color.g * intensity).min(1.0),
        (base_color.b * intensity).min(1.0),
        base_color.a,
    )
}

/// Un vértice simple con posición y color.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

pub use depth_3d::{Vertex3D, WorldMesh};

/// Per-canvas render targets owned by the 3D callback. The texture is sampled
/// during callback paint because egui-wgpu does not expose a depth attachment
/// for its active render pass.
#[allow(dead_code)] // TODO P2: color_texture/depth_texture kept alive for GPU lifetime, reads via views
pub struct DepthRenderTarget {
    color_texture: wgpu::Texture,
    pub color_view: wgpu::TextureView,
    msaa_color_texture: Option<wgpu::Texture>,
    pub render_color_view: wgpu::TextureView,
    depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
    composite_bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    sample_count: u32,
}

impl DepthRenderTarget {
    pub fn matches_size(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }

    pub fn resolve_target(&self) -> Option<&wgpu::TextureView> {
        self.msaa_color_texture.as_ref().map(|_| &self.color_view)
    }

    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }
}

pub(crate) const MAX_GEOMETRY_VERTICES: usize = 1_000_000;
const MAX_GEOMETRY_INDICES: usize = 3_000_000;
const MAX_POLYGON_VERTICES: usize = 65_536;
const MAX_SCREEN_COORDINATE: f32 = 1_000_000.0;

/// Orden visual estable de la escena 2D. Los objetos de una misma capa se
/// ordenan por [`ObjectId`] para que los límites de geometría sean reproducibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SceneLayer2D {
    Background,
    Region,
    Curve,
    Marker,
    Annotation,
}

pub fn scene_layer_2d(object: &GeoObject) -> SceneLayer2D {
    match object {
        GeoObject::Fractal2D(_) | GeoObject::ComplexGrid(_) => SceneLayer2D::Background,
        GeoObject::Circle(_)
        | GeoObject::Polygon(_)
        | GeoObject::Ellipse(_)
        | GeoObject::Histogram(_)
        | GeoObject::BoxPlot(_)
        | GeoObject::Sector(_) => SceneLayer2D::Region,
        GeoObject::Point(_) | GeoObject::ScatterPlot(_) => SceneLayer2D::Marker,
        GeoObject::Text(_) | GeoObject::ComplexIntegral(_) => SceneLayer2D::Annotation,
        _ => SceneLayer2D::Curve,
    }
}

/// Objetos 2D visibles en el único orden usado para pintar y admitir geometría.
pub fn ordered_visible_2d_objects(document: &Document) -> Vec<(ObjectId, &GeoObject)> {
    let mut objects: Vec<_> = document
        .objects_iter()
        .filter(|(_, object)| object.is_visible() && !object.is_3d())
        .map(|(id, object)| (*id, object))
        .collect();
    objects.sort_unstable_by_key(|(id, object)| (scene_layer_2d(object), *id));
    objects
}

/// Indica si el callback 2D emite la geometría base de este objeto.
pub fn gpu_2d_base_owns(document: &Document, object: &GeoObject) -> bool {
    match object {
        GeoObject::Function(_)
        | GeoObject::Fractal2D(_)
        | GeoObject::ParametricCurve2D(_)
        | GeoObject::PolarCurve(_)
        | GeoObject::PhasePortrait(_)
        | GeoObject::ComplexGrid(_)
        | GeoObject::Transformed(_) => true,
        GeoObject::ComplexMapping(mapping) => {
            mapping
                .conformal_map(document.complex_base_symbol.as_str())
                .is_some()
                && document.get_object(mapping.target).is_some_and(|target| {
                    matches!(
                        target,
                        GeoObject::Point(_)
                            | GeoObject::Line(_)
                            | GeoObject::Circle(_)
                            | GeoObject::Polygon(_)
                            | GeoObject::Pencil(_)
                            | GeoObject::Function(_)
                            | GeoObject::ImplicitCurve(_)
                            | GeoObject::ParametricCurve2D(_)
                            | GeoObject::PolarCurve(_)
                            | GeoObject::Ellipse(_)
                            | GeoObject::Parabola(_)
                            | GeoObject::Hyperbola(_)
                            | GeoObject::RegressionLine(_)
                            | GeoObject::VectorField2D(_)
                    )
                })
        }
        _ => false,
    }
}

fn apply_complex_transform_cpu(
    vertices: &mut [Vertex],
    view: &ViewTransform,
    expression: &str,
    variables: &std::collections::HashMap<String, f64>,
    complex_symbol: &str,
) -> bool {
    let Ok(ast) = grafito_complex::math::complex_expr::parse(expression) else {
        return false;
    };
    let mut environment: std::collections::HashMap<_, _> = variables
        .iter()
        .map(|(name, value)| (name.clone(), num_complex::Complex64::new(*value, 0.0)))
        .collect();

    for vertex in vertices {
        let screen = glam::Vec2::new(vertex.position[0], vertex.position[1]);
        let world = view.screen_to_world(screen);
        let z = num_complex::Complex64::new(world.x, world.y);
        environment.insert("x".to_string(), num_complex::Complex64::new(world.x, 0.0));
        environment.insert("y".to_string(), num_complex::Complex64::new(world.y, 0.0));
        environment.insert("z".to_string(), z);
        environment.insert(complex_symbol.to_string(), z);

        let transformed = ast
            .eval(&environment)
            .ok()
            .filter(|value| value.re.is_finite() && value.im.is_finite());
        let screen = transformed
            .and_then(|value| bounded_screen_point(view, Point2::new(value.re, value.im)));
        vertex.position = screen.map_or([f32::NAN, f32::NAN, 0.0], |point| [point.x, point.y, 0.0]);
    }
    true
}

/// Factor común de interpolación entre la geometría fuente (0) y su imagen (1).
pub fn complex_mapping_homotopy_factor(animate: bool, speed: f32, elapsed: f64) -> f64 {
    if !animate {
        return 1.0;
    }
    let phase = elapsed * speed as f64;
    if !phase.is_finite() {
        return 1.0;
    }
    0.5 + 0.5 * phase.cos()
}

fn bounded_screen_point(view: &ViewTransform, point: Point2) -> Option<glam::Vec2> {
    if !point.x.is_finite() || !point.y.is_finite() || !view.screen_size.is_finite() {
        return None;
    }

    let screen = view.world_to_screen(point);
    let margin = view.screen_size.max_element().clamp(1.0, 16_384.0);
    let max_x = (view.screen_size.x + margin).min(MAX_SCREEN_COORDINATE);
    let max_y = (view.screen_size.y + margin).min(MAX_SCREEN_COORDINATE);
    (screen.is_finite()
        && screen.x >= -margin
        && screen.x <= max_x
        && screen.y >= -margin
        && screen.y <= max_y)
        .then_some(screen)
}

fn screen_point_is_renderable(point: glam::Vec2) -> bool {
    point.is_finite()
        && point.x.abs() <= MAX_SCREEN_COORDINATE
        && point.y.abs() <= MAX_SCREEN_COORDINATE
}

fn color_is_renderable(color: Color) -> bool {
    color
        .to_array()
        .iter()
        .all(|component| component.is_finite())
}

fn vector_arrow_end(x: f64, y: f64, u: f64, v: f64, arrow_length: f64) -> Option<Point2> {
    if !x.is_finite()
        || !y.is_finite()
        || !u.is_finite()
        || !v.is_finite()
        || !arrow_length.is_finite()
        || arrow_length <= 0.0
    {
        return None;
    }

    let magnitude = u.hypot(v);
    if !magnitude.is_finite() || magnitude <= 1e-10 {
        return None;
    }

    let end = Point2::new(
        x + u / magnitude * arrow_length,
        y + v / magnitude * arrow_length,
    );
    (end.x.is_finite() && end.y.is_finite()).then_some(end)
}

pub(crate) fn can_append_geometry(
    current_vertices: usize,
    current_indices: usize,
    additional_vertices: usize,
    additional_indices: usize,
) -> bool {
    let max_indexable_vertices = usize::try_from(u32::MAX).unwrap_or(usize::MAX);
    current_vertices
        .checked_add(additional_vertices)
        .is_some_and(|count| count <= MAX_GEOMETRY_VERTICES && count <= max_indexable_vertices)
        && current_indices
            .checked_add(additional_indices)
            .is_some_and(|count| count <= MAX_GEOMETRY_INDICES)
}

fn reserve_geometry(
    vertices: &mut Vec<Vertex>,
    indices: &mut Vec<u32>,
    additional_vertices: usize,
    additional_indices: usize,
) -> Option<u32> {
    if !can_append_geometry(
        vertices.len(),
        indices.len(),
        additional_vertices,
        additional_indices,
    ) {
        return None;
    }
    vertices.try_reserve(additional_vertices).ok()?;
    indices.try_reserve(additional_indices).ok()?;
    u32::try_from(vertices.len()).ok()
}

fn fractal_geometry_fits(
    current_vertices: usize,
    current_indices: usize,
    fractal: &grafito_core::Fractal2DObj,
) -> bool {
    Renderer::fractal_geometry_requirements(fractal).is_some_and(
        |(additional_vertices, additional_indices)| {
            can_append_geometry(
                current_vertices,
                current_indices,
                additional_vertices,
                additional_indices,
            )
        },
    )
}

fn polygon_geometry_is_within_limit(vertex_count: usize) -> bool {
    (3..=MAX_POLYGON_VERTICES).contains(&vertex_count)
}

pub(crate) fn row_major_cell_coordinates(
    index: usize,
    resolution: usize,
) -> Option<(usize, usize)> {
    (resolution > 0 && index < resolution.checked_mul(resolution)?)
        .then_some((index / resolution, index % resolution))
}

impl Vertex {
    pub fn new(x: f32, y: f32, color: Color) -> Self {
        Self {
            position: [x, y, 0.0],
            color: color.to_array(),
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Renderer {
    pub pipeline: wgpu::RenderPipeline,
    pub pipeline_3d: wgpu::RenderPipeline,
    pub pipeline_3d_wire: wgpu::RenderPipeline,
    composite_3d_pipeline: wgpu::RenderPipeline,
    pub mvp_bind_group_layout: wgpu::BindGroupLayout,
    pub mvp_buffer: wgpu::Buffer,
    pub mvp_bind_group: wgpu::BindGroup,
    composite_3d_bind_group_layout: wgpu::BindGroupLayout,
    composite_3d_sampler: wgpu::Sampler,
    target_format: wgpu::TextureFormat,
    scene_3d_sample_count: u32,
    pub implicit_compute: Option<crate::implicit_compute::ImplicitComputePipeline>,
    pub function_compute: Option<crate::function_compute::FunctionComputePipeline>,
    pub parametric_compute: Option<crate::parametric_compute::ParametricComputePipeline>,
    pub vector_compute: Option<crate::vector_compute::VectorComputePipeline>,
    /// Fill-mask compute pipeline, creado lazy en el primer uso. El pipeline
    /// reserva dos buffers 4096×4096 (~128 MiB), así que permanece `None`
    /// hasta que una región implícita pida un fill GPU (operador != Eq) o un
    /// caller invoque [`Renderer::ensure_fill_compute`]. No se activa por frame.
    pub fill_compute: Option<crate::fill_compute::FillComputePipeline>,
    pub complex_compute: Option<crate::complex_compute::ComplexComputePipeline>,
    pub domain_coloring_compute:
        Option<crate::domain_coloring_compute::DomainColoringComputePipeline>,
}

#[allow(dead_code)] // TODO P2: hsv_to_rgb reservado para paleta alternativa (usado en tests de color)
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Color {
    let c = v * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h * 6.0) as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Color::new(r + m, g + m, b + m, 1.0)
}

/// Conversión HSL → RGB con doubles (igual fórmula que render_2d.rs para
/// unificar la coloración de dominio entre wgpu y egui).
fn hsl_to_rgb_f64(h: f64, s: f64, l: f64) -> Color {
    if s == 0.0 {
        return Color::new(l as f32, l as f32, l as f32, 1.0);
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
    Color::new(
        hue_to_rgb(p, q, h + 1.0 / 3.0) as f32,
        hue_to_rgb(p, q, h) as f32,
        hue_to_rgb(p, q, h - 1.0 / 3.0) as f32,
        1.0,
    )
}

fn thermal_colormap(t: f64) -> Color {
    let t = t.clamp(0.0, 1.0) as f32;
    let r = (t * 3.0 - 1.5).clamp(0.0, 1.0);
    let g = (1.5 - (t * 3.0 - 1.5).abs()).clamp(0.0, 1.0);
    let b = (1.5 - t * 3.0).clamp(0.0, 1.0);
    Color::new(r, g, b, 1.0)
}

fn complex_grid_resolution(cg: &ComplexGridObj, quality: RenderQuality) -> usize {
    let base = match cg.render_mode {
        1 => cg.density.clamp(50, 500),
        2 => cg.density.clamp(50, 400),
        _ => cg.density.clamp(1, 128),
    };
    match (quality, cg.render_mode) {
        (RenderQuality::Preview, 0) => base.min(32),
        (RenderQuality::Preview, _) => base.min(64),
        (RenderQuality::Normal, _) => base.min(200),
        (RenderQuality::High, _) => base.min(300),
    }
}

fn surface_point_visible(p: &Point3D) -> bool {
    depth_3d::point_is_renderable(*p)
}

impl Renderer {
    /// Builds world-space 3D data for the depth-enabled GPU path. Clipping is
    /// intentionally left to the perspective pipeline instead of baking it
    /// into CPU screen-space geometry.
    pub fn build_3d_world_mesh(
        document: &Document,
        camera: &Camera3D,
        screen_w: f32,
        screen_h: f32,
    ) -> WorldMesh {
        depth_3d::build_world_mesh(document, camera, screen_w, screen_h)
    }

    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grafito Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let mvp_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("MVP Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&mvp_bind_group_layout],
            push_constant_ranges: &[],
        });

        let composite_3d_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("3D Composite Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let composite_3d_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("3D Composite Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let composite_3d_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("3D Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite_3d.wgsl").into()),
        });
        let composite_3d_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("3D Composite Pipeline Layout"),
            bind_group_layouts: &[&composite_3d_bind_group_layout],
            push_constant_ranges: &[],
        });

        let multisample = if sample_count > 1 {
            wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            }
        } else {
            wgpu::MultisampleState::default()
        };
        // eframe already selected this count for the active surface format on
        // the same adapter, so the offscreen scene must use the exact count.
        let scene_3d_sample_count = sample_count.max(1);
        let scene_3d_multisample = wgpu::MultisampleState {
            count: scene_3d_sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        // The offscreen target is already premultiplied by its own alpha blend.
        // Compositing it with ordinary alpha blending would darken antialiased edges.
        let premultiplied_alpha_blending = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample,
            multiview: None,
            cache: None,
        });

        let pipeline_3d = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Opaque Depth Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex3D::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: scene_3d_multisample,
            multiview: None,
            cache: None,
        });

        let pipeline_3d_wire = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("3D Transparent and Wire Depth Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex3D::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth24Plus,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: scene_3d_multisample,
            multiview: None,
            cache: None,
        });

        let composite_3d_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("3D Offscreen Composite Pipeline"),
                layout: Some(&composite_3d_layout),
                vertex: wgpu::VertexState {
                    module: &composite_3d_shader,
                    entry_point: "vs_composite",
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &composite_3d_shader,
                    entry_point: "fs_composite",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target_format,
                        blend: Some(premultiplied_alpha_blending),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample,
                multiview: None,
                cache: None,
            });

        let mvp_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MVP Buffer"),
            contents: bytemuck::cast_slice(&[glam::Mat4::IDENTITY.to_cols_array()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let mvp_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("MVP Bind Group"),
            layout: &mvp_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: mvp_buffer.as_entire_binding(),
            }],
        });

        let limits = device.limits();
        let has_compute_storage = limits.max_storage_buffers_per_shader_stage >= 3;
        // Instrumentación de timing GPU: TIMESTAMP_QUERY es opcional y solo se
        // cablea tras la feature `profiling` (ver `gpu_timing`). Sin la feature
        // los passes mantienen `timestamp_writes: None` — cero costo en release.
        // Con la feature activa, cada pipeline crea su query set 2 slots/pass y
        // loguea el delta GPU en ns tras el readback síncrono.
        if device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            log::debug!(
                "GPU TIMESTAMP_QUERY disponible — habilitar feature `profiling` para medir compute passes"
            );
        } else {
            log::debug!(
                "GPU TIMESTAMP_QUERY no disponible — timing de compute passes deshabilitado"
            );
        }
        let implicit_compute = if has_compute_storage {
            Some(crate::implicit_compute::ImplicitComputePipeline::new(
                device, queue, 1024,
            ))
        } else {
            log::warn!("GPU compute deshabilitado");
            None
        };
        let function_compute = if has_compute_storage {
            Some(crate::function_compute::FunctionComputePipeline::new(
                device, queue, 10000,
            ))
        } else {
            None
        };
        let parametric_compute = if has_compute_storage {
            Some(crate::parametric_compute::ParametricComputePipeline::new(
                device, queue, 4000, 128,
            ))
        } else {
            None
        };
        let vector_compute = if has_compute_storage {
            Some(crate::vector_compute::VectorComputePipeline::new(
                device, queue, 128,
            ))
        } else {
            None
        };
        let complex_compute = if has_compute_storage {
            Some(crate::complex_compute::ComplexComputePipeline::new(
                device, queue,
            ))
        } else {
            None
        };
        let domain_coloring_compute = if has_compute_storage {
            Some(crate::domain_coloring_compute::DomainColoringComputePipeline::new(device, queue))
        } else {
            None
        };

        Self {
            pipeline,
            pipeline_3d,
            pipeline_3d_wire,
            composite_3d_pipeline,
            mvp_bind_group_layout,
            mvp_buffer,
            mvp_bind_group,
            composite_3d_bind_group_layout,
            composite_3d_sampler,
            target_format,
            scene_3d_sample_count,
            implicit_compute,
            function_compute,
            parametric_compute,
            vector_compute,
            // Fill masks are currently rasterized by the CPU path. Keeping the
            // optional pipeline empty avoids reserving two 4096x4096 buffers
            // (128 MiB) for a feature with no caller. Se crea lazy via
            // `ensure_fill_compute` cuando un documento pide fill (op != Eq).
            fill_compute: None,
            complex_compute,
            domain_coloring_compute,
        }
    }

    pub fn update_mvp(&self, queue: &wgpu::Queue, mvp: glam::Mat4) {
        queue.write_buffer(
            &self.mvp_buffer,
            0,
            bytemuck::cast_slice(&mvp.to_cols_array()),
        );
    }

    /// Crea lazy el pipeline de fill compute en el primer uso. El pipeline
    /// reserva dos buffers 4096×4096 (~128 MiB), así que solo se asigna cuando
    /// una región implícita pide un fill GPU o un caller lo solicita
    /// explícitamente. No se invoca por frame.
    pub fn ensure_fill_compute(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> &crate::fill_compute::FillComputePipeline {
        self.fill_compute.get_or_insert_with(|| {
            log::info!("Lazy init fill compute pipeline (128 MiB GPU buffers)");
            crate::fill_compute::FillComputePipeline::new(device, queue)
        })
    }

    /// Crea lazy el pipeline de fill compute SOLO si `document` contiene una
    /// curva implícita rellenable (operador != `Eq`). Devuelve `None` si el
    /// documento no necesita fill, dejando `self.fill_compute` en `None` y
    /// ahorrando los ~128 MiB de buffers 4096×4096. No se invoca por frame.
    pub fn ensure_fill_compute_for_document(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        document: &Document,
    ) -> Option<&crate::fill_compute::FillComputePipeline> {
        if !Self::document_needs_fill_compute(document) {
            return None;
        }
        Some(self.ensure_fill_compute(device, queue))
    }

    /// Devuelve `true` si el documento contiene una curva implícita con
    /// operador rellenable (`Less`/`LessEq`/`Greater`/`GreaterEq`). `Eq` es
    /// solo contorno y nunca necesita el pipeline de fill.
    pub fn document_needs_fill_compute(document: &Document) -> bool {
        document.objects_iter().any(|(_, object)| {
            matches!(
                object,
                GeoObject::ImplicitCurve(curve) if curve.operator != RelationOperator::Eq
            )
        })
    }

    /// Allocates a multisampled 3D color/depth target plus a resolved texture
    /// that can be sampled while compositing into egui's color-only pass.
    pub fn create_depth_render_target(
        &self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> DepthRenderTarget {
        let width = width.max(1);
        let height = height.max(1);
        let color_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3D Offscreen Color Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (msaa_color_texture, render_color_view) = if self.scene_3d_sample_count > 1 {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("3D Offscreen MSAA Color Target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: self.scene_3d_sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: self.target_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(texture), view)
        } else {
            (
                None,
                color_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            )
        };
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("3D Offscreen Depth Target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: self.scene_3d_sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let composite_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("3D Composite Bind Group"),
            layout: &self.composite_3d_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.composite_3d_sampler),
                },
            ],
        });
        DepthRenderTarget {
            color_texture,
            color_view,
            msaa_color_texture,
            render_color_view,
            depth_texture,
            depth_view,
            composite_bind_group,
            width,
            height,
            sample_count: self.scene_3d_sample_count,
        }
    }

    pub fn composite_depth_render_target(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        target: &DepthRenderTarget,
    ) {
        render_pass.set_pipeline(&self.composite_3d_pipeline);
        render_pass.set_bind_group(0, &target.composite_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    pub fn build_geometry_static(
        document: &Document,
        view: &ViewTransform,
        dark_mode: bool,
        include_overlays: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        if include_overlays {
            Self::build_grid_static(&mut vertices, &mut indices, view, dark_mode);
            Self::build_axes_static(&mut vertices, &mut indices, view, dark_mode);
        }

        for (_, obj) in ordered_visible_2d_objects(document) {
            match obj {
                GeoObject::Point(p) if include_overlays => {
                    let screen = view.world_to_screen(p.position);
                    let size = p.size.max(1.0);
                    Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
                }
                GeoObject::Line(l) => {
                    let start = Point2::new(
                        document.resolve_expr(&l.start_x_expr, l.start.x),
                        document.resolve_expr(&l.start_y_expr, l.start.y),
                    );
                    let end = Point2::new(
                        document.resolve_expr(&l.end_x_expr, l.end.x),
                        document.resolve_expr(&l.end_y_expr, l.end.y),
                    );
                    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(view.screen_size);
                    let view_bounds = grafito_geometry::AABB::new(
                        Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                        Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                    );
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
                        Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                    }
                }
                GeoObject::Circle(c) => {
                    let center = view.world_to_screen(c.center);
                    let radius = (c.radius as f32) * view.scale as f32;
                    Self::add_circle_stroke(
                        &mut vertices,
                        &mut indices,
                        center,
                        radius,
                        c.width,
                        c.color,
                    );
                    if let Some(fill) = c.fill_color {
                        Self::add_circle_fill(&mut vertices, &mut indices, center, radius, fill);
                    }
                }
                GeoObject::Polygon(poly)
                    if polygon_geometry_is_within_limit(poly.vertices.len()) =>
                {
                    let mut screen_verts = Vec::with_capacity(poly.vertices.len());
                    for (i, v) in poly.vertices.iter().enumerate() {
                        let x = document.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                        let y = document.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                        screen_verts.push(view.world_to_screen(Point2::new(x, y)));
                    }
                    if let Some(fill) = poly.fill_color {
                        Self::add_polygon_fill(
                            &mut vertices,
                            &mut indices,
                            &screen_verts,
                            fill,
                            view,
                        );
                    }
                    Self::add_polygon_stroke(
                        &mut vertices,
                        &mut indices,
                        &screen_verts,
                        poly.width,
                        poly.color,
                    );
                }
                GeoObject::Pencil(pencil) if pencil.points.len() >= 2 => {
                    // Polilínea: cada par consecutivo de puntos genera un
                    // segmento con `add_line_segment`. Aplicamos clipping
                    // 2D por segmento para no dibujar fuera del viewport.
                    let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                    let world_br = view.screen_to_world(view.screen_size);
                    let view_bounds = grafito_geometry::AABB::new(
                        Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                        Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                    );
                    for w in pencil.points.windows(2) {
                        let a = w[0];
                        let b = w[1];
                        if let Some((clip_a, clip_b)) =
                            grafito_geometry::clip_segment_to_rect(a, b, view_bounds)
                        {
                            let sa = view.world_to_screen(clip_a);
                            let sb = view.world_to_screen(clip_b);
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                sa,
                                sb,
                                pencil.width,
                                pencil.color,
                            );
                        }
                    }
                }
                GeoObject::Function(fun) => {
                    Self::add_function_geometry(&mut vertices, &mut indices, document, view, fun);
                }
                GeoObject::ParametricCurve2D(curve) => {
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                        curve,
                        1_000,
                        &document.variables,
                    );
                    let mut previous: Option<glam::Vec2> = None;
                    for &(x, y) in samples.iter() {
                        if !x.is_finite() || !y.is_finite() {
                            previous = None;
                            continue;
                        }
                        let current = view.world_to_screen(Point2::new(x, y));
                        if !current.is_finite() {
                            previous = None;
                            continue;
                        }
                        if let Some(previous) = previous {
                            if (current.x - previous.x).abs() < 300.0
                                && (current.y - previous.y).abs() < 300.0
                            {
                                Self::add_line_segment(
                                    &mut vertices,
                                    &mut indices,
                                    previous,
                                    current,
                                    curve.width,
                                    curve.color,
                                );
                            }
                        }
                        previous = Some(current);
                    }
                }
                GeoObject::PolarCurve(curve) => {
                    let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
                        curve,
                        1_000,
                        &document.variables,
                    );
                    let mut previous: Option<glam::Vec2> = None;
                    for &(x, y) in samples.iter() {
                        if !x.is_finite() || !y.is_finite() {
                            previous = None;
                            continue;
                        }
                        let current = view.world_to_screen(Point2::new(x, y));
                        if !current.is_finite() {
                            previous = None;
                            continue;
                        }
                        if let Some(previous) = previous {
                            if (current.x - previous.x).abs() < 300.0
                                && (current.y - previous.y).abs() < 300.0
                            {
                                Self::add_line_segment(
                                    &mut vertices,
                                    &mut indices,
                                    previous,
                                    current,
                                    curve.width,
                                    curve.color,
                                );
                            }
                        }
                        previous = Some(current);
                    }
                }
                GeoObject::ImplicitCurve(curve) => {
                    let top_left = view.screen_to_world(glam::Vec2::ZERO);
                    let bottom_right = view.screen_to_world(view.screen_size);
                    let bounds = (
                        top_left.x.min(bottom_right.x),
                        top_left.x.max(bottom_right.x),
                        top_left.y.min(bottom_right.y),
                        top_left.y.max(bottom_right.y),
                    );
                    let segments = grafito_core::implicit_curve::segments_or_compute(
                        curve,
                        bounds,
                        128,
                        &document.variables,
                        document.render_quality,
                    );
                    for (_, level_segments) in segments.iter() {
                        for (start, end) in level_segments {
                            let start = view.world_to_screen(*start);
                            let end = view.world_to_screen(*end);
                            if start.is_finite() && end.is_finite() {
                                Self::add_line_segment(
                                    &mut vertices,
                                    &mut indices,
                                    start,
                                    end,
                                    curve.width,
                                    curve.color,
                                );
                            }
                        }
                    }
                }
                GeoObject::VectorField2D(vf) => {
                    Self::add_vector_field_geometry(
                        &mut vertices,
                        &mut indices,
                        document,
                        view,
                        vf,
                    );
                }
                GeoObject::ComplexGrid(cg) => {
                    Self::add_complex_grid_geometry(
                        &mut vertices,
                        &mut indices,
                        document,
                        view,
                        cg,
                    );
                }
                GeoObject::ComplexMapping(cm) => {
                    Self::add_complex_mapping_geometry(
                        &mut vertices,
                        &mut indices,
                        document,
                        view,
                        cm,
                        0.0,
                    );
                }
                GeoObject::Fractal2D(fr) => {
                    let _ = Self::add_fractal_geometry(&mut vertices, &mut indices, view, fr);
                }
                GeoObject::Histogram(histogram) => {
                    let bins =
                        grafito_geometry::statistics::histogram(&histogram.data, histogram.bins);
                    let max_count = bins.iter().map(|(_, _, count)| *count).fold(0.0, f64::max);
                    if max_count > 0.0 {
                        let y_scale = (histogram.y_max - histogram.y_min) / max_count;
                        for (left, right, count) in bins {
                            let bottom = view.world_to_screen(Point2::new(left, histogram.y_min));
                            let top = view.world_to_screen(Point2::new(
                                right,
                                histogram.y_min + count * y_scale,
                            ));
                            if bottom.is_finite() && top.is_finite() {
                                Self::add_rect(
                                    &mut vertices,
                                    &mut indices,
                                    bottom,
                                    top.x - bottom.x,
                                    top.y - bottom.y,
                                    histogram.color,
                                );
                            }
                        }
                    }
                }
                GeoObject::ScatterPlot(scatter) => {
                    for (x, y) in scatter.xs.iter().zip(&scatter.ys) {
                        let point = view.world_to_screen(Point2::new(*x, *y));
                        if point.is_finite() {
                            Self::add_rect(
                                &mut vertices,
                                &mut indices,
                                point,
                                scatter.point_size,
                                scatter.point_size,
                                scatter.color,
                            );
                        }
                    }
                }
                GeoObject::BoxPlot(box_plot) => {
                    if let Some((min, q1, _, q3, max, outliers)) =
                        grafito_geometry::statistics::boxplot_stats(&box_plot.data)
                    {
                        let x = view.world_to_screen(Point2::new(box_plot.position, 0.0)).x;
                        let y_min = view.world_to_screen(Point2::new(0.0, min)).y;
                        let y_q1 = view.world_to_screen(Point2::new(0.0, q1)).y;
                        let y_q3 = view.world_to_screen(Point2::new(0.0, q3)).y;
                        let y_max = view.world_to_screen(Point2::new(0.0, max)).y;
                        let half_width = (box_plot.width_box * view.scale) as f32;
                        if [x, y_min, y_q1, y_q3, y_max]
                            .iter()
                            .all(|value| value.is_finite())
                        {
                            Self::add_rect(
                                &mut vertices,
                                &mut indices,
                                glam::Vec2::new(x - half_width, y_q3),
                                half_width * 2.0,
                                (y_q1 - y_q3).abs(),
                                box_plot.color,
                            );
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                glam::Vec2::new(x, y_q3),
                                glam::Vec2::new(x, y_max),
                                box_plot.width,
                                box_plot.color,
                            );
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                glam::Vec2::new(x, y_q1),
                                glam::Vec2::new(x, y_min),
                                box_plot.width,
                                box_plot.color,
                            );
                            for outlier in outliers {
                                let y = view.world_to_screen(Point2::new(0.0, outlier)).y;
                                if y.is_finite() {
                                    Self::add_rect(
                                        &mut vertices,
                                        &mut indices,
                                        glam::Vec2::new(x - 2.0, y - 2.0),
                                        4.0,
                                        4.0,
                                        box_plot.color,
                                    );
                                }
                            }
                        }
                    }
                }
                GeoObject::RegressionLine(regression) => {
                    let x_min = regression.xs.iter().copied().fold(f64::INFINITY, f64::min);
                    let x_max = regression
                        .xs
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max);
                    if x_min.is_finite() && x_max.is_finite() {
                        let start = view.world_to_screen(Point2::new(
                            x_min,
                            regression.slope * x_min + regression.intercept,
                        ));
                        let end = view.world_to_screen(Point2::new(
                            x_max,
                            regression.slope * x_max + regression.intercept,
                        ));
                        if start.is_finite() && end.is_finite() {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                start,
                                end,
                                regression.width,
                                regression.color,
                            );
                        }
                    }
                }
                GeoObject::PhasePortrait(portrait) => {
                    for (start, end) in sample_phase_portrait(portrait, &document.variables) {
                        let start = view.world_to_screen(start);
                        let end = view.world_to_screen(end);
                        if start.is_finite() && end.is_finite() {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                start,
                                end,
                                1.5,
                                portrait.color,
                            );
                        }
                    }
                }
                GeoObject::Arc(arc) => {
                    // Resolución 64 como pide P1.1.
                    let pts = arc.sample_points(64);
                    let mut prev: Option<glam::Vec2> = None;
                    for p in pts {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            prev = None;
                            continue;
                        }
                        let cur = view.world_to_screen(p);
                        if !cur.is_finite() {
                            prev = None;
                            continue;
                        }
                        if let Some(prev) = prev {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                prev,
                                cur,
                                arc.width,
                                arc.color,
                            );
                        }
                        prev = Some(cur);
                    }
                }
                GeoObject::Sector(sector) => {
                    let verts_world = sector.polygon_vertices(64);
                    let mut screen_verts = Vec::with_capacity(verts_world.len());
                    for v in &verts_world {
                        screen_verts.push(view.world_to_screen(*v));
                    }
                    if let Some(fill) = sector.fill_color {
                        Self::add_polygon_fill(
                            &mut vertices,
                            &mut indices,
                            &screen_verts,
                            fill,
                            view,
                        );
                    }
                    Self::add_polygon_stroke(
                        &mut vertices,
                        &mut indices,
                        &screen_verts,
                        sector.width,
                        sector.color,
                    );
                }
                GeoObject::BezierCurve(bez) => {
                    let pts = bez.sample_points(64);
                    let mut prev: Option<glam::Vec2> = None;
                    for p in pts {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            prev = None;
                            continue;
                        }
                        let cur = view.world_to_screen(p);
                        if !cur.is_finite() {
                            prev = None;
                            continue;
                        }
                        if let Some(prev) = prev {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                prev,
                                cur,
                                bez.width,
                                bez.color,
                            );
                        }
                        prev = Some(cur);
                    }
                }
                GeoObject::Spline(spline) => {
                    let pts = spline.sample_points(16);
                    let mut prev: Option<glam::Vec2> = None;
                    for p in pts {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            prev = None;
                            continue;
                        }
                        let cur = view.world_to_screen(p);
                        if !cur.is_finite() {
                            prev = None;
                            continue;
                        }
                        if let Some(prev) = prev {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                prev,
                                cur,
                                spline.width,
                                spline.color,
                            );
                        }
                        prev = Some(cur);
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    /// Construye una representación CPU acotada de un objeto transformado.
    /// La malla base recibe el mapa complejo persistido para que el fallback
    /// nunca sustituya silenciosamente el objeto por su `inner` sin transformar.
    pub fn build_transformed_geometry_static(
        document: &Document,
        transformed: &TransformedObj,
        view: &ViewTransform,
        dark_mode: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        Self::build_transformed_geometry_static_at(document, transformed, view, dark_mode, 0)
    }

    fn build_transformed_geometry_static_at(
        document: &Document,
        transformed: &TransformedObj,
        view: &ViewTransform,
        dark_mode: bool,
        depth: usize,
    ) -> (Vec<Vertex>, Vec<u32>) {
        if depth >= grafito_core::validation::MAX_TRANSFORM_DEPTH {
            return (Vec::new(), Vec::new());
        }

        // Cache keyed por document.version+view+expr para evitar `document.clone()` y recompute
        let cache_key = transformed_cache_key(document, transformed, view, dark_mode, depth);
        if let Some(cached) = TRANSFORMED_CACHE.with(|c| c.borrow().get(&cache_key).cloned()) {
            return cached;
        }

        let (mut vertices, indices) = match transformed.inner.as_ref() {
            GeoObject::Transformed(inner) => Self::build_transformed_geometry_static_at(
                document,
                inner,
                view,
                dark_mode,
                depth + 1,
            ),
            GeoObject::Point(point) => {
                let mut vertices = Vec::new();
                let mut indices = Vec::new();
                let screen = view.world_to_screen(point.position);
                let size = point.size.max(1.0);
                Self::add_rect(&mut vertices, &mut indices, screen, size, size, point.color);
                (vertices, indices)
            }
            inner => {
                // Evita `document.clone()`: genera geometría aislada usando `&Document`
                Self::build_isolated_geometry_static(document, inner, view, dark_mode)
            }
        };
        let _ = apply_complex_transform_cpu(
            &mut vertices,
            view,
            &transformed.complex_expr,
            &document.variables,
            document.complex_base_symbol.as_str(),
        );
        TRANSFORMED_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() >= TRANSFORMED_CACHE_CAP {
                let keys: Vec<u64> = cache
                    .keys()
                    .copied()
                    .take(TRANSFORMED_CACHE_CAP / 2)
                    .collect();
                for k in keys {
                    cache.remove(&k);
                }
            }
            cache.insert(cache_key, (vertices.clone(), indices.clone()));
        });
        (vertices, indices)
    }

    fn build_isolated_geometry_static(
        document: &Document,
        object: &GeoObject,
        view: &ViewTransform,
        dark_mode: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        match object {
            GeoObject::Point(p) => {
                let screen = view.world_to_screen(p.position);
                let size = p.size.max(1.0);
                Self::add_rect(&mut vertices, &mut indices, screen, size, size, p.color);
            }
            GeoObject::Line(l) => {
                let start = Point2::new(
                    document.resolve_expr(&l.start_x_expr, l.start.x),
                    document.resolve_expr(&l.start_y_expr, l.start.y),
                );
                let end = Point2::new(
                    document.resolve_expr(&l.end_x_expr, l.end.x),
                    document.resolve_expr(&l.end_y_expr, l.end.y),
                );
                let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                let world_br = view.screen_to_world(view.screen_size);
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );
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
                    Self::add_line_segment(&mut vertices, &mut indices, a, b, l.width, l.color);
                }
            }
            GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let radius = (c.radius as f32) * view.scale as f32;
                Self::add_circle_stroke(
                    &mut vertices,
                    &mut indices,
                    center,
                    radius,
                    c.width,
                    c.color,
                );
                if let Some(fill) = c.fill_color {
                    Self::add_circle_fill(&mut vertices, &mut indices, center, radius, fill);
                }
            }
            GeoObject::Polygon(poly) if polygon_geometry_is_within_limit(poly.vertices.len()) => {
                let mut screen_verts = Vec::with_capacity(poly.vertices.len());
                for (i, v) in poly.vertices.iter().enumerate() {
                    let x = document.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                    let y = document.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                    screen_verts.push(view.world_to_screen(Point2::new(x, y)));
                }
                if let Some(fill) = poly.fill_color {
                    Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill, view);
                }
                Self::add_polygon_stroke(
                    &mut vertices,
                    &mut indices,
                    &screen_verts,
                    poly.width,
                    poly.color,
                );
            }
            GeoObject::Ellipse(el) => {
                let n = 64;
                let mut pts = Vec::with_capacity(n);
                for i in 0..n {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU;
                    let x = el.center.x + el.rx * t.cos() * el.angle.cos()
                        - el.ry * t.sin() * el.angle.sin();
                    let y = el.center.y
                        + el.rx * t.cos() * el.angle.sin()
                        + el.ry * t.sin() * el.angle.cos();
                    pts.push(view.world_to_screen(Point2::new(x, y)));
                }
                if let Some(fill) = el.fill_color {
                    Self::add_polygon_fill(&mut vertices, &mut indices, &pts, fill, view);
                }
                Self::add_polygon_stroke(&mut vertices, &mut indices, &pts, el.width, el.color);
            }
            GeoObject::Parabola(pb) if pb.p.is_finite() && pb.p.abs() >= 1e-12 => {
                let steps = 128;
                let range = (20.0 / view.scale).clamp(0.1, 500.0);
                let cos_a = pb.angle.cos();
                let sin_a = pb.angle.sin();
                let mut prev: Option<glam::Vec2> = None;
                for i in 0..=steps {
                    let t = -range + 2.0 * range * i as f64 / steps as f64;
                    let wx = pb.vertex.x + t * cos_a - (t * t / (4.0 * pb.p)) * sin_a;
                    let wy = pb.vertex.y + t * sin_a + (t * t / (4.0 * pb.p)) * cos_a;
                    if wx.is_finite() && wy.is_finite() {
                        let s = view.world_to_screen(Point2::new(wx, wy));
                        if let Some(prev) = prev {
                            if (s.x - prev.x).abs() < 300.0 {
                                Self::add_line_segment(
                                    &mut vertices,
                                    &mut indices,
                                    prev,
                                    s,
                                    pb.width,
                                    pb.color,
                                );
                            }
                        }
                        prev = Some(s);
                    }
                }
            }
            GeoObject::Hyperbola(hb) => {
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
                    let mut prev: Option<glam::Vec2> = None;
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
                            if let Some(prev) = prev {
                                if (s.x - prev.x).abs() < 300.0 {
                                    Self::add_line_segment(
                                        &mut vertices,
                                        &mut indices,
                                        prev,
                                        s,
                                        hb.width,
                                        hb.color,
                                    );
                                }
                            }
                            prev = Some(s);
                        }
                    }
                }
            }
            GeoObject::Pencil(pencil) if pencil.points.len() >= 2 => {
                let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                let world_br = view.screen_to_world(view.screen_size);
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );
                for w in pencil.points.windows(2) {
                    let a = w[0];
                    let b = w[1];
                    if let Some((clip_a, clip_b)) =
                        grafito_geometry::clip_segment_to_rect(a, b, view_bounds)
                    {
                        let sa = view.world_to_screen(clip_a);
                        let sb = view.world_to_screen(clip_b);
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            sa,
                            sb,
                            pencil.width,
                            pencil.color,
                        );
                    }
                }
            }
            GeoObject::Arc(arc) => {
                let pts = arc.sample_points(64);
                let mut prev: Option<glam::Vec2> = None;
                for p in pts {
                    if !p.x.is_finite() || !p.y.is_finite() {
                        prev = None;
                        continue;
                    }
                    let cur = view.world_to_screen(p);
                    if !cur.is_finite() {
                        prev = None;
                        continue;
                    }
                    if let Some(prev) = prev {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            prev,
                            cur,
                            arc.width,
                            arc.color,
                        );
                    }
                    prev = Some(cur);
                }
            }
            GeoObject::Sector(sector) => {
                let verts_world = sector.polygon_vertices(64);
                let mut screen_verts = Vec::with_capacity(verts_world.len());
                for v in &verts_world {
                    screen_verts.push(view.world_to_screen(*v));
                }
                if let Some(fill) = sector.fill_color {
                    Self::add_polygon_fill(&mut vertices, &mut indices, &screen_verts, fill, view);
                }
                Self::add_polygon_stroke(
                    &mut vertices,
                    &mut indices,
                    &screen_verts,
                    sector.width,
                    sector.color,
                );
            }
            GeoObject::BezierCurve(bez) => {
                let pts = bez.sample_points(64);
                let mut prev: Option<glam::Vec2> = None;
                for p in pts {
                    if !p.x.is_finite() || !p.y.is_finite() {
                        prev = None;
                        continue;
                    }
                    let cur = view.world_to_screen(p);
                    if !cur.is_finite() {
                        prev = None;
                        continue;
                    }
                    if let Some(prev) = prev {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            prev,
                            cur,
                            bez.width,
                            bez.color,
                        );
                    }
                    prev = Some(cur);
                }
            }
            GeoObject::Spline(spline) => {
                let pts = spline.sample_points(16);
                let mut prev: Option<glam::Vec2> = None;
                for p in pts {
                    if !p.x.is_finite() || !p.y.is_finite() {
                        prev = None;
                        continue;
                    }
                    let cur = view.world_to_screen(p);
                    if !cur.is_finite() {
                        prev = None;
                        continue;
                    }
                    if let Some(prev) = prev {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            prev,
                            cur,
                            spline.width,
                            spline.color,
                        );
                    }
                    prev = Some(cur);
                }
            }
            GeoObject::Function(fun) => {
                Self::add_function_geometry(&mut vertices, &mut indices, document, view, fun);
            }
            GeoObject::ParametricCurve2D(curve) => {
                let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                    curve,
                    1_000,
                    &document.variables,
                );
                let mut previous: Option<glam::Vec2> = None;
                for &(x, y) in samples.iter() {
                    if !x.is_finite() || !y.is_finite() {
                        previous = None;
                        continue;
                    }
                    let current = view.world_to_screen(Point2::new(x, y));
                    if !current.is_finite() {
                        previous = None;
                        continue;
                    }
                    if let Some(prev) = previous {
                        if (current.x - prev.x).abs() < 300.0 && (current.y - prev.y).abs() < 300.0
                        {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                prev,
                                current,
                                curve.width,
                                curve.color,
                            );
                        }
                    }
                    previous = Some(current);
                }
            }
            GeoObject::PolarCurve(curve) => {
                let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
                    curve,
                    1_000,
                    &document.variables,
                );
                let mut previous: Option<glam::Vec2> = None;
                for &(x, y) in samples.iter() {
                    if !x.is_finite() || !y.is_finite() {
                        previous = None;
                        continue;
                    }
                    let current = view.world_to_screen(Point2::new(x, y));
                    if !current.is_finite() {
                        previous = None;
                        continue;
                    }
                    if let Some(prev) = previous {
                        if (current.x - prev.x).abs() < 300.0 && (current.y - prev.y).abs() < 300.0
                        {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                prev,
                                current,
                                curve.width,
                                curve.color,
                            );
                        }
                    }
                    previous = Some(current);
                }
            }
            GeoObject::ImplicitCurve(curve) => {
                let top_left = view.screen_to_world(glam::Vec2::ZERO);
                let bottom_right = view.screen_to_world(view.screen_size);
                let bounds = (
                    top_left.x.min(bottom_right.x),
                    top_left.x.max(bottom_right.x),
                    top_left.y.min(bottom_right.y),
                    top_left.y.max(bottom_right.y),
                );
                let segments = grafito_core::implicit_curve::segments_or_compute(
                    curve,
                    bounds,
                    128,
                    &document.variables,
                    document.render_quality,
                );
                for (_, level_segments) in segments.iter() {
                    for (start, end) in level_segments {
                        let start = view.world_to_screen(*start);
                        let end = view.world_to_screen(*end);
                        if start.is_finite() && end.is_finite() {
                            Self::add_line_segment(
                                &mut vertices,
                                &mut indices,
                                start,
                                end,
                                curve.width,
                                curve.color,
                            );
                        }
                    }
                }
            }
            GeoObject::VectorField2D(vf) => {
                Self::add_vector_field_geometry(&mut vertices, &mut indices, document, view, vf);
            }
            GeoObject::ComplexGrid(cg) => {
                Self::add_complex_grid_geometry(&mut vertices, &mut indices, document, view, cg);
            }
            GeoObject::Fractal2D(fr) => {
                let _ = Self::add_fractal_geometry(&mut vertices, &mut indices, view, fr);
            }
            GeoObject::PhasePortrait(portrait) => {
                for (start, end) in sample_phase_portrait(portrait, &document.variables) {
                    let start = view.world_to_screen(start);
                    let end = view.world_to_screen(end);
                    if start.is_finite() && end.is_finite() {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            start,
                            end,
                            1.5,
                            portrait.color,
                        );
                    }
                }
            }
            _ => {}
        }
        let _ = dark_mode;
        (vertices, indices)
    }

    fn add_function_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view: &ViewTransform,
        fun: &grafito_core::FunctionObj,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);
        let domain = (
            document.resolve_expr(&fun.domain_min_expr, fun.domain_min.unwrap_or(world_tl.x)),
            document.resolve_expr(&fun.domain_max_expr, fun.domain_max.unwrap_or(world_br.x)),
        );
        let grid_size = grafito_core::function_sampling::recommended_grid_size_for_quality(
            view.screen_size.x,
            document.render_quality,
        );
        let samples = grafito_core::function_sampling::samples_or_compute(
            fun,
            domain,
            grid_size,
            &document.variables,
        );

        let mut previous: Option<glam::Vec2> = None;
        for (x, y_opt) in samples.iter() {
            let y = match y_opt {
                Some(y) => Some(*y),
                // Core sampling intentionally marks extreme values as gaps for
                // general plotting. Re-evaluate here so finite samples that are
                // actually in the active view are not discarded by that global cap.
                None if !fun.is_integral => grafito_geometry::expr::eval_function_with_vars(
                    &fun.expr,
                    *x,
                    &document.variables,
                )
                .ok()
                .filter(|value| value.is_finite()),
                None => None,
            };
            let Some(y) = y else {
                previous = None;
                continue;
            };
            let Some(current) = bounded_screen_point(view, Point2::new(*x, y)) else {
                previous = None;
                continue;
            };
            if let Some(previous) = previous {
                if (current.x - previous.x).abs() < 300.0 {
                    Self::add_line_segment(
                        vertices, indices, previous, current, fun.width, fun.color,
                    );
                }
            }
            previous = Some(current);
        }
    }

    fn add_vector_field_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view: &ViewTransform,
        vf: &grafito_core::VectorField2DObj,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);
        let bounds = (
            world_tl.x.min(world_br.x),
            world_tl.x.max(world_br.x),
            world_br.y.min(world_tl.y),
            world_br.y.max(world_tl.y),
        );
        if ![bounds.0, bounds.1, bounds.2, bounds.3]
            .into_iter()
            .all(f64::is_finite)
        {
            return;
        }

        let grid_size = vf.density.clamp(5, 128);
        let cell_width = (bounds.1 - bounds.0).abs() / grid_size as f64;
        let cell_height = (bounds.3 - bounds.2).abs() / grid_size as f64;
        let arrow_length = cell_width.min(cell_height) * 0.8;
        if !arrow_length.is_finite() || arrow_length <= 0.0 {
            return;
        }

        let samples = grafito_core::vector_field_sampling::samples_or_compute(
            vf,
            bounds,
            grid_size,
            &document.variables,
        );
        for (x, y, u, v) in samples.iter() {
            let start = Point2::new(*x, *y);
            let Some(end) = vector_arrow_end(*x, *y, *u, *v, arrow_length) else {
                continue;
            };
            let (Some(start), Some(end)) = (
                bounded_screen_point(view, start),
                bounded_screen_point(view, end),
            ) else {
                continue;
            };
            Self::add_line_segment(vertices, indices, start, end, 1.5, vf.color);
        }
    }

    fn fractal_geometry_requirements(fr: &grafito_core::Fractal2DObj) -> Option<(usize, usize)> {
        if fr.resolution == 0
            || ![fr.x_min, fr.x_max, fr.y_min, fr.y_max]
                .into_iter()
                .all(f64::is_finite)
            || fr.x_min >= fr.x_max
            || fr.y_min >= fr.y_max
            || grafito_geometry::fractals::validate_fractal_budget(
                fr.resolution,
                fr.resolution,
                fr.max_iter,
            )
            .is_err()
        {
            return None;
        }

        let pixels = fr.resolution.checked_mul(fr.resolution)?;
        Some((pixels.checked_mul(4)?, pixels.checked_mul(6)?))
    }

    fn fractal_type(fr: &grafito_core::Fractal2DObj) -> grafito_geometry::fractals::FractalType {
        match fr.fractal_type.as_str() {
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
        }
    }

    fn add_fractal_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view_transform: &ViewTransform,
        fr: &grafito_core::Fractal2DObj,
    ) -> bool {
        let Some((required_vertices, _)) = Self::fractal_geometry_requirements(fr) else {
            return false;
        };
        if !fractal_geometry_fits(vertices.len(), indices.len(), fr) {
            return false;
        }

        let resolution = fr.resolution;
        let fractal = Self::fractal_type(fr);
        let Ok(pixels) = grafito_geometry::fractals::try_compute_fractal(
            &fractal, fr.x_min, fr.x_max, fr.y_min, fr.y_max, resolution, resolution,
        ) else {
            return false;
        };
        if pixels.len() != required_vertices / 4 {
            return false;
        }

        let dx = (fr.x_max - fr.x_min) / resolution as f64;
        let dy = (fr.y_max - fr.y_min) / resolution as f64;
        for pixel in &pixels {
            let (r, g, b, a) = grafito_geometry::fractals::fractal_color_hsv(
                pixel.iter,
                pixel.max_iter,
                pixel.smooth_value,
            );
            let color = Color::new(r, g, b, a);
            let screen = view_transform.world_to_screen(Point2::new(pixel.x, pixel.y));
            let pixel_width = (dx * view_transform.scale) as f32;
            let pixel_height = (dy * view_transform.scale) as f32;
            Self::add_rect(
                vertices,
                indices,
                screen,
                pixel_width.max(1.0),
                pixel_height.max(1.0),
                color,
            );
        }
        true
    }

    fn build_grid_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let mut min_x = world_tl.x.floor() as i64 - 1;
        let mut max_x = world_br.x.ceil() as i64 + 1;
        let mut min_y = world_br.y.floor() as i64 - 1;
        let mut max_y = world_tl.y.ceil() as i64 + 1;

        if max_x.saturating_sub(min_x) > 500 {
            let center = (min_x + max_x) / 2;
            min_x = center - 250;
            max_x = center + 250;
        }
        if max_y.saturating_sub(min_y) > 500 {
            let center = (min_y + max_y) / 2;
            min_y = center - 250;
            max_y = center + 250;
        }

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        for x in min_x..=max_x {
            let a = view.world_to_screen(Point2::new(x as f64, min_y as f64));
            let b = view.world_to_screen(Point2::new(x as f64, max_y as f64));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }

        for y in min_y..=max_y {
            let a = view.world_to_screen(Point2::new(min_x as f64, y as f64));
            let b = view.world_to_screen(Point2::new(max_x as f64, y as f64));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);
        }
    }

    fn build_axes_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let axis_color = if dark_mode {
            Color::new(0.7, 0.7, 0.7, 1.0)
        } else {
            Color::BLACK
        };

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        Self::add_line_segment(vertices, indices, x_axis_a, x_axis_b, 2.0, axis_color);

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        Self::add_line_segment(vertices, indices, y_axis_a, y_axis_b, 2.0, axis_color);
    }

    pub fn build_3d_geometry_static(
        document: &Document,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        Self::build_3d_grid_static(
            &mut vertices,
            &mut indices,
            camera,
            dark_mode,
            screen_w,
            screen_h,
        );
        Self::build_3d_axes_static(&mut vertices, &mut indices, camera, screen_w, screen_h);

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(p) => {
                    if let Some(screen_pos) = camera.project(&p.position, screen_w, screen_h) {
                        let size = p.size.max(1.0);
                        Self::add_rect(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(screen_pos.0, screen_pos.1),
                            size,
                            size,
                            p.color,
                        );
                    }
                }
                GeoObject::Segment3D(s) => {
                    if let (Some(a), Some(b)) = (
                        camera.project(&s.a, screen_w, screen_h),
                        camera.project(&s.b, screen_w, screen_h),
                    ) {
                        Self::add_line_segment(
                            &mut vertices,
                            &mut indices,
                            glam::Vec2::new(a.0, a.1),
                            glam::Vec2::new(b.0, b.1),
                            s.width,
                            s.color,
                        );
                    }
                }
                GeoObject::Plane3D(p) => {
                    Self::add_plane3d_patch(
                        &mut vertices,
                        &mut indices,
                        camera,
                        p.a,
                        p.b,
                        p.c,
                        p.d,
                        p.opacity,
                        p.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Line3D(l) => {
                    Self::add_line3d_object(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &l.point,
                        &l.direction,
                        l.width,
                        l.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Sphere3D(s) => {
                    Self::add_wireframe_sphere(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &s.center,
                        s.radius,
                        s.width,
                        s.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cube3D(c) => {
                    Self::add_wireframe_cube(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.center,
                        c.size,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Tetrahedron3D(t) => {
                    Self::add_wireframe_tetrahedron(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &t.center,
                        t.edge_length,
                        t.width,
                        t.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Pyramid3D(p) => {
                    Self::add_wireframe_pyramid(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &p.base_center,
                        &p.apex,
                        p.base_size,
                        p.width,
                        p.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cone3D(c) => {
                    Self::add_wireframe_cone(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.base_center,
                        &c.apex,
                        c.radius,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cylinder3D(c) => {
                    Self::add_wireframe_cylinder(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.base_center,
                        &c.top_center,
                        c.radius,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Torus3D(t) => {
                    Self::add_wireframe_torus(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &t.center,
                        t.r_major,
                        t.r_minor,
                        t.width,
                        t.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::MoebiusStrip(mb) => {
                    Self::add_wireframe_moebius(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &mb.center,
                        mb.radius,
                        mb.width_r,
                        mb.width,
                        mb.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Surface3D(su) => {
                    Self::add_surface_mesh(
                        &mut vertices,
                        &mut indices,
                        camera,
                        su,
                        &document.variables,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::ParametricCurve3D(pc) => {
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_3d(
                        pc,
                        4000,
                        &document.variables,
                    );
                    let mut prev: Option<Point3D> = None;
                    for &(x, y, z) in samples.iter() {
                        if x.is_finite() && y.is_finite() && z.is_finite() {
                            let p = Point3D::new(x, y, z);
                            if let Some(prev_p) = prev {
                                Self::add_line_3d(
                                    &mut vertices,
                                    &mut indices,
                                    camera,
                                    &prev_p,
                                    &p,
                                    pc.width,
                                    pc.color,
                                    screen_w,
                                    screen_h,
                                );
                            }
                            prev = Some(p);
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::Attractor3D(at) => {
                    let att_type = at.model();
                    let points = grafito_geometry::attractors::integrate_attractor(
                        &att_type, at.x0, at.y0, at.z0, at.dt, at.steps, at.skip,
                    );
                    for w in points.windows(2) {
                        let a = Point3D::new(w[0].x * 0.2, w[0].y * 0.2, w[0].z * 0.2);
                        let b = Point3D::new(w[1].x * 0.2, w[1].y * 0.2, w[1].z * 0.2);
                        Self::add_line_3d(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &a,
                            &b,
                            at.width,
                            at.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    fn build_3d_grid_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let pixels_per_unit = (screen_h / frustum_height) as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-6);
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
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        let center_x = camera.target.x as f64;
        let center_z = camera.target.z as f64;
        let aspect = screen_w / screen_h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let start_x = ((center_x - view_range) / minor_step).floor() * minor_step;
        let end_x = ((center_x + view_range) / minor_step).ceil() * minor_step;
        let start_z = ((center_z - view_range) / minor_step).floor() * minor_step;
        let end_z = ((center_z + view_range) / minor_step).ceil() * minor_step;

        let line_count_x = ((end_x - start_x) / minor_step).round() as i64;
        let line_count_z = ((end_z - start_z) / minor_step).round() as i64;

        if line_count_x <= 500 && line_count_z <= 500 {
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * minor_step;
                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                Self::add_line_3d_static(
                    vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h,
                );
            }

            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * minor_step;
                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                Self::add_line_3d_static(
                    vertices, indices, camera, &p1, &p2, 1.0, color, screen_w, screen_h,
                );
            }
        }
    }

    fn build_3d_axes_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let aspect = screen_w / screen_h.max(1.0);
        let axis_len = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let red = Color::new(0.86, 0.2, 0.2, 1.0);
        let green = Color::new(0.2, 0.7, 0.2, 1.0);
        let blue = Color::new(0.2, 0.2, 0.86, 1.0);

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0,
            red,
            screen_w,
            screen_h,
        );

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0,
            green,
            screen_w,
            screen_h,
        );

        Self::add_line_3d_static(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0,
            blue,
            screen_w,
            screen_h,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_line_3d_static(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        a: &Point3D,
        b: &Point3D,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let (Some(sa), Some(sb)) = (
            camera.project(a, screen_w, screen_h),
            camera.project(b, screen_w, screen_h),
        ) {
            Self::add_line_segment(
                vertices,
                indices,
                glam::Vec2::new(sa.0, sa.1),
                glam::Vec2::new(sb.0, sb.1),
                width,
                color,
            );
        }
    }

    fn add_line3d_object(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        point: &Point3D,
        direction: &Point3D,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let len =
            (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
                .sqrt();
        if len < 1e-15 {
            return;
        }
        let span = (camera.distance as f64 * 4.0).max(20.0);
        let dx = direction.x / len * span;
        let dy = direction.y / len * span;
        let dz = direction.z / len * span;
        let a = Point3D::new(point.x - dx, point.y - dy, point.z - dz);
        let b = Point3D::new(point.x + dx, point.y + dy, point.z + dz);
        Self::add_line_3d(
            vertices, indices, camera, &a, &b, width, color, screen_w, screen_h,
        );
    }

    fn add_plane3d_patch(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        a: f64,
        b: f64,
        c: f64,
        d: f64,
        opacity: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let normal = glam::Vec3::new(a as f32, b as f32, c as f32);
        let norm_sq = normal.length_squared();
        if norm_sq < 1e-12 {
            return;
        }
        let n = normal.normalize();
        let anchor_factor = (-d as f32) / norm_sq;
        let center = normal * anchor_factor;
        let reference = if n.cross(glam::Vec3::Y).length_squared() > 1e-6 {
            glam::Vec3::Y
        } else {
            glam::Vec3::X
        };
        let u = n.cross(reference).normalize();
        let v = n.cross(u).normalize();
        let half = (camera.distance * 1.25).max(6.0);

        let p0 = Point3D::from_vec3(center + (-u - v) * half);
        let p1 = Point3D::from_vec3(center + (u - v) * half);
        let p2 = Point3D::from_vec3(center + (u + v) * half);
        let p3 = Point3D::from_vec3(center + (-u + v) * half);
        let fill = Color::new(color.r, color.g, color.b, opacity.clamp(0.0, 1.0));

        Self::add_solid_triangle_3d(
            vertices, indices, camera, &p0, n, &p1, n, &p2, n, fill, screen_w, screen_h,
        );
        Self::add_solid_triangle_3d(
            vertices, indices, camera, &p0, n, &p2, n, &p3, n, fill, screen_w, screen_h,
        );
        Self::add_line_3d(
            vertices, indices, camera, &p0, &p1, 1.5, color, screen_w, screen_h,
        );
        Self::add_line_3d(
            vertices, indices, camera, &p1, &p2, 1.5, color, screen_w, screen_h,
        );
        Self::add_line_3d(
            vertices, indices, camera, &p2, &p3, 1.5, color, screen_w, screen_h,
        );
        Self::add_line_3d(
            vertices, indices, camera, &p3, &p0, 1.5, color, screen_w, screen_h,
        );
    }

    #[allow(clippy::only_used_in_recursion)]
    fn build_single_geometry(
        &self,
        document: &Document,
        obj: &GeoObject,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view_transform: &grafito_geometry::types::ViewTransform,
        include_overlays: bool,
        dark_mode: bool,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
        fractals_complete: &mut bool,
        homotopy_time: f64,
    ) {
        if !obj.is_visible() {
            return;
        }
        if !include_overlays && !gpu_2d_base_owns(document, obj) {
            return;
        }
        match obj {
            GeoObject::Point(p) if include_overlays => {
                let screen = view_transform.world_to_screen(p.position);
                let size = p.size.max(1.0);
                Self::add_rect(vertices, indices, screen, size, size, p.color);
            }
            GeoObject::Line(l) => {
                let start = Point2::new(
                    document.resolve_expr(&l.start_x_expr, l.start.x),
                    document.resolve_expr(&l.start_y_expr, l.start.y),
                );
                let end = Point2::new(
                    document.resolve_expr(&l.end_x_expr, l.end.x),
                    document.resolve_expr(&l.end_y_expr, l.end.y),
                );
                let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                let world_br = view_transform.screen_to_world(view_transform.screen_size);
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );
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
                    let a = view_transform.world_to_screen(clip_start);
                    let b = view_transform.world_to_screen(clip_end);
                    Self::add_line_segment(vertices, indices, a, b, l.width, l.color);
                }
            }
            GeoObject::Circle(c) => {
                let screen_center = view_transform.world_to_screen(c.center);
                let radius = (c.radius as f32) * (view_transform.scale as f32);
                Self::add_circle_stroke(vertices, indices, screen_center, radius, c.width, c.color);
                if let Some(fill) = c.fill_color {
                    Self::add_circle_fill(vertices, indices, screen_center, radius, fill);
                }
            }
            GeoObject::Polygon(poly) if polygon_geometry_is_within_limit(poly.vertices.len()) => {
                let mut screen_verts = Vec::with_capacity(poly.vertices.len());
                for (i, v) in poly.vertices.iter().enumerate() {
                    let x = document.resolve_expr(poly.x_exprs.get(i).unwrap_or(&None), v.x);
                    let y = document.resolve_expr(poly.y_exprs.get(i).unwrap_or(&None), v.y);
                    screen_verts.push(view_transform.world_to_screen(Point2::new(x, y)));
                }
                if let Some(fill) = poly.fill_color {
                    Self::add_polygon_fill(vertices, indices, &screen_verts, fill, view_transform);
                }
                Self::add_polygon_stroke(vertices, indices, &screen_verts, poly.width, poly.color);
            }
            GeoObject::Pencil(pencil) if pencil.points.len() >= 2 => {
                let world_tl = view_transform.screen_to_world(glam::Vec2::new(0.0, 0.0));
                let world_br = view_transform.screen_to_world(view_transform.screen_size);
                let view_bounds = grafito_geometry::AABB::new(
                    Point2::new(world_tl.x.min(world_br.x), world_tl.y.min(world_br.y)),
                    Point2::new(world_tl.x.max(world_br.x), world_tl.y.max(world_br.y)),
                );
                for w in pencil.points.windows(2) {
                    let a = w[0];
                    let b = w[1];
                    if let Some((clip_a, clip_b)) =
                        grafito_geometry::clip_segment_to_rect(a, b, view_bounds)
                    {
                        let sa = view_transform.world_to_screen(clip_a);
                        let sb = view_transform.world_to_screen(clip_b);
                        Self::add_line_segment(
                            vertices,
                            indices,
                            sa,
                            sb,
                            pencil.width,
                            pencil.color,
                        );
                    }
                }
            }
            GeoObject::Function(fun) => {
                Self::add_function_geometry(vertices, indices, document, view_transform, fun);
            }
            GeoObject::Ellipse(el) => {
                let n = 64;
                let mut pts = Vec::with_capacity(n);
                for i in 0..n {
                    let t = i as f64 / n as f64 * std::f64::consts::TAU;
                    let x = el.center.x + el.rx * t.cos() * el.angle.cos()
                        - el.ry * t.sin() * el.angle.sin();
                    let y = el.center.y
                        + el.rx * t.cos() * el.angle.sin()
                        + el.ry * t.sin() * el.angle.cos();
                    let s = view_transform.world_to_screen(Point2::new(x, y));
                    pts.push(s);
                }
                if let Some(fill) = el.fill_color {
                    Self::add_polygon_fill(vertices, indices, &pts, fill, view_transform);
                }
                Self::add_polygon_stroke(vertices, indices, &pts, el.width, el.color);
            }
            GeoObject::Parabola(pb) => {
                if !pb.p.is_finite() || pb.p.abs() < 1e-12 {
                    return;
                }
                let steps = 128;
                let range = (20.0 / view_transform.scale).clamp(0.1, 500.0);
                let cos_a = pb.angle.cos();
                let sin_a = pb.angle.sin();
                let mut prev: Option<glam::Vec2> = None;
                for i in 0..=steps {
                    let t = -range + 2.0 * range * i as f64 / steps as f64;
                    let lx = t;
                    let ly = t * t / (4.0 * pb.p);
                    let wx = pb.vertex.x + lx * cos_a - ly * sin_a;
                    let wy = pb.vertex.y + lx * sin_a + ly * cos_a;
                    if wx.is_finite() && wy.is_finite() {
                        let s = view_transform.world_to_screen(Point2::new(wx, wy));
                        if let Some(prev_p) = prev {
                            if (s.x - prev_p.x).abs() < 300.0 {
                                Self::add_line_segment(
                                    vertices, indices, prev_p, s, pb.width, pb.color,
                                );
                            }
                        }
                        prev = Some(s);
                    }
                }
            }
            GeoObject::Hyperbola(hb) => {
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
                    let mut prev: Option<glam::Vec2> = None;
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
                            let s = view_transform.world_to_screen(Point2::new(wx, wy));
                            if let Some(prev_p) = prev {
                                if (s.x - prev_p.x).abs() < 300.0 {
                                    Self::add_line_segment(
                                        vertices, indices, prev_p, s, hb.width, hb.color,
                                    );
                                }
                            }
                            prev = Some(s);
                        }
                    }
                }
            }
            GeoObject::Text(txt) => {
                let screen = view_transform.world_to_screen(txt.position);
                Self::add_text_screen(
                    vertices,
                    indices,
                    &txt.content,
                    glam::Vec2::new(screen.x, screen.y),
                    txt.font_size,
                    txt.color,
                );
            }
            GeoObject::ComplexIntegral(integral) => {
                let Some(target_obj) = document.get_object(integral.target) else {
                    return;
                };
                let Ok(parsed) = grafito_complex::math::complex_expr::parse(&integral.expr) else {
                    return;
                };

                let mut path = Vec::new();
                match target_obj {
                    GeoObject::Polygon(p) => {
                        for pt in &p.vertices {
                            path.push(num_complex::Complex64::new(pt.x, pt.y));
                        }
                        if let Some(first) = p.vertices.first() {
                            path.push(num_complex::Complex64::new(first.x, first.y));
                        }
                    }
                    GeoObject::Circle(c) => {
                        let n = 256;
                        for i in 0..=n {
                            let a = i as f64 * std::f64::consts::TAU / n as f64;
                            path.push(num_complex::Complex64::new(
                                c.center.x + c.radius * a.cos(),
                                c.center.y + c.radius * a.sin(),
                            ));
                        }
                    }
                    GeoObject::Line(l) => {
                        path.push(num_complex::Complex64::new(l.start.x, l.start.y));
                        path.push(num_complex::Complex64::new(l.end.x, l.end.y));
                    }
                    _ => {}
                }

                if path.len() >= 2 {
                    let symbol = document.complex_base_symbol.clone();
                    let mut vars = std::collections::HashMap::new();
                    for (k, v) in &document.variables {
                        vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                    }

                    let result = if integral.compute_residue {
                        grafito_complex::complex_calculus::sum_of_residues(
                            &parsed, &path, &vars, &symbol,
                        )
                    } else {
                        grafito_complex::complex_calculus::contour_integral(
                            &parsed, &path, &vars, &symbol,
                        )
                    };

                    if let Ok(res) = result {
                        let center = path
                            .iter()
                            .fold(num_complex::Complex64::new(0.0, 0.0), |acc, z| acc + z)
                            / path.len() as f64;
                        let screen =
                            view_transform.world_to_screen(Point2::new(center.re, center.im));
                        let text = format!("{:.3} + {:.3}i", res.re, res.im);
                        Self::add_text_screen(
                            vertices,
                            indices,
                            &text,
                            glam::Vec2::new(screen.x, screen.y),
                            16.0,
                            integral.color,
                        );
                    }
                }
            }
            GeoObject::ParametricCurve2D(pc) => {
                let steps = 4000;
                let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                    pc,
                    steps,
                    &document.variables,
                );
                let mut prev: Option<[f32; 2]> = None;
                for &(x, y) in samples.iter() {
                    if x.is_finite() && y.is_finite() {
                        let s = view_transform.world_to_screen(Point2::new(x, y));
                        if let Some(p) = prev {
                            if (s.x - p[0]).abs() < 300.0 && (s.y - p[1]).abs() < 300.0 {
                                Self::add_line_segment(
                                    vertices,
                                    indices,
                                    glam::Vec2::new(p[0], p[1]),
                                    s,
                                    pc.width,
                                    pc.color,
                                );
                            }
                        }
                        prev = Some([s.x, s.y]);
                    } else {
                        prev = None;
                    }
                }
            }
            GeoObject::PolarCurve(pol) => {
                let steps = 4000;
                let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
                    pol,
                    steps,
                    &document.variables,
                );
                let mut prev: Option<[f32; 2]> = None;
                for &(x, y) in samples.iter() {
                    if x.is_finite() && y.is_finite() {
                        let s = view_transform.world_to_screen(Point2::new(x, y));
                        if let Some(p) = prev {
                            if (s.x - p[0]).abs() < 300.0 && (s.y - p[1]).abs() < 300.0 {
                                Self::add_line_segment(
                                    vertices,
                                    indices,
                                    glam::Vec2::new(p[0], p[1]),
                                    s,
                                    pol.width,
                                    pol.color,
                                );
                            }
                        }
                        prev = Some([s.x, s.y]);
                    } else {
                        prev = None;
                    }
                }
            }
            GeoObject::ImplicitCurve(_) => {
                // El render del ImplicitCurve se hace por CPU (ver
                // `render_2d.rs::draw_object_styled` → brazo
                // `GeoObject::ImplicitCurve`). Esto evita el doble render
                // y los problemas de offset que ocurrían cuando GPU y CPU
                // dibujaban el mismo objeto en sistemas de coordenadas
                // distintos. La GPU sigue acelerando el cómputo de
                // marching squares vía `implicit_compute`.
            }
            GeoObject::Histogram(h) => {
                let bins = grafito_geometry::statistics::histogram(&h.data, h.bins);
                let max_count = bins.iter().map(|(_, _, c)| *c).fold(0.0f64, f64::max);
                if max_count > 0.0 && !bins.is_empty() {
                    let y_scale = (h.y_max - h.y_min) / max_count;
                    for (left, right, count) in &bins {
                        let bar_h = h.y_min + count * y_scale;
                        let bl = view_transform.world_to_screen(Point2::new(*left, h.y_min));
                        let tr = view_transform.world_to_screen(Point2::new(*right, bar_h));
                        let w = tr.x - bl.x;
                        let h_bar = tr.y - bl.y;
                        Self::add_rect(vertices, indices, bl, w, h_bar, h.color);
                    }
                }
            }
            GeoObject::ScatterPlot(sp) => {
                for (x, y) in sp.xs.iter().zip(sp.ys.iter()) {
                    let s = view_transform.world_to_screen(Point2::new(*x, *y));
                    Self::add_rect(vertices, indices, s, sp.point_size, sp.point_size, sp.color);
                }
            }
            GeoObject::BoxPlot(bp) => {
                if let Some((min, q1, _med, q3, max, outliers)) =
                    grafito_geometry::statistics::boxplot_stats(&bp.data)
                {
                    let half_w = bp.width_box * 0.5;
                    let x = view_transform
                        .world_to_screen(Point2::new(bp.position, 0.0))
                        .x;
                    let y_min = view_transform.world_to_screen(Point2::new(0.0, min)).y;
                    let y_q1 = view_transform.world_to_screen(Point2::new(0.0, q1)).y;
                    let y_q3 = view_transform.world_to_screen(Point2::new(0.0, q3)).y;
                    let y_max = view_transform.world_to_screen(Point2::new(0.0, max)).y;
                    let hw = (half_w * view_transform.scale) as f32;
                    let bx = x - hw;
                    let bw = hw * 2.0;
                    Self::add_rect(
                        vertices,
                        indices,
                        glam::Vec2::new(bx, y_q3),
                        bw,
                        (y_q1 - y_q3).abs(),
                        bp.color,
                    );
                    Self::add_line_segment(
                        vertices,
                        indices,
                        glam::Vec2::new(x, y_q3),
                        glam::Vec2::new(x, y_max),
                        bp.width,
                        bp.color,
                    );
                    Self::add_line_segment(
                        vertices,
                        indices,
                        glam::Vec2::new(x, y_q1),
                        glam::Vec2::new(x, y_min),
                        bp.width,
                        bp.color,
                    );
                    for o in &outliers {
                        let oy = view_transform.world_to_screen(Point2::new(0.0, *o)).y;
                        Self::add_rect(
                            vertices,
                            indices,
                            glam::Vec2::new(x - 2.0, oy - 2.0),
                            4.0,
                            4.0,
                            bp.color,
                        );
                    }
                }
            }
            GeoObject::RegressionLine(rl) => {
                let x_min = rl.xs.iter().cloned().fold(f64::INFINITY, f64::min);
                let x_max = rl.xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let y1 = rl.slope * x_min + rl.intercept;
                let y2 = rl.slope * x_max + rl.intercept;
                let a = view_transform.world_to_screen(Point2::new(x_min, y1));
                let b = view_transform.world_to_screen(Point2::new(x_max, y2));
                Self::add_line_segment(vertices, indices, a, b, rl.width, rl.color);
                for (x, y) in rl.xs.iter().zip(rl.ys.iter()) {
                    let s = view_transform.world_to_screen(Point2::new(*x, *y));
                    Self::add_rect(
                        vertices,
                        indices,
                        s,
                        4.0,
                        4.0,
                        Color::new(0.0, 0.0, 1.0, 1.0),
                    );
                }
            }
            GeoObject::Fractal2D(fr)
                if !Self::add_fractal_geometry(vertices, indices, view_transform, fr) =>
            {
                *fractals_complete = false;
            }
            // The app's CPU overlay owns 2D vector fields so arrowheads and RK4
            // streamlines stay identical in CPU and GPU canvas modes.
            GeoObject::VectorField2D(_) => {}
            GeoObject::ComplexGrid(cg) => {
                self.add_complex_grid_geometry_gpu(
                    vertices,
                    indices,
                    document,
                    view_transform,
                    cg,
                    device,
                    queue,
                );
            }
            GeoObject::ComplexMapping(cm) => {
                Self::add_complex_mapping_geometry(
                    vertices,
                    indices,
                    document,
                    view_transform,
                    cm,
                    homotopy_time,
                );
            }
            GeoObject::PhasePortrait(pp) => {
                for (start, end) in sample_phase_portrait(pp, &document.variables) {
                    let start = view_transform.world_to_screen(start);
                    let end = view_transform.world_to_screen(end);
                    if start.is_finite() && end.is_finite() {
                        Self::add_line_segment(vertices, indices, start, end, 1.5, pp.color);
                    }
                }
            }

            GeoObject::Transformed(t) => {
                let start_v = vertices.len();
                self.build_single_geometry(
                    document,
                    &t.inner,
                    vertices,
                    indices,
                    view_transform,
                    true,
                    dark_mode,
                    device,
                    queue,
                    fractals_complete,
                    homotopy_time,
                );
                if let Ok(ast) = grafito_complex::math::complex_expr::parse(&t.complex_expr) {
                    let mut points = Vec::with_capacity(vertices.len() - start_v);
                    for v in &vertices[start_v..] {
                        let world_p = view_transform
                            .screen_to_world(glam::Vec2::new(v.position[0], v.position[1]));
                        points.push(world_p);
                    }

                    let gpu_result = if let (Some(device), Some(queue), Some(compute)) =
                        (device, queue, &self.complex_compute)
                    {
                        compute.evaluate(device, queue, &ast, &points, &document.variables)
                    } else {
                        None
                    };

                    if let Some(transformed_points) =
                        gpu_result.filter(|points| points.len() == vertices.len() - start_v)
                    {
                        for (i, v) in vertices[start_v..].iter_mut().enumerate() {
                            let tp = transformed_points[i];
                            let screen = bounded_screen_point(
                                view_transform,
                                grafito_geometry::Point2::new(tp.x, tp.y),
                            );
                            v.position = screen
                                .map_or([f32::NAN, f32::NAN, 0.0], |point| [point.x, point.y, 0.0]);
                        }
                    } else {
                        let _ = apply_complex_transform_cpu(
                            &mut vertices[start_v..],
                            view_transform,
                            &t.complex_expr,
                            &document.variables,
                            document.complex_base_symbol.as_str(),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    pub fn build_geometry(
        &self,
        document: &Document,
        dark_mode: bool,
        include_overlays: bool,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let (vertices, indices, _) = self.build_geometry_with_fractal_status(
            document,
            dark_mode,
            include_overlays,
            device,
            queue,
        );
        (vertices, indices)
    }

    /// Construye la escena 2D y señala si toda la geometría visible pudo
    /// materializarse dentro de los presupuestos de cálculo y almacenamiento.
    pub fn build_geometry_with_fractal_status(
        &self,
        document: &Document,
        dark_mode: bool,
        include_overlays: bool,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
    ) -> (Vec<Vertex>, Vec<u32>, bool) {
        self.build_geometry_with_fractal_status_at(
            document,
            dark_mode,
            include_overlays,
            device,
            queue,
            0.0,
        )
    }

    /// Igual que [`Self::build_geometry_with_fractal_status`], usando tiempo
    /// transitorio de homotopía que nunca forma parte del documento persistido.
    pub fn build_geometry_with_fractal_status_at(
        &self,
        document: &Document,
        dark_mode: bool,
        include_overlays: bool,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
        homotopy_time: f64,
    ) -> (Vec<Vertex>, Vec<u32>, bool) {
        let (vertices, indices, _, scene_complete) = self.build_geometry_with_object_ranges_at(
            document,
            dark_mode,
            include_overlays,
            device,
            queue,
            homotopy_time,
        );
        (vertices, indices, scene_complete)
    }

    /// Construye una escena y conserva el rango de índices de cada objeto.
    /// Los rangos permiten intercalar callbacks GPU y formas CPU sin perder el
    /// orden visual global `(SceneLayer2D, ObjectId)`.
    pub fn build_geometry_with_object_ranges_at(
        &self,
        document: &Document,
        dark_mode: bool,
        include_overlays: bool,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
        homotopy_time: f64,
    ) -> (Vec<Vertex>, Vec<u32>, BTreeMap<ObjectId, Range<u32>>, bool) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut object_ranges = BTreeMap::new();
        let mut fractals_complete = true;

        let view_transform = *document.view();

        if include_overlays {
            self.build_grid(&mut vertices, &mut indices, &view_transform, dark_mode);
            self.build_axes(&mut vertices, &mut indices, &view_transform, dark_mode);
        }

        for (id, obj) in ordered_visible_2d_objects(document) {
            let index_start = indices.len();
            self.build_single_geometry(
                document,
                obj,
                &mut vertices,
                &mut indices,
                &view_transform,
                include_overlays,
                dark_mode,
                device,
                queue,
                &mut fractals_complete,
                homotopy_time,
            );
            if indices.len() > index_start {
                let range = u32::try_from(index_start)
                    .ok()
                    .zip(u32::try_from(indices.len()).ok())
                    .map(|(start, end)| start..end);
                if let Some(range) = range {
                    object_ranges.insert(id, range);
                }
            }
        }

        // Primitive builders reserve in quads/triangles. If no smallest quad
        // fits, a later object may already have been truncated at the cap;
        // conservatively keep the CPU fallback for this scene.
        let scene_complete =
            fractals_complete && can_append_geometry(vertices.len(), indices.len(), 4, 6);
        (vertices, indices, object_ranges, scene_complete)
    }

    fn build_grid(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let pixels_per_unit = view.scale;
        let target_world_step = 120.0 / pixels_per_unit.max(1e-50);
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
        let mut min_y = (world_br.y / major_step).floor() as i64 - 1;
        let mut max_y = (world_tl.y / major_step).ceil() as i64 + 1;

        // Safety limit to prevent freezing due to massive zoom / floating point precision loss
        if max_x.saturating_sub(min_x) > 500 {
            let center = (min_x + max_x) / 2;
            min_x = center - 250;
            max_x = center + 250;
        }
        if max_y.saturating_sub(min_y) > 500 {
            let center = (min_y + max_y) / 2;
            min_y = center - 250;
            max_y = center + 250;
        }

        let color = if dark_mode {
            Color::new(0.25, 0.25, 0.25, 1.0)
        } else {
            Color::LIGHT_GRAY
        };

        let precision = if major_step >= 1.0 {
            0
        } else if major_step >= 0.1 {
            1
        } else if major_step >= 0.01 {
            2
        } else {
            4
        };

        let format_num = |v: f64| -> String {
            if v.abs() < 1e-9 {
                return "0".to_string();
            }
            let s = format!("{:.*}", precision, v);
            let s = if s.contains('.') {
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            } else {
                s
            };
            if s.is_empty() || s == "-" {
                "0".to_string()
            } else {
                s
            }
        };

        for xi in min_x..=max_x {
            if xi == 0 {
                continue;
            }
            let x = xi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(x, min_y as f64 * major_step));
            let b = view.world_to_screen(Point2::new(x, max_y as f64 * major_step));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);

            // Draw text label on X axis
            let pos = view.world_to_screen(Point2::new(x, 0.0));
            Self::add_text_screen(
                vertices,
                indices,
                &format_num(x),
                pos + glam::Vec2::new(-8.0, 5.0),
                12.0,
                color,
            );
        }

        for yi in min_y..=max_y {
            if yi == 0 {
                continue;
            }
            let y = yi as f64 * major_step;
            let a = view.world_to_screen(Point2::new(min_x as f64 * major_step, y));
            let b = view.world_to_screen(Point2::new(max_x as f64 * major_step, y));
            Self::add_line_segment(vertices, indices, a, b, 1.0, color);

            // Draw text label on Y axis
            let pos = view.world_to_screen(Point2::new(0.0, y));
            Self::add_text_screen(
                vertices,
                indices,
                &format_num(y),
                pos + glam::Vec2::new(5.0, -8.0),
                12.0,
                color,
            );
        }
    }

    fn build_axes(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        view: &ViewTransform,
        dark_mode: bool,
    ) {
        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);

        let x_axis_y = 0.0f64.clamp(world_br.y, world_tl.y);
        let y_axis_x = 0.0f64.clamp(world_tl.x, world_br.x);

        let axis_color = if dark_mode {
            Color::new(0.7, 0.7, 0.7, 1.0)
        } else {
            Color::BLACK
        };

        let x_axis_a = view.world_to_screen(Point2::new(world_tl.x, x_axis_y));
        let x_axis_b = view.world_to_screen(Point2::new(world_br.x, x_axis_y));
        Self::add_line_segment(vertices, indices, x_axis_a, x_axis_b, 2.0, axis_color);

        let y_axis_a = view.world_to_screen(Point2::new(y_axis_x, world_br.y));
        let y_axis_b = view.world_to_screen(Point2::new(y_axis_x, world_tl.y));
        Self::add_line_segment(vertices, indices, y_axis_a, y_axis_b, 2.0, axis_color);
    }

    /// Versión de instancia de `add_complex_grid_geometry` que usa GPU compute
    /// para la coloración de dominio cuando hay device/queue disponibles.
    /// Si la GPU no está disponible o la expresión no se puede compilar, cae
    /// al path CPU (método estático).
    pub fn add_complex_grid_geometry_gpu(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view: &ViewTransform,
        cg: &ComplexGridObj,
        device: Option<&wgpu::Device>,
        queue: Option<&wgpu::Queue>,
    ) {
        if cg.x_max <= cg.x_min || cg.y_max <= cg.y_min {
            return;
        }

        // Solo usar GPU para domain coloring (render_mode == 1)
        if cg.render_mode == 1 {
            if let (Some(device), Some(queue), Some(dc_pipeline)) =
                (device, queue, &self.domain_coloring_compute)
            {
                let res = complex_grid_resolution(cg, document.render_quality);
                let dx = (cg.x_max - cg.x_min) / res as f64;
                let dy = (cg.y_max - cg.y_min) / res as f64;

                let Ok(parsed) = grafito_complex::complex_expr::parse(&cg.expr) else {
                    return;
                };

                // Construir lista de puntos centrales de celdas
                let mut points: Vec<(f64, f64)> = Vec::with_capacity(res * res);
                for i in 0..res {
                    for j in 0..res {
                        let x = cg.x_min + (i as f64 + 0.5) * dx;
                        let y = cg.y_min + (j as f64 + 0.5) * dy;
                        points.push((x, y));
                    }
                }

                let vars = std::collections::HashMap::new();
                if let Some(colors) = dc_pipeline.evaluate(
                    device,
                    queue,
                    &parsed,
                    &points,
                    &vars,
                    cg.domain_coloring_mode as u32,
                ) {
                    // Generar rectángulos con los colores del GPU
                    for (idx, color) in colors.iter().enumerate() {
                        let Some((i, j)) = row_major_cell_coordinates(idx, res) else {
                            break;
                        };
                        let x = cg.x_min + i as f64 * dx;
                        let y = cg.y_min + j as f64 * dy;
                        let center = view.world_to_screen(Point2::new(x + dx * 0.5, y + dy * 0.5));
                        let c = Color::new(color[0], color[1], color[2], color[3]);
                        Self::add_rect(
                            vertices,
                            indices,
                            center,
                            (dx * view.scale).abs().max(1.0) as f32,
                            (dy * view.scale).abs().max(1.0) as f32,
                            c,
                        );
                    }
                    return;
                }
            }
        }

        // Fallback al path CPU: este path no debe generar 250k `rect_filled` bloqueantes.
        // El canvas pasa `None, None` (ver `canvas.rs` — `MAX_SYNC_GPU_COMPUTE_ATTEMPTS=1`)
        // para forzar el fallback determinista y evitar `device.poll(Wait)`. El path CPU
        // de domain_coloring está acotado por `complex_grid_resolution` (Preview 64,
        // Normal 200, High 300) y, para celdas grandes, el `render_2d` usa textura
        // en lugar de 250k rects (ver `draw_object_styled` domain coloring).
        // Si la resolución supera el presupuesto de geometría, se omite la malla
        // y se deja la textura como owner (evita OOM y 250k draws).
        if cg.render_mode == 1 {
            let res = complex_grid_resolution(cg, document.render_quality);
            if res * res > 65_536 {
                // Evita 250k rect_filled en CPU: usa textura en `render_2d`
                return;
            }
        }
        Self::add_complex_grid_geometry(vertices, indices, document, view, cg);
    }

    /// Añade geometría para `ComplexGrid` respetando su modo de render (CPU path).
    ///
    /// La coloración de dominio y el heatmap siguen siendo geometría de CPU
    /// porque el pipeline 2D actual sólo dibuja vértices coloreados. Durante
    /// pan/zoom se limita la resolución con `RenderQuality::Preview` para no
    /// reconstruir cientos de miles de vértices por frame.
    pub fn add_complex_grid_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view: &ViewTransform,
        cg: &ComplexGridObj,
    ) {
        if cg.x_max <= cg.x_min || cg.y_max <= cg.y_min {
            return;
        }

        let res = complex_grid_resolution(cg, document.render_quality);
        let dx = (cg.x_max - cg.x_min) / res as f64;
        let dy = (cg.y_max - cg.y_min) / res as f64;

        match cg.render_mode {
            1 => {
                let Ok(parsed) = grafito_complex::complex_expr::parse(&cg.expr) else {
                    return;
                };
                let symbol = document.complex_base_symbol.clone();
                let mut vars = std::collections::HashMap::new();
                for (k, v) in &document.variables {
                    vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                }
                vars.insert(symbol.clone(), num_complex::Complex64::default());

                for i in 0..res {
                    let x = cg.x_min + i as f64 * dx;
                    for j in 0..res {
                        let y = cg.y_min + j as f64 * dy;
                        if let Some(z_val) = vars.get_mut(&symbol) {
                            *z_val = num_complex::Complex64::new(x, y);
                        }
                        if let Ok(fz) = parsed.eval(&vars) {
                            if fz.re.is_finite() && fz.im.is_finite() {
                                let mag = (fz.re * fz.re + fz.im * fz.im).sqrt();
                                let ang = fz.im.atan2(fz.re);
                                let hue =
                                    (ang + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
                                let mut lightness = 0.5;
                                if cg.domain_coloring_mode == 0
                                    || cg.domain_coloring_mode == 2
                                    || cg.domain_coloring_mode == 3
                                {
                                    lightness = (mag.max(1e-10).ln().atan()
                                        / std::f64::consts::FRAC_PI_2)
                                        * 0.5
                                        + 0.5;
                                }
                                let sat = if cg.domain_coloring_mode == 1 {
                                    1.0
                                } else {
                                    0.85
                                };
                                let mut color = hsl_to_rgb_f64(hue, sat, lightness.clamp(0.0, 1.0));

                                if cg.domain_coloring_mode == 2 {
                                    let log_mag = mag.max(1e-5).ln();
                                    let mag_grid =
                                        (log_mag * std::f64::consts::PI * 2.0).sin().abs();
                                    let arg_grid = (ang * 10.0).sin().abs();
                                    let grid_shading =
                                        0.5 + 0.5 * (mag_grid * arg_grid).max(0.0).powf(0.15);
                                    color.r *= grid_shading as f32;
                                    color.g *= grid_shading as f32;
                                    color.b *= grid_shading as f32;
                                } else if cg.domain_coloring_mode == 3 {
                                    let grid_re = (fz.re * std::f64::consts::PI * 2.0).sin().abs();
                                    let grid_im = (fz.im * std::f64::consts::PI * 2.0).sin().abs();
                                    let grid_shading =
                                        0.5 + 0.5 * (grid_re * grid_im).max(0.0).powf(0.15);
                                    color.r *= grid_shading as f32;
                                    color.g *= grid_shading as f32;
                                    color.b *= grid_shading as f32;
                                }

                                let center =
                                    view.world_to_screen(Point2::new(x + dx * 0.5, y + dy * 0.5));
                                Self::add_rect(
                                    vertices,
                                    indices,
                                    center,
                                    (dx * view.scale).abs().max(1.0) as f32,
                                    (dy * view.scale).abs().max(1.0) as f32,
                                    color,
                                );
                            }
                        }
                    }
                }
            }
            2 => {
                let Ok(ast) = grafito_geometry::expr::prepare_function_ast(
                    &cg.expr,
                    &document.variables,
                    &["x", "y"],
                ) else {
                    return;
                };
                for i in 0..res {
                    let x = cg.x_min + i as f64 * dx;
                    for j in 0..res {
                        let y = cg.y_min + j as f64 * dy;
                        let val = ast.eval_2d("x", x, "y", y);
                        if val.is_finite() {
                            let t = (val.atan() / std::f64::consts::FRAC_PI_2).clamp(-1.0, 1.0);
                            let color = thermal_colormap((t + 1.0) * 0.5);
                            let center =
                                view.world_to_screen(Point2::new(x + dx * 0.5, y + dy * 0.5));
                            Self::add_rect(
                                vertices,
                                indices,
                                center,
                                (dx * view.scale).abs().max(1.0) as f32,
                                (dy * view.scale).abs().max(1.0) as f32,
                                color,
                            );
                        }
                    }
                }
            }
            3 => {
                let Ok(parsed) = grafito_complex::complex_expr::parse(&cg.expr) else {
                    return;
                };
                let symbol = document.complex_base_symbol.clone();
                let mut vars = std::collections::HashMap::new();
                for (k, v) in &document.variables {
                    vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                }
                vars.insert(symbol.clone(), num_complex::Complex64::default());

                let dx = (cg.x_max - cg.x_min) / res as f64;
                let dy = (cg.y_max - cg.y_min) / res as f64;

                for i in 0..=res {
                    let x = cg.x_min + i as f64 * dx;
                    for j in 0..=res {
                        let y = cg.y_min + j as f64 * dy;
                        if let Ok((u, v)) = grafito_complex::complex_calculus::evaluate_flow(
                            &parsed, x, y, &vars, &symbol,
                        ) {
                            if u.is_finite() && v.is_finite() {
                                let mag = (u * u + v * v).sqrt();
                                if mag > 0.001 {
                                    let start = Point2::new(x, y);
                                    let end =
                                        Point2::new(x + u / mag * 0.4 * dx, y + v / mag * 0.4 * dy);
                                    let s = view.world_to_screen(start);
                                    let e = view.world_to_screen(end);
                                    Self::add_line_segment(vertices, indices, s, e, 1.5, cg.color);
                                }
                            }
                        }
                    }
                }
            }
            4 => {
                // Cuadrantes: colorea las 4 regiones del plano complejo
                let colors = [
                    Color::new(0.8, 0.2, 0.2, 0.3),
                    Color::new(0.2, 0.8, 0.2, 0.3),
                    Color::new(0.2, 0.2, 0.8, 0.3),
                    Color::new(0.8, 0.8, 0.2, 0.3),
                ];
                let q_colors = [
                    Color::new(0.8, 0.2, 0.2, 0.8),
                    Color::new(0.2, 0.8, 0.2, 0.8),
                    Color::new(0.2, 0.2, 0.8, 0.8),
                    Color::new(0.8, 0.8, 0.2, 0.8),
                ];
                let labels = ["Q1", "Q2", "Q3", "Q4"];

                for i in 0..res {
                    let x = cg.x_min + i as f64 * dx;
                    for j in 0..res {
                        let y = cg.y_min + j as f64 * dy;
                        let quadrant = if x >= 0.0 && y >= 0.0 {
                            0
                        } else if x < 0.0 && y >= 0.0 {
                            1
                        } else if x < 0.0 && y < 0.0 {
                            2
                        } else {
                            3
                        };
                        let center = view.world_to_screen(Point2::new(x + dx * 0.5, y + dy * 0.5));
                        Self::add_rect(
                            vertices,
                            indices,
                            center,
                            (dx * view.scale).abs().max(1.0) as f32,
                            (dy * view.scale).abs().max(1.0) as f32,
                            colors[quadrant],
                        );
                    }
                }

                let cx = (cg.x_min + cg.x_max) * 0.5;
                let cy = (cg.y_min + cg.y_max) * 0.5;
                let label_offsets = [
                    (cx * 0.5, cy * 0.5),
                    (-cx.abs() * 0.5, cy * 0.5),
                    (-cx.abs() * 0.5, -cy.abs() * 0.5),
                    (cx * 0.5, -cy.abs() * 0.5),
                ];
                for (q, (lx, ly)) in label_offsets.iter().enumerate() {
                    let pos = view.world_to_screen(Point2::new(*lx, *ly));
                    Self::add_text_screen(vertices, indices, labels[q], pos, 16.0, q_colors[q]);
                }
                let pos_re = view.world_to_screen(Point2::new(cx, 0.0));
                Self::add_text_screen(
                    vertices,
                    indices,
                    "+Re",
                    pos_re + glam::Vec2::new(5.0, -15.0),
                    12.0,
                    q_colors[0],
                );
                let pos_im = view.world_to_screen(Point2::new(0.0, cy));
                Self::add_text_screen(
                    vertices,
                    indices,
                    "+Im",
                    pos_im + glam::Vec2::new(5.0, 0.0),
                    12.0,
                    q_colors[0],
                );
            }
            _ => {
                let Ok(parsed) = grafito_complex::complex_expr::parse(&cg.expr) else {
                    return;
                };
                let symbol = document.complex_base_symbol.clone();
                let mut vars = std::collections::HashMap::new();
                for (k, v) in &document.variables {
                    vars.insert(k.clone(), num_complex::Complex64::new(*v, 0.0));
                }
                vars.insert(symbol.clone(), num_complex::Complex64::default());

                let grid_lines = res;
                let samples_per_cell = match document.render_quality {
                    RenderQuality::Preview => 1,
                    RenderQuality::Normal => 4,
                    RenderQuality::High => 6,
                };
                let steps = grid_lines * samples_per_cell;
                let draw_sample =
                    |z: num_complex::Complex64,
                     vars: &mut std::collections::HashMap<String, num_complex::Complex64>,
                     parsed: &grafito_complex::complex_expr::ComplexExpr|
                     -> Option<glam::Vec2> {
                        if let Some(z_val) = vars.get_mut(&symbol) {
                            *z_val = z;
                        }
                        let result = parsed.eval(vars).ok()?;
                        if result.re.is_finite()
                            && result.im.is_finite()
                            && result.re.abs() < 1e6
                            && result.im.abs() < 1e6
                        {
                            Some(view.world_to_screen(Point2::new(result.re, result.im)))
                        } else {
                            None
                        }
                    };

                for j in 0..=grid_lines {
                    let y = cg.y_min + j as f64 * dy;
                    let mut prev: Option<glam::Vec2> = None;
                    for s in 0..=steps {
                        let x = cg.x_min + s as f64 / steps as f64 * (cg.x_max - cg.x_min);
                        let current =
                            draw_sample(num_complex::Complex64::new(x, y), &mut vars, &parsed);
                        if let (Some(a), Some(b)) = (prev, current) {
                            if (b.x - a.x).abs() < 500.0 && (b.y - a.y).abs() < 500.0 {
                                Self::add_line_segment(vertices, indices, a, b, 1.0, cg.color);
                            }
                        }
                        prev = current;
                    }
                }

                for i in 0..=grid_lines {
                    let x = cg.x_min + i as f64 * dx;
                    let mut prev: Option<glam::Vec2> = None;
                    for s in 0..=steps {
                        let y = cg.y_min + s as f64 / steps as f64 * (cg.y_max - cg.y_min);
                        let current =
                            draw_sample(num_complex::Complex64::new(x, y), &mut vars, &parsed);
                        if let (Some(a), Some(b)) = (prev, current) {
                            if (b.x - a.x).abs() < 500.0 && (b.y - a.y).abs() < 500.0 {
                                Self::add_line_segment(vertices, indices, a, b, 1.0, cg.color);
                            }
                        }
                        prev = current;
                    }
                }
            }
        }
    }

    fn add_complex_mapping_geometry(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        document: &Document,
        view: &ViewTransform,
        cm: &grafito_core::ComplexMappingObj,
        homotopy_time: f64,
    ) -> bool {
        let Some(target_obj) = document.get_object(cm.target) else {
            return false;
        };
        let Some(map) = cm.conformal_map(document.complex_base_symbol.as_str()) else {
            return false;
        };

        let t_val =
            complex_mapping_homotopy_factor(cm.animate_homotopy, cm.homotopy_speed, homotopy_time);

        if let GeoObject::Point(point) = target_obj {
            let source = Point2::new(
                document.resolve_expr(&point.x_expr, point.position.x),
                document.resolve_expr(&point.y_expr, point.position.y),
            );
            let marker = map
                .apply(num_complex::Complex64::new(source.x, source.y))
                .filter(|mapped| mapped.re.is_finite() && mapped.im.is_finite())
                .map(|mapped| {
                    interpolate_complex_mapping_point(
                        source,
                        Point2::new(mapped.re, mapped.im),
                        t_val,
                    )
                })
                // A singular image cannot be represented as a finite point. Keep a
                // visible source marker rather than silently dropping the mapping.
                .unwrap_or(source);
            if let Some(screen) = bounded_screen_point(view, marker) {
                let size = point.size.max(6.0);
                Self::add_rect(vertices, indices, screen, size, size, cm.color);
            }
            return true;
        }

        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
        let world_br = view.screen_to_world(view.screen_size);
        let view_bounds = (
            world_tl.x.min(world_br.x),
            world_tl.x.max(world_br.x),
            world_br.y.min(world_tl.y),
            world_br.y.max(world_tl.y),
        );

        let mut source_segments = Vec::new();

        match target_obj {
            GeoObject::ImplicitCurve(ic) => {
                let grid_size = grafito_core::implicit_curve::recommended_grid_size_for_quality(
                    view.screen_size.x,
                    view.screen_size.y,
                    document.render_quality,
                );
                let cached = grafito_core::implicit_curve::segments_or_compute(
                    ic,
                    view_bounds,
                    grid_size,
                    &document.variables,
                    document.render_quality,
                );
                for (_level, segments) in cached.iter() {
                    for (a, b) in segments {
                        let len = (a.x - b.x).hypot(a.y - b.y);
                        if len >= 1e-3 {
                            source_segments.push((*a, *b));
                        }
                    }
                }
            }
            GeoObject::Line(l) => {
                if l.has_finite_length() {
                    source_segments.push((l.start, l.end));
                } else if let Some((a, b)) = l.clip_to_aabb(grafito_geometry::AABB {
                    min: Point2::new(view_bounds.0, view_bounds.2),
                    max: Point2::new(view_bounds.1, view_bounds.3),
                }) {
                    source_segments.push((a, b));
                }
            }
            GeoObject::Circle(c) => {
                let n = 128;
                for i in 0..n {
                    let a1 = i as f64 * std::f64::consts::TAU / n as f64;
                    let a2 = (i + 1) as f64 * std::f64::consts::TAU / n as f64;
                    source_segments.push((
                        Point2::new(
                            c.center.x + c.radius * a1.cos(),
                            c.center.y + c.radius * a1.sin(),
                        ),
                        Point2::new(
                            c.center.x + c.radius * a2.cos(),
                            c.center.y + c.radius * a2.sin(),
                        ),
                    ));
                }
            }
            GeoObject::Polygon(p) => {
                let len = p.vertices.len().min(MAX_POLYGON_VERTICES);
                if len >= 2 {
                    for i in 0..len {
                        source_segments.push((p.vertices[i], p.vertices[(i + 1) % len]));
                    }
                }
            }
            GeoObject::Pencil(p) => {
                if p.points.len() >= 2 {
                    for i in 0..p.points.len() - 1 {
                        source_segments.push((p.points[i], p.points[i + 1]));
                    }
                }
            }
            GeoObject::Function(fun) => {
                let domain = (
                    document.resolve_expr(
                        &fun.domain_min_expr,
                        fun.domain_min.unwrap_or(view_bounds.0),
                    ),
                    document.resolve_expr(
                        &fun.domain_max_expr,
                        fun.domain_max.unwrap_or(view_bounds.1),
                    ),
                );
                let grid_size = grafito_core::function_sampling::recommended_grid_size_for_quality(
                    view.screen_size.x,
                    document.render_quality,
                );
                let samples = grafito_core::function_sampling::samples_or_compute(
                    fun,
                    domain,
                    grid_size,
                    &document.variables,
                );
                let mut prev: Option<Point2> = None;
                for (x, y_opt) in samples.iter() {
                    if let Some(y) = y_opt {
                        let current = Point2::new(*x, *y);
                        if let Some(p) = prev {
                            source_segments.push((p, current));
                        }
                        prev = Some(current);
                    } else {
                        prev = None;
                    }
                }
            }
            GeoObject::ParametricCurve2D(pc) => {
                let mut vars = document.variables.clone();
                vars.insert("t".to_string(), 0.0);
                if let (Ok(ast_x), Ok(ast_y)) = (
                    grafito_geometry::expr::prepare_function_ast(
                        &pc.expr_x,
                        &document.variables,
                        &["t"],
                    ),
                    grafito_geometry::expr::prepare_function_ast(
                        &pc.expr_y,
                        &document.variables,
                        &["t"],
                    ),
                ) {
                    let steps = match document.render_quality {
                        RenderQuality::Preview => 100,
                        RenderQuality::Normal => 500,
                        RenderQuality::High => 1000,
                    };
                    let mut prev: Option<Point2> = None;
                    for i in 0..=steps {
                        let t = pc.t_min + (pc.t_max - pc.t_min) * (i as f64 / steps as f64);
                        let x = ast_x.eval_2d("t", t, "t", t);
                        let y = ast_y.eval_2d("t", t, "t", t);
                        if x.is_finite() && y.is_finite() {
                            let current = Point2::new(x, y);
                            if let Some(p) = prev {
                                source_segments.push((p, current));
                            }
                            prev = Some(current);
                        } else {
                            prev = None;
                        }
                    }
                }
            }
            GeoObject::PolarCurve(pc) => {
                let mut vars = document.variables.clone();
                vars.insert("t".to_string(), 0.0);
                if let Ok(ast_r) = grafito_geometry::expr::prepare_function_ast(
                    &pc.expr_r,
                    &document.variables,
                    &["t"],
                ) {
                    let steps = match document.render_quality {
                        RenderQuality::Preview => 100,
                        RenderQuality::Normal => 500,
                        RenderQuality::High => 1000,
                    };
                    let mut prev: Option<Point2> = None;
                    for i in 0..=steps {
                        let t = pc.t_min + (pc.t_max - pc.t_min) * (i as f64 / steps as f64);
                        let r = ast_r.eval_2d("t", t, "t", t);
                        if r.is_finite() {
                            let current = Point2::new(r * t.cos(), r * t.sin());
                            if let Some(p) = prev {
                                source_segments.push((p, current));
                            }
                            prev = Some(current);
                        } else {
                            prev = None;
                        }
                    }
                }
            }
            GeoObject::Ellipse(el) => {
                let steps = 128;
                let cos_a = el.angle.cos();
                let sin_a = el.angle.sin();
                let mut previous = None;
                for index in 0..=steps {
                    let t = index as f64 * std::f64::consts::TAU / steps as f64;
                    let point = Point2::new(
                        el.center.x + el.rx * t.cos() * cos_a - el.ry * t.sin() * sin_a,
                        el.center.y + el.rx * t.cos() * sin_a + el.ry * t.sin() * cos_a,
                    );
                    if let Some(previous) = previous {
                        source_segments.push((previous, point));
                    }
                    previous = Some(point);
                }
            }
            GeoObject::Parabola(pb) if pb.p.is_finite() && pb.p.abs() >= 1e-12 => {
                let steps = 128;
                let range = (20.0 / view.scale).clamp(0.1, 500.0);
                let cos_a = pb.angle.cos();
                let sin_a = pb.angle.sin();
                let mut previous = None;
                for index in 0..=steps {
                    let t = -range + 2.0 * range * index as f64 / steps as f64;
                    let point = Point2::new(
                        pb.vertex.x + t * cos_a - (t * t / (4.0 * pb.p)) * sin_a,
                        pb.vertex.y + t * sin_a + (t * t / (4.0 * pb.p)) * cos_a,
                    );
                    if point.x.is_finite() && point.y.is_finite() {
                        if let Some(previous) = previous {
                            source_segments.push((previous, point));
                        }
                        previous = Some(point);
                    } else {
                        previous = None;
                    }
                }
            }
            GeoObject::Hyperbola(hb)
                if hb.a.is_finite() && hb.b.is_finite() && hb.a > 0.0 && hb.b > 0.0 =>
            {
                let steps = 64;
                let epsilon = 0.05;
                let cos_a = hb.angle.cos();
                let sin_a = hb.angle.sin();
                for branch in 0..2 {
                    let start = -std::f64::consts::FRAC_PI_2
                        + epsilon
                        + branch as f64 * std::f64::consts::PI;
                    let end = std::f64::consts::FRAC_PI_2 - epsilon
                        + branch as f64 * std::f64::consts::PI;
                    let mut previous = None;
                    for index in 0..=steps {
                        let t = start + (end - start) * index as f64 / steps as f64;
                        let (local_x, local_y) = if hb.horizontal {
                            (hb.a / t.cos(), hb.b * t.tan())
                        } else {
                            (hb.b * t.tan(), hb.a / t.cos())
                        };
                        let point = Point2::new(
                            hb.center.x + local_x * cos_a - local_y * sin_a,
                            hb.center.y + local_x * sin_a + local_y * cos_a,
                        );
                        if point.x.is_finite() && point.y.is_finite() {
                            if let Some(previous) = previous {
                                source_segments.push((previous, point));
                            }
                            previous = Some(point);
                        } else {
                            previous = None;
                        }
                    }
                }
            }
            GeoObject::RegressionLine(rl)
                if rl.x_min.is_finite()
                    && rl.x_max.is_finite()
                    && rl.slope.is_finite()
                    && rl.intercept.is_finite()
                    && rl.x_min < rl.x_max =>
            {
                source_segments.push((
                    Point2::new(rl.x_min, rl.slope * rl.x_min + rl.intercept),
                    Point2::new(rl.x_max, rl.slope * rl.x_max + rl.intercept),
                ));
            }
            GeoObject::VectorField2D(vf) => {
                let grid_size = vf.density.clamp(5, 128);
                let cell_width = (view_bounds.1 - view_bounds.0).abs() / grid_size as f64;
                let cell_height = (view_bounds.3 - view_bounds.2).abs() / grid_size as f64;
                let arrow_length = cell_width.min(cell_height) * 0.8;
                if arrow_length.is_finite() && arrow_length > 0.0 {
                    let samples = grafito_core::vector_field_sampling::samples_or_compute(
                        vf,
                        view_bounds,
                        grid_size,
                        &document.variables,
                    );
                    for (x, y, u, v) in samples.iter() {
                        if let Some(end) = vector_arrow_end(*x, *y, *u, *v, arrow_length) {
                            source_segments.push((Point2::new(*x, *y), end));
                        }
                    }
                }
            }
            _ => {
                // Not supported yet for this type
                return false;
            }
        }

        let subdivisions = match document.render_quality {
            RenderQuality::Preview => 4,
            RenderQuality::Normal => 8,
            RenderQuality::High => 16,
        };
        for (a, b) in transform_complex_mapping_segments(map, &source_segments, subdivisions, t_val)
        {
            let p1 = view.world_to_screen(a);
            let p2 = view.world_to_screen(b);
            if (p2.x - p1.x).abs() < 300.0 && (p2.y - p1.y).abs() < 300.0 {
                Self::add_line_segment(vertices, indices, p1, p2, 1.5, cm.color);
            }
        }
        true
    }

    fn add_rect(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        w: f32,
        h: f32,
        color: Color,
    ) {
        if !screen_point_is_renderable(center)
            || !w.is_finite()
            || !h.is_finite()
            || !color_is_renderable(color)
        {
            return;
        }
        let hw = w * 0.5;
        let hh = h * 0.5;
        let corners = [
            glam::Vec2::new(center.x - hw, center.y - hh),
            glam::Vec2::new(center.x + hw, center.y - hh),
            glam::Vec2::new(center.x + hw, center.y + hh),
            glam::Vec2::new(center.x - hw, center.y + hh),
        ];
        if corners
            .iter()
            .any(|corner| !screen_point_is_renderable(*corner))
        {
            return;
        }
        let Some(base) = reserve_geometry(vertices, indices, 4, 6) else {
            return;
        };
        vertices.extend(corners.map(|corner| Vertex::new(corner.x, corner.y, color)));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn add_line_segment(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        a: glam::Vec2,
        b: glam::Vec2,
        width: f32,
        color: Color,
    ) {
        if !screen_point_is_renderable(a)
            || !screen_point_is_renderable(b)
            || !width.is_finite()
            || !color_is_renderable(color)
        {
            return;
        }
        let dir = b - a;
        let length_squared = dir.length_squared();
        if !length_squared.is_finite() || length_squared < 0.0001 {
            return;
        }
        let dir = dir.normalize();
        let perp = glam::Vec2::new(-dir.y, dir.x) * (width * 0.5).max(0.5);
        if !perp.is_finite() {
            return;
        }
        let corners = [a + perp, b + perp, b - perp, a - perp];
        if corners
            .iter()
            .any(|corner| !screen_point_is_renderable(*corner))
        {
            return;
        }

        let Some(base) = reserve_geometry(vertices, indices, 4, 6) else {
            return;
        };
        vertices.extend(corners.map(|corner| Vertex::new(corner.x, corner.y, color)));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn add_text_screen(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        text: &str,
        mut pos: glam::Vec2,
        size: f32,
        color: Color,
    ) {
        let width = size * 0.15;
        let char_w = size * 0.6;
        let char_spacing = size * 0.2;

        for c in text.chars() {
            let segments: &[(f32, f32, f32, f32)] = match c {
                '0' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                ],
                '1' => &[(0.5, 0., 0.5, 2.), (0.15, 0.5, 0.5, 0.), (0.2, 2., 0.8, 2.)],
                '2' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 1.),
                    (1., 1., 0., 1.),
                    (0., 1., 0., 2.),
                    (0., 2., 1., 2.),
                ],
                '3' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 1., 1., 1.),
                ],
                '4' => &[(0., 0., 0., 1.), (0., 1., 1., 1.), (1., 0., 1., 2.)],
                '5' => &[
                    (1., 0., 0., 0.),
                    (0., 0., 0., 1.),
                    (0., 1., 1., 1.),
                    (1., 1., 1., 2.),
                    (1., 2., 0., 2.),
                ],
                '6' => &[
                    (1., 0., 0., 0.),
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                ],
                '7' => &[(0., 0., 1., 0.), (1., 0., 0.2, 2.)],
                '8' => &[
                    (0., 0., 1., 0.),
                    (1., 0., 1., 2.),
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                    (0., 1., 1., 1.),
                ],
                '9' => &[
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                    (0., 0., 0., 1.),
                    (0., 1., 1., 1.),
                ],
                '-' => &[(0.2, 1., 0.8, 1.)],
                '.' => &[
                    (0.45, 1.8, 0.55, 1.8),
                    (0.55, 1.8, 0.55, 2.0),
                    (0.55, 2.0, 0.45, 2.0),
                    (0.45, 2.0, 0.45, 1.8),
                ],
                // Uppercase letters
                'A' => &[(0., 2., 0.5, 0.), (0.5, 0., 1., 2.), (0.2, 1., 0.8, 1.)],
                'B' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 0.8, 2.),
                    (0.8, 2., 1., 1.5),
                    (1., 1.5, 0.8, 1.),
                    (0.8, 1., 0., 1.),
                    (0., 1., 0.8, 1.),
                    (0.8, 1., 1., 0.5),
                    (1., 0.5, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'C' => &[(1., 2., 0., 2.), (0., 2., 0., 0.), (0., 0., 1., 0.)],
                'D' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 0.6, 2.),
                    (0.6, 2., 1., 1.),
                    (1., 1., 0.6, 0.),
                    (0.6, 0., 0., 0.),
                ],
                'E' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (0., 1., 0.7, 1.),
                    (0., 0., 1., 0.),
                ],
                'F' => &[(0., 0., 0., 2.), (0., 2., 1., 2.), (0., 1., 0.7, 1.)],
                'G' => &[
                    (1., 2., 0., 2.),
                    (0., 2., 0., 0.),
                    (0., 0., 1., 0.),
                    (1., 0., 1., 1.),
                    (1., 1., 0.5, 1.),
                ],
                'H' => &[(0., 0., 0., 2.), (0., 1., 1., 1.), (1., 0., 1., 2.)],
                'I' => &[(0.5, 0., 0.5, 2.), (0., 0., 1., 0.), (0., 2., 1., 2.)],
                'J' => &[(0.5, 0., 0.5, 1.6), (0.5, 1.6, 0.2, 2.), (0., 2., 0.5, 2.)],
                'K' => &[(0., 0., 0., 2.), (0., 1., 1., 2.), (0., 1., 1., 0.)],
                'L' => &[(0., 0., 0., 2.), (0., 0., 1., 0.)],
                'M' => &[
                    (0., 2., 0., 0.),
                    (0., 0., 0.5, 1.),
                    (0.5, 1., 1., 0.),
                    (1., 0., 1., 2.),
                ],
                'N' => &[(0., 2., 0., 0.), (0., 0., 1., 2.), (1., 2., 1., 0.)],
                'O' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'P' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                ],
                'Q' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 0.),
                    (1., 0., 0., 0.),
                    (0.5, 0.5, 1., 0.),
                ],
                'R' => &[
                    (0., 0., 0., 2.),
                    (0., 2., 1., 2.),
                    (1., 2., 1., 1.),
                    (1., 1., 0., 1.),
                    (0., 1., 1., 0.),
                ],
                'S' => &[
                    (1., 2., 0., 2.),
                    (0., 2., 0., 1.),
                    (0., 1., 1., 1.),
                    (1., 1., 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'T' => &[(0., 2., 1., 2.), (0.5, 2., 0.5, 0.)],
                'U' => &[(0., 2., 0., 0.), (0., 0., 1., 0.), (1., 0., 1., 2.)],
                'V' => &[(0., 2., 0.5, 0.), (0.5, 0., 1., 2.)],
                'W' => &[
                    (0., 2., 0., 0.),
                    (0., 0., 0.5, 1.),
                    (0.5, 1., 1., 0.),
                    (1., 0., 1., 2.),
                ],
                'X' => &[(0., 0., 1., 2.), (0., 2., 1., 0.)],
                'Y' => &[(0., 2., 0.5, 1.), (1., 2., 0.5, 1.), (0.5, 1., 0.5, 0.)],
                'Z' => &[(0., 2., 1., 2.), (1., 2., 0., 0.), (0., 0., 1., 0.)],
                // Lowercase letters
                'a' => &[
                    (0., 0.5, 0., 1.5),
                    (0., 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.),
                    (0., 0., 1., 0.),
                ],
                'b' => &[
                    (0., 0., 0., 2.),
                    (0., 1.5, 0.8, 1.5),
                    (0.8, 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'c' => &[(1., 1.5, 0., 1.5), (0., 1.5, 0., 0.), (0., 0., 1., 0.)],
                'd' => &[
                    (1., 0., 1., 2.),
                    (1., 1.5, 0.2, 1.5),
                    (0.2, 1.5, 0., 0.8),
                    (0., 0.8, 0.2, 0.),
                    (0.2, 0., 1., 0.),
                ],
                'e' => &[
                    (1., 1.5, 0., 1.5),
                    (0., 1.5, 0., 0.5),
                    (0., 0.5, 0.5, 0.5),
                    (0., 0., 1., 0.),
                ],
                'f' => &[(0.3, 2., 0.7, 2.), (0.5, 2., 0.5, 0.), (0., 1., 0.5, 1.)],
                'g' => &[
                    (0., 0.5, 1., 0.5),
                    (1., 0.5, 1., -0.5),
                    (1., -0.5, 0., -0.5),
                    (0., -0.5, 0., 1.5),
                    (0., 1.5, 1., 1.5),
                ],
                'h' => &[
                    (0., 0., 0., 2.),
                    (0., 1., 0.8, 1.),
                    (0.8, 1., 1., 0.),
                    (1., 0., 1., 0.8),
                ],
                'i' => &[(0.5, 0., 0.5, 1.5), (0.3, 2., 0.7, 2.)],
                'j' => &[(0.5, 0., 0.5, 1.5), (0.5, 1.5, 0.2, 2.), (0.3, 2., 0.7, 2.)],
                'k' => &[(0., 0., 0., 2.), (0., 0.8, 0.8, 1.5), (0., 0.8, 1., 0.)],
                'l' => &[(0.5, 0., 0.5, 2.)],
                'm' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 0.3, 0.),
                    (0.3, 0., 0.6, 1.5),
                    (0.6, 1.5, 0.8, 0.),
                    (0.8, 0., 1., 1.5),
                    (1., 1.5, 1., 0.),
                ],
                'n' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 0.8, 0.),
                    (0.8, 0., 1., 1.5),
                    (1., 1.5, 1., 0.),
                ],
                'o' => &[
                    (0., 0., 0., 1.5),
                    (0., 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.),
                    (1., 0., 0., 0.),
                ],
                'p' => &[
                    (0., -0.5, 0., 1.5),
                    (0., 1.5, 0.8, 1.5),
                    (0.8, 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0., 0.),
                ],
                'q' => &[
                    (1., -0.5, 1., 1.5),
                    (0.2, 1.5, 1., 1.5),
                    (1., 1.5, 1., 0.8),
                    (1., 0.8, 0.8, 0.),
                    (0.8, 0., 0.2, 0.),
                    (0.2, 0., 0., 0.8),
                ],
                'r' => &[(0., 0., 0., 1.5), (0., 1.5, 0.8, 1.5), (0.8, 1.5, 0.8, 0.8)],
                's' => &[
                    (1., 1.5, 0., 1.5),
                    (0., 1.5, 0., 0.8),
                    (0., 0.8, 1., 0.8),
                    (1., 0.8, 1., 0.),
                    (1., 0., 0., 0.),
                ],
                't' => &[(0.5, 0., 0.5, 2.), (0., 1., 0.8, 1.)],
                'u' => &[(0., 1.5, 0., 0.), (0., 0., 1., 0.), (1., 0., 1., 1.5)],
                'v' => &[(0., 1.5, 0.5, 0.), (0.5, 0., 1., 1.5)],
                'w' => &[
                    (0., 1.5, 0.25, 0.),
                    (0.25, 0., 0.5, 0.8),
                    (0.5, 0.8, 0.75, 0.),
                    (0.75, 0., 1., 1.5),
                ],
                'x' => &[(0., 0., 1., 1.5), (0., 1.5, 1., 0.)],
                'y' => &[
                    (0., 1.5, 0.5, 0.),
                    (0.5, 0., 1., 1.5),
                    (0., -0.5, 0.5, -0.5),
                ],
                'z' => &[(0., 1.5, 1., 1.5), (1., 1.5, 0., 0.), (0., 0., 1., 0.)],
                // Symbols
                '+' => &[(0.5, 0.3, 0.5, 1.7), (0., 1., 1., 1.)],
                '=' => &[(0., 0.5, 1., 0.5), (0., 1.5, 1., 1.5)],
                '*' => &[
                    (0.2, 0.3, 0.8, 1.7),
                    (0.5, 0., 0.5, 2.),
                    (0., 0.7, 1., 1.4),
                    (0., 1.4, 1., 0.7),
                ],
                '/' => &[(1., 0.3, 0., 1.7)],
                '(' => &[(0.7, 2., 0.2, 1.), (0.2, 1., 0.7, 0.)],
                ')' => &[(0.3, 2., 0.8, 1.), (0.8, 1., 0.3, 0.)],
                '[' => &[(0.8, 2., 0.2, 2.), (0.2, 2., 0.2, 0.), (0.2, 0., 0.8, 0.)],
                ']' => &[(0.2, 2., 0.8, 2.), (0.8, 2., 0.8, 0.), (0.8, 0., 0.2, 0.)],
                ' ' => &[],
                _ => &[],
            };

            for &(x1, y1, x2, y2) in segments {
                let a = pos + glam::Vec2::new(x1 * char_w, y1 * char_w);
                let b = pos + glam::Vec2::new(x2 * char_w, y2 * char_w);
                Self::add_line_segment(vertices, indices, a, b, width, color);
            }
            pos.x += char_w + char_spacing;
        }
    }

    fn add_circle_stroke(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        radius: f32,
        width: f32,
        color: Color,
    ) {
        if !screen_point_is_renderable(center)
            || !radius.is_finite()
            || radius < 0.0
            || !width.is_finite()
            || !color_is_renderable(color)
        {
            return;
        }
        let segments = ((radius * 0.5).clamp(16.0, 128.0)) as usize;
        let inner_r = (radius - width * 0.5).max(0.0);
        let outer_r = radius + width * 0.5;
        if !inner_r.is_finite() || !outer_r.is_finite() {
            return;
        }
        if [
            glam::Vec2::new(center.x - outer_r, center.y),
            glam::Vec2::new(center.x + outer_r, center.y),
            glam::Vec2::new(center.x, center.y - outer_r),
            glam::Vec2::new(center.x, center.y + outer_r),
        ]
        .iter()
        .any(|point| !screen_point_is_renderable(*point))
        {
            return;
        }
        let vertex_count = (segments + 1).saturating_mul(2);
        let Some(base) = reserve_geometry(vertices, indices, vertex_count, segments * 6) else {
            return;
        };

        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let c = theta.cos();
            let s = theta.sin();
            vertices.push(Vertex::new(
                center.x + inner_r * c,
                center.y + inner_r * s,
                color,
            ));
            vertices.push(Vertex::new(
                center.x + outer_r * c,
                center.y + outer_r * s,
                color,
            ));
        }

        for i in 0..segments {
            let Ok(offset) = u32::try_from(i * 2) else {
                return;
            };
            let Some(i0) = base.checked_add(offset) else {
                return;
            };
            let i1 = i0 + 1;
            let i2 = i0 + 2;
            let i3 = i0 + 3;
            indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
        }
    }

    fn add_circle_fill(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        center: glam::Vec2,
        radius: f32,
        color: Color,
    ) {
        if !screen_point_is_renderable(center)
            || !radius.is_finite()
            || radius < 0.0
            || !color_is_renderable(color)
        {
            return;
        }
        if [
            glam::Vec2::new(center.x - radius, center.y),
            glam::Vec2::new(center.x + radius, center.y),
            glam::Vec2::new(center.x, center.y - radius),
            glam::Vec2::new(center.x, center.y + radius),
        ]
        .iter()
        .any(|point| !screen_point_is_renderable(*point))
        {
            return;
        }
        let segments = ((radius * 0.5).max(16.0).min(128.0)) as usize;
        let Some(center_idx) = reserve_geometry(vertices, indices, segments + 2, segments * 3)
        else {
            return;
        };
        vertices.push(Vertex::new(center.x, center.y, color));

        for i in 0..=segments {
            let theta = (i as f32 / segments as f32) * std::f32::consts::TAU;
            vertices.push(Vertex::new(
                center.x + radius * theta.cos(),
                center.y + radius * theta.sin(),
                color,
            ));
        }

        for i in 0..segments {
            let Ok(i) = u32::try_from(i) else {
                return;
            };
            let (Some(i1), Some(i2)) =
                (center_idx.checked_add(1 + i), center_idx.checked_add(2 + i))
            else {
                return;
            };
            indices.extend_from_slice(&[center_idx, i1, i2]);
        }
    }

    fn add_polygon_fill(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        pts: &[glam::Vec2],
        color: Color,
        view: &ViewTransform,
    ) {
        if !polygon_geometry_is_within_limit(pts.len())
            || pts.iter().any(|point| !screen_point_is_renderable(*point))
            || !color_is_renderable(color)
        {
            return;
        }
        let mut path_builder = Path::builder();
        path_builder.begin(point(pts[0].x, pts[0].y));
        for p in &pts[1..] {
            path_builder.line_to(point(p.x, p.y));
        }
        path_builder.end(true);

        let tolerance = lyon_tolerance_for_view_scale(view.scale);
        let mut tess_ok = false;
        let mut geometry: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
        FILL_TESS.with(|cell| {
            let mut tess = cell.borrow_mut();
            tess_ok = tess
                .tessellate_path(
                    &path_builder.build(),
                    &FillOptions::default().with_tolerance(tolerance),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| vertex.position()),
                )
                .is_ok();
        });
        if !tess_ok {
            return;
        }
        let Some(base) = reserve_geometry(
            vertices,
            indices,
            geometry.vertices.len(),
            geometry.indices.len(),
        ) else {
            return;
        };
        for p in geometry.vertices {
            vertices.push(Vertex::new(p.x, p.y, color));
        }
        for index in geometry.indices {
            let Some(index) = base.checked_add(index) else {
                return;
            };
            indices.push(index);
        }
    }

    fn add_polygon_stroke(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        pts: &[glam::Vec2],
        width: f32,
        color: Color,
    ) {
        if pts.len() < 2 {
            return;
        }
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            Self::add_line_segment(vertices, indices, a, b, width, color);
        }
    }

    pub fn build_3d_geometry(
        &self,
        document: &Document,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
        include_overlays: bool,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        if include_overlays {
            self.build_3d_grid(
                &mut vertices,
                &mut indices,
                camera,
                dark_mode,
                screen_w,
                screen_h,
            );
            self.build_3d_axes(
                &mut vertices,
                &mut indices,
                camera,
                dark_mode,
                screen_w,
                screen_h,
            );
        }

        for (_, obj) in document.objects_iter() {
            if !obj.is_visible() {
                continue;
            }
            match obj {
                GeoObject::Point3D(_p) => {
                    // Handled by CPU overlay
                }
                GeoObject::Segment3D(l) => {
                    Self::add_line_3d(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &l.a,
                        &l.b,
                        l.width,
                        l.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Plane3D(p) => {
                    Self::add_plane3d_patch(
                        &mut vertices,
                        &mut indices,
                        camera,
                        p.a,
                        p.b,
                        p.c,
                        p.d,
                        p.opacity,
                        p.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Line3D(l) => {
                    Self::add_line3d_object(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &l.point,
                        &l.direction,
                        l.width,
                        l.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Sphere3D(s) => {
                    if let Some(fc) = s.fill_color {
                        Self::add_solid_sphere(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &s.center,
                            s.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_sphere(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &s.center,
                        s.radius,
                        s.width,
                        s.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cube3D(c) => {
                    if let Some(fc) = c.fill_color {
                        Self::add_solid_cube(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &c.center,
                            c.size,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cube(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &c.center,
                        c.size,
                        c.width,
                        c.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Tetrahedron3D(t) => {
                    if let Some(fill) = t.fill_color {
                        Self::add_solid_tetrahedron(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &t.center,
                            t.edge_length,
                            fill,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_tetrahedron(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &t.center,
                        t.edge_length,
                        t.width,
                        t.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Pyramid3D(p) => {
                    if let Some(fc) = p.fill_color {
                        Self::add_solid_pyramid(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &p.base_center,
                            &p.apex,
                            p.base_size,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_pyramid(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &p.base_center,
                        &p.apex,
                        p.base_size,
                        p.width,
                        p.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cone3D(co) => {
                    if let Some(fc) = co.fill_color {
                        Self::add_solid_cone(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &co.base_center,
                            &co.apex,
                            co.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cone(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &co.base_center,
                        &co.apex,
                        co.radius,
                        co.width,
                        co.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Cylinder3D(cy) => {
                    if let Some(fc) = cy.fill_color {
                        Self::add_solid_cylinder(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &cy.base_center,
                            &cy.top_center,
                            cy.radius,
                            fc,
                            screen_w,
                            screen_h,
                        );
                    }
                    Self::add_wireframe_cylinder(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &cy.base_center,
                        &cy.top_center,
                        cy.radius,
                        cy.width,
                        cy.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Surface3D(su) => {
                    Self::add_surface_mesh(
                        &mut vertices,
                        &mut indices,
                        camera,
                        su,
                        &document.variables,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::Torus3D(to) => {
                    Self::add_solid_torus(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &to.center,
                        to.r_major,
                        to.r_minor,
                        to.color,
                        screen_w,
                        screen_h,
                    );
                    Self::add_wireframe_torus(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &to.center,
                        to.r_major,
                        to.r_minor,
                        to.width,
                        to.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::MoebiusStrip(mb) => {
                    Self::add_solid_moebius(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &mb.center,
                        mb.radius,
                        mb.width_r,
                        mb.color,
                        screen_w,
                        screen_h,
                    );
                    Self::add_wireframe_moebius(
                        &mut vertices,
                        &mut indices,
                        camera,
                        &mb.center,
                        mb.radius,
                        mb.width_r,
                        mb.width,
                        mb.color,
                        screen_w,
                        screen_h,
                    );
                }
                GeoObject::ParametricCurve3D(pc) => {
                    let steps = 4000;
                    let samples = grafito_core::parametric_sampling::samples_or_compute_curve_3d(
                        pc,
                        steps,
                        &document.variables,
                    );
                    let mut prev: Option<Point3D> = None;
                    for &(x, y, z) in samples.iter() {
                        if x.is_finite() && y.is_finite() && z.is_finite() {
                            let p = Point3D::new(x, y, z);
                            if let Some(prev_p) = prev {
                                Self::add_line_3d(
                                    &mut vertices,
                                    &mut indices,
                                    camera,
                                    &prev_p,
                                    &p,
                                    pc.width,
                                    pc.color,
                                    screen_w,
                                    screen_h,
                                );
                            }
                            prev = Some(p);
                        } else {
                            prev = None;
                        }
                    }
                }
                GeoObject::Attractor3D(at) => {
                    use grafito_geometry::attractors::integrate_attractor;
                    let att_type = at.model();
                    let points = integrate_attractor(
                        &att_type, at.x0, at.y0, at.z0, at.dt, at.steps, at.skip,
                    );
                    for w in points.windows(2) {
                        let a = Point3D::new(w[0].x * 0.2, w[0].y * 0.2, w[0].z * 0.2);
                        let b = Point3D::new(w[1].x * 0.2, w[1].y * 0.2, w[1].z * 0.2);
                        Self::add_line_3d(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &a,
                            &b,
                            at.width,
                            at.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                GeoObject::VectorField3D(vf) => {
                    for (start, end) in sample_vector_field_3d(vf, &document.variables) {
                        Self::add_line_3d(
                            &mut vertices,
                            &mut indices,
                            camera,
                            &start,
                            &end,
                            1.5,
                            vf.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                GeoObject::HyperSurface4D(hs) => {
                    let res = hs.resolution.max(4).min(40);
                    let vertices_count = res * res;
                    if vertices_count < 10000 {
                        for i in 0..res {
                            for j in 0..res {
                                let u = -1.0 + 2.0 * i as f64 / res as f64;
                                let v = -1.0 + 2.0 * j as f64 / res as f64;
                                let x = u * (1.0 - v * v / 2.0).cos();
                                let y = v * (1.0 - u * u / 2.0).cos();
                                let z = u
                                    * v
                                    * hs.rotation_angles.first().copied().unwrap_or(0.0).sin();
                                let p = Point3D::new(x * 3.0, y * 3.0, z * 3.0);
                                let sp = camera.project(&p, screen_w, screen_h);
                                if let Some(screen_p) = sp {
                                    Self::add_rect(
                                        &mut vertices,
                                        &mut indices,
                                        glam::Vec2::new(screen_p.0, screen_p.1),
                                        2.0,
                                        2.0,
                                        hs.color,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        (vertices, indices)
    }

    fn build_3d_grid(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        // Calcular densidad de cuadrícula basada en distancia de cámara
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let pixels_per_unit = (screen_h / frustum_height) as f64;
        let target_world_step = 80.0 / pixels_per_unit.max(1e-6);
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
        let minor_step = major_step / 5.0;

        if minor_step <= 1e-9 {
            return;
        }

        // Color adaptativo para dark/light mode
        let minor_color = if dark_mode {
            Color::new(0.20, 0.20, 0.20, 1.0)
        } else {
            Color::new(0.85, 0.85, 0.85, 1.0)
        };

        let major_color = if dark_mode {
            Color::new(0.35, 0.35, 0.35, 1.0)
        } else {
            Color::new(0.70, 0.70, 0.70, 1.0)
        };

        // Calcular rango de vista basado en frustum
        let center_x = camera.target.x as f64;
        let center_z = camera.target.z as f64;
        let aspect = screen_w / screen_h.max(1.0);
        let view_range = (frustum_height * aspect.max(1.0) * 1.5) as f64;

        let start_x = ((center_x - view_range) / minor_step).floor() * minor_step;
        let end_x = ((center_x + view_range) / minor_step).ceil() * minor_step;
        let start_z = ((center_z - view_range) / minor_step).floor() * minor_step;
        let end_z = ((center_z + view_range) / minor_step).ceil() * minor_step;

        let line_count_x = ((end_x - start_x) / minor_step).round() as i64;
        let line_count_z = ((end_z - start_z) / minor_step).round() as i64;

        // Limitar número de líneas para performance (pero permitir más que antes)
        let max_lines = 500;
        if line_count_x > max_lines || line_count_z > max_lines {
            // Si hay demasiadas líneas, usar solo major grid
            let major_line_count_x = ((end_x - start_x) / major_step).round() as i64;
            let major_line_count_z = ((end_z - start_z) / major_step).round() as i64;

            if major_line_count_x <= max_lines && major_line_count_z <= max_lines {
                // Dibujar solo major grid

                let cam_pos = camera.position();
                let mut prev_screen_pos_x: Option<glam::Vec2> = None;
                for xi in 0..=major_line_count_x {
                    let x = start_x + xi as f64 * major_step;
                    let p1 = Point3D::new(x, 0.0, start_z);
                    let p2 = Point3D::new(x, 0.0, end_z);
                    Self::add_line_3d(
                        vertices,
                        indices,
                        camera,
                        &p1,
                        &p2,
                        1.5,
                        major_color,
                        screen_w,
                        screen_h,
                    );

                    if x.abs() > 1e-5 {
                        let dx = x - cam_pos.x as f64;
                        let dy = 0.0 - cam_pos.y as f64;
                        let dz = 0.0 - cam_pos.z as f64;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        if dist <= camera.distance as f64 * 1.8 {
                            if let Some((sx, sy)) =
                                camera.project(&Point3D::new(x, 0.0, 0.0), screen_w, screen_h)
                            {
                                let current_sp = glam::Vec2::new(sx, sy);
                                let overlap = if let Some(prev) = prev_screen_pos_x {
                                    current_sp.distance(prev) < 45.0
                                } else {
                                    false
                                };
                                if !overlap {
                                    prev_screen_pos_x = Some(current_sp);
                                    // CPU overlay handles text
                                }
                            }
                        }
                    }
                }

                let mut prev_screen_pos_z: Option<glam::Vec2> = None;
                for zi in 0..=major_line_count_z {
                    let z = start_z + zi as f64 * major_step;
                    let p1 = Point3D::new(start_x, 0.0, z);
                    let p2 = Point3D::new(end_x, 0.0, z);
                    Self::add_line_3d(
                        vertices,
                        indices,
                        camera,
                        &p1,
                        &p2,
                        1.5,
                        major_color,
                        screen_w,
                        screen_h,
                    );

                    if z.abs() > 1e-5 {
                        let dx = 0.0 - cam_pos.x as f64;
                        let dy = 0.0 - cam_pos.y as f64;
                        let dz = z - cam_pos.z as f64;
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                        if dist <= camera.distance as f64 * 1.8 {
                            if let Some((sx, sy)) =
                                camera.project(&Point3D::new(0.0, 0.0, z), screen_w, screen_h)
                            {
                                let current_sp = glam::Vec2::new(sx, sy);
                                let overlap = if let Some(prev) = prev_screen_pos_z {
                                    current_sp.distance(prev) < 45.0
                                } else {
                                    false
                                };
                                if !overlap {
                                    prev_screen_pos_z = Some(current_sp);
                                    // CPU overlay handles text
                                }
                            }
                        }
                    }
                }
            }
            // Si incluso major grid es demasiado, no dibujar nada
        } else {
            // Dibujar grid completo (minor + major)

            let cam_pos = camera.position();
            let mut prev_screen_pos_x: Option<glam::Vec2> = None;
            for xi in 0..=line_count_x {
                let x = start_x + xi as f64 * minor_step;
                let is_major = ((x / major_step).round() * major_step - x).abs() < minor_step * 0.1;
                let (color, width) = if is_major {
                    (major_color, 1.5)
                } else {
                    (minor_color, 0.8)
                };

                let p1 = Point3D::new(x, 0.0, start_z);
                let p2 = Point3D::new(x, 0.0, end_z);
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );

                if is_major && x.abs() > 1e-5 {
                    let dx = x - cam_pos.x as f64;
                    let dy = 0.0 - cam_pos.y as f64;
                    let dz = 0.0 - cam_pos.z as f64;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist <= camera.distance as f64 * 1.8 {
                        if let Some((sx, sy)) =
                            camera.project(&Point3D::new(x, 0.0, 0.0), screen_w, screen_h)
                        {
                            let current_sp = glam::Vec2::new(sx, sy);
                            let overlap = if let Some(prev) = prev_screen_pos_x {
                                current_sp.distance(prev) < 45.0
                            } else {
                                false
                            };
                            if !overlap {
                                prev_screen_pos_x = Some(current_sp);
                                // CPU overlay handles text
                            }
                        }
                    }
                }
            }

            let mut prev_screen_pos_z: Option<glam::Vec2> = None;
            for zi in 0..=line_count_z {
                let z = start_z + zi as f64 * minor_step;
                let is_major = ((z / major_step).round() * major_step - z).abs() < minor_step * 0.1;
                let (color, width) = if is_major {
                    (major_color, 1.5)
                } else {
                    (minor_color, 0.8)
                };

                let p1 = Point3D::new(start_x, 0.0, z);
                let p2 = Point3D::new(end_x, 0.0, z);
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );

                if is_major && z.abs() > 1e-5 {
                    let dx = 0.0 - cam_pos.x as f64;
                    let dy = 0.0 - cam_pos.y as f64;
                    let dz = z - cam_pos.z as f64;
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist <= camera.distance as f64 * 1.8 {
                        if let Some((sx, sy)) =
                            camera.project(&Point3D::new(0.0, 0.0, z), screen_w, screen_h)
                        {
                            let current_sp = glam::Vec2::new(sx, sy);
                            let overlap = if let Some(prev) = prev_screen_pos_z {
                                current_sp.distance(prev) < 45.0
                            } else {
                                false
                            };
                            if !overlap {
                                prev_screen_pos_z = Some(current_sp);
                                // CPU overlay handles text
                            }
                        }
                    }
                }
            }
        }
    }

    fn build_3d_axes(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        _dark_mode: bool,
        screen_w: f32,
        screen_h: f32,
    ) {
        let fov_rad = camera.fov.to_radians();
        let frustum_height = 2.0 * camera.distance * (fov_rad * 0.5).tan();
        let aspect = screen_w / screen_h.max(1.0);
        let axis_len = (frustum_height * aspect.max(1.0) * 1.8) as f64;

        let red = Color::new(0.86, 0.2, 0.2, 1.0);
        let green = Color::new(0.2, 0.7, 0.2, 1.0);
        let blue = Color::new(0.2, 0.2, 0.86, 1.0);

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(-axis_len, 0.0, 0.0),
            &Point3D::new(axis_len, 0.0, 0.0),
            2.0,
            red,
            screen_w,
            screen_h,
        );

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, -axis_len, 0.0),
            &Point3D::new(0.0, axis_len, 0.0),
            2.0,
            green,
            screen_w,
            screen_h,
        );

        Self::add_line_3d(
            vertices,
            indices,
            camera,
            &Point3D::new(0.0, 0.0, -axis_len),
            &Point3D::new(0.0, 0.0, axis_len),
            2.0,
            blue,
            screen_w,
            screen_h,
        );
    }

    fn add_line_3d(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        a: &Point3D,
        b: &Point3D,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        // Proyección con clipping en near plane (similar a project_segment en render_3d.rs)
        let mvp = camera.mvp();
        let mut clip_a = mvp * a.to_vec3().extend(1.0);
        let mut clip_b = mvp * b.to_vec3().extend(1.0);

        let near = camera.near;

        // Si ambos puntos están detrás del near plane, no dibujar
        if clip_a.w < near && clip_b.w < near {
            return;
        }

        // Clipping: si un punto está detrás del near plane, interpolar
        if clip_a.w < near {
            let t = (near - clip_a.w) / (clip_b.w - clip_a.w);
            clip_a = clip_a + t * (clip_b - clip_a);
        } else if clip_b.w < near {
            let t = (near - clip_b.w) / (clip_a.w - clip_b.w);
            clip_b = clip_b + t * (clip_a - clip_b);
        }

        // Convertir a NDC (Normalized Device Coordinates)
        let ndc_ax = clip_a.x / clip_a.w;
        let ndc_ay = clip_a.y / clip_a.w;
        let ndc_bx = clip_b.x / clip_b.w;
        let ndc_by = clip_b.y / clip_b.w;

        // Frustum culling: si ambos puntos están fuera del mismo lado, no dibujar
        if ndc_ax.abs() > 2.0 && ndc_bx.abs() > 2.0 && ndc_ax.signum() == ndc_bx.signum() {
            return;
        }
        if ndc_ay.abs() > 2.0 && ndc_by.abs() > 2.0 && ndc_ay.signum() == ndc_by.signum() {
            return;
        }

        // Convertir NDC a screen coordinates
        let sa = glam::Vec2::new(
            (ndc_ax + 1.0) * 0.5 * screen_w,
            (1.0 - ndc_ay) * 0.5 * screen_h,
        );
        let sb = glam::Vec2::new(
            (ndc_bx + 1.0) * 0.5 * screen_w,
            (1.0 - ndc_by) * 0.5 * screen_h,
        );

        Self::add_line_segment(vertices, indices, sa, sb, width, color);
    }

    fn add_wireframe_sphere(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let center_vec = center.to_vec3();
        let r = radius as f32;

        for &(u, v) in &[
            (glam::Vec3::X, glam::Vec3::Y),
            (glam::Vec3::X, glam::Vec3::Z),
            (glam::Vec3::Y, glam::Vec3::Z),
        ] {
            let pts = Camera3D::circle_points(center_vec, u, v, r, segments);
            for i in 0..pts.len() {
                let j = (i + 1) % pts.len();
                let p1 = Point3D::from_vec3(pts[i]);
                let p2 = Point3D::from_vec3(pts[j]);
                Self::add_line_3d(
                    vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_wireframe_cube(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        size: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = size * 0.5;
        let x = center.x;
        let y = center.y;
        let z = center.z;

        let corners = [
            Point3D::new(x - h, y - h, z - h),
            Point3D::new(x + h, y - h, z - h),
            Point3D::new(x + h, y + h, z - h),
            Point3D::new(x - h, y + h, z - h),
            Point3D::new(x - h, y - h, z + h),
            Point3D::new(x + h, y - h, z + h),
            Point3D::new(x + h, y + h, z + h),
            Point3D::new(x - h, y + h, z + h),
        ];

        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];

        for &(i, j) in &edges {
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &corners[i],
                &corners[j],
                width,
                color,
                screen_w,
                screen_h,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_wireframe_tetrahedron(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        edge_length: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let tetrahedron = Tetrahedron3D::new(*center, edge_length);
        let points = tetrahedron.vertices();
        for [start, end] in tetrahedron.edges() {
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &points[start],
                &points[end],
                width,
                color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_wireframe_pyramid(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        base_size: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = base_size * 0.5;
        let (cx, cy, cz) = (base_center.x, base_center.y, base_center.z);

        let base_corners = [
            Point3D::new(cx - h, cy, cz - h),
            Point3D::new(cx + h, cy, cz - h),
            Point3D::new(cx + h, cy, cz + h),
            Point3D::new(cx - h, cy, cz + h),
        ];

        for i in 0..4 {
            let j = (i + 1) % 4;
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &base_corners[i],
                &base_corners[j],
                width,
                color,
                screen_w,
                screen_h,
            );
            Self::add_line_3d(
                vertices,
                indices,
                camera,
                &base_corners[i],
                apex,
                width,
                color,
                screen_w,
                screen_h,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_wireframe_cone(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let base_vec = base_center.to_vec3();
        let r = radius as f32;

        let pts = Camera3D::circle_points(base_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);

        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            let p1 = Point3D::from_vec3(pts[i]);
            let p2 = Point3D::from_vec3(pts[j]);
            Self::add_line_3d(
                vertices, indices, camera, &p1, &p2, width, color, screen_w, screen_h,
            );

            if i % 4 == 0 {
                Self::add_line_3d(
                    vertices, indices, camera, &p1, apex, width, color, screen_w, screen_h,
                );
            }
        }
    }

    fn add_wireframe_cylinder(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        top_center: &Point3D,
        radius: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segments = 32;
        let base_vec = base_center.to_vec3();
        let top_vec = top_center.to_vec3();
        let r = radius as f32;

        let base_pts = Camera3D::circle_points(base_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);
        let top_pts = Camera3D::circle_points(top_vec, glam::Vec3::X, glam::Vec3::Z, r, segments);

        for i in 0..base_pts.len() {
            let j = (i + 1) % base_pts.len();
            let bp1 = Point3D::from_vec3(base_pts[i]);
            let bp2 = Point3D::from_vec3(base_pts[j]);
            let tp1 = Point3D::from_vec3(top_pts[i]);
            let tp2 = Point3D::from_vec3(top_pts[j]);

            Self::add_line_3d(
                vertices, indices, camera, &bp1, &bp2, width, color, screen_w, screen_h,
            );
            Self::add_line_3d(
                vertices, indices, camera, &tp1, &tp2, width, color, screen_w, screen_h,
            );

            if i % 8 == 0 {
                Self::add_line_3d(
                    vertices, indices, camera, &bp1, &tp1, width, color, screen_w, screen_h,
                );
            }
        }
    }

    fn add_wireframe_torus(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        r_major: f64,
        r_minor: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 32usize;
        let v_steps = 16usize;
        for i in 0..u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let u_next = (i + 1) as f64 / u_steps as f64 * std::f64::consts::TAU;
            for j in 0..v_steps {
                let v = j as f64 / v_steps as f64 * std::f64::consts::TAU;
                let x = (r_major + r_minor * v.cos()) * u.cos() + center.x;
                let y = (r_major + r_minor * v.cos()) * u.sin() + center.y;
                let z = r_minor * v.sin() + center.z;
                Self::add_line_3d_circle_segment(
                    vertices,
                    indices,
                    camera,
                    Point3D::new(x, y, z),
                    center,
                    r_major,
                    u,
                    u_next,
                    width,
                    color,
                    screen_w,
                    screen_h,
                );
            }
        }
    }

    fn add_line_3d_circle_segment(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        p: Point3D,
        _center: &Point3D,
        _r: f64,
        _u1: f64,
        _u2: f64,
        width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        if let Some(ps) = camera.project(&p, screen_w, screen_h) {
            let s = glam::Vec2::new(ps.0, ps.1);
            Self::add_rect(
                vertices,
                indices,
                s,
                width.max(0.5) * 2.0,
                width.max(0.5) * 2.0,
                color,
            );
        }
    }

    fn add_wireframe_moebius(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width_r: f64,
        line_width: f32,
        color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let steps = 64usize;
        let mut prev: Option<Point3D> = None;
        for i in 0..=steps {
            let t = i as f64 / steps as f64 * std::f64::consts::TAU;
            let s = t * 0.5;
            let x = (radius + width_r * s.cos()) * t.cos() + center.x;
            let y = (radius + width_r * s.cos()) * t.sin() + center.y;
            let z = width_r * s.sin() + center.z;
            let p = Point3D::new(x, y, z);
            if let Some(prev_p) = prev {
                Self::add_line_3d(
                    vertices, indices, camera, &prev_p, &p, line_width, color, screen_w, screen_h,
                );
            }
            prev = Some(p);
        }
    }

    fn add_surface_mesh(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        surface: &grafito_core::Surface3DObj,
        variables: &std::collections::HashMap<String, f64>,
        screen_w: f32,
        screen_h: f32,
    ) {
        let res = surface.mesh_res.min(128);
        let grid =
            grafito_core::parametric_sampling::samples_or_compute_surface(surface, res, variables);
        if grid.is_empty() || grid[0].is_empty() {
            return;
        }
        let n = grid.len().saturating_sub(1);
        let m = grid[0].len().saturating_sub(1);

        for i in 0..=n {
            for j in 0..=m {
                let p = grid[i][j];
                if !surface_point_visible(&p) {
                    continue;
                }
                if i < n {
                    let p_right = grid[i + 1][j];
                    if surface_point_visible(&p_right) {
                        Self::add_line_3d(
                            vertices,
                            indices,
                            camera,
                            &p,
                            &p_right,
                            surface.width,
                            surface.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
                if j < m {
                    let p_down = grid[i][j + 1];
                    if surface_point_visible(&p_down) {
                        Self::add_line_3d(
                            vertices,
                            indices,
                            camera,
                            &p,
                            &p_down,
                            surface.width,
                            surface.color,
                            screen_w,
                            screen_h,
                        );
                    }
                }
            }
        }
    }

    fn face_normal(a: &Point3D, b: &Point3D, c: &Point3D) -> glam::Vec3 {
        let u = glam::Vec3::new((b.x - a.x) as f32, (b.y - a.y) as f32, (b.z - a.z) as f32);
        let v = glam::Vec3::new((c.x - a.x) as f32, (c.y - a.y) as f32, (c.z - a.z) as f32);
        let n = u.cross(v);
        let len = n.length();
        if len < 1e-10 {
            glam::Vec3::new(0.0, 1.0, 0.0)
        } else {
            n / len
        }
    }

    fn icosphere(subdivisions: usize) -> (Vec<(f64, f64, f64)>, Vec<u32>) {
        let t = (1.0 + 5.0f64.sqrt()) / 2.0;
        let mut verts = vec![
            (-1.0, t, 0.0),
            (1.0, t, 0.0),
            (-1.0, -t, 0.0),
            (1.0, -t, 0.0),
            (0.0, -1.0, t),
            (0.0, 1.0, t),
            (0.0, -1.0, -t),
            (0.0, 1.0, -t),
            (t, 0.0, -1.0),
            (t, 0.0, 1.0),
            (-t, 0.0, -1.0),
            (-t, 0.0, 1.0),
        ];
        for v in &mut verts {
            let mag = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt();
            v.0 /= mag;
            v.1 /= mag;
            v.2 /= mag;
        }
        let mut indices: Vec<u32> = vec![
            0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7,
            6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10,
            8, 6, 7, 9, 8, 1,
        ];
        for _ in 0..subdivisions {
            let mut new_indices = Vec::new();
            let mut midpoints = std::collections::HashMap::new();
            let mut get_mid = |v1: u32, v2: u32| -> u32 {
                let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
                *midpoints.entry(key).or_insert_with(|| {
                    let a = verts[v1 as usize];
                    let b = verts[v2 as usize];
                    let x = (a.0 + b.0) / 2.0;
                    let y = (a.1 + b.1) / 2.0;
                    let z = (a.2 + b.2) / 2.0;
                    let mag = (x * x + y * y + z * z).sqrt();
                    verts.push((x / mag, y / mag, z / mag));
                    (verts.len() - 1) as u32
                })
            };
            for tri in indices.chunks(3) {
                let (a, b, c) = (tri[0], tri[1], tri[2]);
                let m1 = get_mid(a, b);
                let m2 = get_mid(b, c);
                let m3 = get_mid(c, a);
                new_indices.extend_from_slice(&[a, m1, m3, m1, b, m2, m3, m2, c, m1, m2, m3]);
            }
            indices = new_indices;
        }
        (verts, indices)
    }

    fn add_solid_triangle_3d(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        p0: &Point3D,
        n0: glam::Vec3,
        p1: &Point3D,
        n1: glam::Vec3,
        p2: &Point3D,
        n2: glam::Vec3,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        let c0 = calculate_lighting(fill_color, n0, light_dir);
        let c1 = calculate_lighting(fill_color, n1, light_dir);
        let c2 = calculate_lighting(fill_color, n2, light_dir);
        if let (Some(s0), Some(s1), Some(s2)) = (
            camera.project(p0, screen_w, screen_h),
            camera.project(p1, screen_w, screen_h),
            camera.project(p2, screen_w, screen_h),
        ) {
            if ![s0, s1, s2]
                .into_iter()
                .all(|(x, y)| screen_point_is_renderable(glam::Vec2::new(x, y)))
                || !color_is_renderable(c0)
                || !color_is_renderable(c1)
                || !color_is_renderable(c2)
            {
                return;
            }
            let Some(base) = reserve_geometry(vertices, indices, 3, 3) else {
                return;
            };
            vertices.push(Vertex::new(s0.0, s0.1, c0));
            vertices.push(Vertex::new(s1.0, s1.1, c1));
            vertices.push(Vertex::new(s2.0, s2.1, c2));
            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }

    fn add_solid_sphere(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let level = 2;
        let (mesh_positions, mesh_indices) = Self::icosphere(level);
        let _light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        for tri in mesh_indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            let v0 = mesh_positions[i0];
            let v1 = mesh_positions[i1];
            let v2 = mesh_positions[i2];
            let p0 = Point3D::new(
                center.x + v0.0 * radius,
                center.y + v0.1 * radius,
                center.z + v0.2 * radius,
            );
            let p1 = Point3D::new(
                center.x + v1.0 * radius,
                center.y + v1.1 * radius,
                center.z + v1.2 * radius,
            );
            let p2 = Point3D::new(
                center.x + v2.0 * radius,
                center.y + v2.1 * radius,
                center.z + v2.2 * radius,
            );
            let n = glam::Vec3::new(v0.0 as f32, v0.1 as f32, v0.2 as f32);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &p0, n, &p1, n, &p2, n, fill_color, screen_w, screen_h,
            );
        }
    }

    fn add_solid_cube(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        size: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = size * 0.5;
        let corners = [
            Point3D::new(center.x - h, center.y - h, center.z - h),
            Point3D::new(center.x + h, center.y - h, center.z - h),
            Point3D::new(center.x + h, center.y + h, center.z - h),
            Point3D::new(center.x - h, center.y + h, center.z - h),
            Point3D::new(center.x - h, center.y - h, center.z + h),
            Point3D::new(center.x + h, center.y - h, center.z + h),
            Point3D::new(center.x + h, center.y + h, center.z + h),
            Point3D::new(center.x - h, center.y + h, center.z + h),
        ];
        let faces: [(usize, usize, usize, usize, glam::Vec3); 6] = [
            (0, 1, 2, 3, glam::Vec3::new(0.0, 0.0, -1.0)),
            (4, 5, 6, 7, glam::Vec3::new(0.0, 0.0, 1.0)),
            (0, 1, 5, 4, glam::Vec3::new(0.0, -1.0, 0.0)),
            (2, 3, 7, 6, glam::Vec3::new(0.0, 1.0, 0.0)),
            (0, 3, 7, 4, glam::Vec3::new(-1.0, 0.0, 0.0)),
            (1, 2, 6, 5, glam::Vec3::new(1.0, 0.0, 0.0)),
        ];
        for (a, b, c, d, n) in &faces {
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &corners[*a],
                *n,
                &corners[*b],
                *n,
                &corners[*c],
                *n,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &corners[*a],
                *n,
                &corners[*c],
                *n,
                &corners[*d],
                *n,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_tetrahedron(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        edge_length: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let tetrahedron = Tetrahedron3D::new(*center, edge_length);
        let points = tetrahedron.vertices();
        for [a, b, c] in tetrahedron.faces() {
            let normal = Self::face_normal(&points[a], &points[b], &points[c]);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &points[a], normal, &points[b], normal, &points[c],
                normal, fill_color, screen_w, screen_h,
            );
        }
    }

    fn add_solid_pyramid(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        base_size: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let h = base_size * 0.5;
        let base = [
            Point3D::new(base_center.x - h, base_center.y, base_center.z - h),
            Point3D::new(base_center.x + h, base_center.y, base_center.z - h),
            Point3D::new(base_center.x + h, base_center.y, base_center.z + h),
            Point3D::new(base_center.x - h, base_center.y, base_center.z + h),
        ];
        let apex = *apex;
        for i in 0..4 {
            let j = (i + 1) % 4;
            let n = Self::face_normal(&base[i], &base[j], &apex);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &base[i], n, &base[j], n, &apex, n, fill_color,
                screen_w, screen_h,
            );
        }
        let n_base = glam::Vec3::new(0.0, -1.0, 0.0);
        Self::add_solid_triangle_3d(
            vertices, indices, camera, &base[0], n_base, &base[1], n_base, &base[2], n_base,
            fill_color, screen_w, screen_h,
        );
        Self::add_solid_triangle_3d(
            vertices, indices, camera, &base[0], n_base, &base[2], n_base, &base[3], n_base,
            fill_color, screen_w, screen_h,
        );
    }

    fn add_solid_cone(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        apex: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segs = 32;
        let mut circle = Vec::new();
        for i in 0..segs {
            let a = i as f64 / segs as f64 * std::f64::consts::TAU;
            circle.push(Point3D::new(
                base_center.x + radius * a.cos(),
                base_center.y,
                base_center.z + radius * a.sin(),
            ));
        }
        for i in 0..segs {
            let j = (i + 1) % segs;
            let n = Self::face_normal(&circle[i], &circle[j], apex);
            Self::add_solid_triangle_3d(
                vertices, indices, camera, &circle[i], n, &circle[j], n, apex, n, fill_color,
                screen_w, screen_h,
            );
        }
        let n_base = glam::Vec3::new(0.0, -1.0, 0.0);
        for i in 0..segs {
            let j = (i + 1) % segs;
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &circle[i],
                n_base,
                &circle[j],
                n_base,
                base_center,
                n_base,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_cylinder(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        base_center: &Point3D,
        top_center: &Point3D,
        radius: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let segs = 32;
        let mut b_circle = Vec::new();
        let mut t_circle = Vec::new();
        for i in 0..segs {
            let a = i as f64 / segs as f64 * std::f64::consts::TAU;
            b_circle.push(Point3D::new(
                base_center.x + radius * a.cos(),
                base_center.y,
                base_center.z + radius * a.sin(),
            ));
            t_circle.push(Point3D::new(
                top_center.x + radius * a.cos(),
                top_center.y,
                top_center.z + radius * a.sin(),
            ));
        }
        for i in 0..segs {
            let j = (i + 1) % segs;
            let n = Self::face_normal(&b_circle[i], &t_circle[i], &b_circle[j]);
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &b_circle[i],
                n,
                &t_circle[i],
                n,
                &b_circle[j],
                n,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &t_circle[i],
                n,
                &t_circle[j],
                n,
                &b_circle[j],
                n,
                fill_color,
                screen_w,
                screen_h,
            );
        }
        let n_bot = glam::Vec3::new(0.0, -1.0, 0.0);
        let n_top = glam::Vec3::new(0.0, 1.0, 0.0);
        for i in 0..segs {
            let j = (i + 1) % segs;
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &b_circle[i],
                n_bot,
                &b_circle[j],
                n_bot,
                base_center,
                n_bot,
                fill_color,
                screen_w,
                screen_h,
            );
            Self::add_solid_triangle_3d(
                vertices,
                indices,
                camera,
                &t_circle[i],
                n_top,
                &t_circle[j],
                n_top,
                top_center,
                n_top,
                fill_color,
                screen_w,
                screen_h,
            );
        }
    }

    fn add_solid_torus(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        r_major: f64,
        r_minor: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 32usize;
        let v_steps = 16usize;
        let mut grid: Vec<Vec<(Point3D, glam::Vec3)>> = Vec::new();
        for i in 0..=u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let mut row = Vec::new();
            for j in 0..=v_steps {
                let v = j as f64 / v_steps as f64 * std::f64::consts::TAU;
                let x = (r_major + r_minor * v.cos()) * u.cos() + center.x;
                let y = r_minor * v.sin() + center.y;
                let z = (r_major + r_minor * v.cos()) * u.sin() + center.z;
                let nx = (v.cos() * u.cos()) as f32;
                let ny = v.sin() as f32;
                let nz = (v.cos() * u.sin()) as f32;
                row.push((Point3D::new(x, y, z), glam::Vec3::new(nx, ny, nz)));
            }
            grid.push(row);
        }
        let _light_dir = glam::Vec3::new(0.5, 1.0, 0.3).normalize();
        for i in 0..u_steps {
            for j in 0..v_steps {
                let (p00, n00) = grid[i][j];
                let (p10, n10) = grid[i + 1][j];
                let (p01, n01) = grid[i][j + 1];
                let (p11, n11) = grid[i + 1][j + 1];
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p10, n10, &p11, n11, fill_color,
                    screen_w, screen_h,
                );
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p11, n11, &p01, n01, fill_color,
                    screen_w, screen_h,
                );
            }
        }
    }

    fn add_solid_moebius(
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        camera: &Camera3D,
        center: &Point3D,
        radius: f64,
        width_r: f64,
        fill_color: Color,
        screen_w: f32,
        screen_h: f32,
    ) {
        let u_steps = 64usize;
        let v_steps = 8usize;
        let mut grid: Vec<Vec<(Point3D, glam::Vec3)>> = Vec::new();
        for i in 0..=u_steps {
            let u = i as f64 / u_steps as f64 * std::f64::consts::TAU;
            let mut row = Vec::new();
            for j in 0..=v_steps {
                let v = (j as f64 / v_steps as f64 - 0.5) * 2.0 * width_r;
                let cu = u.cos();
                let su = u.sin();
                let cu2 = (u * 0.5).cos();
                let su2 = (u * 0.5).sin();
                let x = (radius + v * cu2) * cu + center.x;
                let y = v * su2 + center.y;
                let z = (radius + v * cu2) * su + center.z;
                let nx = cu * cu2;
                let ny = su2;
                let nz = su * cu2;
                let mag = (nx * nx + ny * ny + nz * nz).sqrt().max(0.001);
                row.push((
                    Point3D::new(x, y, z),
                    glam::Vec3::new((nx / mag) as f32, (ny / mag) as f32, (nz / mag) as f32),
                ));
            }
            grid.push(row);
        }
        for i in 0..u_steps {
            for j in 0..v_steps {
                let (p00, n00) = grid[i][j];
                let (p10, n10) = grid[i + 1][j];
                let (p01, n01) = grid[i][j + 1];
                let (p11, n11) = grid[i + 1][j + 1];
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p10, n10, &p11, n11, fill_color,
                    screen_w, screen_h,
                );
                Self::add_solid_triangle_3d(
                    vertices, indices, camera, &p00, n00, &p11, n11, &p01, n01, fill_color,
                    screen_w, screen_h,
                );
            }
        }
    }
}
