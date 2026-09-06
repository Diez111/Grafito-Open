//! Primitivas canonicas para politopos regulares 4D y familias N-D acotadas.
//!
//! `RegularPolychoron` conserva las seis familias regulares convexas 4D. Las
//! familias genericas `RegularPolytopeFamily` se construyen solo para
//! dimensiones `3..=10`, con topologia canonica en `f64` y limites de recursos
//! comprobados antes de reservar memoria. La proyeccion nunca puede cambiar
//! aristas, caras ni celdas.

use crate::types3d::Point3D;
use serde::{Deserialize, Serialize};
use std::fmt;

const TOPOLOGY_TOLERANCE: f64 = 1.0e-9;

/// Margen relativo que protege el plano de perspectiva `w = distance`.
pub const PERSPECTIVE_4D_NEAR_EPSILON: f64 = 1.0e-9;

/// Error de construccion de una topologia regular 4D.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polytope4DError {
    /// No fue posible reservar una coleccion de tamano acotado.
    AllocationFailed,
    /// Las incidencias canonicas no cumplieron las invariantes esperadas.
    InvalidCanonicalTopology,
}

impl fmt::Display for Polytope4DError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllocationFailed => formatter.write_str("no se pudo reservar la topologia 4D"),
            Self::InvalidCanonicalTopology => {
                formatter.write_str("la topologia regular 4D canonica es invalida")
            }
        }
    }
}

impl std::error::Error for Polytope4DError {}

type PolytopeResult<T> = Result<T, Polytope4DError>;

/// Plan escalar para proyectar un politopo regular hacia R3 sin materializar su topologia.
///
/// `source_radius` acota la norma de cualquier vertice tras escalarlo. La distancia
/// respeta el contrato compartido `distance = (n + 2) * max(source_radius, 1)`.
/// `projected_coordinate_bound` es una cota conservadora y finita para cada
/// coordenada XYZ proyectada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegularPolytopeProjectionPlan {
    dimension: usize,
    source_radius: f64,
    distance: f64,
    projected_coordinate_bound: f64,
}

impl RegularPolytopeProjectionPlan {
    /// Dimension euclidea de los vertices de origen.
    pub const fn dimension(self) -> usize {
        self.dimension
    }

    /// Cota radial de los vertices luego de aplicar la escala persistida.
    pub const fn source_radius(self) -> f64 {
        self.source_radius
    }

    /// Distancia de perspectiva que debe reutilizar el renderizador.
    pub const fn distance(self) -> f64 {
        self.distance
    }

    /// Cota conservadora para el valor absoluto de cada coordenada XYZ proyectada.
    pub const fn projected_coordinate_bound(self) -> f64 {
        self.projected_coordinate_bound
    }

    /// Rechaza un plan que no cabe en un limite finito y positivo de coordenadas.
    pub fn ensure_within_coordinate_limit(
        self,
        maximum_coordinate: f64,
    ) -> Result<(), RegularPolytopeProjectionError> {
        if !maximum_coordinate.is_finite() || maximum_coordinate <= 0.0 {
            return Err(RegularPolytopeProjectionError::InvalidCoordinateLimit {
                maximum_coordinate,
            });
        }
        if self.projected_coordinate_bound > maximum_coordinate {
            return Err(RegularPolytopeProjectionError::CoordinateLimitExceeded {
                projected_coordinate_bound: self.projected_coordinate_bound,
                maximum_coordinate,
            });
        }
        Ok(())
    }
}

/// Error al planificar una proyeccion de politopo regular sin generar vertices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegularPolytopeProjectionError {
    /// La dimension no pertenece al intervalo generico publicado.
    InvalidDimension {
        dimension: usize,
        minimum: usize,
        maximum: usize,
    },
    /// La escala persistida no es un numero finito.
    NonFiniteScale,
    /// La escala persistida no es estrictamente positiva.
    NonPositiveScale,
    /// La cota radial canonica no es finita.
    NonFiniteCanonicalRadius,
    /// La cota radial canonica no es estrictamente positiva.
    NonPositiveCanonicalRadius,
    /// La multiplicacion de escala y radio no admite una cota finita segura.
    SourceRadiusOverflow,
    /// La distancia de perspectiva del contrato no admite una representacion finita.
    PerspectiveDistanceOverflow,
    /// La resta `distance - source_radius` no conserva un denominador positivo finito.
    InvalidPerspectiveDenominator,
    /// La cota proyectada no admite una representacion finita.
    ProjectedCoordinateBoundOverflow,
    /// El limite solicitado no es finito y estrictamente positivo.
    InvalidCoordinateLimit { maximum_coordinate: f64 },
    /// La cota proyectada supera el limite solicitado.
    CoordinateLimitExceeded {
        projected_coordinate_bound: f64,
        maximum_coordinate: f64,
    },
}

impl fmt::Display for RegularPolytopeProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimension {
                dimension,
                minimum,
                maximum,
            } => write!(
                formatter,
                "la dimension {dimension} debe pertenecer al intervalo {minimum}..={maximum}"
            ),
            Self::NonFiniteScale => formatter.write_str("la escala debe ser finita"),
            Self::NonPositiveScale => formatter.write_str("la escala debe ser positiva"),
            Self::NonFiniteCanonicalRadius => {
                formatter.write_str("el radio canonico debe ser finito")
            }
            Self::NonPositiveCanonicalRadius => {
                formatter.write_str("el radio canonico debe ser positivo")
            }
            Self::SourceRadiusOverflow => {
                formatter.write_str("el radio escalado no admite una cota finita")
            }
            Self::PerspectiveDistanceOverflow => {
                formatter.write_str("la distancia de perspectiva no admite una cota finita")
            }
            Self::InvalidPerspectiveDenominator => {
                formatter.write_str("el denominador de perspectiva debe ser positivo y finito")
            }
            Self::ProjectedCoordinateBoundOverflow => {
                formatter.write_str("la cota de coordenadas proyectadas no es finita")
            }
            Self::InvalidCoordinateLimit { maximum_coordinate } => write!(
                formatter,
                "el limite de coordenadas {maximum_coordinate} debe ser positivo y finito"
            ),
            Self::CoordinateLimitExceeded {
                projected_coordinate_bound,
                maximum_coordinate,
            } => write!(
                formatter,
                "la cota proyectada {projected_coordinate_bound} excede el limite {maximum_coordinate}"
            ),
        }
    }
}

impl std::error::Error for RegularPolytopeProjectionError {}

/// Punto cartesiano en el espacio euclideo de cuatro dimensiones.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point4D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Point4D {
    /// Crea un punto 4D sin normalizar sus coordenadas.
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }

    /// Indica si las cuatro coordenadas son finitas.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }

    /// Producto escalar euclideo en R4.
    pub fn dot(self, other: Self) -> f64 {
        self.x.mul_add(
            other.x,
            self.y
                .mul_add(other.y, self.z.mul_add(other.z, self.w * other.w)),
        )
    }

    /// Norma euclidea al cuadrado.
    pub fn norm_squared(self) -> f64 {
        self.dot(self)
    }

    /// Norma euclidea.
    pub fn norm(self) -> f64 {
        self.norm_squared().sqrt()
    }

    /// Distancia euclidea a otro punto 4D.
    pub fn distance(self, other: Self) -> f64 {
        self.distance_squared(other).sqrt()
    }

    /// Distancia euclidea al cuadrado a otro punto 4D.
    pub fn distance_squared(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        let dw = self.w - other.w;
        dx.mul_add(dx, dy.mul_add(dy, dz.mul_add(dz, dw * dw)))
    }

    /// Aplica rotaciones de Givens en orden fijo `xy, xz, xw, yz, yw, zw`.
    ///
    /// El orden es parte del contrato porque las rotaciones en distintos planos
    /// de R4 no conmutan. Devuelve `None` para entradas o resultados no finitos.
    pub fn rotate_all_planes(self, angles: [f64; 6]) -> Option<Self> {
        if !self.is_finite() || angles.iter().any(|angle| !angle.is_finite()) {
            return None;
        }

        let mut rotated = self;
        (rotated.x, rotated.y) = rotate_pair(rotated.x, rotated.y, angles[0]);
        (rotated.x, rotated.z) = rotate_pair(rotated.x, rotated.z, angles[1]);
        (rotated.x, rotated.w) = rotate_pair(rotated.x, rotated.w, angles[2]);
        (rotated.y, rotated.z) = rotate_pair(rotated.y, rotated.z, angles[3]);
        (rotated.y, rotated.w) = rotate_pair(rotated.y, rotated.w, angles[4]);
        (rotated.z, rotated.w) = rotate_pair(rotated.z, rotated.w, angles[5]);
        rotated.is_finite().then_some(rotated)
    }

    /// Proyecta R4 en R3 con el factor seguro `distance / (distance - w)`.
    ///
    /// Se rechazan distancias no positivas, valores no finitos y puntos sobre o
    /// demasiado cerca del plano de perspectiva para evitar coordenadas enormes.
    pub fn perspective_project(self, distance: f64) -> Option<Point3D> {
        if !self.is_finite() || !distance.is_finite() || distance <= 0.0 {
            return None;
        }

        let denominator = distance - self.w;
        let near_limit = PERSPECTIVE_4D_NEAR_EPSILON * distance.abs().max(1.0);
        if !denominator.is_finite() || denominator.abs() <= near_limit {
            return None;
        }

        let scale = distance / denominator;
        let projected = Point3D::new(self.x * scale, self.y * scale, self.z * scale);
        projected.is_finite().then_some(projected)
    }

    fn scaled(self, scale: f64) -> Self {
        Self::new(
            self.x * scale,
            self.y * scale,
            self.z * scale,
            self.w * scale,
        )
    }

    fn coordinates(self) -> [f64; 4] {
        [self.x, self.y, self.z, self.w]
    }

    fn from_coordinates(coordinates: [f64; 4]) -> Self {
        Self::new(
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
        )
    }
}

fn rotate_pair(first: f64, second: f64, angle: f64) -> (f64, f64) {
    let (sine, cosine) = angle.sin_cos();
    (
        first.mul_add(cosine, -second * sine),
        first.mul_add(sine, second * cosine),
    )
}

/// Conteos de elementos de la f-vector de un politopo 4D.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Polytope4DCounts {
    pub vertices: usize,
    pub edges: usize,
    pub faces: usize,
    pub cells: usize,
}

/// Una de las seis familias regulares convexas de cuatro dimensiones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegularPolychoron {
    /// El 5-celda, tambien llamado pentacoron o simplex 4D.
    Pentachoron,
    /// El hipercubo de cuatro dimensiones.
    Tesseract,
    /// El politopo cruzado de cuatro dimensiones.
    SixteenCell,
    /// El 24-celda autodual.
    TwentyFourCell,
    /// El dual del 600-celda, con celdas dodecaedricas.
    OneTwentyCell,
    /// El politopo regular con celdas tetraedricas basado en las raices H4.
    SixHundredCell,
}

impl RegularPolychoron {
    /// Alias convencional del pentacoron.
    pub const FIVE_CELL: Self = Self::Pentachoron;

    /// Devuelve una cota radial canónica de sus vertices, sin crear topologia.
    ///
    /// Las seis expresiones corresponden a las coordenadas canónicas del modulo:
    /// el 120-celda usa el radio polar del 600-celda de radio unitario.
    pub fn canonical_radius_bound(self) -> f64 {
        match self {
            Self::Pentachoron => 4.0 / 5.0_f64.sqrt(),
            Self::Tesseract => 2.0,
            Self::SixteenCell | Self::SixHundredCell => 1.0,
            Self::TwentyFourCell => 2.0_f64.sqrt(),
            Self::OneTwentyCell => one_twenty_cell_canonical_radius_bound(),
        }
    }

    /// Planifica la perspectiva 4D con una cota radial cerrada y sin reservar memoria.
    pub fn projection_plan(
        self,
        scale: f64,
    ) -> Result<RegularPolytopeProjectionPlan, RegularPolytopeProjectionError> {
        plan_regular_polytope_projection(4, scale, self.canonical_radius_bound())
    }

    /// Devuelve la f-vector exacta de la familia elegida.
    pub const fn expected_counts(self) -> Polytope4DCounts {
        match self {
            Self::Pentachoron => Polytope4DCounts {
                vertices: 5,
                edges: 10,
                faces: 10,
                cells: 5,
            },
            Self::Tesseract => Polytope4DCounts {
                vertices: 16,
                edges: 32,
                faces: 24,
                cells: 8,
            },
            Self::SixteenCell => Polytope4DCounts {
                vertices: 8,
                edges: 24,
                faces: 32,
                cells: 16,
            },
            Self::TwentyFourCell => Polytope4DCounts {
                vertices: 24,
                edges: 96,
                faces: 96,
                cells: 24,
            },
            Self::OneTwentyCell => Polytope4DCounts {
                vertices: 600,
                edges: 1200,
                faces: 720,
                cells: 120,
            },
            Self::SixHundredCell => Polytope4DCounts {
                vertices: 120,
                edges: 720,
                faces: 1200,
                cells: 600,
            },
        }
    }

    /// Construye la topologia canonica con indices, caras ordenadas y celdas.
    pub fn topology(self) -> Result<Polytope4DTopology, Polytope4DError> {
        match self {
            Self::Pentachoron => pentachoron_topology(),
            Self::Tesseract => tesseract_topology(),
            Self::SixteenCell => sixteen_cell_topology(),
            Self::TwentyFourCell => twenty_four_cell_topology(),
            Self::OneTwentyCell => one_twenty_cell_topology(),
            Self::SixHundredCell => six_hundred_cell_topology(),
        }
    }
}

/// Topologia inmutable por convencion de un politopo regular 4D.
///
/// `faces` conserva ciclos de indices en orden, mientras que cada entrada de
/// `cells` contiene los vertices que pertenecen a una celda tridimensional.
#[derive(Debug, Clone, PartialEq)]
pub struct Polytope4DTopology {
    pub vertices: Vec<Point4D>,
    pub edges: Vec<[usize; 2]>,
    pub faces: Vec<Vec<usize>>,
    pub cells: Vec<Vec<usize>>,
    pub vertex_adjacency: Vec<Vec<usize>>,
}

impl Polytope4DTopology {
    /// Cuenta vertices, aristas, caras y celdas sin reconstruir la topologia.
    pub fn counts(&self) -> Polytope4DCounts {
        Polytope4DCounts {
            vertices: self.vertices.len(),
            edges: self.edges.len(),
            faces: self.faces.len(),
            cells: self.cells.len(),
        }
    }

    /// Calcula `V - E + F - C`, que vale cero para un politopo convexo 4D.
    pub fn euler_characteristic(&self) -> i128 {
        self.vertices.len() as i128 - self.edges.len() as i128 + self.faces.len() as i128
            - self.cells.len() as i128
    }

    /// Devuelve la longitud de la primera arista valida, si existe.
    pub fn edge_length(&self) -> Option<f64> {
        let [first, second] = *self.edges.first()?;
        Some(
            self.vertices
                .get(first)?
                .distance(*self.vertices.get(second)?),
        )
    }

    /// Comprueba indices, ciclos de cara, adyacencia y finitud de coordenadas.
    pub fn validate(&self) -> PolytopeResult<()> {
        if self.vertices.iter().any(|vertex| !vertex.is_finite())
            || self.vertex_adjacency.len() != self.vertices.len()
        {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }

        for (position, &[first, second]) in self.edges.iter().enumerate() {
            if first >= self.vertices.len()
                || second >= self.vertices.len()
                || first >= second
                || (position > 0 && self.edges[position - 1] >= [first, second])
            {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
        }

        for (vertex, neighbours) in self.vertex_adjacency.iter().enumerate() {
            if neighbours.windows(2).any(|pair| pair[0] >= pair[1])
                || neighbours.iter().any(|&neighbour| {
                    neighbour >= self.vertices.len()
                        || neighbour == vertex
                        || !contains_edge(&self.edges, vertex, neighbour)
                })
            {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
        }

        for &[first, second] in &self.edges {
            if self.vertex_adjacency[first].binary_search(&second).is_err()
                || self.vertex_adjacency[second].binary_search(&first).is_err()
            {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
        }

        for face in &self.faces {
            if face.len() < 3 {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
            for (position, &vertex) in face.iter().enumerate() {
                let next = face[(position + 1) % face.len()];
                if vertex >= self.vertices.len()
                    || face[..position].contains(&vertex)
                    || !contains_edge(&self.edges, vertex, next)
                {
                    return Err(Polytope4DError::InvalidCanonicalTopology);
                }
            }
        }

        for cell in &self.cells {
            if cell.len() < 4
                || cell.iter().enumerate().any(|(position, &vertex)| {
                    vertex >= self.vertices.len() || cell[..position].contains(&vertex)
                })
            {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
        }

        Ok(())
    }
}

fn with_capacity<T>(capacity: usize) -> PolytopeResult<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| Polytope4DError::AllocationFailed)?;
    Ok(values)
}

fn index_list(indices: &[usize]) -> PolytopeResult<Vec<usize>> {
    let mut result = with_capacity(indices.len())?;
    result.extend_from_slice(indices);
    Ok(result)
}

fn contains_edge(edges: &[[usize; 2]], first: usize, second: usize) -> bool {
    let edge = if first < second {
        [first, second]
    } else {
        [second, first]
    };
    edges.binary_search(&edge).is_ok()
}

fn finish_topology(
    vertices: Vec<Point4D>,
    edges: Vec<[usize; 2]>,
    faces: Vec<Vec<usize>>,
    cells: Vec<Vec<usize>>,
    expected: Polytope4DCounts,
) -> PolytopeResult<Polytope4DTopology> {
    let topology = Polytope4DTopology {
        vertex_adjacency: build_vertex_adjacency(vertices.len(), &edges)?,
        vertices,
        edges,
        faces,
        cells,
    };
    if topology.counts() != expected || topology.euler_characteristic() != 0 {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }
    topology.validate()?;
    Ok(topology)
}

fn build_vertex_adjacency(
    vertex_count: usize,
    edges: &[[usize; 2]],
) -> PolytopeResult<Vec<Vec<usize>>> {
    let mut degrees = with_capacity(vertex_count)?;
    degrees.resize(vertex_count, 0_usize);
    for &[first, second] in edges {
        if first >= vertex_count || second >= vertex_count || first >= second {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }
        degrees[first] = degrees[first]
            .checked_add(1)
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
        degrees[second] = degrees[second]
            .checked_add(1)
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
    }

    let mut adjacency = with_capacity(vertex_count)?;
    for degree in degrees {
        adjacency.push(with_capacity(degree)?);
    }
    for &[first, second] in edges {
        adjacency[first].push(second);
        adjacency[second].push(first);
    }
    for neighbours in &mut adjacency {
        neighbours.sort_unstable();
    }
    Ok(adjacency)
}

fn complete_edges(vertex_count: usize) -> PolytopeResult<Vec<[usize; 2]>> {
    let pair_product = vertex_count
        .checked_mul(vertex_count.saturating_sub(1))
        .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
    let mut edges = with_capacity(pair_product / 2)?;
    for first in 0..vertex_count {
        for second in (first + 1)..vertex_count {
            edges.push([first, second]);
        }
    }
    Ok(edges)
}

fn edges_at_minimum_distance(
    vertices: &[Point4D],
    expected_count: usize,
) -> PolytopeResult<Vec<[usize; 2]>> {
    let mut minimum = f64::INFINITY;
    for first in 0..vertices.len() {
        for second in (first + 1)..vertices.len() {
            let squared_distance = vertices[first].distance_squared(vertices[second]);
            if !squared_distance.is_finite() || squared_distance <= 0.0 {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
            minimum = minimum.min(squared_distance);
        }
    }
    if !minimum.is_finite() {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }

    let tolerance = TOPOLOGY_TOLERANCE * minimum.max(1.0);
    let mut edges = with_capacity(expected_count)?;
    for first in 0..vertices.len() {
        for second in (first + 1)..vertices.len() {
            if (vertices[first].distance_squared(vertices[second]) - minimum).abs() <= tolerance {
                if edges.len() == expected_count {
                    return Err(Polytope4DError::InvalidCanonicalTopology);
                }
                edges.push([first, second]);
            }
        }
    }
    (edges.len() == expected_count)
        .then_some(edges)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)
}

fn filtered_indices<F>(
    vertex_count: usize,
    expected_count: usize,
    mut matches: F,
) -> PolytopeResult<Vec<usize>>
where
    F: FnMut(usize) -> bool,
{
    let mut indices = with_capacity(expected_count)?;
    for index in 0..vertex_count {
        if matches(index) {
            if indices.len() == expected_count {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
            indices.push(index);
        }
    }
    (indices.len() == expected_count)
        .then_some(indices)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)
}

fn triangular_faces_from_cells(
    cells: &[Vec<usize>],
    edges: &[[usize; 2]],
    expected_count: usize,
) -> PolytopeResult<Vec<Vec<usize>>> {
    let maximum_candidates = cells
        .len()
        .checked_mul(20)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
    let mut triangles = with_capacity(maximum_candidates)?;
    for cell in cells {
        for first in 0..cell.len() {
            for second in (first + 1)..cell.len() {
                for third in (second + 1)..cell.len() {
                    let mut triangle = [cell[first], cell[second], cell[third]];
                    if contains_edge(edges, triangle[0], triangle[1])
                        && contains_edge(edges, triangle[1], triangle[2])
                        && contains_edge(edges, triangle[2], triangle[0])
                    {
                        triangle.sort_unstable();
                        triangles.push(triangle);
                    }
                }
            }
        }
    }
    triangles.sort_unstable();
    triangles.dedup();
    if triangles.len() != expected_count {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }

    let mut faces = with_capacity(expected_count)?;
    for triangle in triangles {
        faces.push(index_list(&triangle)?);
    }
    Ok(faces)
}

fn pentachoron_topology() -> PolytopeResult<Polytope4DTopology> {
    let mut vertices = with_capacity(5)?;
    let base_w = -1.0 / 5.0_f64.sqrt();
    vertices.push(Point4D::new(1.0, 1.0, 1.0, base_w));
    vertices.push(Point4D::new(1.0, -1.0, -1.0, base_w));
    vertices.push(Point4D::new(-1.0, -1.0, 1.0, base_w));
    vertices.push(Point4D::new(-1.0, 1.0, -1.0, base_w));
    vertices.push(Point4D::new(0.0, 0.0, 0.0, 4.0 / 5.0_f64.sqrt()));

    let edges = complete_edges(5)?;
    let mut faces = with_capacity(10)?;
    for first in 0..5 {
        for second in (first + 1)..5 {
            for third in (second + 1)..5 {
                faces.push(index_list(&[first, second, third])?);
            }
        }
    }
    let mut cells = with_capacity(5)?;
    for omitted in 0..5 {
        cells.push(filtered_indices(5, 4, |index| index != omitted)?);
    }

    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::Pentachoron.expected_counts(),
    )
}

fn tesseract_topology() -> PolytopeResult<Polytope4DTopology> {
    let mut vertices = with_capacity(16)?;
    for bits in 0..16 {
        let coordinates =
            core::array::from_fn(|axis| if bits & (1 << axis) == 0 { -1.0 } else { 1.0 });
        vertices.push(Point4D::from_coordinates(coordinates));
    }

    let mut edges = with_capacity(32)?;
    for vertex in 0..16 {
        for axis in 0..4 {
            let neighbour = vertex ^ (1 << axis);
            if vertex < neighbour {
                edges.push([vertex, neighbour]);
            }
        }
    }

    let mut faces = with_capacity(24)?;
    for first_axis in 0..4 {
        for second_axis in (first_axis + 1)..4 {
            let remaining_axes =
                filtered_indices(4, 2, |axis| axis != first_axis && axis != second_axis)?;
            for fixed_bits in 0..4 {
                let mut base = 0;
                for (position, axis) in remaining_axes.iter().enumerate() {
                    if fixed_bits & (1 << position) != 0 {
                        base |= 1 << axis;
                    }
                }
                faces.push(index_list(&[
                    base,
                    base | (1 << first_axis),
                    base | (1 << first_axis) | (1 << second_axis),
                    base | (1 << second_axis),
                ])?);
            }
        }
    }

    let mut cells = with_capacity(8)?;
    for fixed_axis in 0..4 {
        for fixed_bit in 0..2 {
            cells.push(filtered_indices(16, 8, |vertex| {
                (vertex >> fixed_axis) & 1 == fixed_bit
            })?);
        }
    }

    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::Tesseract.expected_counts(),
    )
}

fn sixteen_cell_topology() -> PolytopeResult<Polytope4DTopology> {
    let mut vertices = with_capacity(8)?;
    for axis in 0..4 {
        for sign in [-1.0, 1.0] {
            let mut coordinates = [0.0; 4];
            coordinates[axis] = sign;
            vertices.push(Point4D::from_coordinates(coordinates));
        }
    }

    let mut edges = with_capacity(24)?;
    for first in 0..8 {
        for second in (first + 1)..8 {
            if first / 2 != second / 2 {
                edges.push([first, second]);
            }
        }
    }

    let mut faces = with_capacity(32)?;
    for first_axis in 0..4 {
        for second_axis in (first_axis + 1)..4 {
            for third_axis in (second_axis + 1)..4 {
                for signs in 0..8 {
                    faces.push(index_list(&[
                        first_axis * 2 + (signs & 1),
                        second_axis * 2 + ((signs >> 1) & 1),
                        third_axis * 2 + ((signs >> 2) & 1),
                    ])?);
                }
            }
        }
    }

    let mut cells = with_capacity(16)?;
    for signs in 0..16 {
        let mut cell = with_capacity(4)?;
        for axis in 0..4 {
            cell.push(axis * 2 + ((signs >> axis) & 1));
        }
        cells.push(cell);
    }

    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::SixteenCell.expected_counts(),
    )
}

fn twenty_four_cell_topology() -> PolytopeResult<Polytope4DTopology> {
    let mut vertices = with_capacity(24)?;
    for first_axis in 0..4 {
        for second_axis in (first_axis + 1)..4 {
            for signs in 0..4 {
                let mut coordinates = [0.0; 4];
                coordinates[first_axis] = if signs & 1 == 0 { -1.0 } else { 1.0 };
                coordinates[second_axis] = if signs & 2 == 0 { -1.0 } else { 1.0 };
                vertices.push(Point4D::from_coordinates(coordinates));
            }
        }
    }
    let edges = edges_at_minimum_distance(&vertices, 96)?;

    let mut cells = with_capacity(24)?;
    for axis in 0..4 {
        for sign in [-1.0, 1.0] {
            cells.push(filtered_indices(24, 6, |index| {
                vertices[index].coordinates()[axis] == sign
            })?);
        }
    }
    for signs in 0..16 {
        let signs: [f64; 4] =
            core::array::from_fn(|axis| if signs & (1 << axis) == 0 { -1.0 } else { 1.0 });
        cells.push(filtered_indices(24, 6, |index| {
            vertices[index]
                .coordinates()
                .iter()
                .zip(signs)
                .all(|(&coordinate, sign)| coordinate == 0.0 || coordinate.signum() == sign)
        })?);
    }
    let faces = triangular_faces_from_cells(&cells, &edges, 96)?;

    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::TwentyFourCell.expected_counts(),
    )
}

fn h4_vertices() -> PolytopeResult<Vec<Point4D>> {
    let golden_ratio = (1.0 + 5.0_f64.sqrt()) * 0.5;
    let mut vertices = with_capacity(120)?;

    for axis in 0..4 {
        for sign in [-1.0, 1.0] {
            let mut coordinates = [0.0; 4];
            coordinates[axis] = sign;
            vertices.push(Point4D::from_coordinates(coordinates));
        }
    }
    for signs in 0..16 {
        vertices.push(Point4D::from_coordinates(core::array::from_fn(|axis| {
            if signs & (1 << axis) == 0 {
                -0.5
            } else {
                0.5
            }
        })));
    }

    let values = [0.0, 0.5, golden_ratio * 0.5, 0.5 / golden_ratio];
    for first in 0..4 {
        for second in 0..4 {
            for third in 0..4 {
                for fourth in 0..4 {
                    let permutation = [first, second, third, fourth];
                    if !is_permutation(&permutation) || !is_even_permutation(&permutation) {
                        continue;
                    }
                    for signs in 0..8 {
                        let mut signed_values = values;
                        for (value_index, value) in signed_values.iter_mut().enumerate().skip(1) {
                            if signs & (1 << (value_index - 1)) != 0 {
                                *value = -*value;
                            }
                        }
                        vertices.push(Point4D::from_coordinates(core::array::from_fn(|axis| {
                            signed_values[permutation[axis]]
                        })));
                    }
                }
            }
        }
    }

    (vertices.len() == 120)
        .then_some(vertices)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)
}

fn is_permutation(permutation: &[usize; 4]) -> bool {
    permutation.iter().all(|&value| value < 4)
        && permutation
            .iter()
            .enumerate()
            .all(|(position, &value)| !permutation[..position].contains(&value))
}

fn is_even_permutation(permutation: &[usize; 4]) -> bool {
    let mut inversions = 0;
    for first in 0..4 {
        for second in (first + 1)..4 {
            if permutation[first] > permutation[second] {
                inversions += 1;
            }
        }
    }
    inversions % 2 == 0
}

fn determinant3(matrix: [[f64; 3]; 3]) -> f64 {
    matrix[0][0].mul_add(
        matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1],
        -matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
            + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]),
    )
}

fn tetrahedron_normal(vertices: &[Point4D], cell: &[usize]) -> Option<Point4D> {
    if cell.len() != 4 || cell.iter().any(|&index| index >= vertices.len()) {
        return None;
    }
    let base = vertices[cell[0]].coordinates();
    let mut rows = [[0.0; 4]; 3];
    for (row, &vertex_index) in rows.iter_mut().zip(cell.iter().skip(1)) {
        let point = vertices[vertex_index].coordinates();
        for (entry, (&point_coordinate, &base_coordinate)) in
            row.iter_mut().zip(point.iter().zip(base.iter()))
        {
            *entry = point_coordinate - base_coordinate;
        }
    }

    let mut components = [0.0; 4];
    for (omitted_column, component) in components.iter_mut().enumerate() {
        let mut minor = [[0.0; 3]; 3];
        for (minor_row, row) in minor.iter_mut().zip(rows.iter()) {
            let mut minor_column = 0;
            for (column, &value) in row.iter().enumerate() {
                if column != omitted_column {
                    minor_row[minor_column] = value;
                    minor_column += 1;
                }
            }
        }
        let determinant = determinant3(minor);
        *component = if omitted_column % 2 == 0 {
            determinant
        } else {
            -determinant
        };
    }
    let normal = Point4D::from_coordinates(components);
    (normal.is_finite() && normal.norm_squared() > TOPOLOGY_TOLERANCE.powi(2)).then_some(normal)
}

fn supporting_hyperplane_for_tetrahedron(
    vertices: &[Point4D],
    cell: &[usize],
) -> Option<(Point4D, f64)> {
    let mut normal = tetrahedron_normal(vertices, cell)?;
    let mut offset = normal.dot(*vertices.get(*cell.first()?)?);
    let tolerance = TOPOLOGY_TOLERANCE * normal.norm();
    let all_at_or_below = vertices
        .iter()
        .all(|vertex| normal.dot(*vertex) <= offset + tolerance);
    let all_at_or_above = vertices
        .iter()
        .all(|vertex| normal.dot(*vertex) >= offset - tolerance);
    if !all_at_or_below && !all_at_or_above {
        return None;
    }
    if all_at_or_above {
        normal = normal.scaled(-1.0);
        offset = -offset;
    }
    if !offset.is_finite() || offset <= tolerance {
        return None;
    }
    let coplanar_count = vertices
        .iter()
        .filter(|vertex| (normal.dot(**vertex) - offset).abs() <= tolerance)
        .count();
    (coplanar_count == 4).then_some((normal, offset))
}

fn supporting_tetrahedral_cliques(
    vertices: &[Point4D],
    edges: &[[usize; 2]],
    expected_count: usize,
) -> PolytopeResult<Vec<Vec<usize>>> {
    let adjacency = build_vertex_adjacency(vertices.len(), edges)?;
    let mut cells = with_capacity(expected_count)?;
    for first in 0..vertices.len() {
        for &second in &adjacency[first] {
            if second <= first {
                continue;
            }
            for &third in &adjacency[second] {
                if third <= second || adjacency[first].binary_search(&third).is_err() {
                    continue;
                }
                for &fourth in &adjacency[third] {
                    if fourth <= third
                        || adjacency[first].binary_search(&fourth).is_err()
                        || adjacency[second].binary_search(&fourth).is_err()
                    {
                        continue;
                    }
                    let cell = [first, second, third, fourth];
                    if supporting_hyperplane_for_tetrahedron(vertices, &cell).is_some() {
                        if cells.len() == expected_count {
                            return Err(Polytope4DError::InvalidCanonicalTopology);
                        }
                        cells.push(index_list(&cell)?);
                    }
                }
            }
        }
    }
    (cells.len() == expected_count)
        .then_some(cells)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)
}

fn six_hundred_cell_topology() -> PolytopeResult<Polytope4DTopology> {
    let vertices = h4_vertices()?;
    let edges = edges_at_minimum_distance(&vertices, 720)?;
    let cells = supporting_tetrahedral_cliques(&vertices, &edges, 600)?;
    let faces = triangular_faces_from_cells(&cells, &edges, 1200)?;
    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::SixHundredCell.expected_counts(),
    )
}

fn cells_incident_to_vertices(
    cells: &[Vec<usize>],
    required_vertices: &[usize],
    expected_count: usize,
) -> PolytopeResult<Vec<usize>> {
    let mut incident = with_capacity(expected_count)?;
    for (cell_index, cell) in cells.iter().enumerate() {
        if required_vertices.iter().all(|vertex| cell.contains(vertex)) {
            if incident.len() == expected_count {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
            incident.push(cell_index);
        }
    }
    (incident.len() == expected_count)
        .then_some(incident)
        .ok_or(Polytope4DError::InvalidCanonicalTopology)
}

fn add_neighbour(neighbours: &mut Vec<usize>, neighbour: usize) -> PolytopeResult<()> {
    if !neighbours.contains(&neighbour) {
        if neighbours.len() == 2 {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }
        neighbours.push(neighbour);
    }
    Ok(())
}

fn ordered_cells_around_primal_edge(
    primal: &Polytope4DTopology,
    edge: [usize; 2],
    incident_cells: &[usize],
) -> PolytopeResult<Vec<usize>> {
    if incident_cells.len() != 5 {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }
    let mut neighbours = with_capacity(incident_cells.len())?;
    for _ in incident_cells {
        neighbours.push(with_capacity(2)?);
    }

    for primal_face in &primal.faces {
        if !primal_face.contains(&edge[0]) || !primal_face.contains(&edge[1]) {
            continue;
        }
        let sharing_cells = cells_incident_to_vertices(&primal.cells, primal_face, 2)?;
        let first = incident_cells
            .iter()
            .position(|&cell| cell == sharing_cells[0])
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
        let second = incident_cells
            .iter()
            .position(|&cell| cell == sharing_cells[1])
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
        if first == second {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }
        add_neighbour(&mut neighbours[first], sharing_cells[1])?;
        add_neighbour(&mut neighbours[second], sharing_cells[0])?;
    }
    if neighbours.iter().any(|entries| entries.len() != 2) {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }

    let start = *incident_cells
        .iter()
        .min()
        .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
    let mut ordered = with_capacity(incident_cells.len())?;
    let mut current = start;
    let mut previous = None;
    for position in 0..incident_cells.len() {
        if ordered.contains(&current) {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }
        ordered.push(current);
        let current_position = incident_cells
            .iter()
            .position(|&cell| cell == current)
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
        let next = match previous {
            Some(previous_cell) => *neighbours[current_position]
                .iter()
                .find(|&&candidate| candidate != previous_cell)
                .ok_or(Polytope4DError::InvalidCanonicalTopology)?,
            None => *neighbours[current_position]
                .iter()
                .min()
                .ok_or(Polytope4DError::InvalidCanonicalTopology)?,
        };
        if position + 1 == incident_cells.len() {
            if next != start {
                return Err(Polytope4DError::InvalidCanonicalTopology);
            }
        } else if next == start || ordered.contains(&next) {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        } else {
            previous = Some(current);
            current = next;
        }
    }
    Ok(ordered)
}

fn one_twenty_cell_topology() -> PolytopeResult<Polytope4DTopology> {
    let primal = six_hundred_cell_topology()?;
    let mut vertices = with_capacity(600)?;
    for cell in &primal.cells {
        let (normal, offset) = supporting_hyperplane_for_tetrahedron(&primal.vertices, cell)
            .ok_or(Polytope4DError::InvalidCanonicalTopology)?;
        let dual_vertex = normal.scaled(offset.recip());
        if !dual_vertex.is_finite() {
            return Err(Polytope4DError::InvalidCanonicalTopology);
        }
        vertices.push(dual_vertex);
    }

    let mut edges = with_capacity(1200)?;
    for primal_face in &primal.faces {
        let incident = cells_incident_to_vertices(&primal.cells, primal_face, 2)?;
        let mut edge = [incident[0], incident[1]];
        edge.sort_unstable();
        edges.push(edge);
    }
    edges.sort_unstable();
    if edges.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Polytope4DError::InvalidCanonicalTopology);
    }

    let mut faces = with_capacity(720)?;
    for &primal_edge in &primal.edges {
        let incident = cells_incident_to_vertices(&primal.cells, &primal_edge, 5)?;
        faces.push(ordered_cells_around_primal_edge(
            &primal,
            primal_edge,
            &incident,
        )?);
    }

    let mut cells = with_capacity(120)?;
    for primal_vertex in 0..primal.vertices.len() {
        cells.push(cells_incident_to_vertices(
            &primal.cells,
            &[primal_vertex],
            20,
        )?);
    }

    finish_topology(
        vertices,
        edges,
        faces,
        cells,
        RegularPolychoron::OneTwentyCell.expected_counts(),
    )
}

/// Menor dimension admitida por las familias regulares N-D genericas.
pub const MIN_REGULAR_POLYTOPE_DIMENSION: usize = 3;

/// Mayor dimension admitida por las familias regulares N-D genericas.
///
/// El limite conserva la construccion del hipercubo en un maximo de 1.024
/// vertices y 5.120 aristas, antes de aplicar un presupuesto mas estricto.
pub const MAX_REGULAR_POLYTOPE_DIMENSION: usize = 10;

/// Maximo de vertices que la proyeccion N-D masiva acepta en una llamada.
///
/// Coincide con el maximo de vertices de la topologia generica predeterminada,
/// por lo que todas las familias admitidas se pueden proyectar sin ampliar el
/// presupuesto. Las colecciones mayores se rechazan antes de reservar salida.
pub const MAX_ND_PROJECTION_VERTICES: usize = 1_024;

/// Margen relativo para cada plano de perspectiva durante la reduccion N-D.
pub const PERSPECTIVE_ND_NEAR_EPSILON: f64 = 1.0e-9;

/// Familia de politopos convexos regulares disponible en R^n para `n` en `3..=10`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegularPolytopeFamily {
    /// El simplex regular con `n + 1` vertices.
    Simplex,
    /// El hipercubo regular con `2^n` vertices.
    Hypercube,
    /// El politopo cruzado regular con `2n` vertices antipodales.
    CrossPolytope,
}

/// Conteos procedurales de una topologia regular N-D admitida (`3..=10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegularPolytopeCounts {
    /// Cantidad de vertices canonicos.
    pub vertices: usize,
    /// Cantidad de aristas canonicas.
    pub edges: usize,
}

/// Limites de recursos evaluados antes de reservar memoria para una topologia N-D admitida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegularPolytopeBudget {
    /// Maximo de vertices permitidos.
    pub max_vertices: usize,
    /// Maximo de aristas permitidas.
    pub max_edges: usize,
    /// Maximo de valores `f64` que pueden ocupar las coordenadas de vertices.
    pub max_coordinate_values: usize,
}

impl RegularPolytopeBudget {
    /// Crea un presupuesto explicito para una topologia regular N-D en `3..=10`.
    pub const fn new(max_vertices: usize, max_edges: usize, max_coordinate_values: usize) -> Self {
        Self {
            max_vertices,
            max_edges,
            max_coordinate_values,
        }
    }
}

impl Default for RegularPolytopeBudget {
    fn default() -> Self {
        Self::new(MAX_ND_PROJECTION_VERTICES, 5_120, 10_240)
    }
}

/// Error al construir, rotar o proyectar un politopo regular N-D.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegularPolytopeError {
    /// La dimension no pertenece al intervalo admitido.
    InvalidDimension {
        dimension: usize,
        minimum: usize,
        maximum: usize,
    },
    /// Los conteos calculados exceden el presupuesto antes de reservar memoria.
    ResourceLimitExceeded {
        required_vertices: usize,
        required_edges: usize,
        required_coordinate_values: usize,
        max_vertices: usize,
        max_edges: usize,
        max_coordinate_values: usize,
    },
    /// La proyeccion masiva recibio mas vertices de los permitidos.
    ProjectionVertexLimitExceeded { requested: usize, maximum: usize },
    /// Un calculo de conteos o indices no cabe en `usize`.
    ArithmeticOverflow,
    /// No fue posible reservar una coleccion acotada.
    AllocationFailed,
    /// Los vertices, aristas o adyacencias no respetan las invariantes canonicas.
    InvalidCanonicalTopology,
    /// Un punto no tiene las tres coordenadas minimas para una proyeccion 3D.
    InvalidCoordinateDimension { actual: usize, minimum: usize },
    /// Una coordenada de entrada o un resultado intermedio no es finito.
    NonFiniteCoordinate,
    /// La distancia o el margen de perspectiva no son positivos y finitos.
    InvalidProjectionParameters,
    /// Un eje eliminado cae sobre o demasiado cerca de su plano de perspectiva.
    PerspectiveNearPlane { axis: usize },
    /// Los ejes de la rotacion no forman un plano valido de coordenadas.
    InvalidRotationPlane {
        first_axis: usize,
        second_axis: usize,
        dimension: usize,
    },
    /// El angulo de rotacion no es finito.
    NonFiniteRotationAngle,
}

impl fmt::Display for RegularPolytopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimension {
                dimension,
                minimum,
                maximum,
            } => write!(
                formatter,
                "la dimension {dimension} debe pertenecer al intervalo {minimum}..={maximum}"
            ),
            Self::ResourceLimitExceeded { .. } => formatter.write_str(
                "la topologia regular N-D excede el presupuesto de recursos antes de reservar memoria",
            ),
            Self::ProjectionVertexLimitExceeded { requested, maximum } => write!(
                formatter,
                "la proyeccion N-D recibio {requested} vertices y admite como maximo {maximum}"
            ),
            Self::ArithmeticOverflow => {
                formatter.write_str("los conteos de la topologia regular N-D desbordan usize")
            }
            Self::AllocationFailed => {
                formatter.write_str("no se pudo reservar la topologia regular N-D")
            }
            Self::InvalidCanonicalTopology => {
                formatter.write_str("la topologia regular N-D canonica es invalida")
            }
            Self::InvalidCoordinateDimension { actual, minimum } => write!(
                formatter,
                "la proyeccion requiere al menos {minimum} coordenadas, recibio {actual}"
            ),
            Self::NonFiniteCoordinate => {
                formatter.write_str("las coordenadas de la topologia o proyeccion deben ser finitas")
            }
            Self::InvalidProjectionParameters => formatter.write_str(
                "la distancia y el margen de perspectiva deben ser positivos y finitos",
            ),
            Self::PerspectiveNearPlane { axis } => write!(
                formatter,
                "la coordenada del eje {axis} esta demasiado cerca del plano de perspectiva"
            ),
            Self::InvalidRotationPlane {
                first_axis,
                second_axis,
                dimension,
            } => write!(
                formatter,
                "los ejes {first_axis} y {second_axis} no forman un plano valido en R^{dimension}"
            ),
            Self::NonFiniteRotationAngle => {
                formatter.write_str("el angulo de rotacion debe ser finito")
            }
        }
    }
}

impl std::error::Error for RegularPolytopeError {}

type RegularPolytopeResult<T> = Result<T, RegularPolytopeError>;

impl RegularPolytopeFamily {
    /// Devuelve una cota radial canónica en R^n sin construir vertices ni aristas.
    ///
    /// El simplex parte de `sqrt(n / (n + 1))`, el hipercubo de `sqrt(n)` y el
    /// politopo cruzado de radio unitario. La cota se eleva levemente para no
    /// subestimar las coordenadas canónicas materializadas por redondeo.
    pub fn canonical_radius_bound(
        self,
        dimension: usize,
    ) -> Result<f64, RegularPolytopeProjectionError> {
        validate_projection_dimension(dimension)?;
        let dimension = dimension as f64;
        let radius = match self {
            Self::Simplex => (dimension / (dimension + 1.0)).sqrt(),
            Self::Hypercube => dimension.sqrt(),
            Self::CrossPolytope => 1.0,
        };
        // Los vertices canónicos se construyen por una secuencia distinta de
        // operaciones; elevar la fórmula evita subestimar sus normas por ulps.
        conservative_radius_bound(radius)
            .ok_or(RegularPolytopeProjectionError::NonFiniteCanonicalRadius)
    }

    /// Planifica la perspectiva N-D desde selectores y escala, sin materializar topologia.
    pub fn projection_plan(
        self,
        dimension: usize,
        scale: f64,
    ) -> Result<RegularPolytopeProjectionPlan, RegularPolytopeProjectionError> {
        let canonical_radius = self.canonical_radius_bound(dimension)?;
        plan_regular_polytope_projection(dimension, scale, canonical_radius)
    }

    /// Calcula los conteos exactos de la familia en una dimension `3..=10`.
    pub fn expected_counts(self, dimension: usize) -> RegularPolytopeResult<RegularPolytopeCounts> {
        validate_regular_polytope_dimension(dimension)?;

        let vertices = match self {
            Self::Simplex => dimension
                .checked_add(1)
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
            Self::Hypercube => checked_power_of_two(dimension)?,
            Self::CrossPolytope => dimension
                .checked_mul(2)
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
        };
        let edges = match self {
            Self::Simplex => complete_graph_edge_count(vertices)?,
            Self::Hypercube => dimension
                .checked_mul(
                    vertices
                        .checked_div(2)
                        .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
                )
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
            Self::CrossPolytope => vertices
                .checked_mul(
                    dimension
                        .checked_sub(1)
                        .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
                )
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?,
        };

        Ok(RegularPolytopeCounts { vertices, edges })
    }

    /// Construye la topologia canonica en una dimension `3..=10` bajo el presupuesto predeterminado.
    pub fn topology(self, dimension: usize) -> RegularPolytopeResult<RegularPolytopeTopology> {
        self.topology_with_budget(dimension, RegularPolytopeBudget::default())
    }

    /// Construye la topologia canonica en una dimension `3..=10` tras validar recursos.
    pub fn topology_with_budget(
        self,
        dimension: usize,
        budget: RegularPolytopeBudget,
    ) -> RegularPolytopeResult<RegularPolytopeTopology> {
        let counts = self.expected_counts(dimension)?;
        let coordinate_values = counts
            .vertices
            .checked_mul(dimension)
            .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
        if counts.vertices > budget.max_vertices
            || counts.edges > budget.max_edges
            || coordinate_values > budget.max_coordinate_values
        {
            return Err(RegularPolytopeError::ResourceLimitExceeded {
                required_vertices: counts.vertices,
                required_edges: counts.edges,
                required_coordinate_values: coordinate_values,
                max_vertices: budget.max_vertices,
                max_edges: budget.max_edges,
                max_coordinate_values: budget.max_coordinate_values,
            });
        }

        let vertices = match self {
            Self::Simplex => simplex_nd_vertices(dimension, counts.vertices)?,
            Self::Hypercube => hypercube_nd_vertices(dimension, counts.vertices)?,
            Self::CrossPolytope => cross_polytope_nd_vertices(dimension, counts.vertices)?,
        };
        let edges = match self {
            Self::Simplex => complete_nd_edges(counts.vertices, counts.edges)?,
            Self::Hypercube => hypercube_nd_edges(dimension, counts.vertices, counts.edges)?,
            Self::CrossPolytope => cross_polytope_nd_edges(counts.vertices, counts.edges)?,
        };

        finish_regular_nd_topology(self, dimension, vertices, edges, counts)
    }
}

/// Topologia canonica de una familia regular en R^n para `n` en `3..=10`.
///
/// La API materializa solamente vertices, aristas y adyacencias: las caras de
/// dimension mayor se generan de forma procedimental cuando un renderizador las
/// necesite. Esto evita una explosion exponencial de poligonos que no se usa en
/// el alcance wireframe actual. La topologia nunca depende de una proyeccion.
#[derive(Debug, Clone, PartialEq)]
pub struct RegularPolytopeTopology {
    /// Familia regular que genero la topologia.
    pub family: RegularPolytopeFamily,
    /// Dimension euclidea de cada vertice.
    pub dimension: usize,
    /// Coordenadas canonicas en `f64`, una entrada por vertice.
    pub vertices: Vec<Vec<f64>>,
    /// Aristas ordenadas y sin duplicados como pares de indices de vertices.
    pub edges: Vec<[usize; 2]>,
    /// Vecinos ordenados de cada vertice, derivado de `edges`.
    pub vertex_adjacency: Vec<Vec<usize>>,
}

impl RegularPolytopeTopology {
    /// Construye la topologia de una familia N-D en `3..=10` con el presupuesto predeterminado.
    pub fn try_new(family: RegularPolytopeFamily, dimension: usize) -> RegularPolytopeResult<Self> {
        family.topology(dimension)
    }

    /// Construye la topologia de una familia N-D en `3..=10` con un presupuesto explicito.
    pub fn try_new_with_budget(
        family: RegularPolytopeFamily,
        dimension: usize,
        budget: RegularPolytopeBudget,
    ) -> RegularPolytopeResult<Self> {
        family.topology_with_budget(dimension, budget)
    }

    /// Devuelve los conteos ya materializados sin regenerar la topologia.
    pub fn counts(&self) -> RegularPolytopeCounts {
        RegularPolytopeCounts {
            vertices: self.vertices.len(),
            edges: self.edges.len(),
        }
    }

    /// Devuelve la longitud de la primera arista valida.
    pub fn edge_length(&self) -> Option<f64> {
        let [first, second] = *self.edges.first()?;
        let first_vertex = self.vertices.get(first)?;
        let second_vertex = self.vertices.get(second)?;
        let distance = euclidean_distance(first_vertex, second_vertex);
        distance.is_finite().then_some(distance)
    }

    /// Comprueba dimension, conteos, coordenadas, aristas y adyacencias.
    pub fn validate(&self) -> RegularPolytopeResult<()> {
        validate_regular_polytope_dimension(self.dimension)?;
        let expected = self.family.expected_counts(self.dimension)?;
        if self.counts() != expected
            || self.vertex_adjacency.len() != self.vertices.len()
            || self.vertices.iter().any(|vertex| {
                vertex.len() != self.dimension || vertex.iter().any(|value| !value.is_finite())
            })
        {
            return Err(RegularPolytopeError::InvalidCanonicalTopology);
        }

        let expected_degree = regular_nd_vertex_degree(self.family, self.dimension)?;
        for (position, &[first, second]) in self.edges.iter().enumerate() {
            if first >= self.vertices.len()
                || second >= self.vertices.len()
                || first >= second
                || (position > 0 && self.edges[position - 1] >= [first, second])
            {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
        }

        for (vertex, neighbours) in self.vertex_adjacency.iter().enumerate() {
            if neighbours.len() != expected_degree
                || neighbours.windows(2).any(|pair| pair[0] >= pair[1])
                || neighbours.iter().any(|&neighbour| {
                    neighbour >= self.vertices.len()
                        || neighbour == vertex
                        || !contains_nd_edge(&self.edges, vertex, neighbour)
                })
            {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
        }
        for &[first, second] in &self.edges {
            if self.vertex_adjacency[first].binary_search(&second).is_err()
                || self.vertex_adjacency[second].binary_search(&first).is_err()
            {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
        }

        let edge_length = self
            .edge_length()
            .filter(|length| *length > 0.0)
            .ok_or(RegularPolytopeError::InvalidCanonicalTopology)?;
        let tolerance = TOPOLOGY_TOLERANCE * edge_length.max(1.0);
        for &[first, second] in &self.edges {
            let distance = euclidean_distance(&self.vertices[first], &self.vertices[second]);
            if !distance.is_finite() || (distance - edge_length).abs() > tolerance {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
        }

        Ok(())
    }
}

/// Proyeccion perspectiva determinista que reduce una coordenada de R^n a R3.
///
/// Cada eje desde el ultimo hasta el cuarto se elimina con el factor
/// `distance / (distance - coordinate)`. Por ello todos los ejes adicionales
/// afectan el resultado, y un plano cercano se rechaza antes de producir valores
/// enormes o no finitos. Es un helper numerico para coordenadas de dimension
/// tres o mayor; las topologias regulares genericas de este modulo se limitan a
/// dimensiones `3..=10`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NdPerspectiveProjection {
    distance: f64,
    near_epsilon: f64,
}

impl NdPerspectiveProjection {
    /// Crea una proyeccion con el margen relativo predeterminado.
    pub fn new(distance: f64) -> RegularPolytopeResult<Self> {
        Self::with_near_epsilon(distance, PERSPECTIVE_ND_NEAR_EPSILON)
    }

    /// Crea una proyeccion con un margen relativo explicito.
    pub fn with_near_epsilon(distance: f64, near_epsilon: f64) -> RegularPolytopeResult<Self> {
        if !distance.is_finite()
            || distance <= 0.0
            || !near_epsilon.is_finite()
            || near_epsilon <= 0.0
        {
            return Err(RegularPolytopeError::InvalidProjectionParameters);
        }
        Ok(Self {
            distance,
            near_epsilon,
        })
    }

    /// Distancia de los planos de perspectiva sucesivos.
    pub const fn distance(self) -> f64 {
        self.distance
    }

    /// Margen relativo usado para rechazar cada plano cercano.
    pub const fn near_epsilon(self) -> f64 {
        self.near_epsilon
    }

    /// Proyecta una coordenada de dimension tres o mayor hacia R3.
    pub fn project(self, coordinates: &[f64]) -> RegularPolytopeResult<Point3D> {
        if coordinates.len() < MIN_REGULAR_POLYTOPE_DIMENSION {
            return Err(RegularPolytopeError::InvalidCoordinateDimension {
                actual: coordinates.len(),
                minimum: MIN_REGULAR_POLYTOPE_DIMENSION,
            });
        }
        if coordinates.iter().any(|coordinate| !coordinate.is_finite()) {
            return Err(RegularPolytopeError::NonFiniteCoordinate);
        }

        let near_limit = self.near_epsilon * self.distance.abs().max(1.0);
        if !near_limit.is_finite() {
            return Err(RegularPolytopeError::InvalidProjectionParameters);
        }

        let mut cumulative_scale = 1.0;
        for axis in (MIN_REGULAR_POLYTOPE_DIMENSION..coordinates.len()).rev() {
            let eliminated_coordinate = coordinates[axis] * cumulative_scale;
            if !eliminated_coordinate.is_finite() {
                return Err(RegularPolytopeError::NonFiniteCoordinate);
            }
            let denominator = self.distance - eliminated_coordinate;
            if !denominator.is_finite() || denominator.abs() <= near_limit {
                return Err(RegularPolytopeError::PerspectiveNearPlane { axis });
            }
            let scale = self.distance / denominator;
            if !scale.is_finite() {
                return Err(RegularPolytopeError::NonFiniteCoordinate);
            }
            cumulative_scale *= scale;
            if !cumulative_scale.is_finite() {
                return Err(RegularPolytopeError::NonFiniteCoordinate);
            }
        }

        let projected = Point3D::new(
            coordinates[0] * cumulative_scale,
            coordinates[1] * cumulative_scale,
            coordinates[2] * cumulative_scale,
        );
        projected
            .is_finite()
            .then_some(projected)
            .ok_or(RegularPolytopeError::NonFiniteCoordinate)
    }

    /// Proyecta una coleccion de hasta [`MAX_ND_PROJECTION_VERTICES`] vertices.
    ///
    /// Una coleccion mayor devuelve
    /// [`RegularPolytopeError::ProjectionVertexLimitExceeded`] antes de reservar
    /// el vector de salida. La topologia de origen nunca se modifica.
    pub fn project_vertices(self, vertices: &[Vec<f64>]) -> RegularPolytopeResult<Vec<Point3D>> {
        if vertices.len() > MAX_ND_PROJECTION_VERTICES {
            return Err(RegularPolytopeError::ProjectionVertexLimitExceeded {
                requested: vertices.len(),
                maximum: MAX_ND_PROJECTION_VERTICES,
            });
        }
        let mut projected = nd_vec_with_capacity(vertices.len())?;
        for vertex in vertices {
            projected.push(self.project(vertex)?);
        }
        Ok(projected)
    }
}

/// Rota un vector en el plano determinado por dos ejes distintos de R^n.
///
/// La funcion valida todos los argumentos y el resultado antes de mutar el
/// vector, por lo que una rotacion rechazada conserva las coordenadas originales.
pub fn rotate_nd_in_plane(
    coordinates: &mut [f64],
    first_axis: usize,
    second_axis: usize,
    angle: f64,
) -> RegularPolytopeResult<()> {
    if first_axis >= coordinates.len()
        || second_axis >= coordinates.len()
        || first_axis == second_axis
    {
        return Err(RegularPolytopeError::InvalidRotationPlane {
            first_axis,
            second_axis,
            dimension: coordinates.len(),
        });
    }
    if !angle.is_finite() {
        return Err(RegularPolytopeError::NonFiniteRotationAngle);
    }
    if coordinates.iter().any(|coordinate| !coordinate.is_finite()) {
        return Err(RegularPolytopeError::NonFiniteCoordinate);
    }

    let (sine, cosine) = angle.sin_cos();
    let first = coordinates[first_axis].mul_add(cosine, -coordinates[second_axis] * sine);
    let second = coordinates[first_axis].mul_add(sine, coordinates[second_axis] * cosine);
    if !first.is_finite() || !second.is_finite() {
        return Err(RegularPolytopeError::NonFiniteCoordinate);
    }
    coordinates[first_axis] = first;
    coordinates[second_axis] = second;
    Ok(())
}

fn one_twenty_cell_canonical_radius_bound() -> f64 {
    let golden_ratio = (1.0 + 5.0_f64.sqrt()) * 0.5;
    // El 600-celda canonico tiene radio uno y arista 1/phi. El radio de
    // su celda tetraedrica es sqrt(6) / (4 * phi), asi que el radio polar
    // del 120-celda es el reciproco de la distancia de esa celda al origen.
    let radius = (1.0 - 3.0 / (8.0 * golden_ratio * golden_ratio))
        .sqrt()
        .recip();
    // La topologia polar se materializa por operaciones de punto flotante
    // distintas. Subir la expresion cerrada evita subestimar sus normas.
    conservative_radius_bound(radius).unwrap_or(radius)
}

fn conservative_radius_bound(radius: f64) -> Option<f64> {
    // Los generadores canónicos hacen una cantidad acotada de operaciones por
    // coordenada (dimension <= 10). Este margen relativo cubre sus redondeos
    // sin depender de materializar topología durante la validación.
    let inflated = radius * (1.0 + 16.0 * f64::EPSILON);
    rounded_up_positive(inflated)
}

fn plan_regular_polytope_projection(
    dimension: usize,
    scale: f64,
    canonical_radius: f64,
) -> Result<RegularPolytopeProjectionPlan, RegularPolytopeProjectionError> {
    validate_projection_dimension(dimension)?;
    if !scale.is_finite() {
        return Err(RegularPolytopeProjectionError::NonFiniteScale);
    }
    if scale <= 0.0 {
        return Err(RegularPolytopeProjectionError::NonPositiveScale);
    }
    if !canonical_radius.is_finite() {
        return Err(RegularPolytopeProjectionError::NonFiniteCanonicalRadius);
    }
    if canonical_radius <= 0.0 {
        return Err(RegularPolytopeProjectionError::NonPositiveCanonicalRadius);
    }

    // Se eleva la cota unos ulps para no subestimar por redondeos de productos.
    let source_radius = rounded_up_positive(scale * canonical_radius)
        .ok_or(RegularPolytopeProjectionError::SourceRadiusOverflow)?;
    let perspective_factor = dimension
        .checked_add(2)
        .and_then(|value| u32::try_from(value).ok())
        .map(f64::from)
        .ok_or(RegularPolytopeProjectionError::PerspectiveDistanceOverflow)?;
    let distance = perspective_factor * source_radius.max(1.0);
    if !distance.is_finite() || distance <= 0.0 {
        return Err(RegularPolytopeProjectionError::PerspectiveDistanceOverflow);
    }

    let denominator = rounded_down_positive(distance - source_radius)
        .ok_or(RegularPolytopeProjectionError::InvalidPerspectiveDenominator)?;

    // Algebraicamente equivale a R * distance / (distance - R), pero evita
    // desbordar el producto intermedio R * distance para escalas grandes. El
    // denominador baja y los resultados suben unos ulps para no subestimar.
    let perspective_scale = rounded_up_positive(distance / denominator)
        .ok_or(RegularPolytopeProjectionError::ProjectedCoordinateBoundOverflow)?;
    let projected_coordinate_bound = rounded_up_positive(source_radius * perspective_scale)
        .ok_or(RegularPolytopeProjectionError::ProjectedCoordinateBoundOverflow)?;

    Ok(RegularPolytopeProjectionPlan {
        dimension,
        source_radius,
        distance,
        projected_coordinate_bound,
    })
}

fn rounded_up_positive(value: f64) -> Option<f64> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }

    // `next_up` no esta disponible en todo el MSRV del proyecto. Avanzar los
    // bits de un f64 positivo conserva una cota finita estrictamente superior.
    let bits = value.to_bits().checked_add(1)?;
    let rounded = f64::from_bits(bits);
    (rounded.is_finite() && rounded > 0.0).then_some(rounded)
}

fn rounded_down_positive(value: f64) -> Option<f64> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }

    let bits = value.to_bits().checked_sub(1)?;
    let rounded = f64::from_bits(bits);
    (rounded.is_finite() && rounded > 0.0).then_some(rounded)
}

fn validate_projection_dimension(dimension: usize) -> Result<(), RegularPolytopeProjectionError> {
    if !(MIN_REGULAR_POLYTOPE_DIMENSION..=MAX_REGULAR_POLYTOPE_DIMENSION).contains(&dimension) {
        return Err(RegularPolytopeProjectionError::InvalidDimension {
            dimension,
            minimum: MIN_REGULAR_POLYTOPE_DIMENSION,
            maximum: MAX_REGULAR_POLYTOPE_DIMENSION,
        });
    }
    Ok(())
}

fn validate_regular_polytope_dimension(dimension: usize) -> RegularPolytopeResult<()> {
    if !(MIN_REGULAR_POLYTOPE_DIMENSION..=MAX_REGULAR_POLYTOPE_DIMENSION).contains(&dimension) {
        return Err(RegularPolytopeError::InvalidDimension {
            dimension,
            minimum: MIN_REGULAR_POLYTOPE_DIMENSION,
            maximum: MAX_REGULAR_POLYTOPE_DIMENSION,
        });
    }
    Ok(())
}

fn checked_power_of_two(exponent: usize) -> RegularPolytopeResult<usize> {
    let exponent = u32::try_from(exponent).map_err(|_| RegularPolytopeError::ArithmeticOverflow)?;
    1_usize
        .checked_shl(exponent)
        .ok_or(RegularPolytopeError::ArithmeticOverflow)
}

fn complete_graph_edge_count(vertex_count: usize) -> RegularPolytopeResult<usize> {
    let preceding_vertices = vertex_count
        .checked_sub(1)
        .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
    vertex_count
        .checked_mul(preceding_vertices)
        .ok_or(RegularPolytopeError::ArithmeticOverflow)
        .map(|pair_product| pair_product / 2)
}

fn regular_nd_vertex_degree(
    family: RegularPolytopeFamily,
    dimension: usize,
) -> RegularPolytopeResult<usize> {
    match family {
        RegularPolytopeFamily::Simplex | RegularPolytopeFamily::Hypercube => Ok(dimension),
        RegularPolytopeFamily::CrossPolytope => dimension
            .checked_sub(1)
            .and_then(|value| value.checked_mul(2))
            .ok_or(RegularPolytopeError::ArithmeticOverflow),
    }
}

fn nd_vec_with_capacity<T>(capacity: usize) -> RegularPolytopeResult<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| RegularPolytopeError::AllocationFailed)?;
    Ok(values)
}

fn nd_zero_coordinates(dimension: usize) -> RegularPolytopeResult<Vec<f64>> {
    let mut coordinates = nd_vec_with_capacity(dimension)?;
    coordinates.resize(dimension, 0.0);
    Ok(coordinates)
}

fn simplex_nd_vertices(
    dimension: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<Vec<f64>>> {
    let mut vertices = nd_vec_with_capacity(expected_count)?;
    for vertex_index in 0..expected_count {
        let mut coordinates = nd_zero_coordinates(dimension)?;
        for (axis, coordinate) in coordinates.iter_mut().enumerate() {
            let first_count = axis
                .checked_add(1)
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
            let second_count = first_count
                .checked_add(1)
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
            let normalization = (first_count
                .checked_mul(second_count)
                .ok_or(RegularPolytopeError::ArithmeticOverflow)?
                as f64)
                .sqrt();
            if !normalization.is_finite() || normalization <= 0.0 {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
            if vertex_index <= axis {
                *coordinate = normalization.recip();
            } else if vertex_index == first_count {
                *coordinate = -(first_count as f64) / normalization;
            }
        }
        vertices.push(coordinates);
    }
    (vertices.len() == expected_count)
        .then_some(vertices)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn hypercube_nd_vertices(
    dimension: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<Vec<f64>>> {
    let mut vertices = nd_vec_with_capacity(expected_count)?;
    for bits in 0..expected_count {
        let mut coordinates = nd_zero_coordinates(dimension)?;
        for (axis, coordinate) in coordinates.iter_mut().enumerate() {
            let mask = checked_power_of_two(axis)?;
            *coordinate = if bits & mask == 0 { -1.0 } else { 1.0 };
        }
        vertices.push(coordinates);
    }
    (vertices.len() == expected_count)
        .then_some(vertices)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn cross_polytope_nd_vertices(
    dimension: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<Vec<f64>>> {
    let mut vertices = nd_vec_with_capacity(expected_count)?;
    for axis in 0..dimension {
        for sign in [-1.0, 1.0] {
            let mut coordinates = nd_zero_coordinates(dimension)?;
            coordinates[axis] = sign;
            vertices.push(coordinates);
        }
    }
    (vertices.len() == expected_count)
        .then_some(vertices)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn complete_nd_edges(
    vertex_count: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<[usize; 2]>> {
    let mut edges = nd_vec_with_capacity(expected_count)?;
    for first in 0..vertex_count {
        let first_successor = first
            .checked_add(1)
            .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
        for second in first_successor..vertex_count {
            if edges.len() == expected_count {
                return Err(RegularPolytopeError::InvalidCanonicalTopology);
            }
            edges.push([first, second]);
        }
    }
    (edges.len() == expected_count)
        .then_some(edges)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn hypercube_nd_edges(
    dimension: usize,
    vertex_count: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<[usize; 2]>> {
    let mut edges = nd_vec_with_capacity(expected_count)?;
    for vertex in 0..vertex_count {
        for axis in 0..dimension {
            let neighbour = vertex ^ checked_power_of_two(axis)?;
            if vertex < neighbour {
                if edges.len() == expected_count || neighbour >= vertex_count {
                    return Err(RegularPolytopeError::InvalidCanonicalTopology);
                }
                edges.push([vertex, neighbour]);
            }
        }
    }
    (edges.len() == expected_count)
        .then_some(edges)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn cross_polytope_nd_edges(
    vertex_count: usize,
    expected_count: usize,
) -> RegularPolytopeResult<Vec<[usize; 2]>> {
    let mut edges = nd_vec_with_capacity(expected_count)?;
    for first in 0..vertex_count {
        let first_successor = first
            .checked_add(1)
            .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
        for second in first_successor..vertex_count {
            if first / 2 != second / 2 {
                if edges.len() == expected_count {
                    return Err(RegularPolytopeError::InvalidCanonicalTopology);
                }
                edges.push([first, second]);
            }
        }
    }
    (edges.len() == expected_count)
        .then_some(edges)
        .ok_or(RegularPolytopeError::InvalidCanonicalTopology)
}

fn finish_regular_nd_topology(
    family: RegularPolytopeFamily,
    dimension: usize,
    vertices: Vec<Vec<f64>>,
    edges: Vec<[usize; 2]>,
    expected: RegularPolytopeCounts,
) -> RegularPolytopeResult<RegularPolytopeTopology> {
    let topology = RegularPolytopeTopology {
        family,
        dimension,
        vertex_adjacency: build_nd_vertex_adjacency(vertices.len(), &edges)?,
        vertices,
        edges,
    };
    if topology.counts() != expected {
        return Err(RegularPolytopeError::InvalidCanonicalTopology);
    }
    topology.validate()?;
    Ok(topology)
}

fn build_nd_vertex_adjacency(
    vertex_count: usize,
    edges: &[[usize; 2]],
) -> RegularPolytopeResult<Vec<Vec<usize>>> {
    let mut degrees = nd_vec_with_capacity(vertex_count)?;
    degrees.resize(vertex_count, 0_usize);
    for &[first, second] in edges {
        if first >= vertex_count || second >= vertex_count || first >= second {
            return Err(RegularPolytopeError::InvalidCanonicalTopology);
        }
        degrees[first] = degrees[first]
            .checked_add(1)
            .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
        degrees[second] = degrees[second]
            .checked_add(1)
            .ok_or(RegularPolytopeError::ArithmeticOverflow)?;
    }

    let mut adjacency = nd_vec_with_capacity(vertex_count)?;
    for degree in degrees {
        adjacency.push(nd_vec_with_capacity(degree)?);
    }
    for &[first, second] in edges {
        adjacency[first].push(second);
        adjacency[second].push(first);
    }
    for neighbours in &mut adjacency {
        neighbours.sort_unstable();
    }
    Ok(adjacency)
}

fn contains_nd_edge(edges: &[[usize; 2]], first: usize, second: usize) -> bool {
    let edge = if first < second {
        [first, second]
    } else {
        [second, first]
    };
    edges.binary_search(&edge).is_ok()
}

fn euclidean_distance(first: &[f64], second: &[f64]) -> f64 {
    if first.len() != second.len() {
        return f64::NAN;
    }
    let mut squared_distance = 0.0;
    for (&first_coordinate, &second_coordinate) in first.iter().zip(second) {
        let difference = first_coordinate - second_coordinate;
        squared_distance = difference.mul_add(difference, squared_distance);
    }
    squared_distance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1.0e-9;

    fn has_edge(topology: &Polytope4DTopology, first: usize, second: usize) -> bool {
        let edge = if first < second {
            [first, second]
        } else {
            [second, first]
        };
        topology.edges.contains(&edge)
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn contains_all_indices(container: &[usize], required: &[usize]) -> bool {
        required.iter().all(|index| container.contains(index))
    }

    fn incident_cell_indices(cells: &[Vec<usize>], required: &[usize]) -> Vec<usize> {
        cells
            .iter()
            .enumerate()
            .filter_map(|(index, cell)| contains_all_indices(cell, required).then_some(index))
            .collect()
    }

    fn sorted_indices(indices: &[usize]) -> Vec<usize> {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted
    }

    fn polar_dual_vertex_for_each_primal_cell(
        primal: &Polytope4DTopology,
        dual: &Polytope4DTopology,
    ) -> Vec<usize> {
        let mut mapping = Vec::with_capacity(primal.cells.len());
        for primal_cell in &primal.cells {
            let (normal, offset) =
                supporting_hyperplane_for_tetrahedron(&primal.vertices, primal_cell)
                    .expect("cada tetraedro primal debe tener un plano soporte");
            let expected_vertex = normal.scaled(offset.recip());
            let matches = dual
                .vertices
                .iter()
                .enumerate()
                .filter_map(|(index, vertex)| {
                    (vertex.distance(expected_vertex) <= EPSILON).then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                matches.len(),
                1,
                "cada tetraedro 600-celda debe tener un unico vertice polar"
            );
            mapping.push(matches[0]);
        }

        let mut unique_mapping = mapping.clone();
        unique_mapping.sort_unstable();
        unique_mapping.dedup();
        assert_eq!(unique_mapping.len(), dual.vertices.len());
        mapping
    }

    fn primal_cell_for_dual_vertex(mapping: &[usize], dual_vertex: usize) -> usize {
        mapping
            .iter()
            .position(|&mapped_vertex| mapped_vertex == dual_vertex)
            .expect("cada vertice dual debe provenir de una celda primal")
    }

    #[test]
    fn all_regular_polychora_have_exact_f_vectors_uniform_edges_and_euler_zero() {
        let cases = [
            (RegularPolychoron::Pentachoron, (5, 10, 10, 5)),
            (RegularPolychoron::Tesseract, (16, 32, 24, 8)),
            (RegularPolychoron::SixteenCell, (8, 24, 32, 16)),
            (RegularPolychoron::TwentyFourCell, (24, 96, 96, 24)),
            (RegularPolychoron::OneTwentyCell, (600, 1200, 720, 120)),
            (RegularPolychoron::SixHundredCell, (120, 720, 1200, 600)),
        ];

        for (polychoron, (vertices, edges, faces, cells)) in cases {
            let topology = polychoron
                .topology()
                .expect("la topologia canonica debe construirse");
            assert_eq!(topology.vertices.len(), vertices);
            assert_eq!(topology.edges.len(), edges);
            assert_eq!(topology.faces.len(), faces);
            assert_eq!(topology.cells.len(), cells);
            assert_eq!(topology.euler_characteristic(), 0);

            let edge_length = topology.edge_length().expect("todo politopo tiene aristas");
            for [first, second] in &topology.edges {
                assert_close(
                    topology.vertices[*first].distance(topology.vertices[*second]),
                    edge_length,
                );
            }
        }
    }

    #[test]
    fn faces_are_closed_ordered_loops_and_cells_have_the_expected_membership_size() {
        let cases = [
            (RegularPolychoron::Pentachoron, 3, 4),
            (RegularPolychoron::Tesseract, 4, 8),
            (RegularPolychoron::SixteenCell, 3, 4),
            (RegularPolychoron::TwentyFourCell, 3, 6),
            (RegularPolychoron::OneTwentyCell, 5, 20),
            (RegularPolychoron::SixHundredCell, 3, 4),
        ];

        for (polychoron, face_size, cell_size) in cases {
            let topology = polychoron
                .topology()
                .expect("la topologia canonica debe construirse");
            for face in &topology.faces {
                assert_eq!(face.len(), face_size);
                for (position, &vertex) in face.iter().enumerate() {
                    assert!(vertex < topology.vertices.len());
                    assert_eq!(
                        face.iter()
                            .filter(|&&candidate| candidate == vertex)
                            .count(),
                        1
                    );
                    assert!(has_edge(
                        &topology,
                        vertex,
                        face[(position + 1) % face.len()]
                    ));
                }
            }
            for cell in &topology.cells {
                assert_eq!(cell.len(), cell_size);
                for &vertex in cell {
                    assert!(vertex < topology.vertices.len());
                    assert_eq!(
                        cell.iter()
                            .filter(|&&candidate| candidate == vertex)
                            .count(),
                        1
                    );
                }
            }
        }
    }

    #[test]
    fn all_six_rotation_planes_use_the_documented_order_and_preserve_norm() {
        let point = Point4D::new(1.0, 2.0, 3.0, 4.0);
        let cases = [
            (
                [std::f64::consts::FRAC_PI_2, 0.0, 0.0, 0.0, 0.0, 0.0],
                [-2.0, 1.0, 3.0, 4.0],
            ),
            (
                [0.0, std::f64::consts::FRAC_PI_2, 0.0, 0.0, 0.0, 0.0],
                [-3.0, 2.0, 1.0, 4.0],
            ),
            (
                [0.0, 0.0, std::f64::consts::FRAC_PI_2, 0.0, 0.0, 0.0],
                [-4.0, 2.0, 3.0, 1.0],
            ),
            (
                [0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2, 0.0, 0.0],
                [1.0, -3.0, 2.0, 4.0],
            ),
            (
                [0.0, 0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2, 0.0],
                [1.0, -4.0, 3.0, 2.0],
            ),
            (
                [0.0, 0.0, 0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2],
                [1.0, 2.0, -4.0, 3.0],
            ),
        ];

        for (angles, expected) in cases {
            let rotated = point
                .rotate_all_planes(angles)
                .expect("angulos finitos deben rotar el punto");
            assert_close(rotated.x, expected[0]);
            assert_close(rotated.y, expected[1]);
            assert_close(rotated.z, expected[2]);
            assert_close(rotated.w, expected[3]);
            assert_close(rotated.norm_squared(), point.norm_squared());
        }

        let rotated_in_contract_order = point
            .rotate_all_planes([std::f64::consts::FRAC_PI_2; 6])
            .expect("la composicion finita debe rotar el punto");
        assert_close(rotated_in_contract_order.x, -4.0);
        assert_close(rotated_in_contract_order.y, 3.0);
        assert_close(rotated_in_contract_order.z, -2.0);
        assert_close(rotated_in_contract_order.w, 1.0);
    }

    #[test]
    fn perspective_projection_rejects_nonfinite_and_near_plane_values() {
        assert!(Point4D::new(1.0, 2.0, 3.0, 10.0)
            .perspective_project(10.0)
            .is_none());
        assert!(Point4D::new(1.0, 2.0, 3.0, 10.0 - 1.0e-12)
            .perspective_project(10.0)
            .is_none());
        assert!(Point4D::new(f64::NAN, 2.0, 3.0, 0.0)
            .perspective_project(10.0)
            .is_none());
        assert!(Point4D::new(1.0, 2.0, 3.0, 0.0)
            .perspective_project(f64::INFINITY)
            .is_none());

        let projected = Point4D::new(1.0, 2.0, 3.0, 5.0)
            .perspective_project(10.0)
            .expect("un punto lejos del plano de proyeccion debe ser proyectable");
        assert_close(projected.x, 2.0);
        assert_close(projected.y, 4.0);
        assert_close(projected.z, 6.0);
    }

    #[test]
    fn six_hundred_cell_tetrahedra_are_supporting_facets() {
        let topology = RegularPolychoron::SixHundredCell
            .topology()
            .expect("la topologia canonica debe construirse");

        for cell in &topology.cells {
            let (normal, offset) = supporting_hyperplane_for_tetrahedron(&topology.vertices, cell)
                .expect("cada celda debe definir un hiperplano soporte");
            let tolerance = EPSILON * normal.norm();
            assert!(topology
                .vertices
                .iter()
                .all(|vertex| normal.dot(*vertex) <= offset + tolerance));
        }
    }

    #[test]
    fn every_six_hundred_cell_triangle_maps_to_a_unique_one_twenty_cell_edge() {
        let primal = RegularPolychoron::SixHundredCell
            .topology()
            .expect("la topologia primal debe construirse");
        let dual = RegularPolychoron::OneTwentyCell
            .topology()
            .expect("la topologia dual debe construirse");
        let cell_to_vertex = polar_dual_vertex_for_each_primal_cell(&primal, &dual);
        let mut mapped_edges = Vec::with_capacity(primal.faces.len());

        for primal_face in &primal.faces {
            let incident_cells = incident_cell_indices(&primal.cells, primal_face);
            assert_eq!(incident_cells.len(), 2);
            let mut dual_edge = [
                cell_to_vertex[incident_cells[0]],
                cell_to_vertex[incident_cells[1]],
            ];
            dual_edge.sort_unstable();
            assert_eq!(
                dual.edges.iter().filter(|&&edge| edge == dual_edge).count(),
                1,
                "cada triangulo primal debe corresponder a una arista dual unica"
            );
            mapped_edges.push(dual_edge);
        }

        mapped_edges.sort_unstable();
        mapped_edges.dedup();
        assert_eq!(mapped_edges.len(), primal.faces.len());
        assert_eq!(mapped_edges, dual.edges);
    }

    #[test]
    fn every_six_hundred_cell_edge_maps_to_a_unique_ordered_one_twenty_cell_pentagon() {
        let primal = RegularPolychoron::SixHundredCell
            .topology()
            .expect("la topologia primal debe construirse");
        let dual = RegularPolychoron::OneTwentyCell
            .topology()
            .expect("la topologia dual debe construirse");
        let cell_to_vertex = polar_dual_vertex_for_each_primal_cell(&primal, &dual);
        let mut mapped_face_sets = Vec::with_capacity(primal.edges.len());

        for &primal_edge in &primal.edges {
            let incident_cells = incident_cell_indices(&primal.cells, &primal_edge);
            assert_eq!(incident_cells.len(), 5);
            let dual_vertices = incident_cells
                .iter()
                .map(|&cell| cell_to_vertex[cell])
                .collect::<Vec<_>>();
            let expected_face = sorted_indices(&dual_vertices);
            let matching_faces = dual
                .faces
                .iter()
                .filter(|face| sorted_indices(face) == expected_face)
                .collect::<Vec<_>>();
            assert_eq!(
                matching_faces.len(),
                1,
                "cada arista primal debe corresponder a un pentagono dual unico"
            );
            let dual_face = matching_faces[0];
            assert_eq!(dual_face.len(), 5);

            for (position, &dual_vertex) in dual_face.iter().enumerate() {
                let next_dual_vertex = dual_face[(position + 1) % dual_face.len()];
                assert!(has_edge(&dual, dual_vertex, next_dual_vertex));

                let first_primal_cell = primal_cell_for_dual_vertex(&cell_to_vertex, dual_vertex);
                let second_primal_cell =
                    primal_cell_for_dual_vertex(&cell_to_vertex, next_dual_vertex);
                let shared_primal_faces = primal
                    .faces
                    .iter()
                    .filter(|face| {
                        face.contains(&primal_edge[0])
                            && face.contains(&primal_edge[1])
                            && contains_all_indices(&primal.cells[first_primal_cell], face)
                            && contains_all_indices(&primal.cells[second_primal_cell], face)
                    })
                    .count();
                assert_eq!(
                    shared_primal_faces, 1,
                    "aristas consecutivas del pentagono deben compartir una cara primal"
                );
            }
            mapped_face_sets.push(expected_face);
        }

        mapped_face_sets.sort_unstable();
        mapped_face_sets.dedup();
        assert_eq!(mapped_face_sets.len(), primal.edges.len());
        let mut dual_face_sets = dual
            .faces
            .iter()
            .map(|face| sorted_indices(face))
            .collect::<Vec<_>>();
        dual_face_sets.sort_unstable();
        dual_face_sets.dedup();
        assert_eq!(mapped_face_sets, dual_face_sets);
    }

    #[test]
    fn six_hundred_cell_vertex_incidence_maps_to_unique_one_twenty_cell_dodecahedra() {
        let primal = RegularPolychoron::SixHundredCell
            .topology()
            .expect("la topologia primal debe construirse");
        let dual = RegularPolychoron::OneTwentyCell
            .topology()
            .expect("la topologia dual debe construirse");
        let cell_to_vertex = polar_dual_vertex_for_each_primal_cell(&primal, &dual);
        let mut mapped_cell_sets = Vec::with_capacity(primal.vertices.len());

        for primal_vertex in 0..primal.vertices.len() {
            let incident_cells = incident_cell_indices(&primal.cells, &[primal_vertex]);
            assert_eq!(incident_cells.len(), 20);
            let dual_vertices = incident_cells
                .iter()
                .map(|&cell| cell_to_vertex[cell])
                .collect::<Vec<_>>();
            let expected_cell = sorted_indices(&dual_vertices);
            let matching_cells = dual
                .cells
                .iter()
                .filter(|cell| sorted_indices(cell) == expected_cell)
                .collect::<Vec<_>>();
            assert_eq!(
                matching_cells.len(),
                1,
                "cada vertice primal debe corresponder a una celda dual unica"
            );
            assert_eq!(matching_cells[0].len(), 20);
            mapped_cell_sets.push(expected_cell);
        }

        mapped_cell_sets.sort_unstable();
        mapped_cell_sets.dedup();
        assert_eq!(mapped_cell_sets.len(), primal.vertices.len());
        let mut dual_cell_sets = dual
            .cells
            .iter()
            .map(|cell| sorted_indices(cell))
            .collect::<Vec<_>>();
        dual_cell_sets.sort_unstable();
        dual_cell_sets.dedup();
        assert_eq!(mapped_cell_sets, dual_cell_sets);
    }

    fn assert_regular_nd_topology(
        family: RegularPolytopeFamily,
        dimension: usize,
        expected_vertices: usize,
        expected_edges: usize,
        expected_degree: usize,
    ) {
        let topology = family
            .topology(dimension)
            .expect("la topologia N-D canonica debe construirse");

        assert_eq!(topology.dimension, dimension);
        assert_eq!(topology.vertices.len(), expected_vertices);
        assert_eq!(topology.edges.len(), expected_edges);
        assert_eq!(topology.vertex_adjacency.len(), expected_vertices);
        assert!(topology.validate().is_ok());
        assert!(topology.vertices.iter().all(
            |vertex| vertex.len() == dimension && vertex.iter().all(|value| value.is_finite())
        ));
        assert!(topology.edges.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(topology
            .vertex_adjacency
            .iter()
            .all(|neighbours| neighbours.len() == expected_degree));

        let edge_length = topology
            .edge_length()
            .expect("toda familia N-D tiene aristas");
        for &[first, second] in &topology.edges {
            assert!(first < second && second < topology.vertices.len());
            assert_close(
                euclidean_distance(&topology.vertices[first], &topology.vertices[second]),
                edge_length,
            );
        }
    }

    #[test]
    fn regular_nd_families_have_the_exact_four_and_five_dimensional_formulas() {
        for dimension in [4, 5] {
            assert_regular_nd_topology(
                RegularPolytopeFamily::Simplex,
                dimension,
                dimension + 1,
                dimension * (dimension + 1) / 2,
                dimension,
            );
            assert_regular_nd_topology(
                RegularPolytopeFamily::Hypercube,
                dimension,
                1 << dimension,
                dimension * (1 << (dimension - 1)),
                dimension,
            );
            assert_regular_nd_topology(
                RegularPolytopeFamily::CrossPolytope,
                dimension,
                2 * dimension,
                2 * dimension * (dimension - 1),
                2 * (dimension - 1),
            );
        }
    }

    #[test]
    fn regular_nd_edges_follow_the_canonical_family_rules() {
        for dimension in [4, 5] {
            let simplex =
                RegularPolytopeTopology::try_new(RegularPolytopeFamily::Simplex, dimension)
                    .expect("el constructor comprobado debe construir el simplex");
            let mut complete_edges = Vec::new();
            for first in 0..simplex.vertices.len() {
                let first_successor = first
                    .checked_add(1)
                    .expect("los indices de prueba deben caber en usize");
                for second in first_successor..simplex.vertices.len() {
                    complete_edges.push([first, second]);
                }
            }
            assert_eq!(simplex.edges, complete_edges);

            let hypercube = RegularPolytopeFamily::Hypercube
                .topology(dimension)
                .expect("el hipercubo debe construirse");
            assert!(hypercube.vertices.iter().all(|vertex| {
                vertex
                    .iter()
                    .all(|coordinate| *coordinate == -1.0 || *coordinate == 1.0)
            }));
            for &[first, second] in &hypercube.edges {
                let changed_axes = hypercube.vertices[first]
                    .iter()
                    .zip(&hypercube.vertices[second])
                    .filter(|(left, right)| left != right)
                    .count();
                assert_eq!(changed_axes, 1);
            }

            let cross_polytope = RegularPolytopeFamily::CrossPolytope
                .topology(dimension)
                .expect("el politopo cruzado debe construirse");
            let axes = cross_polytope
                .vertices
                .iter()
                .map(|vertex| {
                    let nonzero_axes = vertex
                        .iter()
                        .enumerate()
                        .filter_map(|(axis, coordinate)| (*coordinate != 0.0).then_some(axis))
                        .collect::<Vec<_>>();
                    assert_eq!(nonzero_axes.len(), 1);
                    assert_eq!(vertex[nonzero_axes[0]].abs(), 1.0);
                    nonzero_axes[0]
                })
                .collect::<Vec<_>>();
            for &[first, second] in &cross_polytope.edges {
                assert_ne!(axes[first], axes[second]);
            }
        }
    }

    #[test]
    fn regular_nd_topology_rejects_dimensions_and_budgets_before_allocation() {
        assert!(matches!(
            RegularPolytopeFamily::Simplex.topology(2),
            Err(RegularPolytopeError::InvalidDimension { .. })
        ));
        assert!(matches!(
            RegularPolytopeFamily::Hypercube.topology(MAX_REGULAR_POLYTOPE_DIMENSION + 1),
            Err(RegularPolytopeError::InvalidDimension { .. })
        ));

        let budget = RegularPolytopeBudget {
            max_vertices: 15,
            ..RegularPolytopeBudget::default()
        };
        assert!(matches!(
            RegularPolytopeFamily::Hypercube.topology_with_budget(4, budget),
            Err(RegularPolytopeError::ResourceLimitExceeded { .. })
        ));
    }

    #[test]
    fn nd_projection_and_plane_rotation_are_safe_deterministic_and_dimension_complete() {
        let projection = NdPerspectiveProjection::new(10.0)
            .expect("una distancia de perspectiva positiva debe ser valida");
        let coordinates = [1.0, 2.0, 3.0, 4.0, 5.0];
        let projected = projection
            .project(&coordinates)
            .expect("la proyeccion N-D finita debe tener exito");
        let repeated = projection
            .project(&coordinates)
            .expect("la proyeccion debe ser determinista");
        assert_close(projected.x, 10.0);
        assert_close(projected.y, 20.0);
        assert_close(projected.z, 30.0);
        assert_close(repeated.x, projected.x);
        assert_close(repeated.y, projected.y);
        assert_close(repeated.z, projected.z);

        for axis in 0..coordinates.len() {
            let mut changed = coordinates;
            changed[axis] += 0.25;
            let changed_projection = projection
                .project(&changed)
                .expect("cada coordenada finita debe seguir siendo proyectable");
            assert!(
                (changed_projection.x - projected.x).abs() > EPSILON
                    || (changed_projection.y - projected.y).abs() > EPSILON
                    || (changed_projection.z - projected.z).abs() > EPSILON,
                "la coordenada {axis} debe afectar la proyeccion"
            );
        }

        assert!(matches!(
            projection.project(&[1.0, 2.0, 3.0, 10.0]),
            Err(RegularPolytopeError::PerspectiveNearPlane { .. })
        ));
        assert!(matches!(
            projection.project(&[1.0, 2.0]),
            Err(RegularPolytopeError::InvalidCoordinateDimension { .. })
        ));

        let mut rotated = coordinates;
        rotate_nd_in_plane(&mut rotated, 3, 4, std::f64::consts::FRAC_PI_2)
            .expect("un plano de coordenadas distinto debe poder rotarse");
        assert_close(rotated[3], -5.0);
        assert_close(rotated[4], 4.0);
        let before_invalid_rotation = rotated;
        assert!(matches!(
            rotate_nd_in_plane(&mut rotated, 2, 2, 0.5),
            Err(RegularPolytopeError::InvalidRotationPlane { .. })
        ));
        assert_eq!(rotated, before_invalid_rotation);

        let topology = RegularPolytopeFamily::Hypercube
            .topology(5)
            .expect("el hipercubo 5D debe construirse");
        let before_projection = topology.clone();
        let projected_vertices = projection
            .project_vertices(&topology.vertices)
            .expect("los vertices canonicos deben proyectarse");
        assert_eq!(projected_vertices.len(), topology.vertices.len());
        assert_eq!(topology, before_projection);
    }

    #[test]
    fn nd_bulk_projection_rejects_excess_vertices_before_allocating_output() {
        let projection = NdPerspectiveProjection::new(10.0)
            .expect("una distancia de perspectiva positiva debe ser valida");
        let normal_vertices = vec![vec![1.0, 2.0, 3.0]; MAX_ND_PROJECTION_VERTICES];
        assert!(projection
            .project_vertices(&[])
            .expect("una coleccion vacia debe seguir siendo valida")
            .is_empty());
        assert_eq!(
            projection
                .project_vertices(&normal_vertices)
                .expect("el limite publico debe conservar entradas normales")
                .len(),
            MAX_ND_PROJECTION_VERTICES
        );

        let oversized_count = MAX_ND_PROJECTION_VERTICES
            .checked_add(1)
            .expect("el limite de prueba debe caber en usize");
        let oversized_vertices = vec![vec![1.0, 2.0, 3.0]; oversized_count];
        assert!(matches!(
            projection.project_vertices(&oversized_vertices),
            Err(RegularPolytopeError::ProjectionVertexLimitExceeded {
                requested,
                maximum,
            }) if requested == oversized_count && maximum == MAX_ND_PROJECTION_VERTICES
        ));
    }

    #[test]
    fn projection_plans_use_closed_form_radius_bounds_without_materializing_topology() {
        fn assert_copy<T: Copy>() {}

        // El plan solo contiene escalares: estas consultas no reciben ni crean una topologia.
        assert_copy::<RegularPolytopeProjectionPlan>();

        let golden_ratio = (1.0 + 5.0_f64.sqrt()) * 0.5;
        let one_twenty_cell_radius = (1.0 - 3.0 / (8.0 * golden_ratio * golden_ratio))
            .sqrt()
            .recip();
        for (kind, expected_radius) in [
            (RegularPolychoron::Pentachoron, 4.0 / 5.0_f64.sqrt()),
            (RegularPolychoron::Tesseract, 2.0),
            (RegularPolychoron::SixteenCell, 1.0),
            (RegularPolychoron::TwentyFourCell, 2.0_f64.sqrt()),
            (RegularPolychoron::OneTwentyCell, one_twenty_cell_radius),
            (RegularPolychoron::SixHundredCell, 1.0),
        ] {
            assert_close(kind.canonical_radius_bound(), expected_radius);
            let plan = kind
                .projection_plan(1.0)
                .expect("un selector 4D canonico debe poder planificarse");
            assert_eq!(plan.dimension(), 4);
            assert!(plan.source_radius() >= expected_radius);
            assert_eq!(plan.distance(), 6.0 * plan.source_radius().max(1.0));
            let unrounded_bound =
                plan.source_radius() * plan.distance() / (plan.distance() - plan.source_radius());
            assert!(plan.projected_coordinate_bound().is_finite());
            assert!(plan.projected_coordinate_bound() >= unrounded_bound);
        }

        for dimension in MIN_REGULAR_POLYTOPE_DIMENSION..=MAX_REGULAR_POLYTOPE_DIMENSION {
            for (family, expected_radius) in [
                (
                    RegularPolytopeFamily::Simplex,
                    (dimension as f64 / (dimension + 1) as f64).sqrt(),
                ),
                (RegularPolytopeFamily::Hypercube, (dimension as f64).sqrt()),
                (RegularPolytopeFamily::CrossPolytope, 1.0),
            ] {
                assert_close(
                    family
                        .canonical_radius_bound(dimension)
                        .expect("la dimension publicada debe ser valida"),
                    expected_radius,
                );
                let plan = family
                    .projection_plan(dimension, 1.0)
                    .expect("la familia publicada debe poder planificarse");
                assert_eq!(plan.dimension(), dimension);
                assert!(plan.source_radius() >= expected_radius);
                assert_eq!(
                    plan.distance(),
                    (dimension + 2) as f64 * plan.source_radius().max(1.0)
                );
                let unrounded_bound = plan.source_radius() * plan.distance()
                    / (plan.distance() - plan.source_radius());
                assert!(plan.projected_coordinate_bound().is_finite());
                assert!(plan.projected_coordinate_bound() >= unrounded_bound);
            }
        }
    }

    #[test]
    fn projection_plan_reports_invalid_scales_and_coordinate_limits() {
        assert!(matches!(
            RegularPolychoron::Tesseract.projection_plan(f64::NAN),
            Err(RegularPolytopeProjectionError::NonFiniteScale)
        ));
        assert!(matches!(
            RegularPolychoron::Tesseract.projection_plan(0.0),
            Err(RegularPolytopeProjectionError::NonPositiveScale)
        ));
        assert!(matches!(
            RegularPolytopeFamily::Hypercube.canonical_radius_bound(2),
            Err(RegularPolytopeProjectionError::InvalidDimension { dimension: 2, .. })
        ));

        let huge_plan = RegularPolychoron::Tesseract
            .projection_plan(1.0e13)
            .expect("la escala finita debe producir un plan finito");
        assert!(matches!(
            huge_plan.ensure_within_coordinate_limit(crate::MAX_WORLD_COORDINATE),
            Err(RegularPolytopeProjectionError::CoordinateLimitExceeded { .. })
        ));
    }

    #[test]
    fn projection_plan_distinguishes_near_and_over_world_coordinate_boundaries() {
        let radius = RegularPolychoron::Tesseract.canonical_radius_bound();
        let threshold_scale = crate::MAX_WORLD_COORDINATE * 5.0 / (6.0 * radius);
        let near_limit = RegularPolychoron::Tesseract
            .projection_plan(threshold_scale * 0.999_999)
            .expect("la escala cercana debe ser finita");
        let over_limit = RegularPolychoron::Tesseract
            .projection_plan(threshold_scale * 1.000_001)
            .expect("la escala apenas superior debe ser finita");

        assert!(near_limit
            .ensure_within_coordinate_limit(crate::MAX_WORLD_COORDINATE)
            .is_ok());
        assert!(matches!(
            over_limit.ensure_within_coordinate_limit(crate::MAX_WORLD_COORDINATE),
            Err(RegularPolytopeProjectionError::CoordinateLimitExceeded { .. })
        ));
    }

    #[test]
    fn one_twenty_cell_projection_plan_distinguishes_threshold_sides_without_topology() {
        let radius = RegularPolychoron::OneTwentyCell.canonical_radius_bound();
        let threshold_scale = crate::MAX_WORLD_COORDINATE * 5.0 / (6.0 * radius);
        let below_limit = RegularPolychoron::OneTwentyCell
            .projection_plan(threshold_scale * 0.999_999)
            .expect("la escala finita cercana debe poder planificarse");
        let above_limit = RegularPolychoron::OneTwentyCell
            .projection_plan(threshold_scale * 1.000_001)
            .expect("la escala finita superior debe poder planificarse");

        assert_eq!(below_limit.dimension(), 4);
        assert_eq!(
            below_limit.distance(),
            6.0 * below_limit.source_radius().max(1.0)
        );
        assert!(below_limit
            .ensure_within_coordinate_limit(crate::MAX_WORLD_COORDINATE)
            .is_ok());
        assert!(matches!(
            above_limit.ensure_within_coordinate_limit(crate::MAX_WORLD_COORDINATE),
            Err(RegularPolytopeProjectionError::CoordinateLimitExceeded { .. })
        ));
    }

    #[test]
    fn canonical_radius_bounds_dominate_independently_materialized_vertex_norms() {
        for kind in [
            RegularPolychoron::Pentachoron,
            RegularPolychoron::Tesseract,
            RegularPolychoron::SixteenCell,
            RegularPolychoron::TwentyFourCell,
            RegularPolychoron::OneTwentyCell,
            RegularPolychoron::SixHundredCell,
        ] {
            let topology = kind
                .topology()
                .expect("cada selector 4D canonico debe materializar su topologia");
            let maximum_vertex_norm = topology
                .vertices
                .iter()
                .map(|vertex| vertex.norm())
                .fold(0.0_f64, f64::max);

            assert!(
                kind.canonical_radius_bound() >= maximum_vertex_norm,
                "{kind:?} no puede subestimar la norma de sus vertices canonicos"
            );
        }

        for dimension in MIN_REGULAR_POLYTOPE_DIMENSION..=MAX_REGULAR_POLYTOPE_DIMENSION {
            for family in [
                RegularPolytopeFamily::Simplex,
                RegularPolytopeFamily::Hypercube,
                RegularPolytopeFamily::CrossPolytope,
            ] {
                let topology = family
                    .topology(dimension)
                    .expect("cada familia publicada debe materializar su topologia");
                let maximum_vertex_norm = topology
                    .vertices
                    .iter()
                    .map(|vertex| {
                        vertex
                            .iter()
                            .map(|coordinate| coordinate * coordinate)
                            .sum::<f64>()
                    })
                    .map(f64::sqrt)
                    .fold(0.0_f64, f64::max);
                let bound = family
                    .canonical_radius_bound(dimension)
                    .expect("la dimension publicada debe admitir una cota radial");

                assert!(
                    bound >= maximum_vertex_norm,
                    "{family:?} R^{dimension} no puede subestimar la norma de sus vertices canonicos: cota {bound}, vertice {maximum_vertex_norm}"
                );
            }
        }
    }
}

// ── G-B: mallas 3D acotadas (Net, lathe/extrusión, platónicos, implícita, picking) ──
//
// Frente G-B vs GeoGebra 3D (`Net`, `Polyhedron`, `Surface of Revolution`,
// `IntersectPath`/`Plane`, `ImplicitSurface3D`). Todo constructor valida
// entradas finitas y presupuestos ANTES de reservar memoria (`checked_mul` y
// cotas duras); fuera de cota devuelve `Err` honesto, nunca pánico ni
// `unwrap`/`expect` en producción.

/// Tolerancia geométrica por defecto para picking rayo-malla y secciones plano-poliedro.
pub const GB_GEOM_EPS: f64 = 1.0e-9;

/// Vértices máximos de una malla G-B (amable con CPU/GPU, muy por debajo de 250k celdas).
pub const GB_MAX_MESH_VERTICES: usize = 65_536;

/// Triángulos máximos de una malla G-B (alineado con `MAX_CELLS` 250k del domain coloring).
pub const GB_MAX_MESH_TRIANGLES: usize = 250_000;

/// Caras máximas de un net desplegable (cubo/prisma/pirámide quedan muy por debajo).
pub const GB_MAX_NET_FACES: usize = 256;

/// Puntos máximos del perfil de revolución o extrusión.
pub const GB_MAX_PROFILE_POINTS: usize = 1_024;

/// Segmentos radiales máximos del lathe (superficie de revolución).
pub const GB_MAX_LATHE_SEGMENTS: usize = 256;

/// Lados máximos de la base de un prisma con net (`64 * altura` cabe en presupuestos).
pub const GB_MAX_PRISM_SIDES: usize = 64;

/// Celdas por eje máximas del marching (`32³ = 32 768` celdas < 250k).
pub const GB_MAX_MARCHING_CELLS_PER_AXIS: usize = 32;

/// Razón áurea `(1 + √5) / 2` para icosaedro/dodecaedro exactos.
pub const GB_GOLDEN_RATIO: f64 = 1.618_033_988_749_895;

/// Error honesto de construcción de mallas G-B.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeshError {
    /// Alguna coordenada de entrada no es finita.
    NonFiniteInput,
    /// La arista/longitud pedida no es finita y estrictamente positiva.
    NonPositiveEdge { value: f64 },
    /// Muy pocos puntos para el perfil/polígono pedido.
    TooFewPoints { found: usize, minimum: usize },
    /// Demasiados puntos para el perfil/polígono pedido.
    TooManyPoints { found: usize, maximum: usize },
    /// Demasiados segmentos radiales/lados para el presupuesto.
    TooManySegments { found: usize, maximum: usize },
    /// La malla resultante excedería el presupuesto de vértices o triángulos.
    MeshBudgetExceeded { what: &'static str, limit: usize },
    /// Geometría degenerada (área nula, perfil sobre el eje, ápice imposible...).
    DegenerateGeometry { reason: &'static str },
    /// El polígono de extrusión no es estrictamente convexo (subconjunto honesto).
    NonConvexPolygon,
    /// El campo implícito no está definido en un nodo de la rejilla (fail-closed).
    FieldUndefined { at: [f64; 3] },
}

impl fmt::Display for MeshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteInput => formatter.write_str("la entrada 3D debe ser finita"),
            Self::NonPositiveEdge { value } => {
                write!(
                    formatter,
                    "la arista debe ser finita y positiva, era {value}"
                )
            }
            Self::TooFewPoints { found, minimum } => write!(
                formatter,
                "se necesitan al menos {minimum} puntos, llegaron {found}"
            ),
            Self::TooManyPoints { found, maximum } => {
                write!(formatter, "como máximo {maximum} puntos, llegaron {found}")
            }
            Self::TooManySegments { found, maximum } => write!(
                formatter,
                "como máximo {maximum} segmentos, llegaron {found}"
            ),
            Self::MeshBudgetExceeded { what, limit } => {
                write!(formatter, "{what} excede el presupuesto de {limit}")
            }
            Self::DegenerateGeometry { reason } => {
                write!(formatter, "geometría degenerada: {reason}")
            }
            Self::NonConvexPolygon => {
                formatter.write_str("la extrusión solo admite polígonos estrictamente convexos")
            }
            Self::FieldUndefined { at } => write!(
                formatter,
                "el campo implícito no está definido en ({}, {}, {})",
                at[0], at[1], at[2]
            ),
        }
    }
}

impl std::error::Error for MeshError {}

/// Malla triangular 3D acotada con índices validados.
#[derive(Debug, Clone, PartialEq)]
pub struct TriangleMesh3D {
    vertices: Vec<Point3D>,
    triangles: Vec<[usize; 3]>,
}

impl TriangleMesh3D {
    /// Construye una malla validando presupuestos, finitud e índices.
    pub fn new(vertices: Vec<Point3D>, triangles: Vec<[usize; 3]>) -> Result<Self, MeshError> {
        if vertices.len() > GB_MAX_MESH_VERTICES {
            return Err(MeshError::MeshBudgetExceeded {
                what: "vértices de malla",
                limit: GB_MAX_MESH_VERTICES,
            });
        }
        if triangles.len() > GB_MAX_MESH_TRIANGLES {
            return Err(MeshError::MeshBudgetExceeded {
                what: "triángulos de malla",
                limit: GB_MAX_MESH_TRIANGLES,
            });
        }
        if vertices.iter().any(|point| !point.is_finite()) {
            return Err(MeshError::NonFiniteInput);
        }
        if triangles
            .iter()
            .any(|triangle| triangle.iter().any(|&index| index >= vertices.len()))
        {
            return Err(MeshError::DegenerateGeometry {
                reason: "triángulo con índice fuera de rango",
            });
        }
        Ok(Self {
            vertices,
            triangles,
        })
    }

    /// Vértices de la malla.
    pub fn vertices(&self) -> &[Point3D] {
        &self.vertices
    }

    /// Triángulos como índices sobre [`Self::vertices`].
    pub fn triangles(&self) -> &[[usize; 3]] {
        &self.triangles
    }

    /// Número de vértices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Número de triángulos.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Área total por suma de triángulos; `None` si no es finita.
    pub fn surface_area(&self) -> Option<f64> {
        let mut total = 0.0_f64;
        for triangle in &self.triangles {
            let a = self.vertices.get(triangle[0])?.to_dvec3();
            let b = self.vertices.get(triangle[1])?.to_dvec3();
            let c = self.vertices.get(triangle[2])?.to_dvec3();
            let area = (b - a).cross(c - a).length() * 0.5;
            total += area;
        }
        total.is_finite().then_some(total)
    }

    /// Caja envolvente; `None` si la malla está vacía.
    pub fn aabb(&self) -> Option<crate::types3d::Aabb3D> {
        crate::types3d::Aabb3D::from_points(self.vertices.iter().copied())
    }

    /// Rango `(mín, máx)` de longitudes de arista únicas; `None` si no hay aristas finitas.
    pub fn edge_length_range(&self) -> Option<(f64, f64)> {
        let mut edges = std::collections::BTreeSet::new();
        for triangle in &self.triangles {
            for side in 0..3 {
                let first = triangle[side];
                let second = triangle[(side + 1) % 3];
                edges.insert((first.min(second), first.max(second)));
            }
        }
        let mut minimum = f64::INFINITY;
        let mut maximum = 0.0_f64;
        for (first, second) in edges {
            let a = self.vertices.get(first)?;
            let b = self.vertices.get(second)?;
            let length = a.distance(b);
            if !length.is_finite() || length <= 0.0 {
                return None;
            }
            minimum = minimum.min(length);
            maximum = maximum.max(length);
        }
        minimum.is_finite().then_some((minimum, maximum))
    }
}

/// Sólido platónico 3D con vértices gold-ratio exactos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatonicSolid {
    /// 12 vértices `(0, ±1, ±φ)` y permutaciones; 20 caras triangulares.
    Icosahedron,
    /// 20 vértices `(±1, ±1, ±1)`, `(0, ±φ, ±1/φ)` y permutaciones cíclicas;
    /// 12 pentágonos. La orientación es la dual del [`PlatonicSolid::Icosahedron`].
    Dodecahedron,
}

fn gb_positive_edge(edge_length: f64) -> Result<f64, MeshError> {
    if !edge_length.is_finite() || edge_length <= 0.0 {
        return Err(MeshError::NonPositiveEdge { value: edge_length });
    }
    Ok(edge_length)
}

/// Vértices canónicos del icosaedro unitario (`a = 1`, arista canónica `2`).
fn icosahedron_unit_vertices() -> [Point3D; 12] {
    let phi = GB_GOLDEN_RATIO;
    [
        Point3D::new(-1.0, phi, 0.0),
        Point3D::new(1.0, phi, 0.0),
        Point3D::new(-1.0, -phi, 0.0),
        Point3D::new(1.0, -phi, 0.0),
        Point3D::new(0.0, -1.0, phi),
        Point3D::new(0.0, 1.0, phi),
        Point3D::new(0.0, -1.0, -phi),
        Point3D::new(0.0, 1.0, -phi),
        Point3D::new(phi, 0.0, -1.0),
        Point3D::new(phi, 0.0, 1.0),
        Point3D::new(-phi, 0.0, -1.0),
        Point3D::new(-phi, 0.0, 1.0),
    ]
}

/// Caras canónicas del icosaedro sobre [`icosahedron_unit_vertices`].
const ICOSAHEDRON_FACES: [[usize; 3]; 20] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

/// Vértices canónicos del dodecaedro unitario (`a = 1`, arista canónica `2/φ`).
///
/// Orientación dual del icosaedro de [`icosahedron_unit_vertices`]: cada
/// vértice del icosaedro apunta al centro de un pentágono.
fn dodecahedron_unit_vertices() -> [Point3D; 20] {
    let phi = GB_GOLDEN_RATIO;
    let inv_phi = 1.0 / phi;
    [
        Point3D::new(1.0, 1.0, 1.0),
        Point3D::new(1.0, 1.0, -1.0),
        Point3D::new(1.0, -1.0, 1.0),
        Point3D::new(1.0, -1.0, -1.0),
        Point3D::new(-1.0, 1.0, 1.0),
        Point3D::new(-1.0, 1.0, -1.0),
        Point3D::new(-1.0, -1.0, 1.0),
        Point3D::new(-1.0, -1.0, -1.0),
        Point3D::new(0.0, phi, inv_phi),
        Point3D::new(0.0, phi, -inv_phi),
        Point3D::new(0.0, -phi, inv_phi),
        Point3D::new(0.0, -phi, -inv_phi),
        Point3D::new(inv_phi, 0.0, phi),
        Point3D::new(inv_phi, 0.0, -phi),
        Point3D::new(-inv_phi, 0.0, phi),
        Point3D::new(-inv_phi, 0.0, -phi),
        Point3D::new(phi, inv_phi, 0.0),
        Point3D::new(phi, -inv_phi, 0.0),
        Point3D::new(-phi, inv_phi, 0.0),
        Point3D::new(-phi, -inv_phi, 0.0),
    ]
}

/// Ordena los 5 vértices de una cara pentagonal alrededor de su eje.
fn order_pentagon_around_axis(
    axis: Point3D,
    candidates: &[(usize, Point3D)],
) -> Option<[usize; 5]> {
    if candidates.len() != 5 {
        return None;
    }
    let axis_vector = axis.to_dvec3();
    if axis_vector.length_squared() <= 1.0e-24 {
        return None;
    }
    let axis_norm = axis_vector.normalize_or_zero();
    let helper = if axis_norm.x.abs() < 0.9 {
        glam::DVec3::X
    } else {
        glam::DVec3::Y
    };
    let tangent = axis_norm.cross(helper).normalize_or_zero();
    let bitangent = axis_norm.cross(tangent);
    if tangent.length_squared() <= 0.5 || bitangent.length_squared() <= 0.5 {
        return None;
    }
    let centroid = candidates
        .iter()
        .fold(glam::DVec3::ZERO, |sum, (_, point)| sum + point.to_dvec3())
        / candidates.len() as f64;
    let mut by_angle: Vec<(f64, usize)> = Vec::with_capacity(5);
    for (index, point) in candidates {
        let radial = point.to_dvec3() - centroid;
        let angle = radial.dot(bitangent).atan2(radial.dot(tangent));
        if !angle.is_finite() {
            return None;
        }
        by_angle.push((angle, *index));
    }
    by_angle.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut face = [0_usize; 5];
    for (slot, (_, index)) in by_angle.iter().enumerate() {
        face[slot] = *index;
    }
    Some(face)
}

/// Pentágonos ordenados del dodecaedro canónico (índices sobre los 20
/// vértices unitarios, un pentágono por vértice del icosaedro dual).
///
/// Cada vértice del icosaedro apunta al centro de un pentágono del dodecaedro
/// concéntrico con la misma orientación; los 5 vértices más alineados con
/// cada eje forman el pentágono. Honesto: `None` si la dualidad no cierra.
/// El render y los tests lo usan para distinguir aristas reales (borde del
/// pentágono) de diagonales del abanico de triangulación.
pub fn dodecahedron_pentagons() -> Option<[[usize; 5]; 12]> {
    let icosa = icosahedron_unit_vertices();
    let dodeca = dodecahedron_unit_vertices();
    let mut faces = [[0_usize; 5]; 12];
    for (face_slot, axis) in icosa.iter().enumerate() {
        let mut dots: Vec<(f64, usize)> = Vec::with_capacity(dodeca.len());
        for (index, vertex) in dodeca.iter().enumerate() {
            let dot = axis.to_dvec3().normalize_or_zero().dot(vertex.to_dvec3());
            if !dot.is_finite() {
                return None;
            }
            dots.push((dot, index));
        }
        dots.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top: Vec<(usize, Point3D)> = dots
            .iter()
            .take(5)
            .map(|(_, index)| (*index, dodeca[*index]))
            .collect();
        // La 5ª y 6ª alineación deben separarse: si empatan, la dualidad no cierra.
        let fifth = dots.get(4)?.0;
        let sixth = dots.get(5)?.0;
        if (fifth - sixth).abs() <= 1.0e-12 {
            return None;
        }
        faces[face_slot] = order_pentagon_around_axis(*axis, &top)?;
    }
    Some(faces)
}

/// Malla exacta del sólido platónico con la arista pedida (vértices gold-ratio).
///
/// El dodecaedro triangula cada pentágono en abanico (3 triángulos por cara).
pub fn platonic_mesh(solid: PlatonicSolid, edge_length: f64) -> Result<TriangleMesh3D, MeshError> {
    gb_positive_edge(edge_length)?;
    match solid {
        PlatonicSolid::Icosahedron => {
            // Arista canónica 2 (p. ej. (-1,φ,0)-(1,φ,0)).
            let scale = edge_length / 2.0;
            if !scale.is_finite() {
                return Err(MeshError::NonPositiveEdge { value: edge_length });
            }
            let vertices: Vec<Point3D> = icosahedron_unit_vertices()
                .iter()
                .map(|point| Point3D::new(point.x * scale, point.y * scale, point.z * scale))
                .collect();
            TriangleMesh3D::new(vertices, ICOSAHEDRON_FACES.to_vec())
        }
        PlatonicSolid::Dodecahedron => {
            // Arista canónica 2/φ (p. ej. (1,1,1)-(0,1/φ,φ)).
            let scale = edge_length * GB_GOLDEN_RATIO / 2.0;
            if !scale.is_finite() {
                return Err(MeshError::NonPositiveEdge { value: edge_length });
            }
            let vertices: Vec<Point3D> = dodecahedron_unit_vertices()
                .iter()
                .map(|point| Point3D::new(point.x * scale, point.y * scale, point.z * scale))
                .collect();
            let faces = dodecahedron_pentagons().ok_or(MeshError::DegenerateGeometry {
                reason: "la dualidad icosaedro-dodecaedro no cerró",
            })?;
            let mut triangles = Vec::with_capacity(36);
            for face in faces {
                for fan in 1..4 {
                    triangles.push([face[0], face[fan], face[fan + 1]]);
                }
            }
            TriangleMesh3D::new(vertices, triangles)
        }
    }
}

// ── Nets desplegables ──

/// Panel 2D de un net: cara (`is_tab = false`) o solapa (`is_tab = true`).
#[derive(Debug, Clone, PartialEq)]
pub struct NetPanel {
    /// Polígono 2D en orden (rectángulo, triángulo, n-gono o trapecio de solapa).
    pub points: Vec<[f64; 2]>,
    /// Índice de la cara 3D de origen (las solapas comparten el de su cara).
    pub face_index: usize,
    /// `true` si es solapa de pegado, `false` si es cara del sólido.
    pub is_tab: bool,
}

/// Net desplegable 2D con caras y solapas.
#[derive(Debug, Clone, PartialEq)]
pub struct PolyhedronNet {
    panels: Vec<NetPanel>,
    edge_length: f64,
}

impl PolyhedronNet {
    fn new(panels: Vec<NetPanel>, edge_length: f64) -> Result<Self, MeshError> {
        gb_positive_edge(edge_length)?;
        if panels.len() > GB_MAX_NET_FACES {
            return Err(MeshError::MeshBudgetExceeded {
                what: "paneles de net",
                limit: GB_MAX_NET_FACES,
            });
        }
        if panels.iter().any(|panel| {
            panel.points.len() < 3
                || panel
                    .points
                    .iter()
                    .any(|point| !point[0].is_finite() || !point[1].is_finite())
        }) {
            return Err(MeshError::DegenerateGeometry {
                reason: "panel de net no finito o con menos de 3 puntos",
            });
        }
        Ok(Self {
            panels,
            edge_length,
        })
    }

    /// Paneles del net (caras + solapas).
    pub fn panels(&self) -> &[NetPanel] {
        &self.panels
    }

    /// Número de caras (sin solapas).
    pub fn face_count(&self) -> usize {
        self.panels.iter().filter(|panel| !panel.is_tab).count()
    }

    /// Número de solapas.
    pub fn tab_count(&self) -> usize {
        self.panels.iter().filter(|panel| panel.is_tab).count()
    }

    /// Arista del sólido de origen.
    pub fn edge_length(&self) -> f64 {
        self.edge_length
    }

    /// Área firmada de un polígono 2D (positiva si es CCW).
    pub fn polygon_area(points: &[[f64; 2]]) -> f64 {
        let mut twice = 0.0_f64;
        for side in 0..points.len() {
            let a = points[side];
            let b = points[(side + 1) % points.len()];
            twice += a[0] * b[1] - b[0] * a[1];
        }
        twice * 0.5
    }

    /// Área total de las caras (sin solapas); `None` si no es finita.
    pub fn face_area(&self) -> Option<f64> {
        let mut total = 0.0_f64;
        for panel in &self.panels {
            if !panel.is_tab {
                total += Self::polygon_area(&panel.points).abs();
            }
        }
        total.is_finite().then_some(total)
    }

    /// Área total de las solapas; `None` si no es finita.
    pub fn tab_area(&self) -> Option<f64> {
        let mut total = 0.0_f64;
        for panel in &self.panels {
            if panel.is_tab {
                total += Self::polygon_area(&panel.points).abs();
            }
        }
        total.is_finite().then_some(total)
    }
}

fn gb_rect_panel(x: f64, y: f64, width: f64, height: f64, face_index: usize) -> NetPanel {
    NetPanel {
        points: vec![
            [x, y],
            [x + width, y],
            [x + width, y + height],
            [x, y + height],
        ],
        face_index,
        is_tab: false,
    }
}

/// Trapecio de solapa sobre la arista `p -> q`, hacia afuera según `outward` unitario.
fn gb_tab_panel(
    p: [f64; 2],
    q: [f64; 2],
    outward: [f64; 2],
    height: f64,
    face_index: usize,
) -> NetPanel {
    let inset = 0.2_f64;
    let inner_p = [p[0] + (q[0] - p[0]) * inset, p[1] + (q[1] - p[1]) * inset];
    let inner_q = [q[0] - (q[0] - p[0]) * inset, q[1] - (q[1] - p[1]) * inset];
    NetPanel {
        points: vec![
            p,
            q,
            [
                inner_q[0] + outward[0] * height,
                inner_q[1] + outward[1] * height,
            ],
            [
                inner_p[0] + outward[0] * height,
                inner_p[1] + outward[1] * height,
            ],
        ],
        face_index,
        is_tab: true,
    }
}

/// Net del cubo en cruz (4 caras en fila + tapa arriba/abajo) con 6 solapas.
pub fn cube_net(edge_length: f64) -> Result<PolyhedronNet, MeshError> {
    gb_positive_edge(edge_length)?;
    let a = edge_length;
    let tab_h = a * 0.25;
    let mut panels = Vec::with_capacity(12);
    // Fila central: izquierda(0), frente(1), derecha(2), atrás(3).
    for (face, origin_x) in [0.0, a, 2.0 * a, 3.0 * a].iter().enumerate() {
        panels.push(gb_rect_panel(*origin_x, 0.0, a, a, face));
    }
    // Tapa (4) arriba del frente, base (5) debajo del frente.
    panels.push(gb_rect_panel(a, a, a, a, 4));
    panels.push(gb_rect_panel(a, -a, a, a, 5));
    // 6 solapas en bordes libres: exterior izq/der de la fila, 2 en tapa, 2 en base.
    panels.push(gb_tab_panel([0.0, 0.0], [0.0, a], [-1.0, 0.0], tab_h, 0));
    panels.push(gb_tab_panel(
        [4.0 * a, a],
        [4.0 * a, 0.0],
        [1.0, 0.0],
        tab_h,
        3,
    ));
    panels.push(gb_tab_panel(
        [a, 2.0 * a],
        [2.0 * a, 2.0 * a],
        [0.0, 1.0],
        tab_h,
        4,
    ));
    panels.push(gb_tab_panel([2.0 * a, a], [a, a], [0.0, -1.0], tab_h, 4));
    panels.push(gb_tab_panel([2.0 * a, -a], [a, -a], [0.0, -1.0], tab_h, 5));
    panels.push(gb_tab_panel([a, 0.0], [a, -a], [-1.0, 0.0], tab_h, 5));
    PolyhedronNet::new(panels, edge_length)
}

/// Net del prisma recto de base regular (`sides` 3..=64): tira lateral + 2 bases, con solapas.
pub fn prism_net(sides: usize, base_edge: f64, height: f64) -> Result<PolyhedronNet, MeshError> {
    gb_positive_edge(base_edge)?;
    gb_positive_edge(height)?;
    if sides < 3 {
        return Err(MeshError::TooFewPoints {
            found: sides,
            minimum: 3,
        });
    }
    if sides > GB_MAX_PRISM_SIDES {
        return Err(MeshError::TooManySegments {
            found: sides,
            maximum: GB_MAX_PRISM_SIDES,
        });
    }
    let a = base_edge;
    let tab_h = a * 0.25;
    let mut panels = Vec::with_capacity(2 * sides + 4);
    // Tira lateral: `sides` rectángulos a × h.
    for face in 0..sides {
        let x = face as f64 * a;
        panels.push(gb_rect_panel(x, 0.0, a, height, face));
    }
    // Bases regulares pegadas al primer rectángulo (arriba y abajo).
    let radius = a / (2.0 * (std::f64::consts::PI / sides as f64).sin());
    if !radius.is_finite() || radius <= 0.0 {
        return Err(MeshError::DegenerateGeometry {
            reason: "radio de base de prisma no finito",
        });
    }
    let apothem = a / (2.0 * (std::f64::consts::PI / sides as f64).tan());
    let top_center = [a * 0.5, height + apothem];
    let bottom_center = [a * 0.5, -apothem];
    // El primer vértice del n-gono debe coincidir con la arista superior/inferior:
    // se rota para que un lado quede horizontal sobre el rectángulo 0.
    let rot = -std::f64::consts::FRAC_PI_2 - std::f64::consts::PI / sides as f64;
    let mut top: Vec<[f64; 2]> = Vec::with_capacity(sides);
    let mut bottom: Vec<[f64; 2]> = Vec::with_capacity(sides);
    for side in 0..sides {
        let angle = rot + side as f64 * std::f64::consts::TAU / sides as f64;
        top.push([
            top_center[0] + radius * angle.cos(),
            top_center[1] + radius * angle.sin(),
        ]);
        bottom.push([
            bottom_center[0] + radius * angle.cos(),
            bottom_center[1] - radius * angle.sin(),
        ]);
    }
    panels.push(NetPanel {
        points: top.clone(),
        face_index: sides,
        is_tab: false,
    });
    panels.push(NetPanel {
        points: bottom.clone(),
        face_index: sides + 1,
        is_tab: false,
    });
    // Solapas en cada arista libre de ambas bases (todas menos la pegada al rectángulo 0).
    for base in [top, bottom] {
        for side in 1..sides {
            let p = base[side];
            let q = base[(side + 1) % sides];
            let mx = (p[0] + q[0]) * 0.5;
            let my = (p[1] + q[1]) * 0.5;
            let cx = if base[0][1] > height * 0.5 {
                top_center
            } else {
                bottom_center
            };
            let mut outward = [mx - cx[0], my - cx[1]];
            let norm = (outward[0] * outward[0] + outward[1] * outward[1]).sqrt();
            if !norm.is_finite() || norm <= 1.0e-12 {
                return Err(MeshError::DegenerateGeometry {
                    reason: "arista de base de prisma degenerada",
                });
            }
            outward[0] /= norm;
            outward[1] /= norm;
            panels.push(gb_tab_panel(p, q, outward, tab_h, sides));
        }
    }
    PolyhedronNet::new(panels, base_edge)
}

/// Net de la pirámide cuadrada (base + 4 triángulos isósceles) con 8 solapas.
///
/// `lateral_edge` debe superar `base_edge / √2` para que el ápice exista.
pub fn pyramid_net(base_edge: f64, lateral_edge: f64) -> Result<PolyhedronNet, MeshError> {
    gb_positive_edge(base_edge)?;
    gb_positive_edge(lateral_edge)?;
    let minimum = base_edge / std::f64::consts::SQRT_2;
    if lateral_edge <= minimum {
        return Err(MeshError::DegenerateGeometry {
            reason: "la arista lateral debe superar base / √2",
        });
    }
    let a = base_edge;
    let tab_h = a * 0.25;
    // Altura del triángulo isósceles (lados e, e, a).
    let tri_height = (lateral_edge * lateral_edge - a * a * 0.25).sqrt();
    if !tri_height.is_finite() || tri_height <= 0.0 {
        return Err(MeshError::DegenerateGeometry {
            reason: "altura de cara triangular no finita",
        });
    }
    let mut panels = Vec::with_capacity(13);
    panels.push(gb_rect_panel(0.0, 0.0, a, a, 0));
    // 4 triángulos, uno por lado de la base; ápice hacia afuera.
    let sides = [
        ([0.0, 0.0], [a, 0.0], [a * 0.5, -tri_height]),
        ([a, 0.0], [a, a], [a + tri_height, a * 0.5]),
        ([a, a], [0.0, a], [a * 0.5, a + tri_height]),
        ([0.0, a], [0.0, 0.0], [-tri_height, a * 0.5]),
    ];
    for (face, (p, q, apex)) in sides.iter().enumerate() {
        panels.push(NetPanel {
            points: vec![*p, *q, *apex],
            face_index: face + 1,
            is_tab: false,
        });
        // Solapas en las 2 aristas libres de cada triángulo (las que concurren al ápice).
        for (u, v) in [(*p, *apex), (*apex, *q)] {
            let mx = (u[0] + v[0]) * 0.5;
            let my = (u[1] + v[1]) * 0.5;
            // Afuera = opuesto al baricentro del triángulo.
            let centroid = [(p[0] + q[0] + apex[0]) / 3.0, (p[1] + q[1] + apex[1]) / 3.0];
            let mut outward = [mx - centroid[0], my - centroid[1]];
            let norm = (outward[0] * outward[0] + outward[1] * outward[1]).sqrt();
            if !norm.is_finite() || norm <= 1.0e-12 {
                return Err(MeshError::DegenerateGeometry {
                    reason: "arista de cara piramidal degenerada",
                });
            }
            outward[0] /= norm;
            outward[1] /= norm;
            panels.push(gb_tab_panel(u, v, outward, tab_h, face + 1));
        }
    }
    PolyhedronNet::new(panels, base_edge)
}

// ── Superficies de revolución (lathe) y extrusión ──

fn gb_check_profile(profile: &[[f64; 2]]) -> Result<(), MeshError> {
    if profile.len() < 2 {
        return Err(MeshError::TooFewPoints {
            found: profile.len(),
            minimum: 2,
        });
    }
    if profile.len() > GB_MAX_PROFILE_POINTS {
        return Err(MeshError::TooManyPoints {
            found: profile.len(),
            maximum: GB_MAX_PROFILE_POINTS,
        });
    }
    if profile
        .iter()
        .any(|point| !point[0].is_finite() || !point[1].is_finite() || point[0] < 0.0)
    {
        return Err(MeshError::NonFiniteInput);
    }
    Ok(())
}

/// Superficie de revolución: polilínea `(radio ≥ 0, altura)` rotada alrededor del eje Y.
///
/// Extremos sobre el eje (`radio ≤ eps`) se vuelven polos con un solo vértice
/// (abanico); perfiles abiertos dejan la malla abierta honestamente.
pub fn lathe_mesh(profile: &[[f64; 2]], segments: usize) -> Result<TriangleMesh3D, MeshError> {
    gb_check_profile(profile)?;
    if segments < 3 {
        return Err(MeshError::TooFewPoints {
            found: segments,
            minimum: 3,
        });
    }
    if segments > GB_MAX_LATHE_SEGMENTS {
        return Err(MeshError::TooManySegments {
            found: segments,
            maximum: GB_MAX_LATHE_SEGMENTS,
        });
    }
    let vertex_budget =
        profile
            .len()
            .checked_mul(segments)
            .ok_or(MeshError::MeshBudgetExceeded {
                what: "vértices de lathe",
                limit: GB_MAX_MESH_VERTICES,
            })?;
    if vertex_budget > GB_MAX_MESH_VERTICES {
        return Err(MeshError::MeshBudgetExceeded {
            what: "vértices de lathe",
            limit: GB_MAX_MESH_VERTICES,
        });
    }
    let triangle_budget = profile
        .len()
        .saturating_sub(1)
        .checked_mul(segments)
        .and_then(|cells| cells.checked_mul(2))
        .ok_or(MeshError::MeshBudgetExceeded {
            what: "triángulos de lathe",
            limit: GB_MAX_MESH_TRIANGLES,
        })?;
    if triangle_budget > GB_MAX_MESH_TRIANGLES {
        return Err(MeshError::MeshBudgetExceeded {
            what: "triángulos de lathe",
            limit: GB_MAX_MESH_TRIANGLES,
        });
    }
    // Puntos consecutivos duplicados colapsarían un anillo entero.
    for pair in profile.windows(2) {
        let dr = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        if !dr.is_finite() || !dy.is_finite() || dr * dr + dy * dy <= 1.0e-24 {
            return Err(MeshError::DegenerateGeometry {
                reason: "puntos consecutivos duplicados en el perfil",
            });
        }
    }

    let mut vertices: Vec<Point3D> = Vec::with_capacity(vertex_budget);
    // Anillos: un polo colapsa a un solo vértice; si no, `segments` vértices.
    let mut rings: Vec<Vec<usize>> = Vec::with_capacity(profile.len());
    for point in profile {
        let (radius, height) = (point[0], point[1]);
        if radius <= GB_GEOM_EPS {
            vertices.push(Point3D::new(0.0, height, 0.0));
            let apex = vertices.len() - 1;
            rings.push(vec![apex]);
        } else {
            let mut ring = Vec::with_capacity(segments);
            for segment in 0..segments {
                let angle = segment as f64 * std::f64::consts::TAU / segments as f64;
                vertices.push(Point3D::new(
                    radius * angle.cos(),
                    height,
                    radius * angle.sin(),
                ));
            }
            let base = vertices.len() - segments;
            for offset in 0..segments {
                ring.push(base + offset);
            }
            rings.push(ring);
        }
    }
    if rings.len() == 2 && rings[0].len() == 1 && rings[1].len() == 1 {
        return Err(MeshError::DegenerateGeometry {
            reason: "el perfil vive sobre el eje de revolución",
        });
    }
    let mut triangles: Vec<[usize; 3]> = Vec::with_capacity(triangle_budget);
    for band in 0..rings.len() - 1 {
        let lower = &rings[band];
        let upper = &rings[band + 1];
        match (lower.len(), upper.len()) {
            (1, 1) => {
                return Err(MeshError::DegenerateGeometry {
                    reason: "tramo del perfil sobre el eje de revolución",
                });
            }
            (1, _) => {
                for side in 0..upper.len() {
                    let next = (side + 1) % upper.len();
                    triangles.push([lower[0], upper[side], upper[next]]);
                }
            }
            (_, 1) => {
                for side in 0..lower.len() {
                    let next = (side + 1) % lower.len();
                    triangles.push([lower[side], upper[0], lower[next]]);
                }
            }
            _ => {
                for side in 0..segments {
                    let next = (side + 1) % segments;
                    triangles.push([lower[side], upper[side], upper[next]]);
                    triangles.push([lower[side], upper[next], lower[next]]);
                }
            }
        }
    }
    TriangleMesh3D::new(vertices, triangles)
}

/// Extrusión recta de un polígono estrictamente convexo entre `y = 0` e `y = height`.
pub fn extrude_mesh(polygon: &[[f64; 2]], height: f64) -> Result<TriangleMesh3D, MeshError> {
    if polygon.len() < 3 {
        return Err(MeshError::TooFewPoints {
            found: polygon.len(),
            minimum: 3,
        });
    }
    if polygon.len() > GB_MAX_PROFILE_POINTS {
        return Err(MeshError::TooManyPoints {
            found: polygon.len(),
            maximum: GB_MAX_PROFILE_POINTS,
        });
    }
    if !height.is_finite() || height <= 0.0 {
        return Err(MeshError::NonPositiveEdge { value: height });
    }
    if polygon
        .iter()
        .any(|point| !point[0].is_finite() || !point[1].is_finite())
    {
        return Err(MeshError::NonFiniteInput);
    }
    // Convexidad estricta: todos los giros no nulos con el mismo signo.
    let mut sign = 0.0_f64;
    for side in 0..polygon.len() {
        let a = polygon[side];
        let b = polygon[(side + 1) % polygon.len()];
        let c = polygon[(side + 2) % polygon.len()];
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if !cross.is_finite() {
            return Err(MeshError::NonFiniteInput);
        }
        if cross.abs() > 1.0e-12 {
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return Err(MeshError::NonConvexPolygon);
            }
        }
    }
    if sign == 0.0 {
        return Err(MeshError::DegenerateGeometry {
            reason: "polígono de extrusión colineal",
        });
    }
    let area = PolyhedronNet::polygon_area(polygon).abs();
    if !area.is_finite() || area <= 1.0e-12 {
        return Err(MeshError::DegenerateGeometry {
            reason: "polígono de extrusión con área nula",
        });
    }
    // CCW para winding exterior consistente.
    let mut ordered: Vec<[f64; 2]> = polygon.to_vec();
    if PolyhedronNet::polygon_area(polygon) < 0.0 {
        ordered.reverse();
    }
    let count = ordered.len();
    let mut vertices = Vec::with_capacity(2 * count);
    for point in &ordered {
        vertices.push(Point3D::new(point[0], 0.0, point[1]));
    }
    for point in &ordered {
        vertices.push(Point3D::new(point[0], height, point[1]));
    }
    let mut triangles = Vec::with_capacity(4 * count - 4);
    for side in 0..count {
        let next = (side + 1) % count;
        let lower_a = side;
        let lower_b = next;
        let upper_a = side + count;
        let upper_b = next + count;
        triangles.push([lower_a, lower_b, upper_b]);
        triangles.push([lower_a, upper_b, upper_a]);
    }
    for fan in 1..count - 1 {
        triangles.push([0, fan + 1, fan]);
        triangles.push([count, count + fan, count + fan + 1]);
    }
    TriangleMesh3D::new(vertices, triangles)
}

// ── Superficie implícita F(x,y,z) = 0 por marching sobre celdas cúbicas ──
//
// Variante marching-tetrahedra (descomposición de Kuhn, 6 tetras por celda):
// misma rejilla cúbica acotada que marching cubes, tabla de 16 casos por
// tetra en lugar de 256 por cubo. Fuera de cota (`cells > 32` o malla que
// exceda 250k triángulos) devuelve `Err` honesto.

/// Malla de la superficie `field(x, y, z) = 0` en la caja `[min, max]`.
///
/// `cells_per_axis` 1..=32 (`32³ = 32 768` celdas). El interior es `f < 0`.
/// Soldadura exacta por arista canónica de rejilla: sin vértices duplicados.
pub fn implicit_surface_mesh(
    field: &dyn Fn(f64, f64, f64) -> Option<f64>,
    min: Point3D,
    max: Point3D,
    cells_per_axis: usize,
) -> Result<TriangleMesh3D, MeshError> {
    if !min.is_finite() || !max.is_finite() || min.x >= max.x || min.y >= max.y || min.z >= max.z {
        return Err(MeshError::NonFiniteInput);
    }
    if cells_per_axis < 1 {
        return Err(MeshError::TooFewPoints {
            found: cells_per_axis,
            minimum: 1,
        });
    }
    if cells_per_axis > GB_MAX_MARCHING_CELLS_PER_AXIS {
        return Err(MeshError::TooManySegments {
            found: cells_per_axis,
            maximum: GB_MAX_MARCHING_CELLS_PER_AXIS,
        });
    }
    let nodes = cells_per_axis + 1;
    let node_count = nodes
        .checked_mul(nodes)
        .and_then(|row| row.checked_mul(nodes))
        .ok_or(MeshError::MeshBudgetExceeded {
            what: "nodos de rejilla implícita",
            limit: GB_MAX_MESH_VERTICES,
        })?;
    let step = [
        (max.x - min.x) / cells_per_axis as f64,
        (max.y - min.y) / cells_per_axis as f64,
        (max.z - min.z) / cells_per_axis as f64,
    ];
    if step.iter().any(|value| !value.is_finite() || *value <= 0.0) {
        return Err(MeshError::NonFiniteInput);
    }
    let node_id = |ix: usize, iy: usize, iz: usize| (iz * nodes + iy) * nodes + ix;
    let node_point = |id: usize| {
        let iz = id / (nodes * nodes);
        let iy = (id % (nodes * nodes)) / nodes;
        let ix = id % nodes;
        [
            min.x + ix as f64 * step[0],
            min.y + iy as f64 * step[1],
            min.z + iz as f64 * step[2],
        ]
    };
    // Muestreo fail-closed: un nodo sin valor aborta con su coordenada.
    let mut values: Vec<f64> = Vec::with_capacity(node_count);
    for id in 0..node_count {
        let point = node_point(id);
        match field(point[0], point[1], point[2]) {
            Some(value) if value.is_finite() => values.push(value),
            _ => {
                return Err(MeshError::FieldUndefined {
                    at: [point[0], point[1], point[2]],
                });
            }
        }
    }
    let mut vertices: Vec<Point3D> = Vec::new();
    let mut triangles: Vec<[usize; 3]> = Vec::new();
    // Soldadura por arista canónica (nodos ordenados): interpolación idéntica
    // bit a bit en los tetras vecinos, sin duplicados.
    let mut weld: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    let mut edge_vertex = |first: usize, second: usize, values: &[f64]| -> Option<usize> {
        let (lo, hi) = (first.min(second), first.max(second));
        let key = (lo as u32, hi as u32);
        if let Some(&index) = weld.get(&key) {
            return Some(index as usize);
        }
        let f_lo = values.get(lo).copied()?;
        let f_hi = values.get(hi).copied()?;
        let denominator = f_lo - f_hi;
        if !denominator.is_finite() || denominator.abs() <= 1.0e-300 {
            return None;
        }
        let ratio = f_lo / denominator;
        if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
            return None;
        }
        let p_lo = node_point(lo);
        let p_hi = node_point(hi);
        let point = Point3D::new(
            p_lo[0] + (p_hi[0] - p_lo[0]) * ratio,
            p_lo[1] + (p_hi[1] - p_lo[1]) * ratio,
            p_lo[2] + (p_hi[2] - p_lo[2]) * ratio,
        );
        if !point.is_finite() {
            return None;
        }
        if vertices.len() >= GB_MAX_MESH_VERTICES {
            return None;
        }
        vertices.push(point);
        let index = vertices.len() - 1;
        weld.insert(key, index as u32);
        Some(index)
    };

    // Descomposición de Kuhn: 6 tetras por celda alrededor de la diagonal principal.
    const KUHN_TETS: [[[usize; 3]; 3]; 6] = [
        [[1, 0, 0], [1, 1, 0], [1, 1, 1]],
        [[1, 0, 0], [1, 0, 1], [1, 1, 1]],
        [[0, 1, 0], [1, 1, 0], [1, 1, 1]],
        [[0, 1, 0], [0, 1, 1], [1, 1, 1]],
        [[0, 0, 1], [1, 0, 1], [1, 1, 1]],
        [[0, 0, 1], [0, 1, 1], [1, 1, 1]],
    ];
    for iz in 0..cells_per_axis {
        for iy in 0..cells_per_axis {
            for ix in 0..cells_per_axis {
                let base = [ix, iy, iz];
                let corner = |dx: usize, dy: usize, dz: usize| {
                    node_id(base[0] + dx, base[1] + dy, base[2] + dz)
                };
                let origin = corner(0, 0, 0);
                let far = corner(1, 1, 1);
                for tet in KUHN_TETS {
                    let tet_nodes = [
                        origin,
                        corner(tet[0][0], tet[0][1], tet[0][2]),
                        corner(tet[1][0], tet[1][1], tet[1][2]),
                        far,
                    ];
                    // Caso de 16 por conteo de vértices interiores (f < 0).
                    let mut inside = [false; 4];
                    for (slot, node) in tet_nodes.iter().enumerate() {
                        inside[slot] = values[*node] < 0.0;
                    }
                    let count = inside.iter().filter(|flag| **flag).count();
                    if count == 0 || count == 4 {
                        continue;
                    }
                    // Aristas del tetra que cruzan la superficie.
                    let tet_edges = |a: usize, b: usize| (tet_nodes[a], tet_nodes[b]);
                    let crossing = |a: usize, b: usize| inside[a] != inside[b];
                    let mut emit = |a: usize, b: usize| {
                        edge_vertex(tet_edges(a, b).0, tet_edges(a, b).1, &values)
                    };
                    let push_tri = |triangles: &mut Vec<[usize; 3]>,
                                    a: Option<usize>,
                                    b: Option<usize>,
                                    c: Option<usize>|
                     -> bool {
                        match (a, b, c) {
                            (Some(x), Some(y), Some(z)) => {
                                triangles.push([x, y, z]);
                                triangles.len() <= GB_MAX_MESH_TRIANGLES
                            }
                            _ => false,
                        }
                    };
                    let ok = match count {
                        1 => {
                            let apex = inside.iter().position(|flag| *flag);
                            let lone = apex.or(Some(0));
                            let apex = lone.unwrap_or(0).min(3);
                            let others: Vec<usize> = (0..4).filter(|slot| *slot != apex).collect();
                            if others.len() != 3 {
                                false
                            } else {
                                push_tri(
                                    &mut triangles,
                                    emit(apex, others[0]),
                                    emit(apex, others[1]),
                                    emit(apex, others[2]),
                                )
                            }
                        }
                        3 => {
                            let apex = inside.iter().position(|flag| !*flag).unwrap_or(0).min(3);
                            let others: Vec<usize> = (0..4).filter(|slot| *slot != apex).collect();
                            if others.len() != 3 {
                                false
                            } else {
                                push_tri(
                                    &mut triangles,
                                    emit(apex, others[0]),
                                    emit(apex, others[2]),
                                    emit(apex, others[1]),
                                )
                            }
                        }
                        _ => {
                            // 2 dentro: cuadrilátero sobre 4 aristas de cruce.
                            let mut singles: Vec<usize> = Vec::with_capacity(2);
                            let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(4);
                            for a in 0..4 {
                                for b in (a + 1)..4 {
                                    if crossing(a, b) {
                                        pairs.push((a, b));
                                    }
                                }
                            }
                            // Los 2 vértices interiores definen el eje del cuadrilátero.
                            for (slot, flag) in inside.iter().enumerate() {
                                if *flag {
                                    singles.push(slot);
                                }
                            }
                            if pairs.len() != 4 || singles.len() != 2 {
                                false
                            } else {
                                // Ordena el ciclo: aristas adyacentes comparten vértice.
                                let mut cycle: Vec<(usize, usize)> = Vec::with_capacity(4);
                                let mut pending = pairs.clone();
                                let first = pending.remove(0);
                                cycle.push(first);
                                while cycle.len() < 4 && !pending.is_empty() {
                                    let tip = cycle[cycle.len() - 1].1;
                                    let mut found = None;
                                    for (slot, edge) in pending.iter().enumerate() {
                                        if edge.0 == tip || edge.1 == tip {
                                            found = Some(slot);
                                            break;
                                        }
                                    }
                                    let Some(slot) = found else {
                                        break;
                                    };
                                    let mut edge = pending.remove(slot);
                                    if edge.0 != tip {
                                        edge = (edge.1, edge.0);
                                    }
                                    cycle.push(edge);
                                }
                                if cycle.len() != 4 {
                                    false
                                } else {
                                    let corners: Vec<Option<usize>> =
                                        cycle.iter().map(|edge| emit(edge.0, edge.1)).collect();
                                    push_tri(&mut triangles, corners[0], corners[1], corners[2])
                                        && push_tri(
                                            &mut triangles,
                                            corners[0],
                                            corners[2],
                                            corners[3],
                                        )
                                }
                            }
                        }
                    };
                    if !ok {
                        if triangles.len() > GB_MAX_MESH_TRIANGLES {
                            return Err(MeshError::MeshBudgetExceeded {
                                what: "triángulos de superficie implícita",
                                limit: GB_MAX_MESH_TRIANGLES,
                            });
                        }
                        return Err(MeshError::DegenerateGeometry {
                            reason: "celda implícita sin interpolación finita",
                        });
                    }
                }
            }
        }
    }
    TriangleMesh3D::new(vertices, triangles)
}

// ── Picking exacto rayo-malla ──

/// Intersección de Möller–Trumbore double-sided contra una malla.
///
/// Devuelve la menor distancia sobre `ray` con tolerancia `eps` (usa
/// [`GB_GEOM_EPS`] si pides exacta honesta). Triángulos degenerados o con
/// índices inválidos se saltan; `None` si no hay impacto en rango.
pub fn ray_mesh_hit(
    ray: &crate::types3d::Ray3D,
    vertices: &[Point3D],
    triangles: &[[usize; 3]],
    eps: f64,
) -> Option<f64> {
    if !eps.is_finite() || eps <= 0.0 {
        return None;
    }
    let origin = ray.origin.to_dvec3();
    let direction = ray.direction.to_dvec3();
    let mut best: Option<f64> = None;
    for triangle in triangles {
        let a = vertices.get(triangle[0])?.to_dvec3();
        let b = vertices.get(triangle[1])?.to_dvec3();
        let c = vertices.get(triangle[2])?.to_dvec3();
        let edge_ab = b - a;
        let edge_ac = c - a;
        let normal = edge_ab.cross(edge_ac);
        if normal.length_squared() <= eps * eps {
            continue;
        }
        let pvec = direction.cross(edge_ac);
        let determinant = edge_ab.dot(pvec);
        if !determinant.is_finite() || determinant.abs() <= eps {
            continue;
        }
        let inverse = determinant.recip();
        let tvec = origin - a;
        let u = tvec.dot(pvec) * inverse;
        if !u.is_finite() || u < -eps || u > 1.0 + eps {
            continue;
        }
        let qvec = tvec.cross(edge_ab);
        let v = direction.dot(qvec) * inverse;
        if !v.is_finite() || v < -eps || u + v > 1.0 + eps {
            continue;
        }
        let distance = edge_ac.dot(qvec) * inverse;
        if !distance.is_finite() || distance < ray.min_distance || distance > ray.max_distance {
            continue;
        }
        // Punto de impacto dentro del frustum visible del rayo.
        if ray.point_at(distance).is_none() {
            continue;
        }
        best = Some(match best {
            Some(current) => current.min(distance),
            None => distance,
        });
    }
    best
}

#[cfg(test)]
mod gb_tests {
    use super::*;
    use crate::types3d::Ray3D;

    fn euler_closed(mesh: &TriangleMesh3D) -> Option<i64> {
        let vertices = mesh.vertex_count() as i64;
        let faces = mesh.triangle_count() as i64;
        let mut edges = std::collections::BTreeSet::new();
        for triangle in mesh.triangles() {
            for side in 0..3 {
                let first = triangle[side];
                let second = triangle[(side + 1) % 3];
                edges.insert((first.min(second), first.max(second)));
            }
        }
        Some(vertices - edges.len() as i64 + faces)
    }

    #[test]
    fn icosahedron_has_golden_ratio_vertices_and_uniform_edges() {
        let mesh = platonic_mesh(PlatonicSolid::Icosahedron, 2.0).expect("icosaedro");
        assert_eq!(mesh.vertex_count(), 12);
        assert_eq!(mesh.triangle_count(), 20);
        assert_eq!(euler_closed(&mesh), Some(2));
        // Vértices gold-ratio: cada coordenada es 0, ±1 o ±φ (escala 1).
        for vertex in mesh.vertices() {
            let mut coords = [vertex.x.abs(), vertex.y.abs(), vertex.z.abs()];
            coords.sort_by(|a, b| a.total_cmp(b));
            assert!(coords[0] < 1e-12, "{vertex:?}");
            assert!((coords[1] - 1.0).abs() < 1e-12, "{vertex:?}");
            assert!((coords[2] - GB_GOLDEN_RATIO).abs() < 1e-12, "{vertex:?}");
        }
        let (minimum, maximum) = mesh.edge_length_range().expect("aristas");
        assert!((minimum - 2.0).abs() < 1e-9, "{minimum}");
        assert!((maximum - 2.0).abs() < 1e-9, "{maximum}");
        let area = mesh.surface_area().expect("área");
        let expected = 5.0 * 3.0_f64.sqrt() * 2.0 * 2.0;
        assert!((area - expected).abs() < 1e-6, "{area} vs {expected}");
    }

    #[test]
    fn dodecahedron_is_closed_with_uniform_edges() {
        let edge = 1.5;
        let mesh = platonic_mesh(PlatonicSolid::Dodecahedron, edge).expect("dodecaedro");
        assert_eq!(mesh.vertex_count(), 20);
        assert_eq!(mesh.triangle_count(), 36);
        assert_eq!(euler_closed(&mesh), Some(2));
        // Aristas reales = borde de los 12 pentágonos (30 únicas, miden
        // `edge`); diagonales del abanico = interiores (24, miden `edge·φ`).
        let pentagons = dodecahedron_pentagons().expect("dualidad");
        let unit = dodecahedron_unit_vertices();
        let mut real: std::collections::BTreeSet<(usize, usize)> =
            std::collections::BTreeSet::new();
        for face in pentagons {
            for side in 0..5 {
                let first = face[side];
                let second = face[(side + 1) % 5];
                real.insert((first.min(second), first.max(second)));
            }
        }
        assert_eq!(real.len(), 30);
        let scale = edge * GB_GOLDEN_RATIO / 2.0;
        for (first, second) in &real {
            let length = unit[*first].distance(&unit[*second]) * scale;
            assert!((length - edge).abs() / edge < 1e-9, "{length}");
        }
        // La malla solo contiene aristas reales o diagonales φ.
        let (minimum, _) = mesh.edge_length_range().expect("aristas");
        assert!((minimum - edge).abs() / edge < 1e-9, "{minimum}");
        // Cada vértice pertenece a exactamente 3 pentágonos (invariante combinatorio).
        let mut pentagon_incidence = vec![0_usize; 20];
        for face in pentagons {
            for index in face {
                pentagon_incidence[index] += 1;
            }
        }
        assert!(
            pentagon_incidence.iter().all(|count| *count == 3),
            "{pentagon_incidence:?}"
        );
        // Sin vértices huérfanos en la malla triangulada.
        let mut mesh_incidence = [0_usize; 20];
        for triangle in mesh.triangles() {
            for index in triangle {
                mesh_incidence[*index] += 1;
            }
        }
        assert!(mesh_incidence.iter().all(|count| *count > 0));
    }

    #[test]
    fn platonic_rejects_bad_edges() {
        assert!(matches!(
            platonic_mesh(PlatonicSolid::Icosahedron, 0.0),
            Err(MeshError::NonPositiveEdge { .. })
        ));
        assert!(matches!(
            platonic_mesh(PlatonicSolid::Dodecahedron, f64::NAN),
            Err(MeshError::NonPositiveEdge { .. })
        ));
    }

    #[test]
    fn cube_net_area_matches_closed_cube() {
        let net = cube_net(2.0).expect("net cubo");
        assert_eq!(net.face_count(), 6);
        assert!(net.tab_count() >= 6, "solapas: {}", net.tab_count());
        let area = net.face_area().expect("área caras");
        assert!((area - 24.0).abs() < 1e-9, "{area}");
        assert!(net.tab_area().expect("área solapas") > 0.0);
    }

    #[test]
    fn prism_net_area_matches_lateral_plus_bases() {
        let net = prism_net(6, 1.0, 2.0).expect("net prisma hexagonal");
        assert_eq!(net.face_count(), 8);
        let apothem = 1.0 / (2.0 * (std::f64::consts::PI / 6.0).tan());
        let expected = 6.0 * 1.0 * 2.0 + 2.0 * 0.5 * 6.0 * 1.0 * apothem;
        let area = net.face_area().expect("área caras");
        assert!((area - expected).abs() < 1e-6, "{area} vs {expected}");
        assert!(prism_net(2, 1.0, 1.0).is_err());
        assert!(matches!(
            prism_net(GB_MAX_PRISM_SIDES + 1, 1.0, 1.0),
            Err(MeshError::TooManySegments { .. })
        ));
    }

    #[test]
    fn pyramid_net_area_matches_base_plus_triangles() {
        let net = pyramid_net(2.0, 3.0).expect("net pirámide");
        assert_eq!(net.face_count(), 5);
        assert_eq!(net.tab_count(), 8);
        let tri = 0.5 * 2.0 * (9.0_f64 - 1.0).sqrt();
        let expected = 4.0 + 4.0 * tri;
        let area = net.face_area().expect("área caras");
        assert!((area - expected).abs() < 1e-9, "{area} vs {expected}");
        assert!(matches!(
            pyramid_net(2.0, 1.0),
            Err(MeshError::DegenerateGeometry { .. })
        ));
    }

    #[test]
    fn lathe_cylinder_matches_lateral_area() {
        let mesh = lathe_mesh(&[[1.0, 0.0], [1.0, 2.0]], 8).expect("lathe cilindro");
        assert_eq!(mesh.vertex_count(), 16);
        assert_eq!(mesh.triangle_count(), 16);
        // Octógono inscrito: área = perímetro_octógono * h < 2πrh.
        let area = mesh.surface_area().expect("área");
        let perimeter = 8.0 * 2.0 * (std::f64::consts::PI / 8.0).sin();
        assert!((area - perimeter * 2.0).abs() < 1e-9, "{area}");
    }

    #[test]
    fn lathe_cone_closes_pole_with_fan() {
        let mesh = lathe_mesh(&[[0.0, 0.0], [1.0, 1.0]], 4).expect("lathe cono");
        assert_eq!(mesh.vertex_count(), 5);
        assert_eq!(mesh.triangle_count(), 4);
        assert!(lathe_mesh(&[[0.0, 0.0], [0.0, 1.0]], 8).is_err());
        assert!(matches!(
            lathe_mesh(&[[1.0, 0.0], [1.0, 1.0]], 2),
            Err(MeshError::TooFewPoints { .. })
        ));
        assert!(matches!(
            lathe_mesh(&[[1.0, 0.0], [1.0, 1.0]], GB_MAX_LATHE_SEGMENTS + 1),
            Err(MeshError::TooManySegments { .. })
        ));
    }

    #[test]
    fn lathe_sphere_profile_approximates_closed_area() {
        let mut profile = Vec::new();
        for step in 0..=8 {
            let angle = step as f64 * std::f64::consts::PI / 8.0;
            profile.push([angle.sin(), -angle.cos()]);
        }
        let mesh = lathe_mesh(&profile, 24).expect("lathe esfera");
        let area = mesh.surface_area().expect("área");
        assert!((area - 4.0 * std::f64::consts::PI).abs() < 1.0, "{area}");
    }

    #[test]
    fn extrude_square_matches_closed_area() {
        let mesh = extrude_mesh(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], 2.0)
            .expect("extrusión");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        let area = mesh.surface_area().expect("área");
        assert!((area - 10.0).abs() < 1e-9, "{area}");
        assert!(matches!(
            extrude_mesh(&[[0.0, 0.0], [2.0, 0.0], [0.5, 0.5], [0.0, 2.0]], 1.0),
            Err(MeshError::NonConvexPolygon)
        ));
        assert!(matches!(
            extrude_mesh(&[[0.0, 0.0], [1.0, 0.0], [1.0, 0.0]], 1.0),
            Err(MeshError::DegenerateGeometry { .. })
        ));
    }

    #[test]
    fn implicit_sphere_produces_welded_surface() {
        let sphere = |x: f64, y: f64, z: f64| Some(x * x + y * y + z * z - 1.0);
        let mesh = implicit_surface_mesh(
            &sphere,
            Point3D::new(-1.5, -1.5, -1.5),
            Point3D::new(1.5, 1.5, 1.5),
            16,
        )
        .expect("esfera implícita");
        assert!(!mesh.triangles().is_empty());
        assert!(mesh.triangle_count() < GB_MAX_MESH_TRIANGLES);
        for vertex in mesh.vertices() {
            let value = vertex.x * vertex.x + vertex.y * vertex.y + vertex.z * vertex.z - 1.0;
            assert!(value.abs() < 0.05, "{vertex:?} f={value}");
        }
        let area = mesh.surface_area().expect("área");
        assert!(
            (area - 4.0 * std::f64::consts::PI).abs() < 2.5,
            "área={area}"
        );
    }

    #[test]
    fn implicit_plane_cuts_single_cell_with_exact_area() {
        let plane = |_x: f64, _y: f64, z: f64| Some(z);
        let mesh = implicit_surface_mesh(
            &plane,
            Point3D::new(-1.5, -1.5, -1.5),
            Point3D::new(1.5, 1.5, 1.5),
            1,
        )
        .expect("plano implícito");
        // Kuhn divide el cubo en 6 tetras: el cuadrado de corte sale
        // triangulado por tetra, pero el área es la exacta del corte.
        assert!(!mesh.triangles().is_empty());
        for vertex in mesh.vertices() {
            assert!(vertex.z.abs() < 1e-12, "{vertex:?}");
        }
        let area = mesh.surface_area().expect("área");
        assert!((area - 9.0).abs() < 1e-9, "{area}");
    }

    #[test]
    fn implicit_rejects_out_of_budget_and_undefined_field() {
        let sphere = |x: f64, y: f64, z: f64| Some(x * x + y * y + z * z - 1.0);
        assert!(matches!(
            implicit_surface_mesh(
                &sphere,
                Point3D::new(-1.0, -1.0, -1.0),
                Point3D::new(1.0, 1.0, 1.0),
                GB_MAX_MARCHING_CELLS_PER_AXIS + 1
            ),
            Err(MeshError::TooManySegments { .. })
        ));
        let hole = |x: f64, _y: f64, _z: f64| {
            if x > 0.0 {
                None
            } else {
                Some(x)
            }
        };
        assert!(matches!(
            implicit_surface_mesh(
                &hole,
                Point3D::new(-1.0, -1.0, -1.0),
                Point3D::new(1.0, 1.0, 1.0),
                2
            ),
            Err(MeshError::FieldUndefined { .. })
        ));
    }

    #[test]
    fn ray_hits_cube_mesh_and_misses() {
        let mesh = TriangleMesh3D::new(
            vec![
                Point3D::new(-1.0, -1.0, -1.0),
                Point3D::new(1.0, -1.0, -1.0),
                Point3D::new(1.0, 1.0, -1.0),
                Point3D::new(-1.0, 1.0, -1.0),
                Point3D::new(-1.0, -1.0, 1.0),
                Point3D::new(1.0, -1.0, 1.0),
                Point3D::new(1.0, 1.0, 1.0),
                Point3D::new(-1.0, 1.0, 1.0),
            ],
            vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [2, 3, 7],
                [2, 7, 6],
                [0, 4, 7],
                [0, 7, 3],
                [1, 2, 6],
                [1, 6, 5],
            ],
        )
        .expect("cubo");
        let hit_ray = Ray3D::new(
            Point3D::new(0.0, 0.0, 5.0),
            Point3D::new(0.0, 0.0, -1.0),
            0.0,
            100.0,
        )
        .expect("rayo");
        let distance = ray_mesh_hit(&hit_ray, mesh.vertices(), mesh.triangles(), GB_GEOM_EPS)
            .expect("impacto");
        assert!((distance - 4.0).abs() < 1e-9, "{distance}");
        let miss_ray = Ray3D::new(
            Point3D::new(5.0, 5.0, 5.0),
            Point3D::new(0.0, 0.0, -1.0),
            0.0,
            100.0,
        )
        .expect("rayo");
        assert_eq!(
            ray_mesh_hit(&miss_ray, mesh.vertices(), mesh.triangles(), GB_GEOM_EPS),
            None
        );
        assert_eq!(
            ray_mesh_hit(&hit_ray, mesh.vertices(), mesh.triangles(), f64::NAN),
            None
        );
    }

    #[test]
    fn mesh_constructor_enforces_budgets() {
        assert!(matches!(
            TriangleMesh3D::new(vec![Point3D::new(0.0, 0.0, 0.0); 3], vec![[0, 1, 5]]),
            Err(MeshError::DegenerateGeometry { .. })
        ));
        assert!(matches!(
            TriangleMesh3D::new(vec![Point3D::new(f64::NAN, 0.0, 0.0)], Vec::new()),
            Err(MeshError::NonFiniteInput)
        ));
    }
}
