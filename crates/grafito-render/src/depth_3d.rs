//! World-space mesh generation for the depth-enabled 3D renderer.
//!
//! Every position stays in document `(x, y, z)` order. `Camera3D::mvp` is the
//! only world-to-camera transform applied by this path.

use grafito_core::{
    Document, GeoObject, RegularPolychoron4DObj, RegularPolytopeNDObj, RenderQuality, Surface3DObj,
};
use grafito_geometry::{
    curve_3d_segment_is_continuous, rotate_nd_in_plane, Camera3D, Color, NdPerspectiveProjection,
    Point3D, Point4D, Polytope4DError, Polytope4DTopology, RegularPolychoron, RegularPolytopeError,
    RegularPolytopeFamily, RegularPolytopeProjectionPlan, RegularPolytopeTopology,
    MAX_REGULAR_POLYTOPE_DIMENSION, MAX_WORLD_COORDINATE, MIN_REGULAR_POLYTOPE_DIMENSION,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

/// Per-stream limits keep one persisted scene from growing callback buffers
/// without bound. Each quad uses four vertices and six indices.
pub const MAX_WORLD_MESH_VERTICES: usize = 524_288;
pub const MAX_WORLD_MESH_INDICES: usize = 786_432;
pub const MAX_WORLD_MESH_ATTRACTORS: usize = 8;
pub const MAX_WORLD_MESH_ATTRACTOR_STEPS: usize = 16_000;
/// Maximum CPU sample/evaluation units consumed while building one world mesh.
pub const MAX_WORLD_MESH_WORK_UNITS: usize = 65_536;

const POLYCHORON_TOPOLOGY_CACHE_SLOTS: usize = 6;
const GENERIC_POLYTOPE_DIMENSION_SLOTS: usize =
    MAX_REGULAR_POLYTOPE_DIMENSION - MIN_REGULAR_POLYTOPE_DIMENSION + 1;
const GENERIC_POLYTOPE_TOPOLOGY_CACHE_SLOTS: usize = 3 * GENERIC_POLYTOPE_DIMENSION_SLOTS;
const MAX_PROJECTED_POLYTOPE_CACHE_ENTRIES: usize = 32;

struct PolychoronTopologyCache {
    entries: Vec<OnceLock<Result<Arc<Polytope4DTopology>, Polytope4DError>>>,
}

struct GenericPolytopeTopologyCache {
    entries: Vec<OnceLock<Result<Arc<RegularPolytopeTopology>, RegularPolytopeError>>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum ProjectedPolytopeKey {
    Polychoron {
        kind: RegularPolychoron,
        scale_bits: u64,
        rotation_bits: [u64; 6],
    },
    Generic {
        family: RegularPolytopeFamily,
        dimension: usize,
        scale_bits: u64,
        rotation_bits: Vec<u64>,
    },
}

struct ProjectedPolytopeCache {
    entries: VecDeque<(ProjectedPolytopeKey, Arc<[Point3D]>)>,
}

#[derive(Clone)]
enum ProjectedRegularPolytopeTopology {
    Polychoron(Arc<Polytope4DTopology>),
    Generic(Arc<RegularPolytopeTopology>),
}

/// Topologia regular proyectada que puede leerse sin mutar las caches del renderizador.
///
/// Las coordenadas proyectadas y la topologia canónica se comparten mediante
/// referencias inmutables; los consumidores solo reciben vistas de lectura.
#[derive(Clone)]
pub struct ProjectedRegularPolytope {
    vertices: Arc<[Point3D]>,
    topology: ProjectedRegularPolytopeTopology,
}

impl ProjectedRegularPolytope {
    /// Vertices proyectados en R3, en el orden de la topologia canonica.
    pub fn vertices(&self) -> &[Point3D] {
        &self.vertices
    }

    /// Aristas ordenadas e indexadas sobre [`Self::vertices`].
    pub fn edges(&self) -> &[[usize; 2]] {
        match &self.topology {
            ProjectedRegularPolytopeTopology::Polychoron(topology) => &topology.edges,
            ProjectedRegularPolytopeTopology::Generic(topology) => &topology.edges,
        }
    }

    /// Caras poligonales ordenadas para politopos 4D.
    ///
    /// Las familias genericas N-D se mantienen wireframe y devuelven una vista vacia.
    pub fn faces(&self) -> &[Vec<usize>] {
        match &self.topology {
            ProjectedRegularPolytopeTopology::Polychoron(topology) => &topology.faces,
            ProjectedRegularPolytopeTopology::Generic(_) => &[],
        }
    }
}

static POLYCHORON_TOPOLOGY_CACHE: OnceLock<Result<PolychoronTopologyCache, ()>> = OnceLock::new();
static GENERIC_POLYTOPE_TOPOLOGY_CACHE: OnceLock<Result<GenericPolytopeTopologyCache, ()>> =
    OnceLock::new();
static PROJECTED_POLYTOPE_CACHE: OnceLock<Mutex<ProjectedPolytopeCache>> = OnceLock::new();

fn once_lock_slots<T>(count: usize) -> Result<Vec<OnceLock<T>>, ()> {
    let mut entries = Vec::new();
    entries.try_reserve_exact(count).map_err(|_| ())?;
    for _ in 0..count {
        entries.push(OnceLock::new());
    }
    Ok(entries)
}

fn polychoron_cache_slot(kind: RegularPolychoron) -> usize {
    match kind {
        RegularPolychoron::Pentachoron => 0,
        RegularPolychoron::Tesseract => 1,
        RegularPolychoron::SixteenCell => 2,
        RegularPolychoron::TwentyFourCell => 3,
        RegularPolychoron::OneTwentyCell => 4,
        RegularPolychoron::SixHundredCell => 5,
    }
}

fn generic_polytope_cache_slot(family: RegularPolytopeFamily, dimension: usize) -> Option<usize> {
    let dimension_slot = dimension.checked_sub(MIN_REGULAR_POLYTOPE_DIMENSION)?;
    if dimension_slot >= GENERIC_POLYTOPE_DIMENSION_SLOTS {
        return None;
    }
    let family_slot: usize = match family {
        RegularPolytopeFamily::Simplex => 0,
        RegularPolytopeFamily::Hypercube => 1,
        RegularPolytopeFamily::CrossPolytope => 2,
    };
    family_slot
        .checked_mul(GENERIC_POLYTOPE_DIMENSION_SLOTS)?
        .checked_add(dimension_slot)
}

fn cached_polychoron_topology(kind: RegularPolychoron) -> Option<Arc<Polytope4DTopology>> {
    let cache = POLYCHORON_TOPOLOGY_CACHE.get_or_init(|| {
        once_lock_slots(POLYCHORON_TOPOLOGY_CACHE_SLOTS)
            .map(|entries| PolychoronTopologyCache { entries })
    });
    let cache = cache.as_ref().ok()?;
    let entry = cache.entries.get(polychoron_cache_slot(kind))?;
    entry
        .get_or_init(|| {
            kind.topology().and_then(|topology| {
                topology.validate()?;
                Ok(Arc::new(topology))
            })
        })
        .as_ref()
        .ok()
        .map(Arc::clone)
}

fn cached_generic_polytope_topology(
    family: RegularPolytopeFamily,
    dimension: usize,
) -> Option<Arc<RegularPolytopeTopology>> {
    let cache = GENERIC_POLYTOPE_TOPOLOGY_CACHE.get_or_init(|| {
        once_lock_slots(GENERIC_POLYTOPE_TOPOLOGY_CACHE_SLOTS)
            .map(|entries| GenericPolytopeTopologyCache { entries })
    });
    let cache = cache.as_ref().ok()?;
    let entry = cache
        .entries
        .get(generic_polytope_cache_slot(family, dimension)?)?;
    entry
        .get_or_init(|| {
            family.topology(dimension).and_then(|topology| {
                topology.validate()?;
                Ok(Arc::new(topology))
            })
        })
        .as_ref()
        .ok()
        .map(Arc::clone)
}

fn projected_polytope_cache() -> &'static Mutex<ProjectedPolytopeCache> {
    PROJECTED_POLYTOPE_CACHE.get_or_init(|| {
        Mutex::new(ProjectedPolytopeCache {
            entries: VecDeque::new(),
        })
    })
}

fn lock_projected_polytope_cache() -> MutexGuard<'static, ProjectedPolytopeCache> {
    match projected_polytope_cache().lock() {
        Ok(cache) => cache,
        Err(poisoned) => {
            // A cache never changes topology. Discard interrupted projected data and retry safely.
            let mut cache = poisoned.into_inner();
            cache.entries.clear();
            cache
        }
    }
}

fn cached_projected_polytope_vertices<F>(
    key: ProjectedPolytopeKey,
    project: F,
) -> Result<Arc<[Point3D]>, ()>
where
    F: FnOnce() -> Result<Vec<Point3D>, ()>,
{
    if let Some((_, vertices)) = lock_projected_polytope_cache()
        .entries
        .iter()
        .find(|(cached_key, _)| cached_key == &key)
    {
        return Ok(Arc::clone(vertices));
    }

    let vertices: Arc<[Point3D]> = Arc::from(project()?);
    let mut cache = lock_projected_polytope_cache();
    if let Some((_, cached_vertices)) = cache
        .entries
        .iter()
        .find(|(cached_key, _)| cached_key == &key)
    {
        return Ok(Arc::clone(cached_vertices));
    }
    if cache.entries.len() == MAX_PROJECTED_POLYTOPE_CACHE_ENTRIES {
        cache.entries.pop_front();
    }
    cache.entries.try_reserve(1).map_err(|_| ())?;
    cache.entries.push_back((key, Arc::clone(&vertices)));
    Ok(vertices)
}

/// Salida máxima de un objeto en los streams opaco y wire del `WorldMesh`.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorldMeshStreamUsage {
    opaque_vertices: usize,
    opaque_indices: usize,
    wire_vertices: usize,
    wire_indices: usize,
}

impl WorldMeshStreamUsage {
    /// Construye la salida para triángulos opacos, triángulos wire y segmentos wire.
    pub fn from_primitives(
        opaque_triangles: usize,
        wire_triangles: usize,
        wire_segments: usize,
    ) -> Option<Self> {
        let opaque_vertices = opaque_triangles.checked_mul(3)?;
        let opaque_indices = opaque_vertices;
        let wire_triangle_vertices = wire_triangles.checked_mul(3)?;
        let wire_triangle_indices = wire_triangle_vertices;
        let wire_vertices = wire_triangle_vertices.checked_add(wire_segments.checked_mul(4)?)?;
        let wire_indices = wire_triangle_indices.checked_add(wire_segments.checked_mul(6)?)?;
        Some(Self {
            opaque_vertices,
            opaque_indices,
            wire_vertices,
            wire_indices,
        })
    }
}

/// Presupuesto conservador de los cuatro streams que componen un `WorldMesh`.
///
/// El callback de la aplicación lo usa antes de disparar compute para no llenar
/// cachés de objetos que el generador de malla no podrá consumir.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorldMeshOutputBudget {
    opaque_vertices: usize,
    opaque_indices: usize,
    wire_vertices: usize,
    wire_indices: usize,
}

impl WorldMeshOutputBudget {
    /// Devuelve si la salida completa cabe sin exceder ningún stream.
    pub fn fits(&self, usage: WorldMeshStreamUsage) -> bool {
        self.opaque_vertices
            .checked_add(usage.opaque_vertices)
            .is_some_and(|count| count <= MAX_WORLD_MESH_VERTICES)
            && self
                .opaque_indices
                .checked_add(usage.opaque_indices)
                .is_some_and(|count| count <= MAX_WORLD_MESH_INDICES)
            && self
                .wire_vertices
                .checked_add(usage.wire_vertices)
                .is_some_and(|count| count <= MAX_WORLD_MESH_VERTICES)
            && self
                .wire_indices
                .checked_add(usage.wire_indices)
                .is_some_and(|count| count <= MAX_WORLD_MESH_INDICES)
    }

    /// Carga una cota conservadora, saturando cada stream en su capacidad máxima.
    pub fn consume(&mut self, usage: WorldMeshStreamUsage) {
        self.opaque_vertices = self
            .opaque_vertices
            .saturating_add(usage.opaque_vertices)
            .min(MAX_WORLD_MESH_VERTICES);
        self.opaque_indices = self
            .opaque_indices
            .saturating_add(usage.opaque_indices)
            .min(MAX_WORLD_MESH_INDICES);
        self.wire_vertices = self
            .wire_vertices
            .saturating_add(usage.wire_vertices)
            .min(MAX_WORLD_MESH_VERTICES);
        self.wire_indices = self
            .wire_indices
            .saturating_add(usage.wire_indices)
            .min(MAX_WORLD_MESH_INDICES);
    }
}

fn solid_stream_triangles(triangles: usize, alpha: Option<f32>) -> (usize, usize) {
    match alpha {
        Some(alpha) if alpha < 0.999 => (0, triangles),
        Some(_) => (triangles, 0),
        None => (0, 0),
    }
}

fn polychoron_face_triangle_count(kind: RegularPolychoron) -> Option<usize> {
    let vertices_per_face = match kind {
        RegularPolychoron::Pentachoron
        | RegularPolychoron::SixteenCell
        | RegularPolychoron::TwentyFourCell
        | RegularPolychoron::SixHundredCell => 3,
        RegularPolychoron::Tesseract => 4,
        RegularPolychoron::OneTwentyCell => 5,
    };
    kind.expected_counts()
        .faces
        .checked_mul(vertices_per_face - 2)
}

/// Conservative stream output for one visible object, matching `WorldMesh`
/// routing of translucent solid triangles into the non-depth-writing stream.
pub fn world_mesh_output_usage(object: &GeoObject) -> Option<WorldMeshStreamUsage> {
    world_mesh_output_usage_for_quality(object, RenderQuality::Normal)
}

/// Conservative stream output for one visible object at a specific render quality.
///
/// This deliberately uses published f-vector counts instead of materializing a
/// topology, so canvas preflight does not regenerate the 120-cell or 600-cell.
pub fn world_mesh_output_usage_for_quality(
    object: &GeoObject,
    quality: RenderQuality,
) -> Option<WorldMeshStreamUsage> {
    let (opaque_triangles, wire_triangles, wire_segments) = match object {
        GeoObject::Point3D(point) => {
            let (opaque, wire) = solid_stream_triangles(2, Some(point.color.a));
            (opaque, wire, 0)
        }
        GeoObject::Segment3D(_) | GeoObject::Line3D(_) => (0, 0, 1),
        GeoObject::Plane3D(plane) => {
            let (opaque, wire) = solid_stream_triangles(2, Some(plane.opacity));
            (opaque, wire, 4)
        }
        GeoObject::Sphere3D(sphere) => {
            let (opaque, wire) =
                solid_stream_triangles(1_600, sphere.fill_color.map(|fill| fill.a));
            (opaque, wire, 96)
        }
        GeoObject::Cube3D(cube) => {
            let (opaque, wire) = solid_stream_triangles(12, cube.fill_color.map(|fill| fill.a));
            (opaque, wire, 12)
        }
        GeoObject::Tetrahedron3D(tetrahedron) => {
            let (opaque, wire) =
                solid_stream_triangles(4, tetrahedron.fill_color.map(|fill| fill.a));
            (opaque, wire, 6)
        }
        GeoObject::RegularPolychoron4D(polychoron) => {
            let triangles = if matches!(quality, RenderQuality::Preview) {
                0
            } else {
                polychoron_face_triangle_count(polychoron.kind)?
            };
            let (opaque, wire) =
                solid_stream_triangles(triangles, polychoron.fill_color.map(|fill| fill.a));
            (opaque, wire, polychoron.kind.expected_counts().edges)
        }
        GeoObject::RegularPolytopeND(polytope) => {
            // Generic N-D topologies currently materialize vertices and edges only.
            (
                0,
                0,
                polytope
                    .family
                    .expected_counts(polytope.dimension)
                    .ok()?
                    .edges,
            )
        }
        GeoObject::Pyramid3D(pyramid) => {
            let (opaque, wire) = solid_stream_triangles(6, pyramid.fill_color.map(|fill| fill.a));
            (opaque, wire, 8)
        }
        GeoObject::Cone3D(cone) => {
            let (opaque, wire) = solid_stream_triangles(64, cone.fill_color.map(|fill| fill.a));
            (opaque, wire, 40)
        }
        GeoObject::Cylinder3D(cylinder) => {
            let (opaque, wire) =
                solid_stream_triangles(128, cylinder.fill_color.map(|fill| fill.a));
            (opaque, wire, 68)
        }
        GeoObject::Torus3D(torus) => {
            let (opaque, wire) = solid_stream_triangles(1_024, Some(torus.color.a));
            (opaque, wire, 304)
        }
        GeoObject::MoebiusStrip(strip) => {
            let (opaque, wire) = solid_stream_triangles(1_024, Some(strip.color.a));
            (opaque, wire, 352)
        }
        GeoObject::Surface3D(surface) => {
            let cells = surface.mesh_res.clamp(2, 128).checked_pow(2)?;
            let triangles = cells.checked_mul(2)?;
            let (opaque, wire) = solid_stream_triangles(
                usize::from(surface.solid).checked_mul(triangles)?,
                surface.solid.then_some(surface.color.a),
            );
            (opaque, wire, cells.checked_mul(2)?)
        }
        GeoObject::ParametricCurve3D(_) => (0, 0, 4_000),
        GeoObject::Attractor3D(attractor) => {
            (0, 0, attractor.steps.min(MAX_WORLD_MESH_ATTRACTOR_STEPS))
        }
        GeoObject::VectorField3D(field) => (0, 0, crate::vector_field_3d_sample_count(field)?),
        _ => return None,
    };
    WorldMeshStreamUsage::from_primitives(opaque_triangles, wire_triangles, wire_segments)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
/// Position and color consumed by the world-space 3D GPU pipeline.
pub struct Vertex3D {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex3D {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
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

/// Separate streams preserve opaque depth writes while keeping wireframes and
/// translucent solids as depth-tested overlays rather than occluders.
#[derive(Debug)]
pub struct WorldMesh {
    pub opaque_vertices: Vec<Vertex3D>,
    pub opaque_indices: Vec<u32>,
    pub wire_vertices: Vec<Vertex3D>,
    pub wire_indices: Vec<u32>,
    complete: bool,
}

impl Default for WorldMesh {
    fn default() -> Self {
        Self {
            opaque_vertices: Vec::new(),
            opaque_indices: Vec::new(),
            wire_vertices: Vec::new(),
            wire_indices: Vec::new(),
            complete: true,
        }
    }
}

impl WorldMesh {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_part(&self.opaque_vertices, &self.opaque_indices)?;
        validate_part(&self.wire_vertices, &self.wire_indices)
    }

    /// Indica si todos los objetos visibles admitidos por esta ruta caben en la malla.
    pub fn is_complete(&self) -> bool {
        self.complete
    }
}

fn validate_part(vertices: &[Vertex3D], indices: &[u32]) -> Result<(), &'static str> {
    if !indices.len().is_multiple_of(3) {
        return Err("3D mesh indices must form triangles");
    }
    if vertices.iter().any(|vertex| {
        !vertex.position.iter().all(|value| value.is_finite())
            || !vertex.color.iter().all(|value| value.is_finite())
    }) {
        return Err("3D mesh contains non-finite vertex data");
    }
    if indices
        .iter()
        .any(|index| usize::try_from(*index).map_or(true, |index| index >= vertices.len()))
    {
        return Err("3D mesh index is outside its vertex stream");
    }
    Ok(())
}

pub fn point_is_renderable(point: Point3D) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.z.is_finite()
        && point.x.abs() <= MAX_WORLD_COORDINATE
        && point.y.abs() <= MAX_WORLD_COORDINATE
        && point.z.abs() <= MAX_WORLD_COORDINATE
}

fn color_is_renderable(color: Color) -> bool {
    color.to_array().iter().all(|value| value.is_finite())
}

fn reserve_mesh_part(
    vertices: &mut Vec<Vertex3D>,
    indices: &mut Vec<u32>,
    additional_vertices: usize,
    additional_indices: usize,
) -> Option<u32> {
    if !mesh_part_has_room(vertices, indices, additional_vertices, additional_indices) {
        return None;
    }
    vertices.try_reserve(additional_vertices).ok()?;
    indices.try_reserve(additional_indices).ok()?;
    u32::try_from(vertices.len()).ok()
}

fn mesh_part_has_room(
    vertices: &[Vertex3D],
    indices: &[u32],
    additional_vertices: usize,
    additional_indices: usize,
) -> bool {
    vertices
        .len()
        .checked_add(additional_vertices)
        .is_some_and(|count| count <= MAX_WORLD_MESH_VERTICES && count <= u32::MAX as usize)
        && indices
            .len()
            .checked_add(additional_indices)
            .is_some_and(|count| count <= MAX_WORLD_MESH_INDICES)
}

fn world_mesh_is_output_exhausted(mesh: &WorldMesh) -> bool {
    !mesh_part_has_room(&mesh.opaque_vertices, &mesh.opaque_indices, 3, 3)
        && !mesh_part_has_room(&mesh.wire_vertices, &mesh.wire_indices, 4, 6)
}

fn wire_object_fits(mesh: &WorldMesh, segment_count: usize) -> bool {
    let Some(vertices) = segment_count.checked_mul(4) else {
        return false;
    };
    let Some(indices) = segment_count.checked_mul(6) else {
        return false;
    };
    mesh_part_has_room(&mesh.wire_vertices, &mesh.wire_indices, vertices, indices)
}

fn surface_output_fits(mesh: &WorldMesh, surface: &Surface3DObj, resolution: usize) -> bool {
    let Some(cells) = resolution.checked_mul(resolution) else {
        return false;
    };
    let Some(wire_segments) = cells.checked_mul(2) else {
        return false;
    };
    let Some(wire_vertices) = wire_segments.checked_mul(4) else {
        return false;
    };
    let Some(wire_indices) = wire_segments.checked_mul(6) else {
        return false;
    };
    if !surface.solid {
        return mesh_part_has_room(
            &mesh.wire_vertices,
            &mesh.wire_indices,
            wire_vertices,
            wire_indices,
        );
    }
    let Some(solid_vertices) = cells.checked_mul(6) else {
        return false;
    };
    if surface.color.a < 0.999 {
        let Some(total_vertices) = wire_vertices.checked_add(solid_vertices) else {
            return false;
        };
        let Some(total_indices) = wire_indices.checked_add(solid_vertices) else {
            return false;
        };
        mesh_part_has_room(
            &mesh.wire_vertices,
            &mesh.wire_indices,
            total_vertices,
            total_indices,
        )
    } else {
        mesh_part_has_room(
            &mesh.wire_vertices,
            &mesh.wire_indices,
            wire_vertices,
            wire_indices,
        ) && mesh_part_has_room(
            &mesh.opaque_vertices,
            &mesh.opaque_indices,
            solid_vertices,
            solid_vertices,
        )
    }
}

fn surface_work_units(resolution: usize) -> Option<usize> {
    resolution
        .checked_add(1)?
        .checked_mul(resolution.checked_add(1)?)
}

struct WorldMeshWorkBudget {
    remaining: usize,
}

impl WorldMeshWorkBudget {
    fn new() -> Self {
        Self {
            remaining: MAX_WORLD_MESH_WORK_UNITS,
        }
    }

    fn reserve(&mut self, units: usize) -> bool {
        let Some(remaining) = self.remaining.checked_sub(units) else {
            return false;
        };
        self.remaining = remaining;
        true
    }
}

fn world_mesh_scene_fits_limits(document: &Document) -> bool {
    const CURVE_STEPS: usize = 4_000;

    let mut output_budget = WorldMeshOutputBudget::default();
    let mut work_budget = WorldMeshWorkBudget::new();
    let mut attractor_count = 0usize;

    for (_, object) in document.objects_iter() {
        if !object.is_visible() {
            continue;
        }
        if matches!(object, GeoObject::Tetrahedron3D(tetrahedron)
            if !grafito_geometry::Tetrahedron3D::new(
                tetrahedron.center,
                tetrahedron.edge_length,
            )
            .is_renderable())
        {
            return false;
        }
        if let Some(usage) = world_mesh_output_usage_for_quality(object, document.render_quality) {
            if !output_budget.fits(usage) {
                return false;
            }
            output_budget.consume(usage);
        }

        let work_units = match object {
            GeoObject::Surface3D(surface) => {
                let resolution = surface.mesh_res.clamp(2, 128);
                let Some(units) = surface_work_units(resolution) else {
                    return false;
                };
                units
            }
            GeoObject::ParametricCurve3D(_) => CURVE_STEPS + 1,
            GeoObject::Attractor3D(attractor) => {
                if attractor_count == MAX_WORLD_MESH_ATTRACTORS {
                    return false;
                }
                attractor_count += 1;
                attractor.steps.min(MAX_WORLD_MESH_ATTRACTOR_STEPS)
            }
            GeoObject::VectorField3D(field) => {
                let Some(units) = crate::vector_field_3d_sample_count(field) else {
                    continue;
                };
                units
            }
            _ => continue,
        };
        if !work_budget.reserve(work_units) {
            return false;
        }
    }
    true
}

fn vertex(point: Point3D, color: Color) -> Vertex3D {
    Vertex3D {
        position: [point.x as f32, point.y as f32, point.z as f32],
        color: color.to_array(),
    }
}

fn append_triangle(
    vertices: &mut Vec<Vertex3D>,
    indices: &mut Vec<u32>,
    a: Point3D,
    b: Point3D,
    c: Point3D,
    color: Color,
) {
    if !point_is_renderable(a)
        || !point_is_renderable(b)
        || !point_is_renderable(c)
        || !color_is_renderable(color)
    {
        return;
    }
    let Some(base) = reserve_mesh_part(vertices, indices, 3, 3) else {
        return;
    };
    vertices.extend([vertex(a, color), vertex(b, color), vertex(c, color)]);
    indices.extend([base, base + 1, base + 2]);
}

fn append_solid_triangle(mesh: &mut WorldMesh, a: Point3D, b: Point3D, c: Point3D, color: Color) {
    let (vertices, indices) = if color.a < 0.999 {
        (&mut mesh.wire_vertices, &mut mesh.wire_indices)
    } else {
        (&mut mesh.opaque_vertices, &mut mesh.opaque_indices)
    };
    append_triangle(vertices, indices, a, b, c, color);
}

fn append_point_billboard(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    point: Point3D,
    size: f32,
    color: Color,
    screen_h: f32,
) {
    if !point_is_renderable(point)
        || !size.is_finite()
        || !color_is_renderable(color)
        || !screen_h.is_finite()
        || screen_h <= 0.0
        || !camera.fov.is_finite()
    {
        return;
    }
    let camera_position = camera.position();
    let forward = camera.target - camera_position;
    if !camera_position.is_finite() || !forward.is_finite() || forward.length_squared() < 1e-12 {
        return;
    }
    let forward = forward.normalize();
    let depth = (point.to_vec3() - camera_position).dot(forward);
    if !depth.is_finite() || depth <= 0.0 {
        return;
    }
    let right = camera.right();
    let up = right.cross(forward);
    if !right.is_finite() || !up.is_finite() || up.length_squared() < 1e-12 {
        return;
    }
    let world_per_pixel = 2.0 * depth * (camera.fov.to_radians() * 0.5).tan() / screen_h;
    let half_extent = size.max(1.0) * world_per_pixel * 0.5;
    if !world_per_pixel.is_finite() || !half_extent.is_finite() {
        return;
    }
    let right = right * half_extent;
    let up = up.normalize() * half_extent;
    let center = point.to_vec3();
    let corners = [
        Point3D::from_vec3(center - right - up),
        Point3D::from_vec3(center + right - up),
        Point3D::from_vec3(center + right + up),
        Point3D::from_vec3(center - right + up),
    ];
    append_solid_triangle(mesh, corners[0], corners[1], corners[2], color);
    append_solid_triangle(mesh, corners[0], corners[2], corners[3], color);
}

fn append_wire_line(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    a: Point3D,
    b: Point3D,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    if !point_is_renderable(a)
        || !point_is_renderable(b)
        || !color_is_renderable(color)
        || !width.is_finite()
        || !screen_h.is_finite()
        || screen_h <= 0.0
        || !camera.distance.is_finite()
        || !camera.fov.is_finite()
    {
        return;
    }
    let a_vec = a.to_vec3();
    let b_vec = b.to_vec3();
    let direction = b_vec - a_vec;
    if direction.length_squared() < 1e-12 {
        return;
    }
    let to_camera = camera.position() - (a_vec + b_vec) * 0.5;
    let mut side = direction.cross(to_camera);
    if side.length_squared() < 1e-12 {
        side = direction.cross(camera.up());
    }
    if side.length_squared() < 1e-12 {
        return;
    }
    let world_per_pixel =
        2.0 * camera.distance * (camera.fov.to_radians() * 0.5).tan() / screen_h.max(1.0);
    let offset = side.normalize() * (width.max(0.5) * world_per_pixel * 0.5);
    if !world_per_pixel.is_finite() || !offset.is_finite() {
        return;
    }
    let a_minus = Point3D::from_vec3(a_vec - offset);
    let a_plus = Point3D::from_vec3(a_vec + offset);
    let b_plus = Point3D::from_vec3(b_vec + offset);
    let b_minus = Point3D::from_vec3(b_vec - offset);
    if !point_is_renderable(a_minus)
        || !point_is_renderable(a_plus)
        || !point_is_renderable(b_plus)
        || !point_is_renderable(b_minus)
    {
        return;
    }
    let Some(base) = reserve_mesh_part(&mut mesh.wire_vertices, &mut mesh.wire_indices, 4, 6)
    else {
        return;
    };
    mesh.wire_vertices.extend([
        vertex(a_minus, color),
        vertex(a_plus, color),
        vertex(b_plus, color),
        vertex(b_minus, color),
    ]);
    mesh.wire_indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn regular_polychoron_style_is_renderable(object: &RegularPolychoron4DObj) -> bool {
    object.width.is_finite()
        && object.width > 0.0
        && color_is_renderable(object.color)
        && object.fill_color.is_none_or(color_is_renderable)
}

fn regular_polytope_style_is_renderable(object: &RegularPolytopeNDObj) -> bool {
    object.width.is_finite()
        && object.width > 0.0
        && color_is_renderable(object.color)
        && object.fill_color.is_none_or(color_is_renderable)
}

fn polychoron_projection_plan(
    object: &RegularPolychoron4DObj,
    effective_rotation_angles: [f64; 6],
) -> Option<RegularPolytopeProjectionPlan> {
    if !object.scale.is_finite()
        || object.scale <= 0.0
        || object
            .rotation_angles
            .iter()
            .any(|angle| !angle.is_finite())
        || effective_rotation_angles
            .iter()
            .any(|angle| !angle.is_finite())
    {
        return None;
    }
    let plan = object.kind.projection_plan(object.scale).ok()?;
    plan.ensure_within_coordinate_limit(MAX_WORLD_COORDINATE)
        .ok()?;
    Some(plan)
}

fn generic_polytope_projection_plan(
    object: &RegularPolytopeNDObj,
    effective_rotation_angles: &[f64],
) -> Option<RegularPolytopeProjectionPlan> {
    let expected_rotation_count =
        RegularPolytopeNDObj::expected_rotation_angle_count(object.dimension)?;
    if !object.scale.is_finite()
        || object.scale <= 0.0
        || object.rotation_angles.len() != expected_rotation_count
        || object
            .rotation_angles
            .iter()
            .any(|angle| !angle.is_finite())
        || effective_rotation_angles.len() != expected_rotation_count
        || effective_rotation_angles
            .iter()
            .any(|angle| !angle.is_finite())
    {
        return None;
    }
    let plan = object
        .family
        .projection_plan(object.dimension, object.scale)
        .ok()?;
    plan.ensure_within_coordinate_limit(MAX_WORLD_COORDINATE)
        .ok()?;
    Some(plan)
}

fn polychoron_projection_key(
    object: &RegularPolychoron4DObj,
    effective_rotation_angles: [f64; 6],
) -> ProjectedPolytopeKey {
    ProjectedPolytopeKey::Polychoron {
        kind: object.kind,
        scale_bits: object.scale.to_bits(),
        rotation_bits: effective_rotation_angles.map(f64::to_bits),
    }
}

fn generic_polytope_projection_key(
    object: &RegularPolytopeNDObj,
    effective_rotation_angles: &[f64],
) -> Option<ProjectedPolytopeKey> {
    let mut rotation_bits = Vec::new();
    rotation_bits
        .try_reserve_exact(effective_rotation_angles.len())
        .ok()?;
    rotation_bits.extend(
        effective_rotation_angles
            .iter()
            .map(|angle| angle.to_bits()),
    );
    Some(ProjectedPolytopeKey::Generic {
        family: object.family,
        dimension: object.dimension,
        scale_bits: object.scale.to_bits(),
        rotation_bits,
    })
}

fn project_polychoron_vertices(
    topology: &Polytope4DTopology,
    object: &RegularPolychoron4DObj,
    effective_rotation_angles: [f64; 6],
    plan: RegularPolytopeProjectionPlan,
) -> Result<Vec<Point3D>, ()> {
    let mut rotated_vertices = Vec::new();
    rotated_vertices
        .try_reserve_exact(topology.vertices.len())
        .map_err(|_| ())?;
    for vertex in &topology.vertices {
        let rotated = vertex
            .rotate_all_planes(effective_rotation_angles)
            .ok_or(())?;
        let scaled = Point4D::new(
            rotated.x * object.scale,
            rotated.y * object.scale,
            rotated.z * object.scale,
            rotated.w * object.scale,
        );
        if !scaled.is_finite() {
            return Err(());
        }
        rotated_vertices.push(scaled);
    }

    let mut projected_vertices = Vec::new();
    projected_vertices
        .try_reserve_exact(rotated_vertices.len())
        .map_err(|_| ())?;
    for vertex in rotated_vertices {
        let point = vertex.perspective_project(plan.distance()).ok_or(())?;
        if !point_is_renderable(point) {
            return Err(());
        }
        projected_vertices.push(point);
    }
    Ok(projected_vertices)
}

fn project_generic_polytope_vertices(
    topology: &RegularPolytopeTopology,
    object: &RegularPolytopeNDObj,
    effective_rotation_angles: &[f64],
    plan: RegularPolytopeProjectionPlan,
) -> Result<Vec<Point3D>, ()> {
    if topology.family != object.family || topology.dimension != object.dimension {
        return Err(());
    }
    let mut transformed_vertices = Vec::new();
    transformed_vertices
        .try_reserve_exact(topology.vertices.len())
        .map_err(|_| ())?;
    for source in &topology.vertices {
        if source.len() != object.dimension {
            return Err(());
        }
        let mut coordinates = Vec::new();
        coordinates
            .try_reserve_exact(source.len())
            .map_err(|_| ())?;
        coordinates.extend_from_slice(source);

        // Rotation angles follow the documented lexicographic coordinate-plane order.
        let mut planes = object.rotation_plane_pairs();
        for &angle in effective_rotation_angles {
            let (first_axis, second_axis) = planes.next().ok_or(())?;
            rotate_nd_in_plane(&mut coordinates, first_axis, second_axis, angle).map_err(|_| ())?;
        }
        if planes.next().is_some() {
            return Err(());
        }
        for coordinate in &mut coordinates {
            *coordinate *= object.scale;
            if !coordinate.is_finite() {
                return Err(());
            }
        }
        transformed_vertices.push(coordinates);
    }

    let projection = NdPerspectiveProjection::new(plan.distance()).map_err(|_| ())?;
    let mut projected_vertices = Vec::new();
    projected_vertices
        .try_reserve_exact(transformed_vertices.len())
        .map_err(|_| ())?;
    for coordinates in &transformed_vertices {
        let point = projection.project(coordinates).map_err(|_| ())?;
        if !point_is_renderable(point) {
            return Err(());
        }
        projected_vertices.push(point);
    }
    Ok(projected_vertices)
}

#[derive(Clone, Copy)]
struct WorldMeshCheckpoint {
    opaque_vertices: usize,
    opaque_indices: usize,
    wire_vertices: usize,
    wire_indices: usize,
}

fn world_mesh_checkpoint(mesh: &WorldMesh) -> WorldMeshCheckpoint {
    WorldMeshCheckpoint {
        opaque_vertices: mesh.opaque_vertices.len(),
        opaque_indices: mesh.opaque_indices.len(),
        wire_vertices: mesh.wire_vertices.len(),
        wire_indices: mesh.wire_indices.len(),
    }
}

fn rollback_world_mesh(mesh: &mut WorldMesh, checkpoint: WorldMeshCheckpoint) {
    mesh.opaque_vertices.truncate(checkpoint.opaque_vertices);
    mesh.opaque_indices.truncate(checkpoint.opaque_indices);
    mesh.wire_vertices.truncate(checkpoint.wire_vertices);
    mesh.wire_indices.truncate(checkpoint.wire_indices);
}

fn reserve_world_mesh_usage(mesh: &mut WorldMesh, usage: WorldMeshStreamUsage) -> bool {
    mesh_part_has_room(
        &mesh.opaque_vertices,
        &mesh.opaque_indices,
        usage.opaque_vertices,
        usage.opaque_indices,
    ) && mesh_part_has_room(
        &mesh.wire_vertices,
        &mesh.wire_indices,
        usage.wire_vertices,
        usage.wire_indices,
    ) && mesh
        .opaque_vertices
        .try_reserve(usage.opaque_vertices)
        .is_ok()
        && mesh
            .opaque_indices
            .try_reserve(usage.opaque_indices)
            .is_ok()
        && mesh.wire_vertices.try_reserve(usage.wire_vertices).is_ok()
        && mesh.wire_indices.try_reserve(usage.wire_indices).is_ok()
}

fn world_mesh_usage_was_appended(
    mesh: &WorldMesh,
    checkpoint: WorldMeshCheckpoint,
    usage: WorldMeshStreamUsage,
) -> bool {
    checkpoint
        .opaque_vertices
        .checked_add(usage.opaque_vertices)
        == Some(mesh.opaque_vertices.len())
        && checkpoint.opaque_indices.checked_add(usage.opaque_indices)
            == Some(mesh.opaque_indices.len())
        && checkpoint.wire_vertices.checked_add(usage.wire_vertices)
            == Some(mesh.wire_vertices.len())
        && checkpoint.wire_indices.checked_add(usage.wire_indices) == Some(mesh.wire_indices.len())
}

fn polychoron_indices_are_valid(
    topology: &Polytope4DTopology,
    projected_vertices: &[Point3D],
) -> bool {
    topology.vertices.len() == projected_vertices.len()
        && topology.edges.iter().all(|&[first, second]| {
            first < projected_vertices.len() && second < projected_vertices.len() && first != second
        })
        && topology.faces.iter().all(|face| {
            face.len() >= 3 && face.iter().all(|&index| index < projected_vertices.len())
        })
}

fn generic_polytope_indices_are_valid(
    topology: &RegularPolytopeTopology,
    projected_vertices: &[Point3D],
) -> bool {
    topology.vertices.len() == projected_vertices.len()
        && topology.edges.iter().all(|&[first, second]| {
            first < projected_vertices.len() && second < projected_vertices.len() && first != second
        })
}

/// Proyecta la topologia canónica de un politopo regular 4D con el plan de geometría.
///
/// `effective_rotation_angles` puede incluir una fase transitoria de animación. La
/// cache se identifica solo por selector, escala y esos ángulos efectivos; el estilo
/// de presentación no participa porque no cambia las coordenadas ni la topología.
pub fn project_regular_polychoron(
    object: &RegularPolychoron4DObj,
    effective_rotation_angles: [f64; 6],
) -> Option<ProjectedRegularPolytope> {
    let plan = polychoron_projection_plan(object, effective_rotation_angles)?;
    let topology = cached_polychoron_topology(object.kind)?;
    let projected_vertices = cached_projected_polytope_vertices(
        polychoron_projection_key(object, effective_rotation_angles),
        || project_polychoron_vertices(&topology, object, effective_rotation_angles, plan),
    )
    .ok()?;
    if !polychoron_indices_are_valid(&topology, &projected_vertices) {
        return None;
    }

    Some(ProjectedRegularPolytope {
        vertices: projected_vertices,
        topology: ProjectedRegularPolytopeTopology::Polychoron(topology),
    })
}

/// Proyecta la topologia canónica de una familia regular N-D con el plan de geometría.
///
/// Las caras se mantienen vacías porque el alcance N-D genérico es wireframe. Los
/// ángulos efectivos deben seguir el orden lexicográfico de planos de la geometría.
pub fn project_regular_polytope_nd(
    object: &RegularPolytopeNDObj,
    effective_rotation_angles: &[f64],
) -> Option<ProjectedRegularPolytope> {
    let plan = generic_polytope_projection_plan(object, effective_rotation_angles)?;
    let topology = cached_generic_polytope_topology(object.family, object.dimension)?;
    let key = generic_polytope_projection_key(object, effective_rotation_angles)?;
    let projected_vertices = cached_projected_polytope_vertices(key, || {
        project_generic_polytope_vertices(&topology, object, effective_rotation_angles, plan)
    })
    .ok()?;
    if !generic_polytope_indices_are_valid(&topology, &projected_vertices) {
        return None;
    }

    Some(ProjectedRegularPolytope {
        vertices: projected_vertices,
        topology: ProjectedRegularPolytopeTopology::Generic(topology),
    })
}

fn polychoron_topology_usage(
    projected: &ProjectedRegularPolytope,
    fill: Option<Color>,
    quality: RenderQuality,
) -> Option<WorldMeshStreamUsage> {
    let mut face_triangles = 0usize;
    if !matches!(quality, RenderQuality::Preview) && fill.is_some() {
        for face in projected.faces() {
            face_triangles = face_triangles.checked_add(face.len().checked_sub(2)?)?;
        }
    }
    let (opaque_triangles, wire_triangles) =
        solid_stream_triangles(face_triangles, fill.map(|color| color.a));
    WorldMeshStreamUsage::from_primitives(opaque_triangles, wire_triangles, projected.edges().len())
}

fn append_regular_polychoron(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    object: &RegularPolychoron4DObj,
    quality: RenderQuality,
    screen_h: f32,
) -> bool {
    if !regular_polychoron_style_is_renderable(object) {
        return false;
    }
    let Some(projected) = project_regular_polychoron(object, object.rotation_angles) else {
        return false;
    };

    let fill = if matches!(quality, RenderQuality::Preview) {
        None
    } else {
        object.fill_color
    };
    let Some(usage) = polychoron_topology_usage(&projected, fill, quality) else {
        return false;
    };
    let checkpoint = world_mesh_checkpoint(mesh);
    if !reserve_world_mesh_usage(mesh, usage) {
        return false;
    }

    if let Some(fill) = fill {
        for face in projected.faces() {
            let first = projected.vertices()[face[0]];
            for index in 1..face.len() - 1 {
                append_solid_triangle(
                    mesh,
                    first,
                    projected.vertices()[face[index]],
                    projected.vertices()[face[index + 1]],
                    fill,
                );
            }
        }
    }
    for &[first, second] in projected.edges() {
        append_wire_line(
            mesh,
            camera,
            projected.vertices()[first],
            projected.vertices()[second],
            object.width,
            object.color,
            screen_h,
        );
    }

    if world_mesh_usage_was_appended(mesh, checkpoint, usage) {
        true
    } else {
        rollback_world_mesh(mesh, checkpoint);
        false
    }
}

fn append_regular_polytope(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    object: &RegularPolytopeNDObj,
    screen_h: f32,
) -> bool {
    if !regular_polytope_style_is_renderable(object) {
        return false;
    }
    let Some(projected) = project_regular_polytope_nd(object, &object.rotation_angles) else {
        return false;
    };
    let Some(usage) = WorldMeshStreamUsage::from_primitives(0, 0, projected.edges().len()) else {
        return false;
    };
    let checkpoint = world_mesh_checkpoint(mesh);
    if !reserve_world_mesh_usage(mesh, usage) {
        return false;
    }

    for &[first, second] in projected.edges() {
        append_wire_line(
            mesh,
            camera,
            projected.vertices()[first],
            projected.vertices()[second],
            object.width,
            object.color,
            screen_h,
        );
    }

    if world_mesh_usage_was_appended(mesh, checkpoint, usage) {
        true
    } else {
        rollback_world_mesh(mesh, checkpoint);
        false
    }
}

fn append_cube(mesh: &mut WorldMesh, center: Point3D, size: f64, color: Color) {
    let half = size * 0.5;
    let corners = [
        Point3D::new(center.x - half, center.y - half, center.z - half),
        Point3D::new(center.x + half, center.y - half, center.z - half),
        Point3D::new(center.x + half, center.y + half, center.z - half),
        Point3D::new(center.x - half, center.y + half, center.z - half),
        Point3D::new(center.x - half, center.y - half, center.z + half),
        Point3D::new(center.x + half, center.y - half, center.z + half),
        Point3D::new(center.x + half, center.y + half, center.z + half),
        Point3D::new(center.x - half, center.y + half, center.z + half),
    ];
    for (a, b, c, d) in [
        (0, 1, 2, 3),
        (4, 7, 6, 5),
        (0, 4, 5, 1),
        (3, 2, 6, 7),
        (0, 3, 7, 4),
        (1, 5, 6, 2),
    ] {
        append_solid_triangle(mesh, corners[a], corners[b], corners[c], color);
        append_solid_triangle(mesh, corners[a], corners[c], corners[d], color);
    }
}

fn append_cube_wire(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    center: Point3D,
    size: f64,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let half = size * 0.5;
    let corners = [
        Point3D::new(center.x - half, center.y - half, center.z - half),
        Point3D::new(center.x + half, center.y - half, center.z - half),
        Point3D::new(center.x + half, center.y + half, center.z - half),
        Point3D::new(center.x - half, center.y + half, center.z - half),
        Point3D::new(center.x - half, center.y - half, center.z + half),
        Point3D::new(center.x + half, center.y - half, center.z + half),
        Point3D::new(center.x + half, center.y + half, center.z + half),
        Point3D::new(center.x - half, center.y + half, center.z + half),
    ];
    for (a, b) in [
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
    ] {
        append_wire_line(mesh, camera, corners[a], corners[b], width, color, screen_h);
    }
}

fn append_tetrahedron(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    center: Point3D,
    edge_length: f64,
    fill: Option<Color>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let tetrahedron = grafito_geometry::Tetrahedron3D::new(center, edge_length);
    if !tetrahedron.is_renderable() {
        mesh.complete = false;
        return;
    }
    let vertices = tetrahedron.vertices();
    if let Some(fill) = fill {
        for [a, b, c] in tetrahedron.faces() {
            append_solid_triangle(mesh, vertices[a], vertices[b], vertices[c], fill);
        }
    }
    for [a, b] in tetrahedron.edges() {
        append_wire_line(
            mesh,
            camera,
            vertices[a],
            vertices[b],
            width,
            color,
            screen_h,
        );
    }
}

fn append_pyramid(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    base_center: Point3D,
    apex: Point3D,
    base_size: f64,
    fill: Option<Color>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let half = base_size * 0.5;
    let base = [
        Point3D::new(base_center.x - half, base_center.y, base_center.z - half),
        Point3D::new(base_center.x + half, base_center.y, base_center.z - half),
        Point3D::new(base_center.x + half, base_center.y, base_center.z + half),
        Point3D::new(base_center.x - half, base_center.y, base_center.z + half),
    ];
    if let Some(fill) = fill {
        for index in 0..4 {
            let next = (index + 1) % 4;
            append_solid_triangle(mesh, base[index], base[next], apex, fill);
        }
        append_solid_triangle(mesh, base[0], base[2], base[1], fill);
        append_solid_triangle(mesh, base[0], base[3], base[2], fill);
    }
    for index in 0..4 {
        append_wire_line(
            mesh,
            camera,
            base[index],
            base[(index + 1) % 4],
            width,
            color,
            screen_h,
        );
        append_wire_line(mesh, camera, base[index], apex, width, color, screen_h);
    }
}

fn append_cone(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    base_center: Point3D,
    apex: Point3D,
    radius: f64,
    fill: Option<Color>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let segments = 32usize;
    let circle: Vec<_> = (0..segments)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            Point3D::new(
                base_center.x + radius * angle.cos(),
                base_center.y,
                base_center.z + radius * angle.sin(),
            )
        })
        .collect();
    for index in 0..segments {
        let next = (index + 1) % segments;
        if let Some(fill) = fill {
            append_solid_triangle(mesh, circle[index], circle[next], apex, fill);
            append_solid_triangle(mesh, circle[next], circle[index], base_center, fill);
        }
        append_wire_line(
            mesh,
            camera,
            circle[index],
            circle[next],
            width,
            color,
            screen_h,
        );
        if index % 4 == 0 {
            append_wire_line(mesh, camera, circle[index], apex, width, color, screen_h);
        }
    }
}

fn append_cylinder(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    base_center: Point3D,
    top_center: Point3D,
    radius: f64,
    fill: Option<Color>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let segments = 32usize;
    let base: Vec<_> = (0..segments)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / segments as f64;
            Point3D::new(
                base_center.x + radius * angle.cos(),
                base_center.y,
                base_center.z + radius * angle.sin(),
            )
        })
        .collect();
    let top: Vec<_> = base
        .iter()
        .map(|point| {
            Point3D::new(
                point.x - base_center.x + top_center.x,
                top_center.y,
                point.z - base_center.z + top_center.z,
            )
        })
        .collect();
    for index in 0..segments {
        let next = (index + 1) % segments;
        if let Some(fill) = fill {
            append_solid_triangle(mesh, base[index], top[index], base[next], fill);
            append_solid_triangle(mesh, top[index], top[next], base[next], fill);
            append_solid_triangle(mesh, base[next], base[index], base_center, fill);
            append_solid_triangle(mesh, top[index], top[next], top_center, fill);
        }
        append_wire_line(
            mesh,
            camera,
            base[index],
            base[next],
            width,
            color,
            screen_h,
        );
        append_wire_line(mesh, camera, top[index], top[next], width, color, screen_h);
        if index % 8 == 0 {
            append_wire_line(
                mesh,
                camera,
                base[index],
                top[index],
                width,
                color,
                screen_h,
            );
        }
    }
}

fn append_torus(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    center: Point3D,
    major: f64,
    minor: f64,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let u_steps = 32usize;
    let v_steps = 16usize;
    let point = |u: f64, v: f64| {
        Point3D::new(
            center.x + (major + minor * v.cos()) * u.cos(),
            center.y + minor * v.sin(),
            center.z + (major + minor * v.cos()) * u.sin(),
        )
    };
    for u_index in 0..u_steps {
        for v_index in 0..v_steps {
            let u0 = std::f64::consts::TAU * u_index as f64 / u_steps as f64;
            let u1 = std::f64::consts::TAU * (u_index + 1) as f64 / u_steps as f64;
            let v0 = std::f64::consts::TAU * v_index as f64 / v_steps as f64;
            let v1 = std::f64::consts::TAU * (v_index + 1) as f64 / v_steps as f64;
            let p00 = point(u0, v0);
            let p10 = point(u1, v0);
            let p01 = point(u0, v1);
            let p11 = point(u1, v1);
            append_solid_triangle(mesh, p00, p10, p11, color);
            append_solid_triangle(mesh, p00, p11, p01, color);
            if v_index == 0 || u_index % 4 == 0 {
                append_wire_line(mesh, camera, p00, p10, width, color, screen_h);
                append_wire_line(mesh, camera, p00, p01, width, color, screen_h);
            }
        }
    }
}

fn append_moebius(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    center: Point3D,
    radius: f64,
    width_radius: f64,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let u_steps = 64usize;
    let v_steps = 8usize;
    let point = |u: f64, v: f64| {
        Point3D::new(
            center.x + (radius + v * (u * 0.5).cos()) * u.cos(),
            center.y + v * (u * 0.5).sin(),
            center.z + (radius + v * (u * 0.5).cos()) * u.sin(),
        )
    };
    for u_index in 0..u_steps {
        for v_index in 0..v_steps {
            let u0 = std::f64::consts::TAU * u_index as f64 / u_steps as f64;
            let u1 = std::f64::consts::TAU * (u_index + 1) as f64 / u_steps as f64;
            let v0 = width_radius * (2.0 * v_index as f64 / v_steps as f64 - 1.0);
            let v1 = width_radius * (2.0 * (v_index + 1) as f64 / v_steps as f64 - 1.0);
            let p00 = point(u0, v0);
            let p10 = point(u1, v0);
            let p01 = point(u0, v1);
            let p11 = point(u1, v1);
            append_solid_triangle(mesh, p00, p10, p11, color);
            append_solid_triangle(mesh, p00, p11, p01, color);
            if v_index == 0 || v_index + 1 == v_steps || u_index % 8 == 0 {
                append_wire_line(mesh, camera, p00, p10, width, color, screen_h);
                append_wire_line(mesh, camera, p00, p01, width, color, screen_h);
            }
        }
    }
}

fn append_surface(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    surface: &Surface3DObj,
    document: &Document,
    screen_h: f32,
    resolution: usize,
) {
    let grid = grafito_core::parametric_sampling::samples_or_compute_surface(
        surface,
        resolution,
        &document.variables,
    );
    if grid.len() < 2 || grid[0].len() < 2 {
        return;
    }
    for rows in grid.windows(2) {
        for column in 0..rows[0].len().saturating_sub(1) {
            let p00 = rows[0][column];
            let p10 = rows[1][column];
            let p01 = rows[0][column + 1];
            let p11 = rows[1][column + 1];
            if surface.solid {
                append_solid_triangle(mesh, p00, p10, p11, surface.color);
                append_solid_triangle(mesh, p00, p11, p01, surface.color);
            }
            append_wire_line(
                mesh,
                camera,
                p00,
                p10,
                surface.width,
                surface.color,
                screen_h,
            );
            append_wire_line(
                mesh,
                camera,
                p00,
                p01,
                surface.width,
                surface.color,
                screen_h,
            );
        }
    }
}

fn append_plane(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    color: Color,
    opacity: f32,
    screen_h: f32,
) {
    if ![a, b, c, d]
        .into_iter()
        .all(|value| value.is_finite() && value.abs() <= MAX_WORLD_COORDINATE)
    {
        return;
    }
    let normal = glam::Vec3::new(a as f32, b as f32, c as f32);
    if normal.length_squared() < 1e-12 {
        return;
    }
    let n = normal.normalize();
    let center = normal * ((-d as f32) / normal.length_squared());
    let seed = if n.cross(glam::Vec3::Y).length_squared() > 1e-6 {
        glam::Vec3::Y
    } else {
        glam::Vec3::X
    };
    let u = n.cross(seed).normalize();
    let v = n.cross(u).normalize();
    let half = (camera.distance * 1.25).max(6.0);
    let points = [
        Point3D::from_vec3(center + (-u - v) * half),
        Point3D::from_vec3(center + (u - v) * half),
        Point3D::from_vec3(center + (u + v) * half),
        Point3D::from_vec3(center + (-u + v) * half),
    ];
    let fill = Color::new(color.r, color.g, color.b, opacity.clamp(0.0, 1.0));
    append_solid_triangle(mesh, points[0], points[1], points[2], fill);
    append_solid_triangle(mesh, points[0], points[2], points[3], fill);
    for (start, end) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
        append_wire_line(
            mesh,
            camera,
            points[start],
            points[end],
            1.5,
            color,
            screen_h,
        );
    }
}

fn append_sphere(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    center: Point3D,
    radius: f64,
    fill: Option<Color>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let segments = 20usize;
    if let Some(fill) = fill {
        for latitude in 0..segments {
            let theta0 = std::f32::consts::PI * latitude as f32 / segments as f32;
            let theta1 = std::f32::consts::PI * (latitude + 1) as f32 / segments as f32;
            for longitude in 0..segments * 2 {
                let phi0 = std::f32::consts::TAU * longitude as f32 / (segments * 2) as f32;
                let phi1 = std::f32::consts::TAU * (longitude + 1) as f32 / (segments * 2) as f32;
                let point = |theta: f32, phi: f32| {
                    Point3D::new(
                        center.x + radius * (theta.sin() * phi.cos()) as f64,
                        center.y + radius * theta.cos() as f64,
                        center.z + radius * (theta.sin() * phi.sin()) as f64,
                    )
                };
                let p00 = point(theta0, phi0);
                let p10 = point(theta1, phi0);
                let p11 = point(theta1, phi1);
                let p01 = point(theta0, phi1);
                append_solid_triangle(mesh, p00, p10, p11, fill);
                append_solid_triangle(mesh, p00, p11, p01, fill);
            }
        }
    }
    for (u, v) in [
        (glam::Vec3::X, glam::Vec3::Y),
        (glam::Vec3::X, glam::Vec3::Z),
        (glam::Vec3::Y, glam::Vec3::Z),
    ] {
        let points = Camera3D::circle_points(center.to_vec3(), u, v, radius as f32, 32);
        for segment in points.windows(2) {
            append_wire_line(
                mesh,
                camera,
                Point3D::from_vec3(segment[0]),
                Point3D::from_vec3(segment[1]),
                width,
                color,
                screen_h,
            );
        }
    }
}

fn append_curve(
    mesh: &mut WorldMesh,
    camera: &Camera3D,
    points: impl IntoIterator<Item = Option<Point3D>>,
    width: f32,
    color: Color,
    screen_h: f32,
) {
    let mut previous = None;
    for point in points {
        let Some(point) = point.filter(|point| point_is_renderable(*point)) else {
            previous = None;
            continue;
        };
        if let Some(last) =
            previous.filter(|last| curve_3d_segment_is_continuous(*last, point, camera))
        {
            append_wire_line(mesh, camera, last, point, width, color, screen_h);
        }
        previous = Some(point);
    }
}

fn sort_non_depth_writing_triangles(mesh: &mut WorldMesh, camera: &Camera3D) {
    if mesh.wire_indices.len() < 6 {
        return;
    }

    let mut overlays = Vec::new();
    let mut transparent = Vec::new();
    if overlays.try_reserve(mesh.wire_indices.len()).is_err()
        || transparent
            .try_reserve(mesh.wire_indices.len() / 3)
            .is_err()
    {
        return;
    }
    let view = camera.view_matrix();
    for triangle in mesh.wire_indices.chunks_exact(3) {
        let &[a, b, c] = triangle else {
            return;
        };
        let (Some(a_vertex), Some(b_vertex), Some(c_vertex)) = (
            mesh.wire_vertices.get(a as usize),
            mesh.wire_vertices.get(b as usize),
            mesh.wire_vertices.get(c as usize),
        ) else {
            return;
        };
        let vertices = [a_vertex, b_vertex, c_vertex];
        if vertices.iter().all(|vertex| vertex.color[3] < 0.999) {
            let centroid = vertices.iter().fold(glam::Vec3::ZERO, |sum, vertex| {
                sum + glam::Vec3::from(vertex.position)
            }) / 3.0;
            let depth = -(view * centroid.extend(1.0)).z;
            if depth.is_finite() {
                transparent.push((depth, [a, b, c]));
            } else {
                overlays.extend([a, b, c]);
            }
        } else {
            overlays.extend([a, b, c]);
        }
    }
    transparent.sort_unstable_by(|left, right| right.0.total_cmp(&left.0));
    overlays.extend(transparent.into_iter().flat_map(|(_, triangle)| triangle));
    mesh.wire_indices = overlays;
}

pub fn build_world_mesh(
    document: &Document,
    camera: &Camera3D,
    screen_w: f32,
    screen_h: f32,
) -> WorldMesh {
    let mut mesh = WorldMesh::default();
    if !screen_w.is_finite() || !screen_h.is_finite() || screen_w <= 0.0 || screen_h <= 0.0 {
        return mesh;
    }
    if !world_mesh_scene_fits_limits(document) {
        mesh.complete = false;
        return mesh;
    }
    let mut objects: Vec<_> = document.objects_iter().collect();
    objects.sort_unstable_by(|left, right| left.0.cmp(right.0));
    let mut attractors_rendered = 0usize;
    let mut work_budget = WorldMeshWorkBudget::new();
    for (_, object) in objects {
        if !object.is_visible() {
            continue;
        }
        if world_mesh_is_output_exhausted(&mesh) {
            mesh.complete = false;
            break;
        }
        match object {
            GeoObject::Point3D(point) => append_point_billboard(
                &mut mesh,
                camera,
                point.position,
                point.size,
                point.color,
                screen_h,
            ),
            GeoObject::Segment3D(segment) => append_wire_line(
                &mut mesh,
                camera,
                segment.a,
                segment.b,
                segment.width,
                segment.color,
                screen_h,
            ),
            GeoObject::Line3D(line) => {
                let direction = line.direction.to_vec3();
                if direction.length_squared() > 1e-12 {
                    let span = (camera.distance as f64 * 4.0).max(20.0);
                    let unit = direction.normalize() * span as f32;
                    append_wire_line(
                        &mut mesh,
                        camera,
                        Point3D::from_vec3(line.point.to_vec3() - unit),
                        Point3D::from_vec3(line.point.to_vec3() + unit),
                        line.width,
                        line.color,
                        screen_h,
                    );
                }
            }
            GeoObject::Plane3D(plane) => append_plane(
                &mut mesh,
                camera,
                plane.a,
                plane.b,
                plane.c,
                plane.d,
                plane.color,
                plane.opacity,
                screen_h,
            ),
            GeoObject::Sphere3D(sphere) => append_sphere(
                &mut mesh,
                camera,
                sphere.center,
                sphere.radius,
                sphere.fill_color,
                sphere.width,
                sphere.color,
                screen_h,
            ),
            GeoObject::Cube3D(cube) => {
                if let Some(fill) = cube.fill_color {
                    append_cube(&mut mesh, cube.center, cube.size, fill);
                }
                append_cube_wire(
                    &mut mesh,
                    camera,
                    cube.center,
                    cube.size,
                    cube.width,
                    cube.color,
                    screen_h,
                );
            }
            GeoObject::Tetrahedron3D(tetrahedron) => append_tetrahedron(
                &mut mesh,
                camera,
                tetrahedron.center,
                tetrahedron.edge_length,
                tetrahedron.fill_color,
                tetrahedron.width,
                tetrahedron.color,
                screen_h,
            ),
            GeoObject::RegularPolychoron4D(polychoron) => {
                mesh.complete &= append_regular_polychoron(
                    &mut mesh,
                    camera,
                    polychoron,
                    document.render_quality,
                    screen_h,
                );
            }
            GeoObject::RegularPolytopeND(polytope) => {
                mesh.complete &= append_regular_polytope(&mut mesh, camera, polytope, screen_h);
            }
            GeoObject::Pyramid3D(pyramid) => append_pyramid(
                &mut mesh,
                camera,
                pyramid.base_center,
                pyramid.apex,
                pyramid.base_size,
                pyramid.fill_color,
                pyramid.width,
                pyramid.color,
                screen_h,
            ),
            GeoObject::Cone3D(cone) => append_cone(
                &mut mesh,
                camera,
                cone.base_center,
                cone.apex,
                cone.radius,
                cone.fill_color,
                cone.width,
                cone.color,
                screen_h,
            ),
            GeoObject::Cylinder3D(cylinder) => append_cylinder(
                &mut mesh,
                camera,
                cylinder.base_center,
                cylinder.top_center,
                cylinder.radius,
                cylinder.fill_color,
                cylinder.width,
                cylinder.color,
                screen_h,
            ),
            GeoObject::Torus3D(torus) => append_torus(
                &mut mesh,
                camera,
                torus.center,
                torus.r_major,
                torus.r_minor,
                torus.width,
                torus.color,
                screen_h,
            ),
            GeoObject::MoebiusStrip(strip) => append_moebius(
                &mut mesh,
                camera,
                strip.center,
                strip.radius,
                strip.width_r,
                strip.width,
                strip.color,
                screen_h,
            ),
            GeoObject::Surface3D(surface) => {
                let resolution = surface.mesh_res.clamp(2, 128);
                let Some(work_units) = surface_work_units(resolution) else {
                    continue;
                };
                if !surface_output_fits(&mesh, surface, resolution)
                    || !work_budget.reserve(work_units)
                {
                    mesh.complete = false;
                    continue;
                }
                append_surface(&mut mesh, camera, surface, document, screen_h, resolution)
            }
            GeoObject::ParametricCurve3D(curve) => {
                const CURVE_STEPS: usize = 4_000;
                if !wire_object_fits(&mesh, CURVE_STEPS) || !work_budget.reserve(CURVE_STEPS + 1) {
                    mesh.complete = false;
                    continue;
                }
                let samples = grafito_core::parametric_sampling::samples_or_compute_curve_3d(
                    curve,
                    CURVE_STEPS,
                    &document.variables,
                );
                let points = samples.iter().map(|&(x, y, z)| {
                    (x.is_finite() && y.is_finite() && z.is_finite())
                        .then_some(Point3D::new(x, y, z))
                });
                append_curve(
                    &mut mesh,
                    camera,
                    points,
                    curve.width,
                    curve.color,
                    screen_h,
                );
            }
            GeoObject::Attractor3D(attractor) => {
                if attractors_rendered >= MAX_WORLD_MESH_ATTRACTORS {
                    mesh.complete = false;
                    continue;
                }
                let steps = attractor.steps.min(MAX_WORLD_MESH_ATTRACTOR_STEPS);
                if !wire_object_fits(&mesh, steps) || !work_budget.reserve(steps) {
                    mesh.complete = false;
                    continue;
                }
                attractors_rendered += 1;
                let skip = attractor.skip.min(steps);
                let points = grafito_geometry::attractors::integrate_attractor(
                    &attractor.model(),
                    attractor.x0,
                    attractor.y0,
                    attractor.z0,
                    attractor.dt,
                    steps,
                    skip,
                )
                .into_iter()
                .map(|point| {
                    let point = Point3D::new(point.x * 0.2, point.y * 0.2, point.z * 0.2);
                    point_is_renderable(point).then_some(point)
                });
                append_curve(
                    &mut mesh,
                    camera,
                    points,
                    attractor.width,
                    attractor.color,
                    screen_h,
                );
            }
            GeoObject::VectorField3D(field) => {
                let Some(vector_count) = crate::vector_field_3d_sample_count(field) else {
                    continue;
                };
                if !wire_object_fits(&mesh, vector_count) || !work_budget.reserve(vector_count) {
                    mesh.complete = false;
                    continue;
                }
                for (start, end) in crate::sample_vector_field_3d(field, &document.variables) {
                    append_wire_line(&mut mesh, camera, start, end, 1.5, field.color, screen_h);
                }
            }
            _ => {}
        }
    }
    sort_non_depth_writing_triangles(&mut mesh, camera);
    mesh
}
