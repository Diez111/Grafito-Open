//! File I/O: save/load documents and export images.

use anyhow::{Context, Result as AnyResult};
use grafito_core::symbolic::{
    clipboard_png_stub, datatable_to_csv, document_to_pdf, ExchangeError, MAX_EXCHANGE_OBJECTS,
};
use grafito_core::{Document, GeoObject, LineKind, ObjectId, RelationOperator};
use grafito_geometry::{Color, Point2, ViewTransform, AABB};
use grafito_whiteboard::WhiteboardElement;
#[cfg(test)]
use image::{ImageBuffer, Rgba, RgbaImage};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;

const MAX_PNG_DIMENSION: u32 = 8_192;
const MAX_PNG_PIXELS: u64 = 16_777_216;
#[cfg(test)]
const MAX_MIDPOINT_CIRCLE_RADIUS: f64 = 16_384.0;
pub(crate) const MAX_EXPORT_DIMENSION: u32 = MAX_PNG_DIMENSION;
const MAX_EXPORT_PIXELS: u64 = MAX_PNG_PIXELS;
const MAX_EXPORT_SCENE_UNITS: usize = 250_000;
const MAX_EXPORT_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_EXPORT_STYLE_PIXELS: f32 = 4_096.0;
const PARAMETRIC_EXPORT_STEPS: usize = 4_000;
const CONIC_EXPORT_STEPS: usize = 256;
const IMPLICIT_EXPORT_GRID: usize = 256;
const IMPLICIT_FILL_GRID: usize = 128;
const MAX_PROJECTED_COORDINATE: f64 = 1.0e12;

static NEXT_EXPORT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// Formatos de exportacion profesional admitidos por la aplicacion.
// NOTE(2026-09-05, OLEADA-M): PDF interino real de 1 página vía
// `grafito_core::symbolic::document_to_pdf` (1.4 mínimo, conteo + hasta 40
// etiquetas, sin geometría inventada). El vectorial con `printpdf` sigue
// pendiente del lead (requiere alta en `Cargo.toml`, fuera de este frente;
// `printpdf` hoy solo está como workspace-dep sin cablear a grafito-app).
// `export_pdf` devuelve `(path, summary)` —el mismo tipo del canal de
// `PendingExportJob`— a propósito: no se añade `ExportFormat::Pdf` para no
// romper los `match` exhaustivos de `app.rs` ni inventar comandos de paleta.
// La pizarra (`Document.whiteboard`) ya entra en SVG/PNG/TikZ vía
// `SceneBuilder::append_whiteboard` / `TikzMathWriter::emit_whiteboard`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ExportFormat {
    Svg,
    Png,
    Tikz,
}

impl ExportFormat {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 3] = [Self::Svg, Self::Png, Self::Tikz];

    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Tikz => "tex",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::Tikz => "TikZ",
        }
    }

    pub(crate) const fn support_for(self, kind: ExportObjectKind) -> ExportSupport {
        let _ = self;
        match kind {
            ExportObjectKind::Point
            | ExportObjectKind::Line
            | ExportObjectKind::Circle
            | ExportObjectKind::Polygon
            | ExportObjectKind::Pencil
            | ExportObjectKind::Function
            | ExportObjectKind::Text
            | ExportObjectKind::Ellipse
            | ExportObjectKind::Parabola
            | ExportObjectKind::Hyperbola
            | ExportObjectKind::ParametricCurve2D
            | ExportObjectKind::PolarCurve
            | ExportObjectKind::ImplicitCurve
            | ExportObjectKind::VectorField2D
            | ExportObjectKind::Histogram
            | ExportObjectKind::ScatterPlot
            | ExportObjectKind::BoxPlot
            | ExportObjectKind::RegressionLine
            | ExportObjectKind::PhasePortrait => ExportSupport::Supported,
            ExportObjectKind::Arc
            | ExportObjectKind::Sector
            | ExportObjectKind::BezierCurve
            | ExportObjectKind::Spline
            | ExportObjectKind::Point3D
            | ExportObjectKind::Segment3D
            | ExportObjectKind::Plane3D
            | ExportObjectKind::Line3D
            | ExportObjectKind::Sphere3D
            | ExportObjectKind::Cube3D
            | ExportObjectKind::Tetrahedron3D
            | ExportObjectKind::Pyramid3D
            | ExportObjectKind::Cone3D
            | ExportObjectKind::Cylinder3D
            | ExportObjectKind::Torus3D
            | ExportObjectKind::MoebiusStrip
            | ExportObjectKind::Surface3D
            | ExportObjectKind::ParametricCurve3D
            | ExportObjectKind::ComplexGrid
            | ExportObjectKind::ComplexMapping
            | ExportObjectKind::ComplexIntegral
            | ExportObjectKind::Attractor3D
            | ExportObjectKind::Fractal2D
            | ExportObjectKind::RegularPolychoron4D
            | ExportObjectKind::RegularPolytopeND
            | ExportObjectKind::HyperSurface4D
            | ExportObjectKind::VectorField3D
            | ExportObjectKind::DataTable
            | ExportObjectKind::Transformed
            | ExportObjectKind::Prism3D
            | ExportObjectKind::Quadric3D => ExportSupport::Unsupported,
        }
    }
}

impl fmt::Display for ExportFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.display_name())
    }
}

/// Modo de exportación TikZ (parámetro `tikz=visual|math`).
///
/// - `Visual`: réplica exacta en pt del lienzo (coordenadas de pantalla).
/// - `Math`: pgfplots editable en coordenadas del mundo (`\addplot` para
///   Function, `\filldraw circle` para Circle); el resto va como comentario
///   honesto que remite al modo visual. Ambos son standalone compilables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TikzMode {
    Visual,
    /// Solo construido vía [`TikzMode::from_param`] (tests) hasta que la UI
    /// exponga el selector (lead).
    #[allow(dead_code)]
    Math,
}

impl TikzMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Visual => "visual",
            Self::Math => "math",
        }
    }

    /// Parsea el parámetro `tikz=visual|math`; `None` si no es reconocido.
    /// Pendiente de cableado UI por el lead.
    #[allow(dead_code)]
    pub(crate) fn from_param(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "visual" => Some(Self::Visual),
            "math" => Some(Self::Math),
            _ => None,
        }
    }
}

/// Inventario estable de variantes del modelo y su fila en la matriz de exportacion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ExportObjectKind {
    Point,
    Line,
    Circle,
    Polygon,
    Pencil,
    Function,
    Text,
    Ellipse,
    Parabola,
    Hyperbola,
    Arc,
    Sector,
    BezierCurve,
    Spline,
    Point3D,
    Segment3D,
    Plane3D,
    Line3D,
    Sphere3D,
    Cube3D,
    Tetrahedron3D,
    Pyramid3D,
    Cone3D,
    Cylinder3D,
    Torus3D,
    MoebiusStrip,
    Surface3D,
    ParametricCurve2D,
    ParametricCurve3D,
    PolarCurve,
    ImplicitCurve,
    VectorField2D,
    ComplexGrid,
    ComplexMapping,
    ComplexIntegral,
    Attractor3D,
    Fractal2D,
    RegularPolychoron4D,
    RegularPolytopeND,
    HyperSurface4D,
    VectorField3D,
    Histogram,
    ScatterPlot,
    BoxPlot,
    RegressionLine,
    DataTable,
    PhasePortrait,
    Transformed,
    Prism3D,
    Quadric3D,
}

impl ExportObjectKind {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 50] = [
        Self::Point,
        Self::Line,
        Self::Circle,
        Self::Polygon,
        Self::Pencil,
        Self::Function,
        Self::Text,
        Self::Ellipse,
        Self::Parabola,
        Self::Hyperbola,
        Self::Arc,
        Self::Sector,
        Self::BezierCurve,
        Self::Spline,
        Self::Point3D,
        Self::Segment3D,
        Self::Plane3D,
        Self::Line3D,
        Self::Sphere3D,
        Self::Cube3D,
        Self::Tetrahedron3D,
        Self::Pyramid3D,
        Self::Cone3D,
        Self::Cylinder3D,
        Self::Torus3D,
        Self::MoebiusStrip,
        Self::Surface3D,
        Self::ParametricCurve2D,
        Self::ParametricCurve3D,
        Self::PolarCurve,
        Self::ImplicitCurve,
        Self::VectorField2D,
        Self::ComplexGrid,
        Self::ComplexMapping,
        Self::ComplexIntegral,
        Self::Attractor3D,
        Self::Fractal2D,
        Self::RegularPolychoron4D,
        Self::RegularPolytopeND,
        Self::HyperSurface4D,
        Self::VectorField3D,
        Self::Histogram,
        Self::ScatterPlot,
        Self::BoxPlot,
        Self::RegressionLine,
        Self::DataTable,
        Self::PhasePortrait,
        Self::Transformed,
        Self::Prism3D,
        Self::Quadric3D,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Point => "Point",
            Self::Line => "Line",
            Self::Circle => "Circle",
            Self::Polygon => "Polygon",
            Self::Pencil => "Pencil",
            Self::Function => "Function",
            Self::Text => "Text",
            Self::Ellipse => "Ellipse",
            Self::Parabola => "Parabola",
            Self::Hyperbola => "Hyperbola",
            Self::Arc => "Arc",
            Self::Sector => "Sector",
            Self::BezierCurve => "BezierCurve",
            Self::Spline => "Spline",
            Self::Point3D => "Point3D",
            Self::Segment3D => "Segment3D",
            Self::Plane3D => "Plane3D",
            Self::Line3D => "Line3D",
            Self::Sphere3D => "Sphere3D",
            Self::Cube3D => "Cube3D",
            Self::Tetrahedron3D => "Tetrahedron3D",
            Self::Pyramid3D => "Pyramid3D",
            Self::Cone3D => "Cone3D",
            Self::Cylinder3D => "Cylinder3D",
            Self::Torus3D => "Torus3D",
            Self::MoebiusStrip => "MoebiusStrip",
            Self::Surface3D => "Surface3D",
            Self::ParametricCurve2D => "ParametricCurve2D",
            Self::ParametricCurve3D => "ParametricCurve3D",
            Self::PolarCurve => "PolarCurve",
            Self::ImplicitCurve => "ImplicitCurve",
            Self::VectorField2D => "VectorField2D",
            Self::ComplexGrid => "ComplexGrid",
            Self::ComplexMapping => "ComplexMapping",
            Self::ComplexIntegral => "ComplexIntegral",
            Self::Attractor3D => "Attractor3D",
            Self::Fractal2D => "Fractal2D",
            Self::RegularPolychoron4D => "RegularPolychoron4D",
            Self::RegularPolytopeND => "RegularPolytopeND",
            Self::HyperSurface4D => "HyperSurface4D",
            Self::VectorField3D => "VectorField3D",
            Self::Histogram => "Histogram",
            Self::ScatterPlot => "ScatterPlot",
            Self::BoxPlot => "BoxPlot",
            Self::RegressionLine => "RegressionLine",
            Self::DataTable => "DataTable",
            Self::PhasePortrait => "PhasePortrait",
            Self::Transformed => "Transformed",
            Self::Prism3D => "Prism3D",
            Self::Quadric3D => "Quadric3D",
        }
    }

    fn from_object(object: &GeoObject) -> Option<Self> {
        Some(match object {
            GeoObject::Point(_) => Self::Point,
            GeoObject::Line(_) => Self::Line,
            GeoObject::Circle(_) => Self::Circle,
            GeoObject::Polygon(_) => Self::Polygon,
            GeoObject::Pencil(_) => Self::Pencil,
            GeoObject::Function(_) => Self::Function,
            GeoObject::Text(_) => Self::Text,
            GeoObject::Ellipse(_) => Self::Ellipse,
            GeoObject::Parabola(_) => Self::Parabola,
            GeoObject::Hyperbola(_) => Self::Hyperbola,
            GeoObject::Arc(_) => Self::Arc,
            GeoObject::Sector(_) => Self::Sector,
            GeoObject::BezierCurve(_) => Self::BezierCurve,
            GeoObject::Spline(_) => Self::Spline,
            GeoObject::Point3D(_) => Self::Point3D,
            GeoObject::Segment3D(_) => Self::Segment3D,
            GeoObject::Plane3D(_) => Self::Plane3D,
            GeoObject::Line3D(_) => Self::Line3D,
            GeoObject::Sphere3D(_) => Self::Sphere3D,
            GeoObject::Cube3D(_) => Self::Cube3D,
            GeoObject::Tetrahedron3D(_) => Self::Tetrahedron3D,
            GeoObject::Pyramid3D(_) => Self::Pyramid3D,
            GeoObject::Cone3D(_) => Self::Cone3D,
            GeoObject::Cylinder3D(_) => Self::Cylinder3D,
            GeoObject::Torus3D(_) => Self::Torus3D,
            GeoObject::MoebiusStrip(_) => Self::MoebiusStrip,
            GeoObject::Surface3D(_) => Self::Surface3D,
            GeoObject::ParametricCurve2D(_) => Self::ParametricCurve2D,
            GeoObject::ParametricCurve3D(_) => Self::ParametricCurve3D,
            GeoObject::PolarCurve(_) => Self::PolarCurve,
            GeoObject::ImplicitCurve(_) => Self::ImplicitCurve,
            GeoObject::VectorField2D(_) => Self::VectorField2D,
            GeoObject::ComplexGrid(_) => Self::ComplexGrid,
            GeoObject::ComplexMapping(_) => Self::ComplexMapping,
            GeoObject::ComplexIntegral(_) => Self::ComplexIntegral,
            GeoObject::Attractor3D(_) => Self::Attractor3D,
            GeoObject::Fractal2D(_) => Self::Fractal2D,
            GeoObject::RegularPolychoron4D(_) => Self::RegularPolychoron4D,
            GeoObject::RegularPolytopeND(_) => Self::RegularPolytopeND,
            GeoObject::HyperSurface4D(_) => Self::HyperSurface4D,
            GeoObject::VectorField3D(_) => Self::VectorField3D,
            GeoObject::Histogram(_) => Self::Histogram,
            GeoObject::ScatterPlot(_) => Self::ScatterPlot,
            GeoObject::BoxPlot(_) => Self::BoxPlot,
            GeoObject::RegressionLine(_) => Self::RegressionLine,
            GeoObject::DataTable(_) => Self::DataTable,
            GeoObject::PhasePortrait(_) => Self::PhasePortrait,
            GeoObject::Transformed(_) => Self::Transformed,
            GeoObject::Prism3D(_) => Self::Prism3D,
            GeoObject::Quadric3D(_) => Self::Quadric3D,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportSupport {
    Supported,
    Unsupported,
}

/// Identidad estable de un objeto que impidio completar una exportacion.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ExportItem {
    pub(crate) object_type: String,
    pub(crate) label: String,
    pub(crate) object_id: String,
}

impl ExportItem {
    fn from_object(object: &GeoObject) -> Self {
        Self {
            object_type: ExportObjectKind::from_object(object)
                .map(|kind| kind.as_str())
                .unwrap_or_else(|| object.name())
                .to_string(),
            label: object.label().to_string(),
            object_id: object.id().to_string(),
        }
    }

    fn display_label(&self) -> &str {
        if self.label.is_empty() {
            "<sin etiqueta>"
        } else {
            &self.label
        }
    }
}

/// Resultado verificable de una exportacion completada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportReport {
    pub(crate) format: ExportFormat,
    pub(crate) path: PathBuf,
    pub(crate) exported_objects: usize,
    pub(crate) hidden_objects: usize,
    pub(crate) primitive_count: usize,
    pub(crate) object_types: BTreeMap<String, usize>,
}

impl ExportReport {
    pub(crate) fn summary(&self) -> String {
        let types = self
            .object_types
            .iter()
            .map(|(object_type, count)| format!("{object_type} x{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        let hidden = if self.hidden_objects == 0 {
            String::new()
        } else {
            format!("; {} ocultos", self.hidden_objects)
        };
        format!(
            "{} exportado: {} objetos ({}){} -> {}",
            self.format,
            self.exported_objects,
            if types.is_empty() {
                "sin objetos"
            } else {
                &types
            },
            hidden,
            self.path.display()
        )
    }
}

/// Error tipado: ninguna variante se devuelve despues de reemplazar el destino.
#[derive(Debug)]
pub(crate) enum ExportError {
    UnsupportedObjects {
        format: ExportFormat,
        objects: Vec<ExportItem>,
    },
    InvalidObject {
        format: ExportFormat,
        object: ExportItem,
        reason: String,
    },
    InvalidView {
        format: ExportFormat,
        reason: String,
    },
    ResourceLimit {
        format: ExportFormat,
        resource: &'static str,
        attempted: u64,
        limit: u64,
        object: Option<ExportItem>,
    },
    Encoding {
        format: ExportFormat,
        reason: String,
    },
    /// Funcionalidad pendiente (p. ej. PDF sin `printpdf`): nunca toca el
    /// destino; el mensaje pinnea "no disponible en esta build".
    Unavailable {
        feature: &'static str,
        reason: String,
    },
    Io {
        format: ExportFormat,
        path: PathBuf,
        source: io::Error,
    },
}

impl ExportError {
    #[cfg(test)]
    pub(crate) fn omitted_objects(&self) -> &[ExportItem] {
        match self {
            Self::UnsupportedObjects { objects, .. } => objects,
            _ => &[],
        }
    }
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedObjects { format, objects } => {
                let list = objects
                    .iter()
                    .map(|object| format!("{} '{}'", object.object_type, object.display_label()))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "{format} no reemplazo el destino; objetos visibles no compatibles: {list}"
                )
            }
            Self::InvalidObject {
                format,
                object,
                reason,
            } => write!(
                formatter,
                "{format} no reemplazo el destino; {} '{}': {reason}",
                object.object_type,
                object.display_label()
            ),
            Self::InvalidView { format, reason } => {
                write!(
                    formatter,
                    "{format} no reemplazo el destino; vista invalida: {reason}"
                )
            }
            Self::ResourceLimit {
                format,
                resource,
                attempted,
                limit,
                object,
            } => {
                write!(
                    formatter,
                    "{format} no reemplazo el destino; {resource} {attempted} excede el limite {limit}"
                )?;
                if let Some(object) = object {
                    write!(
                        formatter,
                        " en {} '{}'",
                        object.object_type,
                        object.display_label()
                    )?;
                }
                Ok(())
            }
            Self::Encoding { format, reason } => {
                write!(
                    formatter,
                    "{format} no reemplazo el destino; codificacion: {reason}"
                )
            }
            Self::Unavailable { feature, reason } => write!(
                formatter,
                "{feature} no disponible en esta build; {reason} (destino intacto)"
            ),
            Self::Io {
                format,
                path,
                source,
            } => write!(
                formatter,
                "{format} no pudo escribir {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExportOptions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl ExportOptions {
    pub(crate) const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn from_document(document: &Document, format: ExportFormat) -> Result<Self, ExportError> {
        let size = document.view().screen_size;
        if !size.x.is_finite() || !size.y.is_finite() || size.x <= 0.0 || size.y <= 0.0 {
            return Err(ExportError::InvalidView {
                format,
                reason: "el tamano del lienzo debe ser finito y positivo".to_string(),
            });
        }
        if size.x > MAX_EXPORT_DIMENSION as f32 || size.y > MAX_EXPORT_DIMENSION as f32 {
            return Err(ExportError::ResourceLimit {
                format,
                resource: "dimension del lienzo",
                attempted: f64::from(size.x.max(size.y)).ceil() as u64,
                limit: u64::from(MAX_EXPORT_DIMENSION),
                object: None,
            });
        }
        Ok(Self::new(size.x.round() as u32, size.y.round() as u32))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenPoint {
    x: f64,
    y: f64,
}

impl ScreenPoint {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}

#[derive(Debug, Clone, Copy)]
struct StrokeStyle {
    color: Color,
    width: f32,
}

#[derive(Debug)]
enum ScenePrimitive {
    Path {
        points: Vec<ScreenPoint>,
        closed: bool,
        stroke: Option<StrokeStyle>,
        fill: Option<Color>,
    },
    Text {
        position: ScreenPoint,
        content: String,
        font_size: f32,
        color: Color,
    },
}

#[derive(Debug)]
struct SceneObject {
    item: ExportItem,
    primitives: Vec<ScenePrimitive>,
}

#[derive(Debug)]
struct ExportScene {
    width: u32,
    height: u32,
    hidden_objects: usize,
    object_types: BTreeMap<String, usize>,
    objects: Vec<SceneObject>,
    scene_units: usize,
}

impl ExportScene {
    fn primitive_count(&self) -> usize {
        self.objects
            .iter()
            .map(|object| object.primitives.len())
            .sum()
    }
}

struct ExportView {
    transform: ViewTransform,
    bounds: AABB,
    width: f64,
    height: f64,
}

impl ExportView {
    fn new(
        document: &Document,
        options: ExportOptions,
        format: ExportFormat,
    ) -> Result<Self, ExportError> {
        validate_export_options(options, format)?;
        let mut transform = *document.view();
        if !transform.scale.is_finite() || transform.scale <= 0.0 {
            return Err(ExportError::InvalidView {
                format,
                reason: "la escala debe ser finita y positiva".to_string(),
            });
        }
        if !transform.offset.x.is_finite() || !transform.offset.y.is_finite() {
            return Err(ExportError::InvalidView {
                format,
                reason: "el desplazamiento debe ser finito".to_string(),
            });
        }
        transform.screen_size = glam::Vec2::new(options.width as f32, options.height as f32);
        let top_left = transform.screen_to_world(glam::Vec2::ZERO);
        let bottom_right = transform.screen_to_world(transform.screen_size);
        if !point_is_finite(top_left) || !point_is_finite(bottom_right) {
            return Err(ExportError::InvalidView {
                format,
                reason: "los limites visibles desbordan el rango numerico".to_string(),
            });
        }
        let bounds = AABB::new(
            Point2::new(
                top_left.x.min(bottom_right.x),
                top_left.y.min(bottom_right.y),
            ),
            Point2::new(
                top_left.x.max(bottom_right.x),
                top_left.y.max(bottom_right.y),
            ),
        );
        if bounds.min.x >= bounds.max.x || bounds.min.y >= bounds.max.y {
            return Err(ExportError::InvalidView {
                format,
                reason: "los limites visibles son degenerados".to_string(),
            });
        }
        Ok(Self {
            transform,
            bounds,
            width: f64::from(options.width),
            height: f64::from(options.height),
        })
    }

    fn project(&self, point: Point2) -> Option<ScreenPoint> {
        if !point_is_finite(point)
            || (self.transform.x_log && point.x <= 0.0)
            || (self.transform.y_log && point.y <= 0.0)
        {
            return None;
        }
        let origin_x = self.width * 0.5 + self.transform.offset.x;
        let origin_y = self.height * 0.5 + self.transform.offset.y;
        let x = if self.transform.x_log {
            origin_x + point.x.log10() * self.transform.scale
        } else {
            origin_x + point.x * self.transform.scale
        };
        let y = if self.transform.y_log {
            origin_y - point.y.log10() * self.transform.scale
        } else {
            origin_y - point.y * self.transform.scale
        };
        (x.is_finite()
            && y.is_finite()
            && x.abs() <= MAX_PROJECTED_COORDINATE
            && y.abs() <= MAX_PROJECTED_COORDINATE)
            .then_some(ScreenPoint::new(x, y))
    }

    fn unproject(&self, point: ScreenPoint) -> Point2 {
        self.transform
            .screen_to_world(glam::Vec2::new(point.x as f32, point.y as f32))
    }

    fn contains_with_margin(&self, point: ScreenPoint, margin: f64) -> bool {
        point.x >= -margin
            && point.x <= self.width + margin
            && point.y >= -margin
            && point.y <= self.height + margin
    }
}

#[cfg(test)]
fn validate_png_dimensions(width: u32, height: u32) -> AnyResult<()> {
    if width == 0 || height == 0 {
        anyhow::bail!("PNG dimensions must be greater than zero");
    }
    if width > MAX_PNG_DIMENSION || height > MAX_PNG_DIMENSION {
        anyhow::bail!("PNG dimensions must not exceed {MAX_PNG_DIMENSION} pixels");
    }

    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .context("PNG pixel count overflow")?;
    if pixels > MAX_PNG_PIXELS {
        anyhow::bail!("PNG image must not exceed {MAX_PNG_PIXELS} pixels");
    }

    Ok(())
}

fn escape_xml(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&apos;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Save document to a JSON file.
pub fn save_document(doc: &Document, path: &str) -> AnyResult<()> {
    grafito_core::write_document_atomic(doc, path).context("Failed to save document")?;
    Ok(())
}

/// Load document from a JSON file.
pub fn load_document(path: &str) -> AnyResult<Document> {
    grafito_core::read_document_file(path).context("Failed to load document")
}

fn validate_export_options(
    options: ExportOptions,
    format: ExportFormat,
) -> std::result::Result<(), ExportError> {
    if options.width == 0 || options.height == 0 {
        return Err(ExportError::InvalidView {
            format,
            reason: "las dimensiones deben ser mayores que cero".to_string(),
        });
    }
    if options.width > MAX_EXPORT_DIMENSION || options.height > MAX_EXPORT_DIMENSION {
        return Err(ExportError::ResourceLimit {
            format,
            resource: "dimension del lienzo",
            attempted: u64::from(options.width.max(options.height)),
            limit: u64::from(MAX_EXPORT_DIMENSION),
            object: None,
        });
    }
    let pixels = u64::from(options.width)
        .checked_mul(u64::from(options.height))
        .ok_or(ExportError::ResourceLimit {
            format,
            resource: "pixeles del lienzo",
            attempted: u64::MAX,
            limit: MAX_EXPORT_PIXELS,
            object: None,
        })?;
    if pixels > MAX_EXPORT_PIXELS {
        return Err(ExportError::ResourceLimit {
            format,
            resource: "pixeles del lienzo",
            attempted: pixels,
            limit: MAX_EXPORT_PIXELS,
            object: None,
        });
    }
    Ok(())
}

fn point_is_finite(point: Point2) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

/// Variables del documento en orden determinista (hash/evaluación estable).
fn sorted_document_variables(document: &Document) -> Vec<(String, f64)> {
    let mut variables = document
        .variables
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect::<Vec<_>>();
    variables.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    variables
}

fn color_is_valid(color: Color) -> bool {
    [color.r, color.g, color.b, color.a]
        .into_iter()
        .all(|component| component.is_finite() && (0.0..=1.0).contains(&component))
}

fn whiteboard_rgb_to_color(rgb: (u8, u8, u8)) -> Color {
    Color::new(
        f32::from(rgb.0) / 255.0,
        f32::from(rgb.1) / 255.0,
        f32::from(rgb.2) / 255.0,
        1.0,
    )
}

fn finite_whiteboard_pair(point: (f64, f64)) -> bool {
    point.0.is_finite() && point.1.is_finite()
}

fn invalid_object(
    format: ExportFormat,
    item: &ExportItem,
    reason: impl Into<String>,
) -> ExportError {
    ExportError::InvalidObject {
        format,
        object: item.clone(),
        reason: reason.into(),
    }
}

fn validate_stroke(
    format: ExportFormat,
    item: &ExportItem,
    width: f32,
    color: Color,
) -> std::result::Result<StrokeStyle, ExportError> {
    if !width.is_finite() || width <= 0.0 || width > MAX_EXPORT_STYLE_PIXELS {
        return Err(invalid_object(
            format,
            item,
            format!("el grosor {width} debe estar entre 0 y {MAX_EXPORT_STYLE_PIXELS} px"),
        ));
    }
    if !color_is_valid(color) {
        return Err(invalid_object(format, item, "el color RGBA no es valido"));
    }
    Ok(StrokeStyle { color, width })
}

fn validate_fill(
    format: ExportFormat,
    item: &ExportItem,
    fill: Option<Color>,
) -> std::result::Result<Option<Color>, ExportError> {
    if fill.is_some_and(|color| !color_is_valid(color)) {
        return Err(invalid_object(
            format,
            item,
            "el color de relleno RGBA no es valido",
        ));
    }
    Ok(fill)
}

struct SceneBuilder<'a> {
    document: &'a Document,
    format: ExportFormat,
    view: ExportView,
    variables: Vec<(String, f64)>,
    scene_units: usize,
}

impl<'a> SceneBuilder<'a> {
    fn resolve_binding(
        &self,
        item: &ExportItem,
        expression: &Option<String>,
        fallback: f64,
        field: &str,
    ) -> std::result::Result<f64, ExportError> {
        let value = match expression {
            Some(expression) => grafito_geometry::expr::evaluate(expression, &self.variables)
                .map_err(|error| {
                    invalid_object(
                        self.format,
                        item,
                        format!("{field} no se pudo evaluar: {error}"),
                    )
                })?,
            None => fallback,
        };
        if !value.is_finite() {
            return Err(invalid_object(
                self.format,
                item,
                format!("{field} produjo un valor no finito"),
            ));
        }
        Ok(value)
    }

    fn charge(&mut self, item: &ExportItem, units: usize) -> std::result::Result<(), ExportError> {
        let attempted =
            self.scene_units
                .checked_add(units)
                .ok_or_else(|| ExportError::ResourceLimit {
                    format: self.format,
                    resource: "unidades de geometria",
                    attempted: u64::MAX,
                    limit: MAX_EXPORT_SCENE_UNITS as u64,
                    object: Some(item.clone()),
                })?;
        if attempted > MAX_EXPORT_SCENE_UNITS {
            return Err(ExportError::ResourceLimit {
                format: self.format,
                resource: "unidades de geometria",
                attempted: attempted as u64,
                limit: MAX_EXPORT_SCENE_UNITS as u64,
                object: Some(item.clone()),
            });
        }
        self.scene_units = attempted;
        Ok(())
    }

    fn push_path(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        points: Vec<ScreenPoint>,
        closed: bool,
        stroke: Option<StrokeStyle>,
        fill: Option<Color>,
    ) -> std::result::Result<(), ExportError> {
        let minimum = if closed || fill.is_some() { 3 } else { 2 };
        if points.len() < minimum {
            return Ok(());
        }
        if points.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || point.x.abs() > MAX_PROJECTED_COORDINATE
                || point.y.abs() > MAX_PROJECTED_COORDINATE
        }) {
            return Err(invalid_object(
                self.format,
                item,
                "la geometria proyectada desborda el rango numerico",
            ));
        }
        self.charge(item, points.len() + 1)?;
        primitives.push(ScenePrimitive::Path {
            points,
            closed,
            stroke,
            fill,
        });
        Ok(())
    }

    fn push_text(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        position: ScreenPoint,
        content: &str,
        font_size: f32,
        color: Color,
    ) -> std::result::Result<(), ExportError> {
        if !font_size.is_finite() || font_size <= 0.0 || font_size > MAX_EXPORT_STYLE_PIXELS {
            return Err(invalid_object(
                self.format,
                item,
                format!(
                    "el tamano de texto {font_size} debe estar entre 0 y {MAX_EXPORT_STYLE_PIXELS} px"
                ),
            ));
        }
        if !color_is_valid(color) {
            return Err(invalid_object(
                self.format,
                item,
                "el color RGBA no es valido",
            ));
        }
        self.charge(item, content.chars().count().saturating_add(1))?;
        primitives.push(ScenePrimitive::Text {
            position,
            content: content.to_string(),
            font_size,
            color,
        });
        Ok(())
    }

    fn required_projection(
        &self,
        item: &ExportItem,
        point: Point2,
    ) -> std::result::Result<ScreenPoint, ExportError> {
        if !point_is_finite(point) {
            return Err(invalid_object(
                self.format,
                item,
                "una coordenada evaluada no es finita",
            ));
        }
        self.view.project(point).ok_or_else(|| {
            invalid_object(
                self.format,
                item,
                "una coordenada no puede representarse en la vista actual",
            )
        })
    }

    fn projected_runs<I>(&self, points: I, limit_jumps: bool) -> Vec<Vec<ScreenPoint>>
    where
        I: IntoIterator<Item = Option<Point2>>,
    {
        let mut runs = Vec::new();
        let mut current = Vec::new();
        let mut previous: Option<ScreenPoint> = None;
        let max_jump = self.view.width.max(self.view.height) * 0.75;

        let flush = |current: &mut Vec<ScreenPoint>, runs: &mut Vec<Vec<ScreenPoint>>| {
            if current.len() >= 2 {
                runs.push(std::mem::take(current));
            } else {
                current.clear();
            }
        };

        for world in points {
            let projected = world.and_then(|point| self.view.project(point));
            let Some(point) = projected else {
                flush(&mut current, &mut runs);
                previous = None;
                continue;
            };
            if let Some(previous_point) = previous {
                if limit_jumps && previous_point.distance(point) > max_jump {
                    flush(&mut current, &mut runs);
                } else if let Some((start, end)) =
                    clip_segment_to_canvas(previous_point, point, self.view.width, self.view.height)
                {
                    if current
                        .last()
                        .is_none_or(|last| last.distance(start) > 1.0e-6)
                    {
                        flush(&mut current, &mut runs);
                        current.push(start);
                    }
                    if current
                        .last()
                        .is_none_or(|last| last.distance(end) > 1.0e-6)
                    {
                        current.push(end);
                    }
                } else {
                    flush(&mut current, &mut runs);
                }
            }
            previous = Some(point);
        }
        flush(&mut current, &mut runs);
        runs
    }

    fn push_world_polyline<I>(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        points: I,
        stroke: StrokeStyle,
        limit_jumps: bool,
    ) -> std::result::Result<Vec<Vec<ScreenPoint>>, ExportError>
    where
        I: IntoIterator<Item = Option<Point2>>,
    {
        let runs = self.projected_runs(points, limit_jumps);
        for run in &runs {
            self.push_path(item, primitives, run.clone(), false, Some(stroke), None)?;
        }
        Ok(runs)
    }

    fn push_closed_world_shape(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        world_points: &[Point2],
        stroke: StrokeStyle,
        fill: Option<Color>,
    ) -> std::result::Result<(), ExportError> {
        let projected = world_points
            .iter()
            .copied()
            .map(|point| self.required_projection(item, point))
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if let Some(fill) = fill {
            let clipped =
                clip_polygon_to_canvas(projected.clone(), self.view.width, self.view.height);
            self.push_path(item, primitives, clipped, true, None, Some(fill))?;
        }

        let stroke_points = world_points
            .iter()
            .copied()
            .chain(world_points.first().copied())
            .map(Some);
        self.push_world_polyline(item, primitives, stroke_points, stroke, false)?;
        Ok(())
    }

    fn push_screen_rect(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        first: ScreenPoint,
        second: ScreenPoint,
        stroke: Option<StrokeStyle>,
        fill: Option<Color>,
    ) -> std::result::Result<(), ExportError> {
        let min_x = first.x.min(second.x).clamp(0.0, self.view.width);
        let max_x = first.x.max(second.x).clamp(0.0, self.view.width);
        let min_y = first.y.min(second.y).clamp(0.0, self.view.height);
        let max_y = first.y.max(second.y).clamp(0.0, self.view.height);
        if min_x >= max_x || min_y >= max_y {
            return Ok(());
        }
        self.push_path(
            item,
            primitives,
            vec![
                ScreenPoint::new(min_x, min_y),
                ScreenPoint::new(max_x, min_y),
                ScreenPoint::new(max_x, max_y),
                ScreenPoint::new(min_x, max_y),
            ],
            true,
            stroke,
            fill,
        )
    }

    fn push_marker(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        center: ScreenPoint,
        radius: f32,
        color: Color,
    ) -> std::result::Result<(), ExportError> {
        if !radius.is_finite() || radius <= 0.0 || radius > MAX_EXPORT_STYLE_PIXELS {
            return Err(invalid_object(
                self.format,
                item,
                format!("el radio del marcador {radius} no es representable"),
            ));
        }
        if !color_is_valid(color) {
            return Err(invalid_object(
                self.format,
                item,
                "el color RGBA no es valido",
            ));
        }
        if !self.view.contains_with_margin(center, f64::from(radius)) {
            return Ok(());
        }
        let points = (0..32)
            .map(|index| {
                let angle = index as f64 * std::f64::consts::TAU / 32.0;
                ScreenPoint::new(
                    center.x + f64::from(radius) * angle.cos(),
                    center.y + f64::from(radius) * angle.sin(),
                )
            })
            .collect();
        self.push_path(item, primitives, points, true, None, Some(color))
    }

    /// Vuelca `Document.whiteboard` a la escena: SVG vectorial (texto como
    /// `<text>`) y PNG raster (compone escena + pizarra vía `render_png`).
    /// Con pizarra vacía no añade nada (export geométrico intacto).
    fn append_whiteboard(
        &mut self,
        objects: &mut Vec<SceneObject>,
        object_types: &mut BTreeMap<String, usize>,
    ) -> std::result::Result<(), ExportError> {
        let elements = self.document.whiteboard.elements().to_vec();
        if elements.is_empty() {
            return Ok(());
        }
        let item = ExportItem {
            object_type: "Whiteboard".to_string(),
            label: "pizarra".to_string(),
            object_id: "whiteboard".to_string(),
        };
        let mut primitives = Vec::new();
        let mut included = 0usize;
        for element in &elements {
            let before = primitives.len();
            self.push_whiteboard_element(&item, element, &mut primitives)?;
            if primitives.len() > before {
                included += 1;
            }
        }
        if primitives.is_empty() {
            return Ok(());
        }
        *object_types.entry(item.object_type.clone()).or_insert(0) += included;
        objects.push(SceneObject { item, primitives });
        Ok(())
    }

    fn push_whiteboard_element(
        &mut self,
        item: &ExportItem,
        element: &WhiteboardElement,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        // Tinta de formas/texto sin color propio: casi negro, legible sobre
        // el fondo blanco del export (igual que `text_primary` en claro).
        let shape_color = whiteboard_rgb_to_color((26, 26, 26));
        match element {
            WhiteboardElement::Stroke {
                points,
                color,
                width,
            } => {
                let Ok(stroke) = validate_stroke(
                    self.format,
                    item,
                    *width as f32,
                    whiteboard_rgb_to_color(*color),
                ) else {
                    return Ok(());
                };
                self.push_world_polyline(
                    item,
                    primitives,
                    points.iter().map(|&(x, y)| {
                        (x.is_finite() && y.is_finite()).then_some(Point2::new(x, y))
                    }),
                    stroke,
                    false,
                )?;
            }
            WhiteboardElement::Rectangle { min, max, fill } => {
                if !finite_whiteboard_pair(*min) || !finite_whiteboard_pair(*max) {
                    return Ok(());
                }
                let Ok(stroke) = validate_stroke(self.format, item, 1.8, shape_color) else {
                    return Ok(());
                };
                let Ok(fill) = validate_fill(self.format, item, fill.map(whiteboard_rgb_to_color))
                else {
                    return Ok(());
                };
                let corners = [
                    Point2::new(min.0, min.1),
                    Point2::new(max.0, min.1),
                    Point2::new(max.0, max.1),
                    Point2::new(min.0, max.1),
                ];
                self.push_closed_world_shape(item, primitives, &corners, stroke, fill)?;
            }
            WhiteboardElement::Ellipse { center, rx, ry } => {
                if !finite_whiteboard_pair(*center)
                    || !rx.is_finite()
                    || !ry.is_finite()
                    || *rx <= 0.0
                    || *ry <= 0.0
                {
                    return Ok(());
                }
                let Ok(stroke) = validate_stroke(self.format, item, 1.8, shape_color) else {
                    return Ok(());
                };
                let points = sampled_ellipse(Point2::new(center.0, center.1), *rx, *ry, 0.0);
                self.push_closed_world_shape(item, primitives, &points, stroke, None)?;
            }
            WhiteboardElement::Arrow { from, to } => {
                if !finite_whiteboard_pair(*from) || !finite_whiteboard_pair(*to) {
                    return Ok(());
                }
                let Ok(stroke) = validate_stroke(self.format, item, 1.8, shape_color) else {
                    return Ok(());
                };
                let start = Point2::new(from.0, from.1);
                let end = Point2::new(to.0, to.1);
                self.push_world_polyline(
                    item,
                    primitives,
                    [Some(start), Some(end)],
                    stroke,
                    false,
                )?;
                for wing in [
                    grafito_whiteboard::arrow_tip(*from, *to, 0.55).0,
                    grafito_whiteboard::arrow_tip(*from, *to, 0.55).1,
                ] {
                    self.push_world_polyline(
                        item,
                        primitives,
                        [Some(Point2::new(wing.0, wing.1)), Some(end)],
                        stroke,
                        false,
                    )?;
                }
            }
            WhiteboardElement::Text { at, text, size } => {
                if text.is_empty() || !finite_whiteboard_pair(*at) {
                    return Ok(());
                }
                if !size.is_finite() || *size <= 0.0 {
                    return Ok(());
                }
                let Some(position) = self.view.project(Point2::new(at.0, at.1)) else {
                    return Ok(());
                };
                if !self.view.contains_with_margin(position, *size) {
                    return Ok(());
                }
                // `push_text` valida tamaño/color y genera `<text>` en SVG.
                if self
                    .push_text(item, primitives, position, text, *size as f32, shape_color)
                    .is_err()
                {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}

fn clip_segment_to_canvas(
    start: ScreenPoint,
    end: ScreenPoint,
    width: f64,
    height: f64,
) -> Option<(ScreenPoint, ScreenPoint)> {
    if !start.x.is_finite() || !start.y.is_finite() || !end.x.is_finite() || !end.y.is_finite() {
        return None;
    }
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    for (p, q) in [
        (-dx, start.x),
        (dx, width - start.x),
        (-dy, start.y),
        (dy, height - start.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            enter = enter.max(t);
        } else {
            exit = exit.min(t);
        }
        if enter > exit {
            return None;
        }
    }
    Some((
        ScreenPoint::new(start.x + enter * dx, start.y + enter * dy),
        ScreenPoint::new(start.x + exit * dx, start.y + exit * dy),
    ))
}

fn clip_polygon_to_canvas(
    mut polygon: Vec<ScreenPoint>,
    width: f64,
    height: f64,
) -> Vec<ScreenPoint> {
    #[derive(Clone, Copy)]
    enum Edge {
        Left,
        Right,
        Top,
        Bottom,
    }

    fn inside(point: ScreenPoint, edge: Edge, width: f64, height: f64) -> bool {
        match edge {
            Edge::Left => point.x >= 0.0,
            Edge::Right => point.x <= width,
            Edge::Top => point.y >= 0.0,
            Edge::Bottom => point.y <= height,
        }
    }

    fn intersection(
        start: ScreenPoint,
        end: ScreenPoint,
        edge: Edge,
        width: f64,
        height: f64,
    ) -> ScreenPoint {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        match edge {
            Edge::Left | Edge::Right => {
                let x = if matches!(edge, Edge::Left) {
                    0.0
                } else {
                    width
                };
                let t = if dx == 0.0 { 0.0 } else { (x - start.x) / dx };
                ScreenPoint::new(x, start.y + t * dy)
            }
            Edge::Top | Edge::Bottom => {
                let y = if matches!(edge, Edge::Top) {
                    0.0
                } else {
                    height
                };
                let t = if dy == 0.0 { 0.0 } else { (y - start.y) / dy };
                ScreenPoint::new(start.x + t * dx, y)
            }
        }
    }

    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        if polygon.is_empty() {
            break;
        }
        let input = std::mem::take(&mut polygon);
        let Some(last) = input.last().copied() else {
            continue;
        };
        let mut previous = last;
        let mut previous_inside = inside(previous, edge, width, height);
        for current in input {
            let current_inside = inside(current, edge, width, height);
            match (previous_inside, current_inside) {
                (true, true) => polygon.push(current),
                (true, false) => {
                    polygon.push(intersection(previous, current, edge, width, height));
                }
                (false, true) => {
                    polygon.push(intersection(previous, current, edge, width, height));
                    polygon.push(current);
                }
                (false, false) => {}
            }
            previous = current;
            previous_inside = current_inside;
        }
    }
    polygon
}

fn build_export_scene(
    document: &Document,
    format: ExportFormat,
    options: ExportOptions,
) -> std::result::Result<ExportScene, ExportError> {
    let view = ExportView::new(document, options, format)?;
    let hidden_objects = document
        .objects_iter()
        .filter(|(_, object)| !object.is_visible())
        .count();
    let mut visible: Vec<(ObjectId, &GeoObject)> = document
        .objects_iter()
        .filter(|(_, object)| object.is_visible())
        .map(|(id, object)| (*id, object))
        .collect();

    let mut unsupported = visible
        .iter()
        .filter_map(|(_, object)| {
            let support = ExportObjectKind::from_object(object)
                .map(|kind| format.support_for(kind))
                .unwrap_or(ExportSupport::Unsupported);
            (support == ExportSupport::Unsupported).then(|| ExportItem::from_object(object))
        })
        .collect::<Vec<_>>();
    unsupported.sort();
    if !unsupported.is_empty() {
        return Err(ExportError::UnsupportedObjects {
            format,
            objects: unsupported,
        });
    }

    visible.sort_unstable_by_key(|(id, object)| (grafito_render::scene_layer_2d(object), *id));
    let mut builder = SceneBuilder {
        document,
        format,
        view,
        variables: sorted_document_variables(document),
        scene_units: 0,
    };
    let mut object_types = BTreeMap::new();
    let mut objects = Vec::with_capacity(visible.len());

    for (_, object) in visible {
        let item = ExportItem::from_object(object);
        grafito_core::validation::validate_object_candidate(document, object)
            .map_err(|reason| invalid_object(format, &item, reason))?;
        let primitives = builder.build_object(object, &item)?;
        *object_types.entry(item.object_type.clone()).or_insert(0) += 1;
        objects.push(SceneObject { item, primitives });
    }
    builder.append_whiteboard(&mut objects, &mut object_types)?;

    Ok(ExportScene {
        width: options.width,
        height: options.height,
        hidden_objects,
        object_types,
        objects,
        scene_units: builder.scene_units,
    })
}

impl SceneBuilder<'_> {
    fn build_object(
        &mut self,
        object: &GeoObject,
        item: &ExportItem,
    ) -> std::result::Result<Vec<ScenePrimitive>, ExportError> {
        let mut primitives = Vec::new();
        match object {
            GeoObject::Point(point) => {
                let position = Point2::new(
                    self.resolve_binding(item, &point.x_expr, point.position.x, "Point.x_expr")?,
                    self.resolve_binding(item, &point.y_expr, point.position.y, "Point.y_expr")?,
                );
                let projected = self.required_projection(item, position)?;
                self.push_marker(item, &mut primitives, projected, point.size, point.color)?;
            }
            GeoObject::Line(line) => {
                let start = Point2::new(
                    self.resolve_binding(
                        item,
                        &line.start_x_expr,
                        line.start.x,
                        "Line.start_x_expr",
                    )?,
                    self.resolve_binding(
                        item,
                        &line.start_y_expr,
                        line.start.y,
                        "Line.start_y_expr",
                    )?,
                );
                let end = Point2::new(
                    self.resolve_binding(item, &line.end_x_expr, line.end.x, "Line.end_x_expr")?,
                    self.resolve_binding(item, &line.end_y_expr, line.end.y, "Line.end_y_expr")?,
                );
                if !point_is_finite(start) || !point_is_finite(end) {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "los extremos evaluados deben ser finitos",
                    ));
                }
                if start == end && line.kind != LineKind::Segment {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "una recta o semirrecta necesita una direccion no nula",
                    ));
                }
                let clipped = match line.kind {
                    LineKind::Segment => {
                        grafito_geometry::clip_segment_to_rect(start, end, self.view.bounds)
                    }
                    LineKind::Ray => {
                        grafito_geometry::clip_ray_to_rect(start, end, self.view.bounds)
                    }
                    LineKind::Line => {
                        grafito_geometry::clip_line_to_rect(start, end, self.view.bounds)
                    }
                };
                if let Some((start, end)) = clipped {
                    let stroke = validate_stroke(self.format, item, line.width, line.color)?;
                    self.push_world_polyline(
                        item,
                        &mut primitives,
                        [Some(start), Some(end)],
                        stroke,
                        false,
                    )?;
                }
            }
            GeoObject::Circle(circle) => {
                let radius = self.resolve_binding(
                    item,
                    &circle.radius_expr,
                    circle.radius,
                    "Circle.radius_expr",
                )?;
                if !radius.is_finite() || radius <= 0.0 {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "el radio evaluado debe ser finito y positivo",
                    ));
                }
                let stroke = validate_stroke(self.format, item, circle.width, circle.color)?;
                let fill = validate_fill(self.format, item, circle.fill_color)?;
                let points = sampled_ellipse(circle.center, radius, radius, 0.0);
                self.push_closed_world_shape(item, &mut primitives, &points, stroke, fill)?;
            }
            GeoObject::Polygon(polygon) => {
                if polygon.vertices.len() < 3 {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "un poligono visible necesita al menos tres vertices",
                    ));
                }
                let vertices = polygon
                    .vertices
                    .iter()
                    .enumerate()
                    .map(|(index, vertex)| {
                        Ok(Point2::new(
                            self.resolve_binding(
                                item,
                                polygon.x_exprs.get(index).unwrap_or(&None),
                                vertex.x,
                                &format!("Polygon.x_exprs[{index}]"),
                            )?,
                            self.resolve_binding(
                                item,
                                polygon.y_exprs.get(index).unwrap_or(&None),
                                vertex.y,
                                &format!("Polygon.y_exprs[{index}]"),
                            )?,
                        ))
                    })
                    .collect::<std::result::Result<Vec<_>, ExportError>>()?;
                if vertices
                    .iter()
                    .copied()
                    .any(|point| !point_is_finite(point))
                {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "un vertice evaluado no es finito",
                    ));
                }
                let stroke = validate_stroke(self.format, item, polygon.width, polygon.color)?;
                let fill = validate_fill(self.format, item, polygon.fill_color)?;
                self.push_closed_world_shape(item, &mut primitives, &vertices, stroke, fill)?;
            }
            GeoObject::Pencil(pencil) => {
                if pencil.points.len() < 2 {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "un trazo visible necesita al menos dos puntos",
                    ));
                }
                let stroke = validate_stroke(self.format, item, pencil.width, pencil.color)?;
                self.push_world_polyline(
                    item,
                    &mut primitives,
                    pencil.points.iter().copied().map(Some),
                    stroke,
                    false,
                )?;
            }
            GeoObject::Function(function) => {
                self.build_function(item, function, &mut primitives)?;
            }
            GeoObject::Text(text) => {
                let position = self.required_projection(item, text.position)?;
                if self
                    .view
                    .contains_with_margin(position, f64::from(text.font_size))
                {
                    self.push_text(
                        item,
                        &mut primitives,
                        position,
                        &text.content,
                        text.font_size,
                        text.color,
                    )?;
                }
            }
            GeoObject::Ellipse(ellipse) => {
                let stroke = validate_stroke(self.format, item, ellipse.width, ellipse.color)?;
                let fill = validate_fill(self.format, item, ellipse.fill_color)?;
                let points = sampled_ellipse(ellipse.center, ellipse.rx, ellipse.ry, ellipse.angle);
                self.push_closed_world_shape(item, &mut primitives, &points, stroke, fill)?;
            }
            GeoObject::Parabola(parabola) => {
                let stroke = validate_stroke(self.format, item, parabola.width, parabola.color)?;
                let points = sample_parabola(parabola, self.view.bounds);
                if points.len() < 2 {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "la parabola no produjo geometria finita",
                    ));
                }
                self.push_world_polyline(
                    item,
                    &mut primitives,
                    points.into_iter().map(Some),
                    stroke,
                    false,
                )?;
            }
            GeoObject::Hyperbola(hyperbola) => {
                let stroke = validate_stroke(self.format, item, hyperbola.width, hyperbola.color)?;
                for branch in sample_hyperbola(hyperbola, self.view.bounds) {
                    self.push_world_polyline(
                        item,
                        &mut primitives,
                        branch.into_iter().map(Some),
                        stroke,
                        false,
                    )?;
                }
            }
            GeoObject::ParametricCurve2D(curve) => {
                let t_min = self.resolve_binding(
                    item,
                    &curve.t_min_expr,
                    curve.t_min,
                    "ParametricCurve2D.t_min_expr",
                )?;
                let t_max = self.resolve_binding(
                    item,
                    &curve.t_max_expr,
                    curve.t_max,
                    "ParametricCurve2D.t_max_expr",
                )?;
                if t_min >= t_max {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "el dominio parametrico evaluado debe ser creciente",
                    ));
                }
                let stroke = validate_stroke(self.format, item, curve.width, curve.color)?;
                let samples = grafito_core::parametric_sampling::samples_or_compute_curve_2d(
                    curve,
                    PARAMETRIC_EXPORT_STEPS,
                    &self.document.variables,
                )
                .clone();
                if !samples.iter().any(|(x, y)| x.is_finite() && y.is_finite()) {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "la curva parametrica no produjo muestras finitas",
                    ));
                }
                self.push_world_polyline(
                    item,
                    &mut primitives,
                    samples.into_iter().map(|(x, y)| {
                        (x.is_finite() && y.is_finite()).then_some(Point2::new(x, y))
                    }),
                    stroke,
                    true,
                )?;
            }
            GeoObject::PolarCurve(curve) => {
                self.build_polar(item, curve, &mut primitives)?;
            }
            GeoObject::ImplicitCurve(curve) => {
                self.build_implicit(item, curve, &mut primitives)?;
            }
            GeoObject::VectorField2D(field) => {
                self.build_vector_field(item, field, &mut primitives)?;
            }
            GeoObject::Histogram(histogram) => {
                self.build_histogram(item, histogram, &mut primitives)?;
            }
            GeoObject::ScatterPlot(scatter) => {
                if scatter.xs.is_empty() || scatter.xs.len() != scatter.ys.len() {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "el diagrama de dispersion necesita pares x/y completos",
                    ));
                }
                for (&x, &y) in scatter.xs.iter().zip(&scatter.ys) {
                    let center = self.required_projection(item, Point2::new(x, y))?;
                    self.push_marker(
                        item,
                        &mut primitives,
                        center,
                        scatter.point_size,
                        scatter.color,
                    )?;
                }
            }
            GeoObject::BoxPlot(box_plot) => {
                self.build_box_plot(item, box_plot, &mut primitives)?;
            }
            GeoObject::RegressionLine(regression) => {
                self.build_regression(item, regression, &mut primitives)?;
            }
            GeoObject::PhasePortrait(portrait) => {
                let stroke = validate_stroke(self.format, item, 1.5, portrait.color)?;
                let segments =
                    grafito_render::sample_phase_portrait(portrait, &self.document.variables);
                if segments.is_empty()
                    && !phase_portrait_has_finite_sample(portrait, &self.document.variables)
                {
                    return Err(invalid_object(
                        self.format,
                        item,
                        "el retrato de fase no produjo valores finitos",
                    ));
                }
                for (start, end) in segments {
                    self.push_world_polyline(
                        item,
                        &mut primitives,
                        [Some(start), Some(end)],
                        stroke,
                        false,
                    )?;
                }
            }
            GeoObject::Point3D(_)
            | GeoObject::Segment3D(_)
            | GeoObject::Plane3D(_)
            | GeoObject::Line3D(_)
            | GeoObject::Sphere3D(_)
            | GeoObject::Cube3D(_)
            | GeoObject::Tetrahedron3D(_)
            | GeoObject::Pyramid3D(_)
            | GeoObject::Cone3D(_)
            | GeoObject::Cylinder3D(_)
            | GeoObject::Torus3D(_)
            | GeoObject::MoebiusStrip(_)
            | GeoObject::Surface3D(_)
            | GeoObject::ParametricCurve3D(_)
            | GeoObject::ComplexGrid(_)
            | GeoObject::ComplexMapping(_)
            | GeoObject::ComplexIntegral(_)
            | GeoObject::Attractor3D(_)
            | GeoObject::Fractal2D(_)
            | GeoObject::RegularPolychoron4D(_)
            | GeoObject::RegularPolytopeND(_)
            | GeoObject::HyperSurface4D(_)
            | GeoObject::VectorField3D(_)
            | GeoObject::Transformed(_) => {
                return Err(ExportError::UnsupportedObjects {
                    format: self.format,
                    objects: vec![item.clone()],
                });
            }
            _ => {
                return Err(ExportError::UnsupportedObjects {
                    format: self.format,
                    objects: vec![item.clone()],
                });
            }
        }
        Ok(primitives)
    }

    fn build_function(
        &mut self,
        item: &ExportItem,
        function: &grafito_core::FunctionObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        let min = self.resolve_binding(
            item,
            &function.domain_min_expr,
            function.domain_min.unwrap_or(self.view.bounds.min.x),
            "Function.domain_min_expr",
        )?;
        let max = self.resolve_binding(
            item,
            &function.domain_max_expr,
            function.domain_max.unwrap_or(self.view.bounds.max.x),
            "Function.domain_max_expr",
        )?;
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(invalid_object(
                self.format,
                item,
                "el dominio evaluado debe ser finito y creciente",
            ));
        }
        let visible_min = min.max(self.view.bounds.min.x);
        let visible_max = max.min(self.view.bounds.max.x);
        if visible_min >= visible_max {
            return Ok(());
        }
        let grid_size = grafito_core::function_sampling::recommended_grid_size_for_quality(
            self.view.width as f32,
            self.document.render_quality,
        );
        let samples = grafito_core::function_sampling::samples_or_compute(
            function,
            (visible_min, visible_max),
            grid_size,
            &self.document.variables,
        )
        .clone();
        if !samples.iter().any(|(_, y)| y.is_some_and(f64::is_finite)) {
            return Err(invalid_object(
                self.format,
                item,
                "la funcion no produjo muestras finitas en la vista",
            ));
        }
        let stroke = validate_stroke(self.format, item, function.width, function.color)?;
        let runs = self.projected_runs(
            samples.iter().map(|(x, y)| {
                y.filter(|value| value.is_finite())
                    .map(|y| Point2::new(*x, y))
            }),
            true,
        );
        if let Some(fill) = validate_fill(self.format, item, function.fill_color)? {
            let baseline = self
                .view
                .project(Point2::new(visible_min, 0.0))
                .ok_or_else(|| {
                    invalid_object(
                        self.format,
                        item,
                        "el relleno hasta y=0 no es representable en el eje logaritmico",
                    )
                })?
                .y;
            for run in &runs {
                let Some(&first) = run.first() else {
                    continue;
                };
                let Some(&last) = run.last() else {
                    continue;
                };
                let mut polygon = run.clone();
                polygon.push(ScreenPoint::new(last.x, baseline));
                polygon.push(ScreenPoint::new(first.x, baseline));
                let polygon = clip_polygon_to_canvas(polygon, self.view.width, self.view.height);
                self.push_path(item, primitives, polygon, true, None, Some(fill))?;
            }
        }
        for run in runs {
            self.push_path(item, primitives, run, false, Some(stroke), None)?;
        }
        Ok(())
    }

    fn build_polar(
        &mut self,
        item: &ExportItem,
        curve: &grafito_core::PolarCurveObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        let t_min = self.resolve_binding(
            item,
            &curve.t_min_expr,
            curve.t_min,
            "PolarCurve.t_min_expr",
        )?;
        let t_max = self.resolve_binding(
            item,
            &curve.t_max_expr,
            curve.t_max,
            "PolarCurve.t_max_expr",
        )?;
        if t_min >= t_max {
            return Err(invalid_object(
                self.format,
                item,
                "el dominio polar evaluado debe ser creciente",
            ));
        }
        let samples = grafito_core::parametric_sampling::samples_or_compute_polar(
            curve,
            PARAMETRIC_EXPORT_STEPS,
            &self.document.variables,
        )
        .clone();
        if !samples.iter().any(|(x, y)| x.is_finite() && y.is_finite()) {
            return Err(invalid_object(
                self.format,
                item,
                "la curva polar no produjo muestras finitas",
            ));
        }
        let stroke = validate_stroke(self.format, item, curve.width, curve.color)?;
        let runs = self.projected_runs(
            samples
                .into_iter()
                .map(|(x, y)| (x.is_finite() && y.is_finite()).then_some(Point2::new(x, y))),
            true,
        );
        if let Some(fill) = validate_fill(self.format, item, curve.fill_color)? {
            let origin = self.view.project(Point2::new(0.0, 0.0)).ok_or_else(|| {
                invalid_object(
                    self.format,
                    item,
                    "el relleno polar no es representable con ejes logaritmicos",
                )
            })?;
            for run in &runs {
                let mut polygon = run.clone();
                polygon.push(origin);
                let polygon = clip_polygon_to_canvas(polygon, self.view.width, self.view.height);
                self.push_path(item, primitives, polygon, true, None, Some(fill))?;
            }
        }
        for run in runs {
            self.push_path(item, primitives, run, false, Some(stroke), None)?;
        }
        Ok(())
    }

    fn build_implicit(
        &mut self,
        item: &ExportItem,
        curve: &grafito_core::ImplicitCurveObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        let (lhs, rhs) = curve
            .get_cached_asts(&self.document.variables, &["x", "y"])
            .ok_or_else(|| {
                invalid_object(
                    self.format,
                    item,
                    "las expresiones implicitas no se pudieron evaluar",
                )
            })?;
        let bounds = (
            self.view.bounds.min.x,
            self.view.bounds.max.x,
            self.view.bounds.min.y,
            self.view.bounds.max.y,
        );
        let mut finite_probe = false;
        for x_index in 0..=4 {
            for y_index in 0..=4 {
                let x = bounds.0 + (bounds.1 - bounds.0) * x_index as f64 / 4.0;
                let y = bounds.2 + (bounds.3 - bounds.2) * y_index as f64 / 4.0;
                let left = lhs.eval_2d("x", x, "y", y);
                let right = rhs.eval_2d("x", x, "y", y);
                finite_probe |= left.is_finite() && right.is_finite();
            }
        }
        if !finite_probe {
            return Err(invalid_object(
                self.format,
                item,
                "la curva implicita no produjo valores finitos en la vista",
            ));
        }

        if let Some(fill) = validate_fill(self.format, item, curve.fill_color)? {
            if curve.operator != RelationOperator::Eq {
                self.build_implicit_fill(item, primitives, curve.operator, &lhs, &rhs, fill)?;
            }
        }

        let segments = grafito_core::implicit_curve::segments_or_compute(
            curve,
            bounds,
            IMPLICIT_EXPORT_GRID,
            &self.document.variables,
            self.document.render_quality,
        )
        .clone();
        let default_stroke = validate_stroke(self.format, item, curve.width, curve.color)?;
        let contour_count = segments.len().max(1);
        for (index, (_, level_segments)) in segments.into_iter().enumerate() {
            let color = curve
                .contour_colors
                .as_deref()
                .and_then(|colors| colors.get(index))
                .copied()
                .unwrap_or_else(|| {
                    if curve.contour_levels.is_some() {
                        let t = index as f32 / contour_count as f32;
                        Color::new(0.5 + t * 0.5, 0.2 + (1.0 - t) * 0.6, 0.2, 1.0)
                    } else {
                        curve.color
                    }
                });
            let stroke = StrokeStyle {
                color,
                ..default_stroke
            };
            for (start, end) in level_segments {
                self.push_world_polyline(
                    item,
                    primitives,
                    [Some(start), Some(end)],
                    stroke,
                    false,
                )?;
            }
        }
        Ok(())
    }

    fn build_implicit_fill(
        &mut self,
        item: &ExportItem,
        primitives: &mut Vec<ScenePrimitive>,
        operator: RelationOperator,
        lhs: &grafito_geometry::ast::Expr,
        rhs: &grafito_geometry::ast::Expr,
        fill: Color,
    ) -> std::result::Result<(), ExportError> {
        let cell_width = self.view.width / IMPLICIT_FILL_GRID as f64;
        let cell_height = self.view.height / IMPLICIT_FILL_GRID as f64;
        let mut finite_values = 0usize;

        for row in 0..IMPLICIT_FILL_GRID {
            let y0 = row as f64 * cell_height;
            let y1 = (row + 1) as f64 * cell_height;
            let mut run_start = None;
            for column in 0..=IMPLICIT_FILL_GRID {
                let inside = if column == IMPLICIT_FILL_GRID {
                    false
                } else {
                    let center = ScreenPoint::new(
                        (column as f64 + 0.5) * cell_width,
                        (row as f64 + 0.5) * cell_height,
                    );
                    let world = self.view.unproject(center);
                    let left = lhs.eval_2d("x", world.x, "y", world.y);
                    let right = rhs.eval_2d("x", world.x, "y", world.y);
                    if left.is_finite() && right.is_finite() {
                        finite_values += 1;
                        match operator {
                            RelationOperator::Eq => false,
                            RelationOperator::Less => left < right,
                            RelationOperator::Greater => left > right,
                            RelationOperator::LessEq => left <= right,
                            RelationOperator::GreaterEq => left >= right,
                        }
                    } else {
                        false
                    }
                };

                match (run_start, inside) {
                    (None, true) => run_start = Some(column),
                    (Some(start), false) => {
                        let x0 = start as f64 * cell_width;
                        let x1 = column as f64 * cell_width;
                        self.push_screen_rect(
                            item,
                            primitives,
                            ScreenPoint::new(x0, y0),
                            ScreenPoint::new(x1, y1),
                            None,
                            Some(fill),
                        )?;
                        run_start = None;
                    }
                    _ => {}
                }
            }
        }
        if finite_values == 0 {
            return Err(invalid_object(
                self.format,
                item,
                "el relleno implicito no produjo valores finitos",
            ));
        }
        Ok(())
    }

    fn build_vector_field(
        &mut self,
        item: &ExportItem,
        field: &grafito_core::VectorField2DObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        let bounds = (
            self.view.bounds.min.x,
            self.view.bounds.max.x,
            self.view.bounds.min.y,
            self.view.bounds.max.y,
        );
        let grid_size = field.density.clamp(5, 80);
        let dx = (bounds.1 - bounds.0) / grid_size as f64;
        let dy = (bounds.3 - bounds.2) / grid_size as f64;
        let arrow_length = dx.abs().min(dy.abs()) * 0.8;
        if !arrow_length.is_finite() || arrow_length <= 0.0 {
            return Err(invalid_object(
                self.format,
                item,
                "la escala del campo vectorial no es representable",
            ));
        }
        let samples = grafito_core::vector_field_sampling::samples_or_compute(
            field,
            bounds,
            grid_size,
            &self.document.variables,
        )
        .clone();
        if !samples
            .iter()
            .any(|(_, _, u, v)| u.is_finite() && v.is_finite())
        {
            return Err(invalid_object(
                self.format,
                item,
                "el campo vectorial no produjo valores finitos",
            ));
        }
        let stroke = validate_stroke(self.format, item, 1.5, field.color)?;
        for (x, y, u, v) in samples {
            if x < bounds.0 - dx
                || x > bounds.1 + dx
                || y < bounds.2 - dy
                || y > bounds.3 + dy
                || !u.is_finite()
                || !v.is_finite()
            {
                continue;
            }
            let magnitude = u.hypot(v);
            if !magnitude.is_finite() || magnitude <= 1.0e-10 {
                continue;
            }
            let start_world = Point2::new(x, y);
            let end_world = Point2::new(
                x + u / magnitude * arrow_length,
                y + v / magnitude * arrow_length,
            );
            let (Some(start), Some(end)) =
                (self.view.project(start_world), self.view.project(end_world))
            else {
                continue;
            };
            let Some((clipped_start, clipped_end)) =
                clip_segment_to_canvas(start, end, self.view.width, self.view.height)
            else {
                continue;
            };
            self.push_path(
                item,
                primitives,
                vec![clipped_start, clipped_end],
                false,
                Some(stroke),
                None,
            )?;
            let angle = (end.y - start.y).atan2(end.x - start.x);
            let head_length = (end.distance(start) * 0.3).clamp(3.0, 12.0);
            for delta in [-0.45_f64, 0.45_f64] {
                let tip = ScreenPoint::new(
                    end.x - head_length * (angle + delta).cos(),
                    end.y - head_length * (angle + delta).sin(),
                );
                if let Some((head_start, head_end)) =
                    clip_segment_to_canvas(end, tip, self.view.width, self.view.height)
                {
                    self.push_path(
                        item,
                        primitives,
                        vec![head_start, head_end],
                        false,
                        Some(stroke),
                        None,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn build_histogram(
        &mut self,
        item: &ExportItem,
        histogram: &grafito_core::HistogramObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        if histogram.data.is_empty() {
            return Err(invalid_object(
                self.format,
                item,
                "el histograma visible no contiene datos",
            ));
        }
        let bins = grafito_geometry::statistics::histogram(&histogram.data, histogram.bins);
        let max_count = bins.iter().map(|(_, _, count)| *count).fold(0.0, f64::max);
        if !max_count.is_finite() || max_count <= 0.0 {
            return Err(invalid_object(
                self.format,
                item,
                "el histograma no produjo frecuencias finitas",
            ));
        }
        let stroke = validate_stroke(self.format, item, histogram.width, histogram.color)?;
        let fill = validate_fill(self.format, item, histogram.fill_color)?;
        let y_scale = (histogram.y_max - histogram.y_min) / max_count;
        for (left, right, count) in bins {
            let bottom_left = self.required_projection(item, Point2::new(left, histogram.y_min))?;
            let top_right = self
                .required_projection(item, Point2::new(right, histogram.y_min + count * y_scale))?;
            self.push_screen_rect(item, primitives, bottom_left, top_right, Some(stroke), fill)?;
        }
        Ok(())
    }

    fn build_box_plot(
        &mut self,
        item: &ExportItem,
        box_plot: &grafito_core::BoxPlotObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        let Some((low, q1, median, q3, high, outliers)) =
            grafito_geometry::statistics::boxplot_stats(&box_plot.data)
        else {
            return Err(invalid_object(
                self.format,
                item,
                "el diagrama de caja visible no contiene datos suficientes",
            ));
        };
        let stroke = validate_stroke(self.format, item, box_plot.width, box_plot.color)?;
        let fill = validate_fill(self.format, item, box_plot.fill_color)?;
        let half_width = box_plot.width_box * 0.5;
        let box_min =
            self.required_projection(item, Point2::new(box_plot.position - half_width, q1))?;
        let box_max =
            self.required_projection(item, Point2::new(box_plot.position + half_width, q3))?;
        self.push_screen_rect(item, primitives, box_min, box_max, Some(stroke), fill)?;
        let median_stroke = StrokeStyle {
            width: (box_plot.width * 2.0).min(MAX_EXPORT_STYLE_PIXELS),
            ..stroke
        };
        for (start, end, current_stroke) in [
            (
                Point2::new(box_plot.position - half_width, median),
                Point2::new(box_plot.position + half_width, median),
                median_stroke,
            ),
            (
                Point2::new(box_plot.position, low),
                Point2::new(box_plot.position, q1),
                stroke,
            ),
            (
                Point2::new(box_plot.position, q3),
                Point2::new(box_plot.position, high),
                stroke,
            ),
            (
                Point2::new(box_plot.position - half_width * 0.4, low),
                Point2::new(box_plot.position + half_width * 0.4, low),
                stroke,
            ),
            (
                Point2::new(box_plot.position - half_width * 0.4, high),
                Point2::new(box_plot.position + half_width * 0.4, high),
                stroke,
            ),
        ] {
            self.push_world_polyline(
                item,
                primitives,
                [Some(start), Some(end)],
                current_stroke,
                false,
            )?;
        }
        for outlier in outliers {
            let center = self.required_projection(item, Point2::new(box_plot.position, outlier))?;
            self.push_marker(item, primitives, center, 3.0, box_plot.color)?;
        }
        Ok(())
    }

    fn build_regression(
        &mut self,
        item: &ExportItem,
        regression: &grafito_core::RegressionLineObj,
        primitives: &mut Vec<ScenePrimitive>,
    ) -> std::result::Result<(), ExportError> {
        if regression.xs.len() != regression.ys.len() {
            return Err(invalid_object(
                self.format,
                item,
                "la regresion necesita pares x/y completos",
            ));
        }
        let stroke = validate_stroke(self.format, item, regression.width, regression.color)?;
        let start = Point2::new(
            regression.x_min,
            regression.slope * regression.x_min + regression.intercept,
        );
        let end = Point2::new(
            regression.x_max,
            regression.slope * regression.x_max + regression.intercept,
        );
        if !point_is_finite(start) || !point_is_finite(end) {
            return Err(invalid_object(
                self.format,
                item,
                "la recta de regresion desborda el rango numerico",
            ));
        }
        self.push_world_polyline(item, primitives, [Some(start), Some(end)], stroke, false)?;
        for (&x, &y) in regression.xs.iter().zip(&regression.ys) {
            let center = self.required_projection(item, Point2::new(x, y))?;
            self.push_marker(item, primitives, center, 4.0, regression.color)?;
        }
        Ok(())
    }
}

fn sampled_ellipse(center: Point2, rx: f64, ry: f64, angle: f64) -> Vec<Point2> {
    let cos_angle = angle.cos();
    let sin_angle = angle.sin();
    (0..CONIC_EXPORT_STEPS)
        .map(|index| {
            let parameter = index as f64 * std::f64::consts::TAU / CONIC_EXPORT_STEPS as f64;
            let local_x = rx * parameter.cos();
            let local_y = ry * parameter.sin();
            Point2::new(
                center.x + local_x * cos_angle - local_y * sin_angle,
                center.y + local_x * sin_angle + local_y * cos_angle,
            )
        })
        .collect()
}

fn phase_portrait_has_finite_sample(
    portrait: &grafito_core::PhasePortraitObj,
    variables: &std::collections::HashMap<String, f64>,
) -> bool {
    let prepared_dx =
        grafito_geometry::expr::prepare_function_ast(&portrait.expr_dx, variables, &["x", "y"])
            .ok();
    let prepared_dy =
        grafito_geometry::expr::prepare_function_ast(&portrait.expr_dy, variables, &["x", "y"])
            .ok();
    let (Some(dx), Some(dy)) = (prepared_dx, prepared_dy) else {
        return false;
    };
    for x_index in 0..=4 {
        let x = portrait.x_min + (portrait.x_max - portrait.x_min) * x_index as f64 / 4.0;
        for y_index in 0..=4 {
            let y = portrait.y_min + (portrait.y_max - portrait.y_min) * y_index as f64 / 4.0;
            if dx.eval_2d("x", x, "y", y).is_finite() && dy.eval_2d("x", x, "y", y).is_finite() {
                return true;
            }
        }
    }
    false
}

fn sample_parabola(parabola: &grafito_core::ParabolaObj, bounds: AABB) -> Vec<Point2> {
    if !parabola.p.is_finite() || parabola.p == 0.0 || !parabola.angle.is_finite() {
        return Vec::new();
    }
    let cos_angle = parabola.angle.cos();
    let sin_angle = parabola.angle.sin();
    let corners = [
        Point2::new(bounds.min.x, bounds.min.y),
        Point2::new(bounds.min.x, bounds.max.y),
        Point2::new(bounds.max.x, bounds.min.y),
        Point2::new(bounds.max.x, bounds.max.y),
    ];
    let local_x = corners.map(|corner| {
        let dx = corner.x - parabola.vertex.x;
        let dy = corner.y - parabola.vertex.y;
        dx * cos_angle + dy * sin_angle
    });
    let min = local_x.into_iter().fold(f64::INFINITY, f64::min);
    let max = local_x.into_iter().fold(f64::NEG_INFINITY, f64::max);
    if !min.is_finite() || !max.is_finite() || min >= max {
        return Vec::new();
    }
    (0..=CONIC_EXPORT_STEPS)
        .filter_map(|index| {
            let local_x = min + (max - min) * index as f64 / CONIC_EXPORT_STEPS as f64;
            let local_y = local_x * local_x / (4.0 * parabola.p);
            let point = Point2::new(
                parabola.vertex.x + local_x * cos_angle - local_y * sin_angle,
                parabola.vertex.y + local_x * sin_angle + local_y * cos_angle,
            );
            point_is_finite(point).then_some(point)
        })
        .collect()
}

fn sample_hyperbola(hyperbola: &grafito_core::HyperbolaObj, bounds: AABB) -> [Vec<Point2>; 2] {
    let extent = [
        (bounds.min.x - hyperbola.center.x).abs(),
        (bounds.max.x - hyperbola.center.x).abs(),
        (bounds.min.y - hyperbola.center.y).abs(),
        (bounds.max.y - hyperbola.center.y).abs(),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    let denominator = hyperbola.a.min(hyperbola.b).max(f64::MIN_POSITIVE);
    let parameter_max = (extent / denominator + 2.0)
        .max(1.0)
        .acosh()
        .clamp(1.0, 20.0);
    let cos_angle = hyperbola.angle.cos();
    let sin_angle = hyperbola.angle.sin();
    [1.0, -1.0].map(|sign| {
        (0..=CONIC_EXPORT_STEPS)
            .filter_map(|index| {
                let parameter =
                    -parameter_max + 2.0 * parameter_max * index as f64 / CONIC_EXPORT_STEPS as f64;
                let (local_x, local_y) = if hyperbola.horizontal {
                    (
                        sign * hyperbola.a * parameter.cosh(),
                        hyperbola.b * parameter.sinh(),
                    )
                } else {
                    (
                        hyperbola.b * parameter.sinh(),
                        sign * hyperbola.a * parameter.cosh(),
                    )
                };
                let point = Point2::new(
                    hyperbola.center.x + local_x * cos_angle - local_y * sin_angle,
                    hyperbola.center.y + local_x * sin_angle + local_y * cos_angle,
                );
                point_is_finite(point).then_some(point)
            })
            .collect()
    })
}

fn svg_color(color: Color) -> String {
    format!(
        "rgb({},{},{})",
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8
    )
}

fn serialize_svg(scene: &ExportScene) -> Vec<u8> {
    use std::fmt::Write as _;

    let mut svg = String::with_capacity(scene.scene_units.saturating_mul(24).min(4_000_000));
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">",
        scene.width, scene.height, scene.width, scene.height
    )
    .ok();
    svg.push_str("<title>Grafito - exportacion SVG</title>\n");
    writeln!(
        svg,
        "<defs><clipPath id=\"grafito-export-clip\"><rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"/></clipPath></defs>",
        scene.width, scene.height
    )
    .ok();
    writeln!(
        svg,
        "<rect width=\"{}\" height=\"{}\" fill=\"white\"/>",
        scene.width, scene.height
    )
    .ok();
    svg.push_str("<g clip-path=\"url(#grafito-export-clip)\">\n");

    for object in &scene.objects {
        writeln!(
            svg,
            "<g data-grafito-type=\"{}\" data-grafito-label=\"{}\" data-grafito-id=\"{}\">",
            escape_xml(&object.item.object_type),
            escape_xml(&object.item.label),
            escape_xml(&object.item.object_id)
        )
        .ok();
        for primitive in &object.primitives {
            match primitive {
                ScenePrimitive::Path {
                    points,
                    closed,
                    stroke,
                    fill,
                } => {
                    let mut data = String::with_capacity(points.len() * 24);
                    for (index, point) in points.iter().enumerate() {
                        if index > 0 {
                            data.push(' ');
                        }
                        write!(
                            data,
                            "{} {:.3} {:.3}",
                            if index == 0 { 'M' } else { 'L' },
                            point.x,
                            point.y
                        )
                        .ok();
                    }
                    if *closed {
                        data.push_str(" Z");
                    }
                    write!(svg, "<path d=\"{data}\"").ok();
                    if let Some(stroke) = stroke {
                        write!(
                            svg,
                            " stroke=\"{}\" stroke-opacity=\"{:.4}\" stroke-width=\"{:.3}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
                            svg_color(stroke.color),
                            stroke.color.a,
                            stroke.width
                        )
                        .ok();
                    } else {
                        svg.push_str(" stroke=\"none\"");
                    }
                    if let Some(fill) = fill {
                        write!(
                            svg,
                            " fill=\"{}\" fill-opacity=\"{:.4}\"",
                            svg_color(*fill),
                            fill.a
                        )
                        .ok();
                    } else {
                        svg.push_str(" fill=\"none\"");
                    }
                    svg.push_str("/>\n");
                }
                ScenePrimitive::Text {
                    position,
                    content,
                    font_size,
                    color,
                } => {
                    writeln!(
                        svg,
                        "<text x=\"{:.3}\" y=\"{:.3}\" fill=\"{}\" fill-opacity=\"{:.4}\" font-family=\"Ubuntu, sans-serif\" font-size=\"{:.3}\">{}</text>",
                        position.x,
                        position.y,
                        svg_color(*color),
                        color.a,
                        font_size,
                        escape_xml(content)
                    )
                    .ok();
                }
            }
        }
        svg.push_str("</g>\n");
    }
    svg.push_str("</g>\n</svg>\n");
    svg.into_bytes()
}

fn escape_tikz(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        escaped.push_str(match character {
            '\\' => r"\textbackslash{}",
            '{' => r"\{",
            '}' => r"\}",
            '$' => r"\$",
            '&' => r"\&",
            '#' => r"\#",
            '^' => r"\textasciicircum{}",
            '_' => r"\_",
            '~' => r"\textasciitilde{}",
            '%' => r"\%",
            '\n' => r"\\",
            _ => {
                escaped.push(character);
                continue;
            }
        });
    }
    escaped
}

fn tikz_color(color: Color) -> String {
    format!(
        "{{rgb,1:red,{:.5};green,{:.5};blue,{:.5}}}",
        color.r, color.g, color.b
    )
}

fn serialize_tikz(scene: &ExportScene) -> Vec<u8> {
    use std::fmt::Write as _;

    let mut tex = String::with_capacity(scene.scene_units.saturating_mul(28).min(4_000_000));
    tex.push_str("\\documentclass{standalone}\n");
    tex.push_str("\\usepackage[utf8]{inputenc}\n");
    tex.push_str("\\usepackage{xcolor}\n");
    tex.push_str("\\usepackage{tikz}\n");
    tex.push_str("\\begin{document}\n");
    tex.push_str("\\begin{tikzpicture}[x=1pt,y=1pt,line cap=round,line join=round]\n");
    writeln!(
        tex,
        "\\path[fill=white] (0,0) rectangle ({},{});",
        scene.width, scene.height
    )
    .ok();
    writeln!(
        tex,
        "\\clip (0,0) rectangle ({},{});",
        scene.width, scene.height
    )
    .ok();

    for object in &scene.objects {
        writeln!(
            tex,
            "% {} label={} id={}",
            object.item.object_type,
            object.item.label.replace(['\n', '\r'], " "),
            object.item.object_id
        )
        .ok();
        for primitive in &object.primitives {
            match primitive {
                ScenePrimitive::Path {
                    points,
                    closed,
                    stroke,
                    fill,
                } => {
                    let mut options = Vec::new();
                    if let Some(stroke) = stroke {
                        options.push(format!("draw={}", tikz_color(stroke.color)));
                        options.push(format!("draw opacity={:.5}", stroke.color.a));
                        options.push(format!("line width={:.3}pt", stroke.width));
                    } else {
                        options.push("draw=none".to_string());
                    }
                    if let Some(fill) = fill {
                        options.push(format!("fill={}", tikz_color(*fill)));
                        options.push(format!("fill opacity={:.5}", fill.a));
                    } else {
                        options.push("fill=none".to_string());
                    }
                    write!(tex, "\\path[{}] ", options.join(",")).ok();
                    for (index, point) in points.iter().enumerate() {
                        if index > 0 {
                            tex.push_str(" -- ");
                        }
                        write!(
                            tex,
                            "({:.3},{:.3})",
                            point.x,
                            f64::from(scene.height) - point.y
                        )
                        .ok();
                    }
                    if *closed {
                        tex.push_str(" -- cycle");
                    }
                    tex.push_str(";\n");
                }
                ScenePrimitive::Text {
                    position,
                    content,
                    font_size,
                    color,
                } => {
                    writeln!(
                        tex,
                        "\\node[anchor=west,text={},text opacity={:.5},font=\\fontsize{{{:.3}}}{{{:.3}}}\\selectfont] at ({:.3},{:.3}) {{{}}};",
                        tikz_color(*color),
                        color.a,
                        font_size,
                        font_size * 1.2,
                        position.x,
                        f64::from(scene.height) - position.y,
                        escape_tikz(content)
                    )
                    .ok();
                }
            }
        }
    }
    tex.push_str("\\end{tikzpicture}\n\\end{document}\n");
    tex.into_bytes()
}

/// Número de mundo formateado para TikZ math; `None` si no es representable.
fn math_num(value: f64) -> Option<String> {
    value.is_finite().then(|| format!("{value:.4}"))
}

/// Sanea una expresión Grafito para `\addplot{...}` (sintaxis pgfplots).
/// Devuelve `None` si requiere revisión manual: el llamador emite un
/// comentario honesto en vez de romper la compilación del `.tex`.
fn tikz_math_expr(expression: &str) -> Option<String> {
    let trimmed = expression.trim().replace(['\n', '\r'], " ");
    if trimmed.is_empty() || trimmed.len() > grafito_core::validation::MAX_EXPR_LENGTH {
        return None;
    }
    let portable = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+-*/^().,!_ =<>|".contains(c));
    portable.then_some(trimmed)
}

fn resolve_math_binding(
    item: &ExportItem,
    variables: &[(String, f64)],
    expression: &Option<String>,
    fallback: f64,
    field: &str,
) -> std::result::Result<f64, ExportError> {
    const FORMAT: ExportFormat = ExportFormat::Tikz;
    let value = match expression {
        Some(expression) => {
            grafito_geometry::expr::evaluate(expression, variables).map_err(|error| {
                invalid_object(FORMAT, item, format!("{field} no se pudo evaluar: {error}"))
            })?
        }
        None => fallback,
    };
    if !value.is_finite() {
        return Err(invalid_object(
            FORMAT,
            item,
            format!("{field} produjo un valor no finito"),
        ));
    }
    Ok(value)
}

/// Escritor del modo TikZ math: coordenadas del mundo dentro de un `axis` de
/// pgfplots (editable), a diferencia de `serialize_tikz` (réplica en pt).
struct TikzMathWriter {
    tex: String,
    variables: Vec<(String, f64)>,
    bounds: AABB,
}

impl TikzMathWriter {
    const FORMAT: ExportFormat = ExportFormat::Tikz;

    fn fallback_comment(&mut self, item: &ExportItem, detail: &str) {
        use std::fmt::Write as _;
        writeln!(
            self.tex,
            "% {} '{}': {detail} (ver tikz=visual).",
            item.object_type,
            item.label.replace(['\n', '\r'], " ")
        )
        .ok();
    }

    fn invalid(&self, item: &ExportItem, reason: &str) -> ExportError {
        invalid_object(Self::FORMAT, item, reason)
    }

    fn emit_function(
        &mut self,
        item: &ExportItem,
        function: &grafito_core::FunctionObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        let min = resolve_math_binding(
            item,
            &self.variables,
            &function.domain_min_expr,
            function.domain_min.unwrap_or(self.bounds.min.x),
            "Function.domain_min_expr",
        )?;
        let max = resolve_math_binding(
            item,
            &self.variables,
            &function.domain_max_expr,
            function.domain_max.unwrap_or(self.bounds.max.x),
            "Function.domain_max_expr",
        )?;
        if min >= max {
            return Err(self.invalid(item, "el dominio evaluado debe ser creciente"));
        }
        let visible_min = min.max(self.bounds.min.x);
        let visible_max = max.min(self.bounds.max.x);
        if visible_min >= visible_max {
            return Ok(());
        }
        let Some(expr) = tikz_math_expr(&function.expr) else {
            self.fallback_comment(item, "expresion no portable a pgfplots; revisar sintaxis");
            return Ok(());
        };
        let (Some(lo), Some(hi)) = (math_num(visible_min), math_num(visible_max)) else {
            return Err(self.invalid(item, "el dominio visible no es representable"));
        };
        let stroke = validate_stroke(Self::FORMAT, item, function.width, function.color)?;
        if function.fill_color.is_some() {
            self.fallback_comment(item, "nota: relleno hasta y=0 solo en tikz=visual");
        }
        writeln!(
            self.tex,
            "% Function '{}': y = {expr}",
            item.label.replace(['\n', '\r'], " ")
        )
        .ok();
        writeln!(
            self.tex,
            "\\addplot[domain={lo}:{hi}, samples=200, draw={}, draw opacity={:.5}, line width={:.3}pt] {{{expr}}};",
            tikz_color(stroke.color),
            stroke.color.a,
            stroke.width
        )
        .ok();
        Ok(())
    }

    fn emit_circle(
        &mut self,
        item: &ExportItem,
        circle: &grafito_core::CircleObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        let radius = resolve_math_binding(
            item,
            &self.variables,
            &circle.radius_expr,
            circle.radius,
            "Circle.radius_expr",
        )?;
        if radius <= 0.0 {
            return Err(self.invalid(item, "el radio evaluado debe ser positivo"));
        }
        let stroke = validate_stroke(Self::FORMAT, item, circle.width, circle.color)?;
        let fill = validate_fill(Self::FORMAT, item, circle.fill_color)?;
        let (Some(cx), Some(cy), Some(radius)) = (
            math_num(circle.center.x),
            math_num(circle.center.y),
            math_num(radius),
        ) else {
            return Err(self.invalid(item, "el centro o el radio no es representable"));
        };
        let fill_option = fill.map_or_else(
            || "fill=none".to_string(),
            |color| format!("fill={},fill opacity={:.5}", tikz_color(color), color.a),
        );
        writeln!(
            self.tex,
            "\\filldraw[draw={},draw opacity={:.5},line width={:.3}pt,{fill_option}] ({cx},{cy}) circle ({radius});",
            tikz_color(stroke.color),
            stroke.color.a,
            stroke.width
        )
        .ok();
        Ok(())
    }

    fn emit_point(
        &mut self,
        item: &ExportItem,
        point: &grafito_core::PointObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        let x = resolve_math_binding(
            item,
            &self.variables,
            &point.x_expr,
            point.position.x,
            "Point.x_expr",
        )?;
        let y = resolve_math_binding(
            item,
            &self.variables,
            &point.y_expr,
            point.position.y,
            "Point.y_expr",
        )?;
        if !point.size.is_finite() || point.size <= 0.0 || point.size > MAX_EXPORT_STYLE_PIXELS {
            return Err(self.invalid(item, "el tamano del marcador no es representable"));
        }
        if !color_is_valid(point.color) {
            return Err(self.invalid(item, "el color RGBA no es valido"));
        }
        let (Some(px), Some(py)) = (math_num(x), math_num(y)) else {
            return Err(self.invalid(item, "la posicion no es representable"));
        };
        let node = if point.label.trim().is_empty() {
            String::new()
        } else {
            format!(" node[above right]{{{}}}", escape_tikz(&point.label))
        };
        writeln!(
            self.tex,
            "\\filldraw[fill={},draw={}] ({px},{py}) circle ({:.3}pt){node};",
            tikz_color(point.color),
            tikz_color(point.color),
            point.size
        )
        .ok();
        Ok(())
    }

    fn emit_line(
        &mut self,
        item: &ExportItem,
        line: &grafito_core::LineObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        if line.kind != LineKind::Segment {
            self.fallback_comment(item, "semirrecta/recta infinita: usar tikz=visual");
            return Ok(());
        }
        let start = (
            resolve_math_binding(
                item,
                &self.variables,
                &line.start_x_expr,
                line.start.x,
                "Line.start_x_expr",
            )?,
            resolve_math_binding(
                item,
                &self.variables,
                &line.start_y_expr,
                line.start.y,
                "Line.start_y_expr",
            )?,
        );
        let end = (
            resolve_math_binding(
                item,
                &self.variables,
                &line.end_x_expr,
                line.end.x,
                "Line.end_x_expr",
            )?,
            resolve_math_binding(
                item,
                &self.variables,
                &line.end_y_expr,
                line.end.y,
                "Line.end_y_expr",
            )?,
        );
        if start == end {
            self.fallback_comment(item, "segmento degenerado sin longitud");
            return Ok(());
        }
        let stroke = validate_stroke(Self::FORMAT, item, line.width, line.color)?;
        let (Some(ax), Some(ay), Some(bx), Some(by)) = (
            math_num(start.0),
            math_num(start.1),
            math_num(end.0),
            math_num(end.1),
        ) else {
            return Err(self.invalid(item, "los extremos no son representables"));
        };
        writeln!(
            self.tex,
            "\\draw[draw={},draw opacity={:.5},line width={:.3}pt] ({ax},{ay}) -- ({bx},{by});",
            tikz_color(stroke.color),
            stroke.color.a,
            stroke.width
        )
        .ok();
        Ok(())
    }

    fn emit_polygon(
        &mut self,
        item: &ExportItem,
        polygon: &grafito_core::PolygonObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        if polygon.vertices.len() < 3 {
            return Err(self.invalid(item, "un poligono visible necesita al menos tres vertices"));
        }
        let mut corners = Vec::with_capacity(polygon.vertices.len());
        for (index, vertex) in polygon.vertices.iter().enumerate() {
            let x = resolve_math_binding(
                item,
                &self.variables,
                polygon.x_exprs.get(index).unwrap_or(&None),
                vertex.x,
                &format!("Polygon.x_exprs[{index}]"),
            )?;
            let y = resolve_math_binding(
                item,
                &self.variables,
                polygon.y_exprs.get(index).unwrap_or(&None),
                vertex.y,
                &format!("Polygon.y_exprs[{index}]"),
            )?;
            let (Some(px), Some(py)) = (math_num(x), math_num(y)) else {
                return Err(self.invalid(item, "un vertice no es representable"));
            };
            corners.push(format!("({px},{py})"));
        }
        let stroke = validate_stroke(Self::FORMAT, item, polygon.width, polygon.color)?;
        let fill = validate_fill(Self::FORMAT, item, polygon.fill_color)?;
        let fill_option = fill.map_or_else(
            || "fill=none".to_string(),
            |color| format!("fill={},fill opacity={:.5}", tikz_color(color), color.a),
        );
        writeln!(
            self.tex,
            "\\draw[draw={},draw opacity={:.5},line width={:.3}pt,{fill_option}] {} -- cycle;",
            tikz_color(stroke.color),
            stroke.color.a,
            stroke.width,
            corners.join(" -- ")
        )
        .ok();
        Ok(())
    }

    fn emit_text(
        &mut self,
        item: &ExportItem,
        text: &grafito_core::TextObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        let (Some(px), Some(py)) = (math_num(text.position.x), math_num(text.position.y)) else {
            return Err(self.invalid(item, "la posicion no es representable"));
        };
        if !text.font_size.is_finite()
            || text.font_size <= 0.0
            || text.font_size > MAX_EXPORT_STYLE_PIXELS
        {
            return Err(self.invalid(item, "el tamano de texto no es representable"));
        }
        if !color_is_valid(text.color) {
            return Err(self.invalid(item, "el color RGBA no es valido"));
        }
        writeln!(
            self.tex,
            "\\node[anchor=west,text={},text opacity={:.5},font=\\fontsize{{{:.3}}}{{{:.3}}}\\selectfont] at ({px},{py}) {{{}}};",
            tikz_color(text.color),
            text.color.a,
            text.font_size,
            text.font_size * 1.2,
            escape_tikz(&text.content)
        )
        .ok();
        Ok(())
    }

    fn emit_ellipse(
        &mut self,
        item: &ExportItem,
        ellipse: &grafito_core::EllipseObj,
    ) -> std::result::Result<(), ExportError> {
        use std::fmt::Write as _;
        let (Some(cx), Some(cy), Some(rx), Some(ry)) = (
            math_num(ellipse.center.x),
            math_num(ellipse.center.y),
            math_num(ellipse.rx),
            math_num(ellipse.ry),
        ) else {
            return Err(self.invalid(item, "el centro o los semiejes no son representables"));
        };
        if !ellipse.rx.is_finite()
            || !ellipse.ry.is_finite()
            || ellipse.rx <= 0.0
            || ellipse.ry <= 0.0
            || !ellipse.angle.is_finite()
        {
            return Err(self.invalid(item, "los semiejes deben ser finitos y positivos"));
        }
        let stroke = validate_stroke(Self::FORMAT, item, ellipse.width, ellipse.color)?;
        let fill = validate_fill(Self::FORMAT, item, ellipse.fill_color)?;
        let fill_option = fill.map_or_else(
            || "fill=none".to_string(),
            |color| format!(",fill={},fill opacity={:.5}", tikz_color(color), color.a),
        );
        writeln!(
            self.tex,
            "\\draw[draw={},draw opacity={:.5},line width={:.3}pt{fill_option},rotate around={{{:.2}:({cx},{cy})}}] ({cx},{cy}) ellipse ({rx} and {ry});",
            tikz_color(stroke.color),
            stroke.color.a,
            stroke.width,
            ellipse.angle.to_degrees()
        )
        .ok();
        Ok(())
    }

    /// Vuelca `Document.whiteboard` en coordenadas del mundo. Los elementos
    /// inválidos se omiten en silencio, igual que en `append_whiteboard`.
    fn emit_whiteboard(&mut self, document: &Document) {
        use std::fmt::Write as _;
        let elements = document.whiteboard.elements().to_vec();
        if elements.is_empty() {
            return;
        }
        let item = ExportItem {
            object_type: "Whiteboard".to_string(),
            label: "pizarra".to_string(),
            object_id: "whiteboard".to_string(),
        };
        let mut included = 0usize;
        for element in &elements {
            if self.emit_whiteboard_element(&item, element) {
                included += 1;
            }
        }
        if included > 0 {
            writeln!(self.tex, "% Whiteboard: {included} elementos").ok();
        }
    }

    fn emit_whiteboard_element(&mut self, item: &ExportItem, element: &WhiteboardElement) -> bool {
        use std::fmt::Write as _;
        let shape_color = whiteboard_rgb_to_color((26, 26, 26));
        match element {
            WhiteboardElement::Stroke {
                points,
                color,
                width,
            } => {
                let Ok(stroke) = validate_stroke(
                    Self::FORMAT,
                    item,
                    *width as f32,
                    whiteboard_rgb_to_color(*color),
                ) else {
                    return false;
                };
                let nodes = points
                    .iter()
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .filter_map(|(x, y)| {
                        math_num(*x).and_then(|px| math_num(*y).map(|py| (px, py)))
                    })
                    .map(|(px, py)| format!("({px},{py})"))
                    .collect::<Vec<_>>();
                if nodes.len() < 2 {
                    return false;
                }
                writeln!(
                    self.tex,
                    "\\draw[draw={},draw opacity={:.5},line width={:.3}pt,line cap=round,line join=round] {};",
                    tikz_color(stroke.color),
                    stroke.color.a,
                    stroke.width,
                    nodes.join(" -- ")
                )
                .ok();
                true
            }
            WhiteboardElement::Rectangle { min, max, fill } => {
                if !finite_whiteboard_pair(*min) || !finite_whiteboard_pair(*max) {
                    return false;
                }
                let Ok(stroke) = validate_stroke(Self::FORMAT, item, 1.8, shape_color) else {
                    return false;
                };
                let Ok(fill) = validate_fill(Self::FORMAT, item, fill.map(whiteboard_rgb_to_color))
                else {
                    return false;
                };
                let (Some(x0), Some(y0), Some(x1), Some(y1)) = (
                    math_num(min.0.min(max.0)),
                    math_num(min.1.min(max.1)),
                    math_num(min.0.max(max.0)),
                    math_num(min.1.max(max.1)),
                ) else {
                    return false;
                };
                if x0 == x1 || y0 == y1 {
                    return false;
                }
                let fill_option = fill.map_or_else(
                    || "fill=none".to_string(),
                    |color| format!(",fill={},fill opacity={:.5}", tikz_color(color), color.a),
                );
                writeln!(
                    self.tex,
                    "\\draw[draw={},draw opacity={:.5},line width={:.3}pt{fill_option}] ({x0},{y0}) rectangle ({x1},{y1});",
                    tikz_color(stroke.color),
                    stroke.color.a,
                    stroke.width
                )
                .ok();
                true
            }
            WhiteboardElement::Ellipse { center, rx, ry } => {
                if !finite_whiteboard_pair(*center)
                    || !rx.is_finite()
                    || !ry.is_finite()
                    || *rx <= 0.0
                    || *ry <= 0.0
                {
                    return false;
                }
                let Ok(stroke) = validate_stroke(Self::FORMAT, item, 1.8, shape_color) else {
                    return false;
                };
                let (Some(cx), Some(cy), Some(rx), Some(ry)) = (
                    math_num(center.0),
                    math_num(center.1),
                    math_num(*rx),
                    math_num(*ry),
                ) else {
                    return false;
                };
                writeln!(
                    self.tex,
                    "\\draw[draw={},draw opacity={:.5},line width={:.3}pt] ({cx},{cy}) ellipse ({rx} and {ry});",
                    tikz_color(stroke.color),
                    stroke.color.a,
                    stroke.width
                )
                .ok();
                true
            }
            WhiteboardElement::Arrow { from, to } => {
                if !finite_whiteboard_pair(*from) || !finite_whiteboard_pair(*to) || from == to {
                    return false;
                }
                let Ok(stroke) = validate_stroke(Self::FORMAT, item, 1.8, shape_color) else {
                    return false;
                };
                let (Some(ax), Some(ay), Some(bx), Some(by)) = (
                    math_num(from.0),
                    math_num(from.1),
                    math_num(to.0),
                    math_num(to.1),
                ) else {
                    return false;
                };
                writeln!(
                    self.tex,
                    "\\draw[->,draw={},draw opacity={:.5},line width={:.3}pt] ({ax},{ay}) -- ({bx},{by});",
                    tikz_color(stroke.color),
                    stroke.color.a,
                    stroke.width
                )
                .ok();
                true
            }
            WhiteboardElement::Text { at, text, size } => {
                if text.is_empty() || !finite_whiteboard_pair(*at) {
                    return false;
                }
                if !size.is_finite() || *size <= 0.0 || *size as f32 > MAX_EXPORT_STYLE_PIXELS {
                    return false;
                }
                let (Some(px), Some(py)) = (math_num(at.0), math_num(at.1)) else {
                    return false;
                };
                writeln!(
                    self.tex,
                    "\\node[anchor=west,text={},font=\\fontsize{{{:.3}}}{{{:.3}}}\\selectfont] at ({px},{py}) {{{}}};",
                    tikz_color(shape_color),
                    *size as f32,
                    *size as f32 * 1.2,
                    escape_tikz(text)
                )
                .ok();
                true
            }
        }
    }
}

/// Serializa la escena en modo TikZ math: standalone compilable con pgfplots,
/// editable en coordenadas del mundo. Requiere la escena ya construida (los
/// presupuestos —dimensiones, unidades, bytes— se aplican igual que en visual).
fn serialize_tikz_math(
    document: &Document,
    scene: &ExportScene,
    options: ExportOptions,
) -> std::result::Result<Vec<u8>, ExportError> {
    use std::fmt::Write as _;

    const FORMAT: ExportFormat = ExportFormat::Tikz;
    let view = ExportView::new(document, options, FORMAT)?;
    let (Some(xmin), Some(xmax), Some(ymin), Some(ymax)) = (
        math_num(view.bounds.min.x),
        math_num(view.bounds.max.x),
        math_num(view.bounds.min.y),
        math_num(view.bounds.max.y),
    ) else {
        return Err(ExportError::Encoding {
            format: FORMAT,
            reason: "los limites visibles no son representables en TikZ".to_string(),
        });
    };

    let mut writer = TikzMathWriter {
        tex: String::with_capacity(scene.scene_units.saturating_mul(28).min(4_000_000)),
        variables: sorted_document_variables(document),
        bounds: view.bounds,
    };
    writer.tex.push_str("\\documentclass{standalone}\n");
    writer.tex.push_str("\\usepackage[utf8]{inputenc}\n");
    writer.tex.push_str("\\usepackage{xcolor}\n");
    writer.tex.push_str("\\usepackage{tikz}\n");
    writer.tex.push_str("\\usepackage{pgfplots}\n");
    writer.tex.push_str("\\pgfplotsset{compat=1.18}\n");
    writeln!(
        writer.tex,
        "% grafito tikz-mode={} (editable; visual=replica exacta en pt)",
        TikzMode::Math.as_str()
    )
    .ok();
    writer.tex.push_str("\\begin{document}\n");
    writer.tex.push_str("\\begin{tikzpicture}\n");
    writeln!(
        writer.tex,
        "\\begin{{axis}}[xmin={xmin}, xmax={xmax}, ymin={ymin}, ymax={ymax}, axis lines=middle, axis equal image, enlargelimits=false]"
    )
    .ok();

    let mut visible: Vec<(ObjectId, &GeoObject)> = document
        .objects_iter()
        .filter(|(_, object)| object.is_visible())
        .map(|(id, object)| (*id, object))
        .collect();
    visible.sort_unstable_by_key(|(id, object)| (grafito_render::scene_layer_2d(object), *id));
    for (_, object) in &visible {
        let item = ExportItem::from_object(object);
        match object {
            GeoObject::Function(function) => writer.emit_function(&item, function)?,
            GeoObject::Circle(circle) => writer.emit_circle(&item, circle)?,
            GeoObject::Point(point) => writer.emit_point(&item, point)?,
            GeoObject::Line(line) => writer.emit_line(&item, line)?,
            GeoObject::Polygon(polygon) => writer.emit_polygon(&item, polygon)?,
            GeoObject::Text(text) => writer.emit_text(&item, text)?,
            GeoObject::Ellipse(ellipse) => writer.emit_ellipse(&item, ellipse)?,
            _ => writer.fallback_comment(&item, "sin equivalente matematico directo"),
        }
    }
    writer.emit_whiteboard(document);

    writer
        .tex
        .push_str("\\end{axis}\n\\end{tikzpicture}\n\\end{document}\n");
    Ok(writer.tex.into_bytes())
}

fn tiny_skia_color(color: Color) -> tiny_skia::Color {
    if let Some(valid) = tiny_skia::Color::from_rgba(color.r, color.g, color.b, color.a) {
        valid
    } else {
        debug_assert!(false, "scene colors were validated");
        tiny_skia::Color::BLACK
    }
}

fn tiny_skia_path(points: &[ScreenPoint], closed: bool) -> Option<tiny_skia::Path> {
    let first = points.first()?;
    let mut builder = tiny_skia::PathBuilder::new();
    builder.move_to(first.x as f32, first.y as f32);
    for point in &points[1..] {
        builder.line_to(point.x as f32, point.y as f32);
    }
    if closed {
        builder.close();
    }
    builder.finish()
}

fn render_png(
    scene: &ExportScene,
    format: ExportFormat,
) -> std::result::Result<Vec<u8>, ExportError> {
    let mut pixmap = tiny_skia::Pixmap::new(scene.width, scene.height).ok_or_else(|| {
        ExportError::ResourceLimit {
            format,
            resource: "pixeles del lienzo",
            attempted: u64::from(scene.width) * u64::from(scene.height),
            limit: MAX_EXPORT_PIXELS,
            object: None,
        }
    })?;
    pixmap.fill(tiny_skia::Color::WHITE);

    let font_definitions = egui::FontDefinitions::default();
    let font_data = font_definitions
        .font_data
        .get("Ubuntu-Light")
        .ok_or_else(|| ExportError::Encoding {
            format,
            reason: "la fuente integrada Ubuntu-Light no esta disponible".to_string(),
        })?;
    let font = ab_glyph::FontRef::try_from_slice(font_data.font.as_ref()).map_err(|error| {
        ExportError::Encoding {
            format,
            reason: format!("la fuente integrada no es valida: {error}"),
        }
    })?;

    for object in &scene.objects {
        for primitive in &object.primitives {
            match primitive {
                ScenePrimitive::Path {
                    points,
                    closed,
                    stroke,
                    fill,
                } => {
                    let Some(path) = tiny_skia_path(points, *closed) else {
                        continue;
                    };
                    if let Some(fill) = fill {
                        let mut paint = tiny_skia::Paint::default();
                        paint.set_color(tiny_skia_color(*fill));
                        paint.anti_alias = true;
                        pixmap.fill_path(
                            &path,
                            &paint,
                            tiny_skia::FillRule::Winding,
                            tiny_skia::Transform::identity(),
                            None,
                        );
                    }
                    if let Some(stroke) = stroke {
                        let mut paint = tiny_skia::Paint::default();
                        paint.set_color(tiny_skia_color(stroke.color));
                        paint.anti_alias = true;
                        let path_stroke = tiny_skia::Stroke {
                            width: stroke.width,
                            line_cap: tiny_skia::LineCap::Round,
                            line_join: tiny_skia::LineJoin::Round,
                            ..Default::default()
                        };
                        pixmap.stroke_path(
                            &path,
                            &paint,
                            &path_stroke,
                            tiny_skia::Transform::identity(),
                            None,
                        );
                    }
                }
                ScenePrimitive::Text {
                    position,
                    content,
                    font_size,
                    color,
                } => render_text_to_pixmap(
                    &mut pixmap,
                    &font,
                    *position,
                    content,
                    *font_size,
                    *color,
                    format,
                    &object.item,
                )?,
            }
        }
    }
    pixmap.encode_png().map_err(|error| ExportError::Encoding {
        format,
        reason: error.to_string(),
    })
}

#[allow(clippy::too_many_arguments)]
fn render_text_to_pixmap(
    pixmap: &mut tiny_skia::Pixmap,
    font: &ab_glyph::FontRef<'_>,
    position: ScreenPoint,
    content: &str,
    font_size: f32,
    color: Color,
    format: ExportFormat,
    item: &ExportItem,
) -> std::result::Result<(), ExportError> {
    use ab_glyph::{point, Font as _, ScaleFont as _};

    let scale = ab_glyph::PxScale::from(font_size);
    let scaled = font.as_scaled(scale);
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).max(font_size);
    let mut caret_x = position.x as f32;
    let mut baseline_y = position.y as f32 + scaled.ascent() * 0.5;
    let line_start = caret_x;
    let mut previous = None;

    for character in content.chars() {
        if character == '\n' {
            caret_x = line_start;
            baseline_y += line_height;
            previous = None;
            continue;
        }
        let glyph_id = font.glyph_id(character);
        if glyph_id.0 == 0 && character != '\0' {
            return Err(invalid_object(
                format,
                item,
                format!("la fuente PNG no contiene el caracter {character:?}"),
            ));
        }
        if let Some(previous_id) = previous {
            caret_x += scaled.kern(previous_id, glyph_id);
        }
        let glyph = glyph_id.with_scale_and_position(scale, point(caret_x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            let origin_x = bounds.min.x.floor() as i64;
            let origin_y = bounds.min.y.floor() as i64;
            outlined.draw(|x, y, coverage| {
                let x = origin_x + i64::from(x);
                let y = origin_y + i64::from(y);
                if x >= 0
                    && y >= 0
                    && x < i64::from(pixmap.width())
                    && y < i64::from(pixmap.height())
                {
                    blend_opaque_pixel(pixmap, x as u32, y as u32, color, coverage);
                }
            });
        }
        caret_x += scaled.h_advance(glyph_id);
        previous = Some(glyph_id);
    }
    Ok(())
}

fn blend_opaque_pixel(pixmap: &mut tiny_skia::Pixmap, x: u32, y: u32, color: Color, coverage: f32) {
    let index = y as usize * pixmap.width() as usize + x as usize;
    let destination = pixmap.pixels()[index];
    let alpha = (color.a * coverage.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    let blend = |source: f32, destination: u8| -> u8 {
        (source.mul_add(alpha, f32::from(destination) * (1.0 - alpha)))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    let Some(pixel) = tiny_skia::PremultipliedColorU8::from_rgba(
        blend(color.r * 255.0, destination.red()),
        blend(color.g * 255.0, destination.green()),
        blend(color.b * 255.0, destination.blue()),
        255,
    ) else {
        debug_assert!(false, "opaque RGB is premultiplied");
        return;
    };
    pixmap.pixels_mut()[index] = pixel;
}

/// Exporta usando el tamano actual del lienzo del documento.
pub(crate) fn export_document(
    document: &Document,
    format: ExportFormat,
    path: impl AsRef<Path>,
) -> std::result::Result<ExportReport, ExportError> {
    let options = ExportOptions::from_document(document, format)?;
    export_document_with_options(document, format, path, options)
}

/// Valida el presupuesto de salida y reemplaza atómicamente el destino.
/// Toda exportación verificable pasa por aquí: ningún error escribe a medias.
fn finish_export(
    format: ExportFormat,
    scene: &ExportScene,
    bytes: Vec<u8>,
    path: &Path,
) -> std::result::Result<ExportReport, ExportError> {
    if bytes.len() > MAX_EXPORT_OUTPUT_BYTES {
        return Err(ExportError::ResourceLimit {
            format,
            resource: "bytes codificados",
            attempted: bytes.len() as u64,
            limit: MAX_EXPORT_OUTPUT_BYTES as u64,
            object: None,
        });
    }
    write_file_atomic(path, &bytes).map_err(|source| ExportError::Io {
        format,
        path: path.to_path_buf(),
        source,
    })?;
    Ok(ExportReport {
        format,
        path: path.to_path_buf(),
        exported_objects: scene.objects.len(),
        hidden_objects: scene.hidden_objects,
        primitive_count: scene.primitive_count(),
        object_types: scene.object_types.clone(),
    })
}

/// Construye y valida toda la salida antes de reemplazar atomicamente el destino.
pub(crate) fn export_document_with_options(
    document: &Document,
    format: ExportFormat,
    path: impl AsRef<Path>,
    options: ExportOptions,
) -> std::result::Result<ExportReport, ExportError> {
    let path = path.as_ref();
    let scene = build_export_scene(document, format, options)?;
    let bytes = match format {
        ExportFormat::Svg => serialize_svg(&scene),
        ExportFormat::Png => render_png(&scene, format)?,
        ExportFormat::Tikz => serialize_tikz(&scene),
    };
    finish_export(format, &scene, bytes, path)
}

/// Exporta TikZ en el modo indicado (`tikz=visual|math`); `Visual` conserva
/// el comportamiento histórico (réplica exacta en pt).
pub(crate) fn export_document_with_tikz_mode(
    document: &Document,
    path: impl AsRef<Path>,
    options: ExportOptions,
    mode: TikzMode,
) -> std::result::Result<ExportReport, ExportError> {
    let path = path.as_ref();
    let scene = build_export_scene(document, ExportFormat::Tikz, options)?;
    let bytes = match mode {
        TikzMode::Visual => serialize_tikz(&scene),
        TikzMode::Math => serialize_tikz_math(document, &scene, options)?,
    };
    finish_export(ExportFormat::Tikz, &scene, bytes, path)
}

pub(crate) fn export_svg(
    document: &Document,
    path: impl AsRef<Path>,
) -> std::result::Result<ExportReport, ExportError> {
    export_document(document, ExportFormat::Svg, path)
}

pub(crate) fn export_png(
    document: &Document,
    path: impl AsRef<Path>,
) -> std::result::Result<ExportReport, ExportError> {
    export_document(document, ExportFormat::Png, path)
}

pub(crate) fn export_tikz(
    document: &Document,
    path: impl AsRef<Path>,
) -> std::result::Result<ExportReport, ExportError> {
    export_tikz_with_mode(document, path, TikzMode::Visual)
}

/// Exporta TikZ en el modo indicado (`tikz=visual|math`).
pub(crate) fn export_tikz_with_mode(
    document: &Document,
    path: impl AsRef<Path>,
    mode: TikzMode,
) -> std::result::Result<ExportReport, ExportError> {
    let options = ExportOptions::from_document(document, ExportFormat::Tikz)?;
    export_document_with_tikz_mode(document, path, options, mode)
}

fn pdf_failure(reason: impl Into<String>) -> String {
    reason.into()
}

fn map_core_exchange_error(context: &'static str, error: ExchangeError) -> String {
    match error {
        ExchangeError::TooManyObjects { got } => format!(
            "{context} no reemplazó el destino; {got} objetos exceden el límite {MAX_EXCHANGE_OBJECTS}"
        ),
        ExchangeError::InvalidData { feature, detail } => {
            format!("{context} no reemplazó el destino; dato inválido en {feature}: {detail}")
        }
        ExchangeError::NotImplemented { feature, hint } => {
            format!("{context} no reemplazó el destino; {feature} pendiente: {hint}")
        }
    }
}

/// Exporta el PDF interino de 1 página (conteo + etiquetas, sin geometría
/// inventada). Puro + escritura atómica: ningún error toca el destino.
/// Devuelve `(path, summary)` como el canal de `PendingExportJob`.
pub(crate) fn export_pdf(
    document: &Document,
    path: impl AsRef<Path>,
) -> Result<(PathBuf, String), String> {
    let path = path.as_ref();
    let bytes = document_to_pdf(document)
        .map_err(|error| pdf_failure(map_core_exchange_error("PDF", error)))?;
    if bytes.len() > MAX_EXPORT_OUTPUT_BYTES {
        return Err(pdf_failure(format!(
            "PDF no reemplazó el destino; {} bytes exceden el límite {MAX_EXPORT_OUTPUT_BYTES}",
            bytes.len()
        )));
    }
    write_file_atomic(path, &bytes).map_err(|error| {
        pdf_failure(format!("PDF no pudo escribir {}: {error}", path.display()))
    })?;
    let total = document.objects_iter_sorted().count();
    let hidden = document
        .objects_iter()
        .filter(|(_, object)| !object.is_visible())
        .count();
    Ok((
        path.to_path_buf(),
        format!(
            "PDF exportado: {total} objetos ({hidden} ocultos) -> {}",
            path.display()
        ),
    ))
}

/// Spawns PDF en background — mismo contrato que `spawn_export` en `app.rs`
/// (canal `Result<(PathBuf, String), String>` apto para `PendingExportJob`).
/// Render + write van al worker; el summary se publica en `poll_background_jobs`.
pub(crate) fn spawn_pdf_export(
    document: Document,
    path: PathBuf,
    ctx: &egui::Context,
) -> Receiver<Result<(PathBuf, String), String>> {
    let ctx = egui::Context::clone(ctx);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("pdf-export".into())
        .spawn(move || {
            let _ = tx.send(export_pdf(&document, &path));
            ctx.request_repaint();
        });
    rx
}

/// Texto CSV RFC 4180 de una tabla viva del documento (puro, sin I/O).
/// Devuelve `(etiqueta, csv)`; el write va al worker vía [`spawn_csv_export`].
pub(crate) fn datatable_csv_text(
    document: &Document,
    table: ObjectId,
) -> Result<(String, String), String> {
    let object = document.get_object(table).ok_or_else(|| {
        "CSV no reemplazó el destino; la tabla ya no está en el documento".to_string()
    })?;
    let data = match object {
        GeoObject::DataTable(data) => data,
        other => {
            return Err(format!(
                "CSV no reemplazó el destino; '{}' no es una tabla de datos (es {})",
                other.label(),
                other.name()
            ));
        }
    };
    let csv = datatable_to_csv(data).map_err(|error| map_core_exchange_error("CSV", error))?;
    Ok((data.label.clone(), csv))
}

/// Spawns escritura CSV en background — canal apto para `PendingExportJob`;
/// el summary lleva etiqueta + filas reales del texto generado.
pub(crate) fn spawn_csv_export(
    csv_text: String,
    label: String,
    path: PathBuf,
    ctx: &egui::Context,
) -> Receiver<Result<(PathBuf, String), String>> {
    let ctx = egui::Context::clone(ctx);
    let rows = csv_text.lines().count().saturating_sub(1);
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let _ = std::thread::Builder::new()
        .name("csv-export".into())
        .spawn(move || {
            let result = write_text_atomic(&path, &csv_text)
                .map(|()| {
                    let shown = if label.is_empty() { "tabla" } else { &label };
                    (
                        path.clone(),
                        format!(
                            "Tabla '{shown}' exportada a CSV ({rows} filas) -> {}",
                            path.display()
                        ),
                    )
                })
                .map_err(|error| {
                    format!(
                        "CSV no reemplazó el destino; no pudo escribir {}: {error}",
                        path.display()
                    )
                });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    rx
}

/// Tallo seguro para `set_file_name` del diálogo (etiqueta → `[A-Za-z0-9_-]`,
/// máx. 64; fallback `"tabla"`). Puro y testeado.
pub(crate) fn sanitize_export_stem(raw: &str) -> String {
    let stem: String = raw
        .chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '-')
        .take(64)
        .collect();
    if stem.is_empty() {
        "tabla".to_string()
    } else {
        stem
    }
}

/// Portapapeles PNG honesto: el core exige raster (`image`/`tiny-skia` en
/// app, fuera del frente F10-C), así que hoy siempre es `Unavailable` con
/// destino intacto. Mantiene viva la variante para el mensaje honesto.
pub(crate) fn clipboard_png_honest() -> Result<Vec<u8>, ExportError> {
    clipboard_png_stub().map_err(|error| match error {
        ExchangeError::NotImplemented { feature, hint } => ExportError::Unavailable {
            feature: "Portapapeles PNG",
            reason: format!("{feature}: {hint}"),
        },
        other => ExportError::Encoding {
            format: ExportFormat::Png,
            reason: other.to_string(),
        },
    })
}

pub(crate) fn write_text_atomic(path: impl AsRef<Path>, text: &str) -> io::Result<()> {
    write_file_atomic(path.as_ref(), text.as_bytes())
}

fn write_file_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_file_atomic_with(path, |file| file.write_all(bytes))
}

fn write_file_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "export destination must include a file name",
        )
    })?;
    let (mut temporary_file, temporary_path) = create_export_temporary_file(parent, file_name)?;
    let write_result: io::Result<()> = (|| {
        write(&mut temporary_file)?;
        temporary_file.sync_all()?;
        #[cfg(unix)]
        {
            apply_export_destination_permissions(&temporary_file, path)?;
            temporary_file.sync_all()?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    drop(temporary_file);
    if let Err(error) = replace_export_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn apply_export_destination_permissions(
    temporary_file: &File,
    destination: &Path,
) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Do not expose a partially written export with the destination's broader mode.
    let mode = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => metadata.permissions().mode() & 0o777,
        Ok(_) => 0o600,
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0o600,
        Err(error) => return Err(error),
    };
    temporary_file.set_permissions(fs::Permissions::from_mode(mode))
}

fn create_export_temporary_file(
    parent: &Path,
    file_name: &std::ffi::OsStr,
) -> io::Result<(File, PathBuf)> {
    for _ in 0..16 {
        let id = NEXT_EXPORT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let temporary_path = parent.join(format!(
            ".{}-{}-{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            id
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        match options.open(&temporary_path) {
            Ok(file) => return Ok((file, temporary_path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a unique temporary export file",
    ))
}

#[cfg(not(windows))]
fn replace_export_file(temporary_path: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary_path, destination)
}

#[cfg(windows)]
fn replace_export_file(temporary_path: &Path, destination: &Path) -> io::Result<()> {
    if destination.try_exists()? {
        replace_existing_export_file_windows(temporary_path, destination)
    } else {
        fs::rename(temporary_path, destination)
    }
}

#[cfg(windows)]
fn replace_existing_export_file_windows(
    temporary_path: &Path,
    destination: &Path,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn path_as_wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows paths cannot contain an interior NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    let destination = path_as_wide(destination)?;
    let replacement = path_as_wide(temporary_path)?;
    // SAFETY: both buffers are validated NUL-terminated UTF-16 paths and stay
    // alive for the complete synchronous ReplaceFileW call.
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Export the document as SVG (basic implementation).
#[cfg(test)]
fn legacy_export_svg(doc: &Document, width: f64, height: f64) -> String {
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}">"#,
        width, height, 0, 0, width, height
    );
    svg.push_str(r#"<rect width="100%" height="100%" fill="white"/>"#);

    let view = doc.view();
    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        let c = obj.color();
        let rgb = format!(
            "rgb({},{},{})",
            (c.r * 255.0).clamp(0.0, 255.0) as u8,
            (c.g * 255.0).clamp(0.0, 255.0) as u8,
            (c.b * 255.0).clamp(0.0, 255.0) as u8
        );
        match obj {
            grafito_core::GeoObject::Point(p) => {
                let screen = view.world_to_screen(p.position);
                svg.push_str(&format!(
                    r#"<circle cx="{:.1}" cy="{:.1}" r="{}" fill="{}"/>"#,
                    screen.x, screen.y, p.size, rgb
                ));
            }
            grafito_core::GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    grafito_core::LineKind::Segment => (l.start, l.end),
                    _ => {
                        let world_tl = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                        let world_br =
                            view.screen_to_world(glam::Vec2::new(width as f32, height as f32));
                        let mut view_bounds = grafito_geometry::AABB::new(world_tl, world_tl);
                        view_bounds.expand(&world_br);
                        l.clip_to_aabb(view_bounds).unwrap_or((l.start, l.end))
                    }
                };
                let sa = view.world_to_screen(a);
                let sb = view.world_to_screen(b);
                svg.push_str(&format!(
                    r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.1}"/>"#,
                    sa.x, sa.y, sb.x, sb.y, rgb, l.width
                ));
            }
            grafito_core::GeoObject::Circle(c) => {
                let center = view.world_to_screen(c.center);
                let r = c.radius * view.scale;
                svg.push_str(&format!(
                    r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                    center.x, center.y, r, rgb, c.width
                ));
            }
            grafito_core::GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let points: Vec<String> = poly
                    .vertices
                    .iter()
                    .map(|v| {
                        let s = view.world_to_screen(*v);
                        format!("{:.1},{:.1}", s.x, s.y)
                    })
                    .collect();
                svg.push_str(&format!(
                    r#"<polygon points="{}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                    points.join(" "),
                    rgb,
                    poly.width
                ));
            }
            grafito_core::GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let steps = 200;
                let dx = (x_max - x_min) / steps as f64;
                let mut path = String::new();
                for i in 0..=steps {
                    let x = x_min + i as f64 * dx;
                    if let Ok(y) =
                        grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                    {
                        if y.is_finite() {
                            let s = view.world_to_screen(grafito_geometry::Point2::new(x, y));
                            if i == 0 {
                                path.push_str(&format!("M{:.1},{:.1} ", s.x, s.y));
                            } else {
                                path.push_str(&format!("L{:.1},{:.1} ", s.x, s.y));
                            }
                        }
                    }
                }
                if !path.is_empty() {
                    svg.push_str(&format!(
                        r#"<path d="{}" fill="none" stroke="{}" stroke-width="{:.1}"/>"#,
                        path, rgb, f.width
                    ));
                }
            }
            grafito_core::GeoObject::Text(txt) => {
                let s = view.world_to_screen(txt.position);
                svg.push_str(&format!(
                    r#"<text x="{:.1}" y="{:.1}" fill="{}" font-size="{}">{}</text>"#,
                    s.x,
                    s.y,
                    rgb,
                    txt.font_size,
                    escape_xml(&txt.content)
                ));
            }
            _ => {}
        }
    }
    svg.push_str("</svg>");
    svg
}

/// Export the document as a PNG raster image using CPU-side primitive rendering.
#[cfg(test)]
fn legacy_export_png(doc: &Document, width: u32, height: u32, path: &str) -> AnyResult<()> {
    validate_png_dimensions(width, height)?;
    let mut img: RgbaImage = ImageBuffer::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let view = doc.view();

    let to_screen = |wp: Point2| -> Option<(f64, f64)> {
        let s = view.world_to_screen(wp);
        let (x, y) = (f64::from(s.x), f64::from(s.y));
        (x.is_finite() && y.is_finite()).then_some((x, y))
    };

    let to_color = |c: grafito_geometry::Color| -> Rgba<u8> {
        Rgba([
            (c.r * 255.0).clamp(0.0, 255.0) as u8,
            (c.g * 255.0).clamp(0.0, 255.0) as u8,
            (c.b * 255.0).clamp(0.0, 255.0) as u8,
            255,
        ])
    };

    // Draw axes
    if let Some((ax, ay)) = to_screen(Point2::new(0.0, 0.0)) {
        draw_line(
            &mut img,
            ax,
            0.0,
            ax,
            f64::from(height),
            Rgba([180, 180, 180, 255]),
        );
        draw_line(
            &mut img,
            0.0,
            ay,
            f64::from(width),
            ay,
            Rgba([180, 180, 180, 255]),
        );
    }

    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        let color = to_color(obj.color());
        match obj {
            GeoObject::Point(p) => {
                if let Some((px, py)) = to_screen(p.position) {
                    draw_circle_filled(&mut img, px, py, f64::from(p.size), color);
                }
            }
            GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    LineKind::Segment => (l.start, l.end),
                    _ => {
                        let wt = view.screen_to_world(glam::Vec2::new(0.0, 0.0));
                        let wb = view.screen_to_world(glam::Vec2::new(width as f32, height as f32));
                        let mut bounds = grafito_geometry::AABB::new(wt, wt);
                        bounds.expand(&wb);
                        l.clip_to_aabb(bounds).unwrap_or((l.start, l.end))
                    }
                };
                if let (Some((x1, y1)), Some((x2, y2))) = (to_screen(a), to_screen(b)) {
                    draw_line(&mut img, x1, y1, x2, y2, color);
                }
            }
            GeoObject::Circle(c) => {
                if let Some((cx, cy)) = to_screen(c.center) {
                    draw_circle_outline(&mut img, cx, cy, c.radius * view.scale, color);
                }
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                for i in 0..poly.vertices.len() {
                    if let (Some((x1, y1)), Some((x2, y2))) = (
                        to_screen(poly.vertices[i]),
                        to_screen(poly.vertices[(i + 1) % poly.vertices.len()]),
                    ) {
                        draw_line(&mut img, x1, y1, x2, y2, color);
                    }
                }
            }
            GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let steps = 500;
                let dx = (x_max - x_min) / steps as f64;
                let mut prev: Option<(f64, f64)> = None;
                for i in 0..=steps {
                    let x = x_min + i as f64 * dx;
                    if let Ok(y) =
                        grafito_geometry::expr::evaluate(&f.expr, &[("x".to_string(), x)])
                    {
                        if y.is_finite() {
                            if let Some((sx, sy)) = to_screen(Point2::new(x, y)) {
                                if let Some((px, py)) = prev {
                                    draw_line(&mut img, px, py, sx, sy, color);
                                }
                                prev = Some((sx, sy));
                            } else {
                                prev = None;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    img.save(path).context("Failed to write PNG file")?;
    Ok(())
}

/// Export the document as LaTeX (tikz/pgfplots) code.
#[cfg(test)]
fn legacy_export_latex(doc: &Document) -> String {
    let mut tex = String::new();
    tex.push_str("\\documentclass{standalone}\n");
    tex.push_str("\\usepackage{tikz}\n");
    tex.push_str("\\usepackage{pgfplots}\n");
    tex.push_str("\\pgfplotsset{compat=1.18}\n");
    tex.push_str("\\begin{document}\n");
    tex.push_str("\\begin{tikzpicture}\n");

    // Coordinate transform: world → tikz (just pass world coords directly,
    // tikz uses mathematical coordinates natively)
    for (_, obj) in doc.objects_iter() {
        if !obj.is_visible() {
            continue;
        }
        match obj {
            GeoObject::Point(p) => {
                let label = escape_latex(&p.label);
                tex.push_str(&format!(
                    "\\filldraw ({:.4},{:.4}) circle (2pt) node[above right]{{{}}};\n",
                    p.position.x, p.position.y, label
                ));
            }
            GeoObject::Line(l) => {
                let (a, b) = match l.kind {
                    LineKind::Segment => (l.start, l.end),
                    _ => (l.start, l.end),
                };
                let cmd = match l.kind {
                    LineKind::Segment => "--",
                    LineKind::Ray => "--",
                    LineKind::Line => "--",
                };
                tex.push_str(&format!(
                    "\\draw ({:.4},{:.4}) {} ({:.4},{:.4});\n",
                    a.x, a.y, cmd, b.x, b.y
                ));
            }
            GeoObject::Circle(c) => {
                tex.push_str(&format!(
                    "\\draw ({:.4},{:.4}) circle ({:.4});\n",
                    c.center.x, c.center.y, c.radius
                ));
            }
            GeoObject::Polygon(poly) if poly.vertices.len() >= 3 => {
                let pts: Vec<String> = poly
                    .vertices
                    .iter()
                    .map(|v| format!("({:.4},{:.4})", v.x, v.y))
                    .collect();
                tex.push_str(&format!("\\draw {} -- cycle;\n", pts.join(" -- ")));
            }
            GeoObject::Function(f) => {
                let x_min = f.domain_min.unwrap_or(-10.0);
                let x_max = f.domain_max.unwrap_or(10.0);
                let expr = escape_latex(&f.expr);
                tex.push_str(&format!(
                    "\\begin{{axis}}[xmin={:.2}, xmax={:.2}, axis lines=middle]\n",
                    x_min, x_max
                ));
                tex.push_str(&format!(
                    "\\addplot[domain={:.2}:{:.2}, samples=200] {{{}}};\n",
                    x_min, x_max, expr
                ));
                tex.push_str("\\end{axis}\n");
            }
            GeoObject::Ellipse(e) => {
                tex.push_str(&format!(
                    "\\draw[rotate around={{{:.2}deg:({:.4},{:.4})}}] ({:.4},{:.4}) ellipse ({:.4} and {:.4});\n",
                    e.angle.to_degrees(),
                    e.center.x, e.center.y,
                    e.center.x, e.center.y,
                    e.rx, e.ry
                ));
            }
            GeoObject::Text(txt) => {
                let content = escape_latex(&txt.content);
                tex.push_str(&format!(
                    "\\node at ({:.4},{:.4}) {{{}}};\n",
                    txt.position.x, txt.position.y, content
                ));
            }
            _ => {}
        }
    }

    tex.push_str("\\end{tikzpicture}\n");
    tex.push_str("\\end{document}\n");
    tex
}

#[cfg(test)]
fn escape_latex(s: &str) -> String {
    s.replace('\\', r"\textbackslash{}")
        .replace('{', r"\{")
        .replace('}', r"\}")
        .replace('$', r"\$")
        .replace('&', r"\&")
        .replace('#', r"\#")
        .replace('^', r"\textasciicircum{}")
        .replace('_', r"\_")
        .replace('~', r"\textasciitilde{}")
        .replace('%', r"\%")
}

#[cfg(test)]
fn draw_line(img: &mut RgbaImage, x0: f64, y0: f64, x1: f64, y1: f64, color: Rgba<u8>) {
    let Some((x0, y0, x1, y1)) = clip_line_to_image(img, x0, y0, x1, y1) else {
        return;
    };
    let max_x = i32::try_from(img.width() - 1).expect("PNG dimensions fit in i32");
    let max_y = i32::try_from(img.height() - 1).expect("PNG dimensions fit in i32");
    let (Some(x0), Some(y0), Some(x1), Some(y1)) = (
        bounded_pixel_coordinate(x0, max_x),
        bounded_pixel_coordinate(y0, max_y),
        bounded_pixel_coordinate(x1, max_x),
        bounded_pixel_coordinate(y1, max_y),
    ) else {
        return;
    };
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        img.put_pixel(x as u32, y as u32, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
fn draw_circle_filled(img: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: Rgba<u8>) {
    if !cx.is_finite() || !cy.is_finite() || !radius.is_finite() || radius < 0.0 {
        return;
    }

    let cx = cx.trunc();
    let cy = cy.trunc();
    let radius = radius.trunc();
    let max_x = f64::from(img.width() - 1);
    let max_y = f64::from(img.height() - 1);
    let min_x = (cx - radius).ceil().clamp(0.0, max_x);
    let max_x = (cx + radius).floor().clamp(0.0, max_x);
    let min_y = (cy - radius).ceil().clamp(0.0, max_y);
    let max_y = (cy + radius).floor().clamp(0.0, max_y);
    if min_x > max_x || min_y > max_y {
        return;
    }

    let min_x = min_x as u32;
    let max_x = max_x as u32;
    let min_y = min_y as u32;
    let max_y = max_y as u32;
    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let dx = f64::from(px) - cx;
            let dy = f64::from(py) - cy;
            if dx.mul_add(dx, dy * dy) <= radius * radius {
                img.put_pixel(px, py, color);
            }
        }
    }
}

#[cfg(test)]
fn draw_circle_outline(img: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: Rgba<u8>) {
    if !cx.is_finite() || !cy.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let cx = cx.trunc();
    let cy = cy.trunc();
    let radius = radius.trunc();
    if radius <= 0.0 || !circle_outline_intersects_image(img, cx, cy, radius) {
        return;
    }

    if radius > MAX_MIDPOINT_CIRCLE_RADIUS {
        draw_large_circle_outline(img, cx, cy, radius, color);
        return;
    }

    let max_x = i32::try_from(img.width() - 1).expect("PNG dimensions fit in i32");
    let max_y = i32::try_from(img.height() - 1).expect("PNG dimensions fit in i32");
    let Some(cx) = bounded_i32_coordinate(cx, -radius, f64::from(max_x) + radius) else {
        return;
    };
    let Some(cy) = bounded_i32_coordinate(cy, -radius, f64::from(max_y) + radius) else {
        return;
    };
    let r = radius as i32;
    let mut x = r;
    let mut y = 0;
    let mut err = 0;
    while x >= y {
        plot4(img, cx, cy, x, y, color);
        plot4(img, cx, cy, y, x, color);
        y += 1;
        if err <= 0 {
            err += 2 * y + 1;
        }
        if err > 0 {
            x -= 1;
            err -= 2 * x + 1;
        }
    }
}

#[cfg(test)]
fn plot4(img: &mut RgbaImage, cx: i32, cy: i32, dx: i32, dy: i32, color: Rgba<u8>) {
    for &(px, py) in &[
        (cx + dx, cy + dy),
        (cx - dx, cy + dy),
        (cx + dx, cy - dy),
        (cx - dx, cy - dy),
    ] {
        if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
            img.put_pixel(px as u32, py as u32, color);
        }
    }
}

#[cfg(test)]
fn clip_line_to_image(
    img: &RgbaImage,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> Option<(f64, f64, f64, f64)> {
    if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
        return None;
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let mut enter: f64 = 0.0;
    let mut exit: f64 = 1.0;
    let max_x = f64::from(img.width() - 1);
    let max_y = f64::from(img.height() - 1);
    for (p, q) in [(-dx, x0), (dx, max_x - x0), (-dy, y0), (dy, max_y - y0)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > exit {
                    return None;
                }
                enter = enter.max(t);
            } else {
                if t < enter {
                    return None;
                }
                exit = exit.min(t);
            }
        }
    }

    Some((
        x0 + enter * dx,
        y0 + enter * dy,
        x0 + exit * dx,
        y0 + exit * dy,
    ))
}

#[cfg(test)]
fn bounded_pixel_coordinate(value: f64, max: i32) -> Option<i32> {
    bounded_i32_coordinate(value, 0.0, f64::from(max))
}

#[cfg(test)]
fn bounded_i32_coordinate(value: f64, min: f64, max: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    Some(value.trunc().clamp(min, max) as i32)
}

#[cfg(test)]
fn circle_outline_intersects_image(img: &RgbaImage, cx: f64, cy: f64, radius: f64) -> bool {
    let max_x = f64::from(img.width() - 1);
    let max_y = f64::from(img.height() - 1);
    let closest_x = cx.clamp(0.0, max_x);
    let closest_y = cy.clamp(0.0, max_y);
    let min_distance = (cx - closest_x).hypot(cy - closest_y);
    let max_distance = [(0.0, 0.0), (max_x, 0.0), (0.0, max_y), (max_x, max_y)]
        .into_iter()
        .map(|(x, y)| (cx - x).hypot(cy - y))
        .fold(0.0, f64::max);
    min_distance <= radius && radius <= max_distance
}

#[cfg(test)]
fn draw_large_circle_outline(img: &mut RgbaImage, cx: f64, cy: f64, radius: f64, color: Rgba<u8>) {
    for px in 0..img.width() {
        let dx = f64::from(px) - cx;
        if let Some(offset) = circle_offset(radius, dx) {
            plot_finite_pixel(img, f64::from(px), cy + offset, color);
            plot_finite_pixel(img, f64::from(px), cy - offset, color);
        }
    }
    for py in 0..img.height() {
        let dy = f64::from(py) - cy;
        if let Some(offset) = circle_offset(radius, dy) {
            plot_finite_pixel(img, cx + offset, f64::from(py), color);
            plot_finite_pixel(img, cx - offset, f64::from(py), color);
        }
    }
}

#[cfg(test)]
fn circle_offset(radius: f64, delta: f64) -> Option<f64> {
    if delta.abs() > radius {
        return None;
    }
    let ratio = delta / radius;
    Some(radius * (1.0 - ratio * ratio).max(0.0).sqrt())
}

#[cfg(test)]
fn plot_finite_pixel(img: &mut RgbaImage, x: f64, y: f64, color: Rgba<u8>) {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return;
    }
    let max_x = i32::try_from(img.width() - 1).expect("PNG dimensions fit in i32");
    let max_y = i32::try_from(img.height() - 1).expect("PNG dimensions fit in i32");
    if x > f64::from(max_x) || y > f64::from(max_y) {
        return;
    }
    let x = bounded_pixel_coordinate(x, max_x).expect("finite coordinate was checked");
    let y = bounded_pixel_coordinate(y, max_y).expect("finite coordinate was checked");
    img.put_pixel(x as u32, y as u32, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{Document, GeoObject, PointObj};

    #[test]
    fn test_export_latex_point_and_line() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(1.0, 2.0));
        doc.add_object(GeoObject::Point(p));
        let tex = legacy_export_latex(&doc);
        assert!(tex.contains("\\documentclass{standalone}"));
        assert!(tex.contains("\\begin{tikzpicture}"));
        assert!(tex.contains("(1.0000,2.0000)"));
        assert!(tex.contains("\\end{tikzpicture}"));
        assert!(tex.contains("\\end{document}"));
    }

    #[test]
    fn test_export_latex_escape() {
        assert_eq!(escape_latex("100%"), "100\\%");
        assert_eq!(escape_latex("a_b"), "a\\_b");
    }

    #[test]
    fn test_export_svg_basic() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(0.0, 0.0));
        doc.add_object(GeoObject::Point(p));
        let svg = legacy_export_svg(&doc, 800.0, 600.0);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_export_png_no_panic() {
        let mut doc = Document::new();
        let p = PointObj::new(grafito_geometry::Point2::new(1.0, 1.0));
        doc.add_object(GeoObject::Point(p));
        let path = std::env::temp_dir().join("grafito_test_export.png");
        let result = legacy_export_png(&doc, 200, 200, path.to_str().unwrap());
        assert!(result.is_ok(), "export_png failed: {result:?}");
        assert!(path.exists());
    }

    #[test]
    fn png_default_dimensions_are_valid() {
        assert!(validate_png_dimensions(1280, 720).is_ok());
    }

    #[test]
    fn png_rejects_zero_dimensions() {
        assert!(validate_png_dimensions(0, 720).is_err());
        assert!(validate_png_dimensions(1280, 0).is_err());
    }

    #[test]
    fn png_rejects_oversized_dimensions() {
        assert!(validate_png_dimensions(MAX_PNG_DIMENSION + 1, 1).is_err());
    }

    #[test]
    fn png_rejects_oversized_or_overflowing_pixel_counts() {
        assert!(validate_png_dimensions(MAX_PNG_DIMENSION, MAX_PNG_DIMENSION).is_err());
        assert!(validate_png_dimensions(u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn export_png_rejects_invalid_dimensions_before_creating_a_file() {
        let doc = Document::new();
        let path =
            std::env::temp_dir().join(format!("grafito_invalid_export_{}.png", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let result = legacy_export_png(&doc, 0, 720, path.to_str().unwrap());

        assert!(result.is_err());
        assert!(!path.exists());
    }

    #[test]
    fn raster_line_clips_extreme_coordinates_before_bresenham() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let mut img = ImageBuffer::from_pixel(8, 8, white);

        draw_line(&mut img, -1.0e12, 4.0, 1.0e12, 4.0, black);

        assert_eq!(*img.get_pixel(0, 4), black);
        assert_eq!(*img.get_pixel(7, 4), black);
    }

    #[test]
    fn raster_line_skips_nonintersecting_or_nonfinite_segments() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let mut img = ImageBuffer::from_pixel(8, 8, white);

        draw_line(&mut img, -1.0e12, -1.0e12, -1.0e11, -1.0e11, black);
        draw_line(&mut img, f64::NAN, 0.0, 1.0, 1.0, black);

        assert!(img.pixels().all(|pixel| *pixel == white));
    }

    #[test]
    fn filled_point_clips_bounds_and_skips_invalid_centers() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let mut img = ImageBuffer::from_pixel(8, 8, white);

        draw_circle_filled(&mut img, 4.0, 4.0, 1.0e12, black);
        assert!(img.pixels().all(|pixel| *pixel == black));

        let mut skipped = ImageBuffer::from_pixel(8, 8, white);
        draw_circle_filled(&mut skipped, f64::NAN, 4.0, 2.0, black);
        draw_circle_filled(&mut skipped, 1.0e12, 1.0e12, 2.0, black);
        assert!(skipped.pixels().all(|pixel| *pixel == white));
    }

    #[test]
    fn large_circle_outline_uses_bounded_screen_sampling() {
        let white = Rgba([255, 255, 255, 255]);
        let black = Rgba([0, 0, 0, 255]);
        let mut img = ImageBuffer::from_pixel(8, 8, white);

        draw_circle_outline(&mut img, 1.0e12, 4.0, 1.0e12, black);
        assert_eq!(*img.get_pixel(0, 4), black);

        let mut enclosed = ImageBuffer::from_pixel(8, 8, white);
        draw_circle_outline(&mut enclosed, 4.0, 4.0, 1.0e12, black);
        draw_circle_outline(&mut enclosed, f64::INFINITY, 4.0, 1.0, black);
        assert!(enclosed.pixels().all(|pixel| *pixel == white));
    }

    fn temp_export_path(name: &str) -> std::path::PathBuf {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "grafito_export_{name}_{}_{}",
            std::process::id(),
            id
        ))
    }

    fn common_2d_document() -> Document {
        use grafito_core::{
            BoxPlotObj, CircleObj, EllipseObj, FunctionObj, HistogramObj, HyperbolaObj,
            ImplicitCurveObj, LineObj, ParabolaObj, ParametricCurve2DObj, PhasePortraitObj,
            PolarCurveObj, PolygonObj, RegressionLineObj, ScatterPlotObj, TextObj,
            VectorField2DObj,
        };
        use grafito_core::{PencilObj, RelationOperator};

        let mut document = Document::new();
        document.view_mut().screen_size = glam::Vec2::new(320.0, 240.0);
        document.view_mut().scale = 20.0;
        let objects = [
            GeoObject::Point(PointObj::new(Point2::new(1.0, 1.0))),
            GeoObject::Line(LineObj::new_with_kind(
                Point2::new(-1.0, -1.0),
                Point2::new(1.0, 1.0),
                LineKind::Line,
            )),
            GeoObject::Circle(CircleObj::new(Point2::new(0.0, 0.0), 2.0)),
            GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(-2.0, -1.0),
                Point2::new(-1.0, 1.0),
                Point2::new(0.0, -1.0),
            ])),
            GeoObject::Pencil(PencilObj::new(vec![
                Point2::new(-3.0, 0.0),
                Point2::new(-2.0, 1.0),
                Point2::new(-1.0, 0.0),
            ])),
            GeoObject::Function(FunctionObj::new("x^2")),
            GeoObject::Text(TextObj::new("Grafito", Point2::new(0.0, 2.0))),
            GeoObject::Ellipse(EllipseObj::new(Point2::new(2.0, 0.0), 1.5, 0.75)),
            GeoObject::Parabola(ParabolaObj::new(Point2::new(0.0, -2.0), 1.0)),
            GeoObject::Hyperbola(HyperbolaObj::new(Point2::new(0.0, 0.0), 1.0, 0.5)),
            GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
                "2*cos(t)",
                "2*sin(t)",
                0.0,
                std::f64::consts::TAU,
            )),
            GeoObject::PolarCurve(PolarCurveObj::new(
                "1+0.5*cos(3*t)",
                0.0,
                std::f64::consts::TAU,
            )),
            GeoObject::ImplicitCurve(ImplicitCurveObj::new("x^2+y^2", "4", RelationOperator::Eq)),
            GeoObject::VectorField2D(VectorField2DObj::new("-y", "x")),
            GeoObject::Histogram(HistogramObj::new(vec![1.0, 1.5, 2.0, 2.5], 2)),
            GeoObject::ScatterPlot(ScatterPlotObj::new(
                vec![-1.0, 0.0, 1.0],
                vec![1.0, 0.0, 1.0],
            )),
            GeoObject::BoxPlot(BoxPlotObj::new(vec![1.0, 2.0, 3.0, 4.0, 8.0])),
            GeoObject::RegressionLine(RegressionLineObj::linear(
                vec![-1.0, 0.0, 1.0],
                vec![-1.0, 1.0, 3.0],
                2.0,
                1.0,
                1.0,
            )),
            GeoObject::PhasePortrait(PhasePortraitObj::new("-y", "x", -2.0, 2.0, -2.0, 2.0)),
        ];
        for object in objects {
            document
                .try_add_object(object)
                .expect("common export fixture must be valid");
        }
        document
    }

    fn enum_variant_names(source: &str) -> std::collections::BTreeSet<String> {
        let body = source
            .split_once("pub enum GeoObject {")
            .expect("GeoObject enum declaration")
            .1
            .split_once("\n}")
            .expect("GeoObject enum body")
            .0;
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"))
            .filter_map(|line| {
                line.split_once('(')
                    .map(|(name, _)| name.trim().to_string())
            })
            .collect()
    }

    #[test]
    fn export_policy_inventory_matches_every_geo_object_variant() {
        let source = include_str!("../../grafito-core/src/object.rs");
        let model_variants = enum_variant_names(source);
        let policy_variants: std::collections::BTreeSet<_> = ExportObjectKind::ALL
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect();

        assert_eq!(policy_variants, model_variants);
        for format in ExportFormat::ALL {
            for kind in ExportObjectKind::ALL {
                let _policy: ExportSupport = format.support_for(kind);
            }
        }
    }

    #[test]
    fn all_formats_share_the_declared_common_2d_support_matrix() {
        let supported: std::collections::BTreeSet<_> = [
            "Point",
            "Line",
            "Circle",
            "Polygon",
            "Pencil",
            "Function",
            "Text",
            "Ellipse",
            "Parabola",
            "Hyperbola",
            "ParametricCurve2D",
            "PolarCurve",
            "ImplicitCurve",
            "VectorField2D",
            "Histogram",
            "ScatterPlot",
            "BoxPlot",
            "RegressionLine",
            "PhasePortrait",
        ]
        .into_iter()
        .collect();

        for format in ExportFormat::ALL {
            let actual: std::collections::BTreeSet<_> = ExportObjectKind::ALL
                .iter()
                .filter(|kind| format.support_for(**kind) == ExportSupport::Supported)
                .map(|kind| kind.as_str())
                .collect();
            assert_eq!(actual, supported, "wrong matrix for {format}");
        }
    }

    #[test]
    fn supported_families_have_content_and_counts_in_each_format() {
        let document = common_2d_document();
        let options = ExportOptions::new(320, 240);

        for format in ExportFormat::ALL {
            let path = temp_export_path(format.extension());
            let report = export_document_with_options(&document, format, &path, options)
                .expect("all common 2D families should export");
            assert_eq!(report.exported_objects, 19);
            assert_eq!(report.object_types.len(), 19);
            assert!(report.primitive_count > 19);

            let bytes = std::fs::read(&path).expect("export should exist");
            assert!(!bytes.is_empty());
            match format {
                ExportFormat::Svg => {
                    let content = String::from_utf8(bytes).expect("SVG is UTF-8");
                    for object_type in [
                        "Point",
                        "Pencil",
                        "Ellipse",
                        "ParametricCurve2D",
                        "PolarCurve",
                        "ImplicitCurve",
                        "Histogram",
                        "ScatterPlot",
                        "BoxPlot",
                        "RegressionLine",
                        "PhasePortrait",
                    ] {
                        assert!(
                            content.contains(&format!("data-grafito-type=\"{object_type}\"")),
                            "missing SVG content for {object_type}"
                        );
                    }
                    assert!(content.contains(">Grafito</text>"));
                }
                ExportFormat::Png => {
                    let image =
                        image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
                            .expect("PNG should decode")
                            .to_rgba8();
                    assert_eq!(image.dimensions(), (320, 240));
                    assert!(image.pixels().any(|pixel| pixel.0 != [255, 255, 255, 255]));
                }
                ExportFormat::Tikz => {
                    let content = String::from_utf8(bytes).expect("TikZ is UTF-8");
                    assert!(content.contains("\\begin{tikzpicture}"));
                    assert!(content.contains("% Histogram"));
                    assert!(content.contains("% ImplicitCurve"));
                    assert!(content.contains("Grafito"));
                }
            }
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn svg_golden_uses_export_view_clipping_and_exact_object_style() {
        use grafito_core::LineObj;

        let mut document = Document::new();
        document.view_mut().screen_size = glam::Vec2::new(100.0, 100.0);
        document.view_mut().scale = 10.0;
        let mut line = LineObj::new_with_kind(
            Point2::new(-1.0, 0.0),
            Point2::new(1.0, 0.0),
            LineKind::Line,
        );
        line.color = grafito_geometry::Color::new(0.2, 0.4, 0.9, 0.5);
        line.width = 3.25;
        document
            .try_add_object(GeoObject::Line(line))
            .expect("valid styled line");
        let path = temp_export_path("svg");

        export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(100, 100),
        )
        .expect("styled line should export");
        let svg = std::fs::read_to_string(&path).expect("read SVG");

        assert!(svg.contains("M 0.000 50.000 L 100.000 50.000"));
        assert!(svg.contains("stroke=\"rgb(51,102,230)\""));
        assert!(svg.contains("stroke-opacity=\"0.5000\""));
        assert!(svg.contains("stroke-width=\"3.250\""));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn unsupported_visible_objects_are_sorted_reported_and_never_replace_destination() {
        use grafito_core::{Cube3DObj, Point3DObj};
        use grafito_geometry::Point3D;

        let mut document = Document::new();
        let mut point = Point3DObj::new(Point3D::new(0.0, 0.0, 0.0));
        point.label = "zeta".to_string();
        document
            .try_add_object(GeoObject::Point3D(point))
            .expect("valid point");
        let mut cube = Cube3DObj::new(Point3D::new(0.0, 0.0, 0.0), 1.0);
        cube.label = "alpha".to_string();
        document
            .try_add_object(GeoObject::Cube3D(cube))
            .expect("valid cube");

        for format in ExportFormat::ALL {
            let path = temp_export_path(format.extension());
            std::fs::write(&path, b"existing destination").expect("write sentinel");
            let error = export_document_with_options(
                &document,
                format,
                &path,
                ExportOptions::new(320, 240),
            )
            .expect_err("3D objects must be rejected");
            let omitted = error.omitted_objects();
            assert_eq!(omitted.len(), 2);
            assert_eq!(omitted[0].object_type, "Cube3D");
            assert_eq!(omitted[0].label, "alpha");
            assert_eq!(omitted[1].object_type, "Point3D");
            assert_eq!(omitted[1].label, "zeta");
            assert_eq!(
                std::fs::read(&path).expect("sentinel remains"),
                b"existing destination"
            );
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn hidden_unsupported_objects_do_not_block_export_and_are_counted() {
        use grafito_core::Point3DObj;
        use grafito_geometry::Point3D;

        let mut document = Document::new();
        document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("valid point");
        let mut hidden = Point3DObj::new(Point3D::new(0.0, 0.0, 0.0));
        hidden.visible = false;
        document
            .try_add_object(GeoObject::Point3D(hidden))
            .expect("valid hidden point");
        let path = temp_export_path("svg");

        let report = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect("hidden unsupported object should be ignored");

        assert_eq!(report.exported_objects, 1);
        assert_eq!(report.hidden_objects, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_visible_geometry_preserves_every_existing_destination() {
        let mut document = Document::new();
        let id = document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("valid point");
        let GeoObject::Point(point) = document.get_object_mut(id).expect("point exists") else {
            panic!("expected point")
        };
        point.position.x = f64::NAN;

        for format in ExportFormat::ALL {
            let path = temp_export_path(format.extension());
            std::fs::write(&path, b"keep me").expect("write sentinel");
            let error = export_document_with_options(
                &document,
                format,
                &path,
                ExportOptions::new(320, 240),
            )
            .expect_err("non-finite point must fail");
            assert!(matches!(error, ExportError::InvalidObject { .. }));
            assert_eq!(std::fs::read(&path).expect("sentinel remains"), b"keep me");
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn everywhere_nonfinite_vector_and_phase_fields_are_not_reported_as_exported() {
        use grafito_core::{PhasePortraitObj, VectorField2DObj};

        for object in [
            GeoObject::VectorField2D(VectorField2DObj::new("1/0", "1/0")),
            GeoObject::PhasePortrait(PhasePortraitObj::new("1/0", "1/0", -1.0, 1.0, -1.0, 1.0)),
        ] {
            let mut document = Document::new();
            document
                .try_add_object(object)
                .expect("syntactically valid field should enter the fixture");
            let path = temp_export_path("svg");
            std::fs::write(&path, b"keep field destination").expect("write sentinel");

            let error = export_document_with_options(
                &document,
                ExportFormat::Svg,
                &path,
                ExportOptions::new(320, 240),
            )
            .expect_err("an everywhere non-finite field must be rejected");

            assert!(matches!(error, ExportError::InvalidObject { .. }));
            assert_eq!(
                std::fs::read(&path).expect("sentinel remains"),
                b"keep field destination"
            );
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn nonfinite_bound_expression_never_exports_stale_fallback_geometry() {
        let mut point = PointObj::new(Point2::new(3.0, 4.0));
        point.x_expr = Some("1/0".to_string());
        let mut document = Document::new();
        document
            .try_add_object(GeoObject::Point(point))
            .expect("syntactically valid bound expression should enter the fixture");
        let path = temp_export_path("svg");
        std::fs::write(&path, b"keep expression destination").expect("write sentinel");

        let error = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect_err("non-finite binding must not fall back to the stale point position");

        assert!(matches!(error, ExportError::InvalidObject { .. }));
        assert!(error.to_string().contains("x_expr"));
        assert_eq!(
            std::fs::read(&path).expect("sentinel remains"),
            b"keep expression destination"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn finite_zero_vector_and_phase_fields_remain_valid_empty_scenes() {
        use grafito_core::{PhasePortraitObj, VectorField2DObj};

        for object in [
            GeoObject::VectorField2D(VectorField2DObj::new("0", "0")),
            GeoObject::PhasePortrait(PhasePortraitObj::new("0", "0", -1.0, 1.0, -1.0, 1.0)),
        ] {
            let mut document = Document::new();
            document.try_add_object(object).expect("valid zero field");
            let path = temp_export_path("svg");

            let report = export_document_with_options(
                &document,
                ExportFormat::Svg,
                &path,
                ExportOptions::new(320, 240),
            )
            .expect("a finite zero field is mathematically empty, not invalid");

            assert_eq!(report.exported_objects, 1);
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn dimensions_and_scene_work_are_bounded_before_destination_replacement() {
        let document = Document::new();
        let path = temp_export_path("svg");
        std::fs::write(&path, b"bounded").expect("write sentinel");

        let error = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(MAX_EXPORT_DIMENSION + 1, 1),
        )
        .expect_err("oversized vector canvas must fail too");

        assert!(matches!(error, ExportError::ResourceLimit { .. }));
        assert_eq!(std::fs::read(&path).expect("sentinel remains"), b"bounded");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn aggregate_scene_budget_failure_preserves_existing_destination() {
        use grafito_core::PencilObj;

        let points = (0..8_000)
            .map(|index| Point2::new(index as f64 / 8_000.0 - 0.5, index as f64 % 2.0))
            .collect::<Vec<_>>();
        let mut document = Document::new();
        for _ in 0..32 {
            document
                .try_add_object(GeoObject::Pencil(PencilObj::new(points.clone())))
                .expect("bounded pencil fixture");
        }
        let path = temp_export_path("svg");
        std::fs::write(&path, b"scene budget sentinel").expect("write sentinel");

        let error = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect_err("aggregate scene must be bounded");

        assert!(matches!(error, ExportError::ResourceLimit { .. }));
        assert_eq!(
            std::fs::read(&path).expect("sentinel remains"),
            b"scene budget sentinel"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn injected_partial_write_failure_is_cleaned_and_preserves_destination() {
        use std::io::Write as _;

        let path = temp_export_path("atomic");
        std::fs::write(&path, b"original").expect("write sentinel");

        let result = write_file_atomic_with(&path, |file| {
            file.write_all(b"partial")?;
            Err(std::io::Error::other("injected failure"))
        });

        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).expect("sentinel remains"), b"original");
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_export_preserves_existing_destination_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_export_path("permissions");
        std::fs::write(&path, b"original").expect("write sentinel");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("restrict destination permissions");

        write_file_atomic(&path, b"replacement").expect("atomic export");

        let mode = std::fs::metadata(&path)
            .expect("destination metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
        let _ = std::fs::remove_file(path);
    }

    fn whiteboard_fixture_document() -> Document {
        let mut document = Document::new();
        document.view_mut().screen_size = glam::Vec2::new(320.0, 240.0);
        document.view_mut().scale = 20.0;
        document.whiteboard.add(WhiteboardElement::Text {
            at: (0.0, 1.0),
            text: "Hola pizarra".to_string(),
            size: 14.0,
        });
        document.whiteboard.add(WhiteboardElement::Stroke {
            points: vec![(-2.0, -1.0), (-1.0, 0.0), (0.0, -1.0)],
            color: (26, 26, 26),
            width: 2.0,
        });
        document
    }

    #[test]
    fn document_whiteboard_roundtrips_through_serde_json() {
        let document = whiteboard_fixture_document();
        let json = serde_json::to_string(&document).expect("la pizarra debe serializar");
        let restored: Document = serde_json::from_str(&json).expect("la pizarra debe deserializar");
        assert_eq!(
            restored.whiteboard.elements(),
            document.whiteboard.elements()
        );
    }

    #[test]
    fn export_svg_includes_whiteboard_text_as_text_element() {
        let document = whiteboard_fixture_document();
        let path = temp_export_path("svg");
        let report = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect("la pizarra debe exportar a SVG");
        assert_eq!(report.object_types.get("Whiteboard"), Some(&2));

        let svg = std::fs::read_to_string(&path).expect("read SVG");
        assert!(
            svg.contains("data-grafito-type=\"Whiteboard\""),
            "falta el grupo de pizarra"
        );
        assert!(svg.contains("<text"), "el texto debe ir como <text>");
        assert!(svg.contains("Hola pizarra"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn export_png_composites_whiteboard_over_scene() {
        let document = whiteboard_fixture_document();
        let path = temp_export_path("png");
        export_document_with_options(
            &document,
            ExportFormat::Png,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect("la pizarra debe exportar a PNG");

        let bytes = std::fs::read(&path).expect("export should exist");
        let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .expect("PNG should decode")
            .to_rgba8();
        assert_eq!(image.dimensions(), (320, 240));
        assert!(
            image.pixels().any(|pixel| pixel.0 != [255, 255, 255, 255]),
            "la pizarra debe componer píxeles sobre la escena"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_whiteboard_leaves_geometric_export_untouched() {
        let document = common_2d_document();
        assert!(document.whiteboard.is_empty());
        let path = temp_export_path("svg");
        let report = export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect("export sin pizarra");
        assert_eq!(report.exported_objects, 19);
        assert!(!report.object_types.contains_key("Whiteboard"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn svg_standalone_carries_title() {
        let document = common_2d_document();
        let path = temp_export_path("svg");
        export_document_with_options(
            &document,
            ExportFormat::Svg,
            &path,
            ExportOptions::new(320, 240),
        )
        .expect("export SVG");
        let svg = std::fs::read_to_string(&path).expect("read SVG");
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(
            svg.contains("<title>"),
            "el SVG standalone debe llevar <title>"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn tikz_mode_param_parsing() {
        assert_eq!(TikzMode::from_param("visual"), Some(TikzMode::Visual));
        assert_eq!(TikzMode::from_param(" math "), Some(TikzMode::Math));
        assert_eq!(TikzMode::from_param("MATH"), Some(TikzMode::Math));
        assert_eq!(TikzMode::from_param("pdf"), None);
        assert_eq!(TikzMode::from_param(""), None);
        assert_eq!(TikzMode::Visual.as_str(), "visual");
        assert_eq!(TikzMode::Math.as_str(), "math");
    }

    fn tikz_math_fixture_document() -> Document {
        use grafito_core::{CircleObj, FunctionObj};

        let mut document = Document::new();
        document.view_mut().screen_size = glam::Vec2::new(320.0, 240.0);
        document.view_mut().scale = 20.0;
        document
            .try_add_object(GeoObject::Function(FunctionObj::new("x^2")))
            .expect("funcion valida");
        document
            .try_add_object(GeoObject::Circle(CircleObj::new(
                Point2::new(0.0, 0.0),
                2.0,
            )))
            .expect("circulo valido");
        document
    }

    #[test]
    fn tikz_math_mode_is_editable_while_visual_is_a_pt_replica() {
        let document = tikz_math_fixture_document();
        let options = ExportOptions::new(320, 240);
        let math_path = temp_export_path("tex");
        let visual_path = temp_export_path("tex");

        export_document_with_tikz_mode(&document, &math_path, options, TikzMode::Math)
            .expect("modo math");
        export_document_with_tikz_mode(&document, &visual_path, options, TikzMode::Visual)
            .expect("modo visual");

        let math = std::fs::read_to_string(&math_path).expect("read math TikZ");
        let visual = std::fs::read_to_string(&visual_path).expect("read visual TikZ");
        assert!(
            math.contains("\\addplot"),
            "math debe emitir \\addplot para Function"
        );
        assert!(
            math.contains("\\filldraw"),
            "math debe emitir \\filldraw circle para Circle"
        );
        assert!(math.contains("\\begin{axis}"), "math usa pgfplots");
        assert!(
            math.contains("\\end{document}"),
            "math es standalone compilable"
        );
        assert!(
            !visual.contains("\\addplot"),
            "visual es replica en pt, sin pgfplots"
        );
        assert!(
            !visual.contains("\\filldraw"),
            "visual es replica en pt, sin primitivas matematicas"
        );
        assert!(
            visual.contains("\\begin{tikzpicture}"),
            "visual sigue siendo TikZ compilable"
        );
        let _ = std::fs::remove_file(math_path);
        let _ = std::fs::remove_file(visual_path);
    }

    #[test]
    fn tikz_math_mode_preserves_destination_on_invalid_geometry() {
        let mut document = Document::new();
        document.view_mut().screen_size = glam::Vec2::new(320.0, 240.0);
        let id = document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("punto valido");
        let GeoObject::Point(point) = document.get_object_mut(id).expect("el punto existe") else {
            panic!("se esperaba un punto")
        };
        point.position.x = f64::NAN;

        let path = temp_export_path("tex");
        std::fs::write(&path, b"keep me").expect("write sentinel");
        let error = export_document_with_tikz_mode(
            &document,
            &path,
            ExportOptions::new(320, 240),
            TikzMode::Math,
        )
        .expect_err("la geometria no finita debe fallar en math");
        assert!(matches!(error, ExportError::InvalidObject { .. }));
        assert_eq!(std::fs::read(&path).expect("sentinel remains"), b"keep me");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pdf_interim_writes_one_page_and_reports_counts() {
        let document = common_2d_document();
        let total = document.objects_iter_sorted().count();
        let path = temp_export_path("pdf");
        let (written, summary) = export_pdf(&document, &path).expect("PDF interino fixture");
        assert_eq!(written, path);
        assert!(
            summary.contains(&format!("{total} objetos")),
            "summary honesto esperado, fue: {summary}"
        );
        let bytes = std::fs::read(&path).expect("pdf escrito");
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pdf_failure_preserves_destination() {
        let document = common_2d_document();
        // Sin nombre de archivo el write atómico falla antes de tocar nada.
        let error = export_pdf(&document, "").expect_err("destino inválido debe fallar");
        assert!(
            error.contains("no pudo escribir"),
            "error honesto esperado, fue: {error}"
        );
    }

    #[test]
    fn datatable_csv_text_is_honest_rfc4180() {
        use grafito_core::DataTableObj;
        let mut document = Document::new();
        let id = document
            .try_add_object(GeoObject::DataTable(
                DataTableObj::new("x", "y", vec![1.0, 2.0], vec![3.0, 4.0])
                    .with_label("mediciones"),
            ))
            .expect("tabla fixture");
        let (label, csv) = datatable_csv_text(&document, id).expect("csv fixture");
        assert!(csv.starts_with("x,y\r\n"), "cabeza + CRLF, fue: {csv:?}");
        assert!(csv.contains("1,3\r\n"));
        assert_eq!(label, "mediciones");
        // Id inexistente y objeto no-tabla fallan honesto sin I/O.
        let missing =
            datatable_csv_text(&document, ObjectId::new()).expect_err("id ausente debe fallar");
        assert!(missing.contains("ya no está"));
        let point_id = document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("punto fixture");
        let not_table = datatable_csv_text(&document, point_id).expect_err("no-tabla debe fallar");
        assert!(not_table.contains("no es una tabla de datos"));
    }

    #[test]
    fn sanitize_export_stem_keeps_safe_chars_with_fallback() {
        assert_eq!(sanitize_export_stem("Mi tabla 2026!"), "Mitabla2026");
        assert_eq!(sanitize_export_stem("a-b_c"), "a-b_c");
        assert_eq!(sanitize_export_stem("!!!"), "tabla");
        assert_eq!(sanitize_export_stem(""), "tabla");
        assert!(!sanitize_export_stem("áé").is_empty());
    }

    #[test]
    fn clipboard_png_stays_honest_unavailable() {
        let error = clipboard_png_honest().expect_err("PNG pendiente");
        assert!(
            error.to_string().contains("no disponible en esta build"),
            "error honesto esperado, fue: {error}"
        );
    }
}
