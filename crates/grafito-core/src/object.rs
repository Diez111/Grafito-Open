use crate::id::ObjectId;
use crate::pencil::PencilObj;
use grafito_geometry::conformal::algebraic_mappings::ConformalMap;
use grafito_geometry::{Circle as GeomCircle, Color, Point2, Point3D, AABB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, RwLock};

/// A geometric object in the document (2D and 3D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeoObject {
    // 2D
    Point(PointObj),
    Line(LineObj),
    Circle(CircleObj),
    Polygon(PolygonObj),
    Pencil(PencilObj),
    Function(FunctionObj),
    Text(TextObj),
    Ellipse(EllipseObj),
    Parabola(ParabolaObj),
    Hyperbola(HyperbolaObj),
    // 3D
    Point3D(Point3DObj),
    Segment3D(Segment3DObj),
    Sphere3D(Sphere3DObj),
    Cube3D(Cube3DObj),
    Pyramid3D(Pyramid3DObj),
    Cone3D(Cone3DObj),
    Cylinder3D(Cylinder3DObj),
    Torus3D(Torus3DObj),
    MoebiusStrip(MoebiusStripObj),
    Surface3D(Surface3DObj),

    // AM2/AM3 Advanced
    ParametricCurve2D(ParametricCurve2DObj),
    ParametricCurve3D(ParametricCurve3DObj),
    PolarCurve(PolarCurveObj),
    ImplicitCurve(ImplicitCurveObj),
    VectorField2D(VectorField2DObj),
    ComplexGrid(ComplexGridObj),
    ComplexMapping(ComplexMappingObj),

    // AM4 Advanced: Attractors, Fractals, 4D, Statistics
    Attractor3D(Attractor3DObj),
    Fractal2D(Fractal2DObj),
    HyperSurface4D(HyperSurface4DObj),
    VectorField3D(VectorField3DObj),
    Histogram(HistogramObj),
    ScatterPlot(ScatterPlotObj),
    BoxPlot(BoxPlotObj),
    RegressionLine(RegressionLineObj),
    PhasePortrait(PhasePortraitObj),
}

impl GeoObject {
    pub fn id(&self) -> ObjectId {
        match self {
            GeoObject::Point(o) => o.id,
            GeoObject::Line(o) => o.id,
            GeoObject::Circle(o) => o.id,
            GeoObject::Polygon(o) => o.id,
            GeoObject::Function(o) => o.id,
            GeoObject::Text(o) => o.id,
            GeoObject::Ellipse(o) => o.id,
            GeoObject::Parabola(o) => o.id,
            GeoObject::Hyperbola(o) => o.id,
            GeoObject::Point3D(o) => o.id,
            GeoObject::Segment3D(o) => o.id,
            GeoObject::Sphere3D(o) => o.id,
            GeoObject::Cube3D(o) => o.id,
            GeoObject::Pyramid3D(o) => o.id,
            GeoObject::Cone3D(o) => o.id,
            GeoObject::Cylinder3D(o) => o.id,
            GeoObject::Torus3D(o) => o.id,
            GeoObject::MoebiusStrip(o) => o.id,
            GeoObject::Surface3D(o) => o.id,
            GeoObject::ParametricCurve2D(o) => o.id,
            GeoObject::ParametricCurve3D(o) => o.id,
            GeoObject::PolarCurve(o) => o.id,
            GeoObject::VectorField2D(o) => o.id,
            GeoObject::ComplexGrid(o) => o.id,
            GeoObject::ComplexMapping(o) => o.id,
            GeoObject::ImplicitCurve(o) => o.id,
            GeoObject::Attractor3D(o) => o.id,
            GeoObject::Fractal2D(o) => o.id,
            GeoObject::HyperSurface4D(o) => o.id,
            GeoObject::VectorField3D(o) => o.id,
            GeoObject::Histogram(o) => o.id,
            GeoObject::ScatterPlot(o) => o.id,
            GeoObject::BoxPlot(o) => o.id,
            GeoObject::RegressionLine(o) => o.id,
            GeoObject::PhasePortrait(o) => o.id,
            GeoObject::Pencil(o) => o.id,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            GeoObject::Point(o) => &o.label,
            GeoObject::Line(o) => &o.label,
            GeoObject::Circle(o) => &o.label,
            GeoObject::Polygon(o) => &o.label,
            GeoObject::Function(o) => &o.label,
            GeoObject::Text(o) => &o.label,
            GeoObject::Ellipse(o) => &o.label,
            GeoObject::Parabola(o) => &o.label,
            GeoObject::Hyperbola(o) => &o.label,
            GeoObject::Point3D(o) => &o.label,
            GeoObject::Segment3D(o) => &o.label,
            GeoObject::Sphere3D(o) => &o.label,
            GeoObject::Cube3D(o) => &o.label,
            GeoObject::Pyramid3D(o) => &o.label,
            GeoObject::Cone3D(o) => &o.label,
            GeoObject::Cylinder3D(o) => &o.label,
            GeoObject::Torus3D(o) => &o.label,
            GeoObject::MoebiusStrip(o) => &o.label,
            GeoObject::Surface3D(o) => &o.label,
            GeoObject::ParametricCurve2D(o) => &o.label,
            GeoObject::ParametricCurve3D(o) => &o.label,
            GeoObject::PolarCurve(o) => &o.label,
            GeoObject::VectorField2D(o) => &o.label,
            GeoObject::ComplexGrid(o) => &o.label,
            GeoObject::ComplexMapping(o) => &o.label,
            GeoObject::ImplicitCurve(o) => &o.label,
            GeoObject::Attractor3D(o) => &o.label,
            GeoObject::Fractal2D(o) => &o.label,
            GeoObject::HyperSurface4D(o) => &o.label,
            GeoObject::VectorField3D(o) => &o.label,
            GeoObject::Histogram(o) => &o.label,
            GeoObject::ScatterPlot(o) => &o.label,
            GeoObject::BoxPlot(o) => &o.label,
            GeoObject::RegressionLine(o) => &o.label,
            GeoObject::PhasePortrait(o) => &o.label,
            GeoObject::Pencil(o) => &o.label,
        }
    }

    pub fn set_label(&mut self, label: String) {
        match self {
            GeoObject::Point(o) => o.label = label,
            GeoObject::Line(o) => o.label = label,
            GeoObject::Circle(o) => o.label = label,
            GeoObject::Polygon(o) => o.label = label,
            GeoObject::Function(o) => o.label = label,
            GeoObject::Text(o) => o.label = label,
            GeoObject::Ellipse(o) => o.label = label,
            GeoObject::Parabola(o) => o.label = label,
            GeoObject::Hyperbola(o) => o.label = label,
            GeoObject::Point3D(o) => o.label = label,
            GeoObject::Segment3D(o) => o.label = label,
            GeoObject::Sphere3D(o) => o.label = label,
            GeoObject::Cube3D(o) => o.label = label,
            GeoObject::Pyramid3D(o) => o.label = label,
            GeoObject::Cone3D(o) => o.label = label,
            GeoObject::Cylinder3D(o) => o.label = label,
            GeoObject::Torus3D(o) => o.label = label.clone(),
            GeoObject::MoebiusStrip(o) => o.label = label.clone(),
            GeoObject::Surface3D(o) => o.label = label,
            GeoObject::ParametricCurve2D(o) => o.label = label,
            GeoObject::ParametricCurve3D(o) => o.label = label,
            GeoObject::PolarCurve(o) => o.label = label,
            GeoObject::VectorField2D(o) => o.label = label,
            GeoObject::ComplexGrid(o) => o.label = label,
            GeoObject::ComplexMapping(o) => o.label = label,
            GeoObject::ImplicitCurve(o) => o.label = label,
            GeoObject::Attractor3D(o) => o.label = label,
            GeoObject::Fractal2D(o) => o.label = label,
            GeoObject::HyperSurface4D(o) => o.label = label,
            GeoObject::VectorField3D(o) => o.label = label,
            GeoObject::Histogram(o) => o.label = label,
            GeoObject::ScatterPlot(o) => o.label = label,
            GeoObject::BoxPlot(o) => o.label = label,
            GeoObject::RegressionLine(o) => o.label = label,
            GeoObject::PhasePortrait(o) => o.label = label,
            GeoObject::Pencil(o) => o.label = label,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            GeoObject::Point(o) => o.color,
            GeoObject::Line(o) => o.color,
            GeoObject::Circle(o) => o.color,
            GeoObject::Polygon(o) => o.color,
            GeoObject::Pencil(o) => o.color,
            GeoObject::Function(o) => o.color,
            GeoObject::Text(o) => o.color,
            GeoObject::Ellipse(o) => o.color,
            GeoObject::Parabola(o) => o.color,
            GeoObject::Hyperbola(o) => o.color,
            GeoObject::Point3D(o) => o.color,
            GeoObject::Segment3D(o) => o.color,
            GeoObject::Sphere3D(o) => o.color,
            GeoObject::Cube3D(o) => o.color,
            GeoObject::Pyramid3D(o) => o.color,
            GeoObject::Cone3D(o) => o.color,
            GeoObject::Cylinder3D(o) => o.color,
            GeoObject::Torus3D(o) => o.color,
            GeoObject::MoebiusStrip(o) => o.color,
            GeoObject::Surface3D(o) => o.color,
            GeoObject::ParametricCurve2D(o) => o.color,
            GeoObject::ParametricCurve3D(o) => o.color,
            GeoObject::PolarCurve(o) => o.color,
            GeoObject::VectorField2D(o) => o.color,
            GeoObject::ComplexGrid(o) => o.color,
            GeoObject::ComplexMapping(o) => o.color,
            GeoObject::ImplicitCurve(o) => o.color,
            GeoObject::Attractor3D(o) => o.color,
            GeoObject::Fractal2D(o) => o.color,
            GeoObject::HyperSurface4D(o) => o.color,
            GeoObject::VectorField3D(o) => o.color,
            GeoObject::Histogram(o) => o.color,
            GeoObject::ScatterPlot(o) => o.color,
            GeoObject::BoxPlot(o) => o.color,
            GeoObject::RegressionLine(o) => o.color,
            GeoObject::PhasePortrait(o) => o.color,
        }
    }

    pub fn set_color(&mut self, color: Color) {
        match self {
            GeoObject::Point(o) => o.color = color,
            GeoObject::Line(o) => o.color = color,
            GeoObject::Circle(o) => o.color = color,
            GeoObject::Polygon(o) => o.color = color,
            GeoObject::Pencil(o) => o.color = color,
            GeoObject::Function(o) => o.color = color,
            GeoObject::Text(o) => o.color = color,
            GeoObject::Ellipse(o) => o.color = color,
            GeoObject::Parabola(o) => o.color = color,
            GeoObject::Hyperbola(o) => o.color = color,
            GeoObject::Point3D(o) => o.color = color,
            GeoObject::Segment3D(o) => o.color = color,
            GeoObject::Sphere3D(o) => o.color = color,
            GeoObject::Cube3D(o) => o.color = color,
            GeoObject::Pyramid3D(o) => o.color = color,
            GeoObject::Cone3D(o) => o.color = color,
            GeoObject::Cylinder3D(o) => o.color = color,
            GeoObject::Torus3D(o) => o.color = color,
            GeoObject::MoebiusStrip(o) => o.color = color,
            GeoObject::Surface3D(o) => o.color = color,
            GeoObject::ParametricCurve2D(o) => o.color = color,
            GeoObject::ParametricCurve3D(o) => o.color = color,
            GeoObject::PolarCurve(o) => o.color = color,
            GeoObject::VectorField2D(o) => o.color = color,
            GeoObject::ComplexGrid(o) => o.color = color,
            GeoObject::ComplexMapping(o) => o.color = color,
            GeoObject::ImplicitCurve(o) => o.color = color,
            GeoObject::Attractor3D(o) => o.color = color,
            GeoObject::Fractal2D(o) => o.color = color,
            GeoObject::HyperSurface4D(o) => o.color = color,
            GeoObject::VectorField3D(o) => o.color = color,
            GeoObject::Histogram(o) => o.color = color,
            GeoObject::ScatterPlot(o) => o.color = color,
            GeoObject::BoxPlot(o) => o.color = color,
            GeoObject::RegressionLine(o) => o.color = color,
            GeoObject::PhasePortrait(o) => o.color = color,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            GeoObject::Point(o) => o.visible,
            GeoObject::Line(o) => o.visible,
            GeoObject::Circle(o) => o.visible,
            GeoObject::Polygon(o) => o.visible,
            GeoObject::Pencil(o) => o.visible,
            GeoObject::Function(o) => o.visible,
            GeoObject::Text(o) => o.visible,
            GeoObject::Ellipse(o) => o.visible,
            GeoObject::Parabola(o) => o.visible,
            GeoObject::Hyperbola(o) => o.visible,
            GeoObject::Point3D(o) => o.visible,
            GeoObject::Segment3D(o) => o.visible,
            GeoObject::Sphere3D(o) => o.visible,
            GeoObject::Cube3D(o) => o.visible,
            GeoObject::Pyramid3D(o) => o.visible,
            GeoObject::Cone3D(o) => o.visible,
            GeoObject::Cylinder3D(o) => o.visible,
            GeoObject::Torus3D(o) => o.visible,
            GeoObject::MoebiusStrip(o) => o.visible,
            GeoObject::Surface3D(o) => o.visible,
            GeoObject::ParametricCurve2D(o) => o.visible,
            GeoObject::ParametricCurve3D(o) => o.visible,
            GeoObject::PolarCurve(o) => o.visible,
            GeoObject::VectorField2D(o) => o.visible,
            GeoObject::ComplexGrid(o) => o.visible,
            GeoObject::ComplexMapping(o) => o.visible,
            GeoObject::ImplicitCurve(o) => o.visible,
            GeoObject::Attractor3D(o) => o.visible,
            GeoObject::Fractal2D(o) => o.visible,
            GeoObject::HyperSurface4D(o) => o.visible,
            GeoObject::VectorField3D(o) => o.visible,
            GeoObject::Histogram(o) => o.visible,
            GeoObject::ScatterPlot(o) => o.visible,
            GeoObject::BoxPlot(o) => o.visible,
            GeoObject::RegressionLine(o) => o.visible,
            GeoObject::PhasePortrait(o) => o.visible,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        match self {
            GeoObject::Point(o) => o.visible = visible,
            GeoObject::Line(o) => o.visible = visible,
            GeoObject::Circle(o) => o.visible = visible,
            GeoObject::Polygon(o) => o.visible = visible,
            GeoObject::Pencil(o) => o.visible = visible,
            GeoObject::Function(o) => o.visible = visible,
            GeoObject::Text(o) => o.visible = visible,
            GeoObject::Ellipse(o) => o.visible = visible,
            GeoObject::Parabola(o) => o.visible = visible,
            GeoObject::Hyperbola(o) => o.visible = visible,
            GeoObject::Point3D(o) => o.visible = visible,
            GeoObject::Segment3D(o) => o.visible = visible,
            GeoObject::Sphere3D(o) => o.visible = visible,
            GeoObject::Cube3D(o) => o.visible = visible,
            GeoObject::Pyramid3D(o) => o.visible = visible,
            GeoObject::Cone3D(o) => o.visible = visible,
            GeoObject::Cylinder3D(o) => o.visible = visible,
            GeoObject::Torus3D(o) => o.visible = visible,
            GeoObject::MoebiusStrip(o) => o.visible = visible,
            GeoObject::Surface3D(o) => o.visible = visible,
            GeoObject::ParametricCurve2D(o) => o.visible = visible,
            GeoObject::ParametricCurve3D(o) => o.visible = visible,
            GeoObject::PolarCurve(o) => o.visible = visible,
            GeoObject::VectorField2D(o) => o.visible = visible,
            GeoObject::ComplexGrid(o) => o.visible = visible,
            GeoObject::ComplexMapping(o) => o.visible = visible,
            GeoObject::ImplicitCurve(o) => o.visible = visible,
            GeoObject::Attractor3D(o) => o.visible = visible,
            GeoObject::Fractal2D(o) => o.visible = visible,
            GeoObject::HyperSurface4D(o) => o.visible = visible,
            GeoObject::VectorField3D(o) => o.visible = visible,
            GeoObject::Histogram(o) => o.visible = visible,
            GeoObject::ScatterPlot(o) => o.visible = visible,
            GeoObject::BoxPlot(o) => o.visible = visible,
            GeoObject::RegressionLine(o) => o.visible = visible,
            GeoObject::PhasePortrait(o) => o.visible = visible,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            GeoObject::Point(_) => "Point",
            GeoObject::Line(_) => "Line",
            GeoObject::Circle(_) => "Circle",
            GeoObject::Polygon(_) => "Polygon",
            GeoObject::Pencil(_) => "Pencil",
            GeoObject::Function(_) => "Function",
            GeoObject::Text(_) => "Text",
            GeoObject::Ellipse(_) => "Ellipse",
            GeoObject::Parabola(_) => "Parabola",
            GeoObject::Hyperbola(_) => "Hyperbola",
            GeoObject::Point3D(_) => "Point3D",
            GeoObject::Segment3D(_) => "Segment3D",
            GeoObject::Sphere3D(_) => "Sphere3D",
            GeoObject::Cube3D(_) => "Cube3D",
            GeoObject::Pyramid3D(_) => "Pyramid3D",
            GeoObject::Cone3D(_) => "Cone3D",
            GeoObject::Cylinder3D(_) => "Cylinder3D",
            GeoObject::Torus3D(_) => "Torus3D",
            GeoObject::MoebiusStrip(_) => "MoebiusStrip",
            GeoObject::Surface3D(_) => "Surface3D",
            GeoObject::ParametricCurve2D(_) => "ParametricCurve2D",
            GeoObject::ParametricCurve3D(_) => "ParametricCurve3D",
            GeoObject::PolarCurve(_) => "PolarCurve",
            GeoObject::VectorField2D(_) => "VectorField2D",
            GeoObject::ComplexGrid(_) => "ComplexGrid",
            GeoObject::ComplexMapping(_) => "ComplexMapping",
            GeoObject::ImplicitCurve(_) => "ImplicitCurve",
            GeoObject::Attractor3D(_) => "Attractor3D",
            GeoObject::Fractal2D(_) => "Fractal2D",
            GeoObject::HyperSurface4D(_) => "HyperSurface4D",
            GeoObject::VectorField3D(_) => "VectorField3D",
            GeoObject::Histogram(_) => "Histogram",
            GeoObject::ScatterPlot(_) => "ScatterPlot",
            GeoObject::BoxPlot(_) => "BoxPlot",
            GeoObject::RegressionLine(_) => "RegressionLine",
            GeoObject::PhasePortrait(_) => "PhasePortrait",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointObj {
    pub id: ObjectId,
    pub label: String,
    pub position: Point2,
    #[serde(default)]
    pub x_expr: Option<String>,
    #[serde(default)]
    pub y_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub size: f32,
}

impl PointObj {
    pub fn new(position: Point2) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            position,
            x_expr: None,
            y_expr: None,
            color: Color::BLUE,
            visible: true,
            size: 6.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    /// Finite segment between two endpoints.
    #[default]
    Segment,
    /// Infinite line through two points.
    Line,
    /// Ray starting at `start` and passing through `end`.
    Ray,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineObj {
    pub id: ObjectId,
    pub label: String,
    pub start: Point2,
    pub end: Point2,
    #[serde(default)]
    pub kind: LineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_x_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_y_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_x_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_y_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl LineObj {
    pub fn new(start: Point2, end: Point2) -> Self {
        Self::new_with_kind(start, end, LineKind::Segment)
    }

    pub fn new_with_kind(start: Point2, end: Point2, kind: LineKind) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            start,
            end,
            kind,
            start_x_expr: None,
            start_y_expr: None,
            end_x_expr: None,
            end_y_expr: None,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_start_expr(mut self, x: &str, y: &str) -> Self {
        self.start_x_expr = Some(x.to_string());
        self.start_y_expr = Some(y.to_string());
        self
    }

    pub fn with_end_expr(mut self, x: &str, y: &str) -> Self {
        self.end_x_expr = Some(x.to_string());
        self.end_y_expr = Some(y.to_string());
        self
    }

    pub fn length(&self) -> f64 {
        self.start.distance(&self.end)
    }

    /// True for Segment or Ray; infinite lines have no finite length.
    pub fn has_finite_length(&self) -> bool {
        self.kind != LineKind::Line
    }

    pub fn point_at(&self, t: f64) -> Point2 {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        Point2::new(self.start.x + t * dx, self.start.y + t * dy)
    }

    pub fn param_at_point(&self, p: Point2) -> f64 {
        grafito_geometry::line_param_at_point(p, self.start, self.end)
    }

    pub fn distance_to_point(&self, p: Point2) -> f64 {
        match self.kind {
            LineKind::Segment => {
                grafito_geometry::distance_point_to_segment(p, self.start, self.end)
            }
            LineKind::Ray => grafito_geometry::distance_point_to_ray(p, self.start, self.end),
            LineKind::Line => grafito_geometry::distance_point_to_line(p, self.start, self.end),
        }
    }

    pub fn clip_to_aabb(&self, rect: AABB) -> Option<(Point2, Point2)> {
        match self.kind {
            LineKind::Segment => grafito_geometry::clip_segment_to_rect(self.start, self.end, rect),
            LineKind::Ray => grafito_geometry::clip_ray_to_rect(self.start, self.end, rect),
            LineKind::Line => grafito_geometry::clip_line_to_rect(self.start, self.end, rect),
        }
    }

    pub fn kind_contains_t(&self, t: f64) -> bool {
        match self.kind {
            LineKind::Segment => (0.0..=1.0).contains(&t),
            LineKind::Ray => t >= 0.0,
            LineKind::Line => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CircleObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub radius: f64,
    #[serde(default)]
    pub radius_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}

impl CircleObj {
    pub fn new(center: Point2, radius: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            radius,
            radius_expr: None,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
            fill_color: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn to_geom(&self) -> GeomCircle {
        GeomCircle::new(self.center, self.radius)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolygonObj {
    pub id: ObjectId,
    pub label: String,
    pub vertices: Vec<Point2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub x_exprs: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub y_exprs: Vec<Option<String>>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}

impl PolygonObj {
    pub fn new(vertices: Vec<Point2>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            vertices,
            x_exprs: Vec::new(),
            y_exprs: Vec::new(),
            color: Color::BLACK,
            visible: true,
            width: 2.0,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.2)),
        }
    }

    pub fn with_vertex_exprs(mut self, x: &str, y: &str) -> Self {
        self.x_exprs.push(Some(x.to_string()));
        self.y_exprs.push(Some(y.to_string()));
        self
    }

    pub fn set_vertex_expr(&mut self, index: usize, x: Option<String>, y: Option<String>) {
        if index >= self.x_exprs.len() {
            self.x_exprs.resize(index + 1, None);
        }
        if index >= self.y_exprs.len() {
            self.y_exprs.resize(index + 1, None);
        }
        self.x_exprs[index] = x;
        self.y_exprs[index] = y;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub domain_min: Option<f64>,
    pub domain_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_max_expr: Option<String>,
    pub fill_color: Option<Color>,
    // Integral function: ∫_[integral_lower]^x expr(var) d(var)
    pub is_integral: bool,
    pub integral_var: String,
    pub integral_lower: f64,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<FunctionCacheKey>>>,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<FunctionSamples>>,
}

impl Clone for FunctionObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr: self.expr.clone(),
            color: self.color,
            visible: self.visible,
            width: self.width,
            domain_min: self.domain_min,
            domain_max: self.domain_max,
            domain_min_expr: self.domain_min_expr.clone(),
            domain_max_expr: self.domain_max_expr.clone(),
            fill_color: self.fill_color,
            is_integral: self.is_integral,
            integral_var: self.integral_var.clone(),
            integral_lower: self.integral_lower,
            // Share the cache through Arc
            cached_key: self.cached_key.clone(),
            cached_samples: self.cached_samples.clone(),
        }
    }
}

impl PartialEq for FunctionObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr == other.expr
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
            && self.domain_min == other.domain_min
            && self.domain_max == other.domain_max
            && self.domain_min_expr == other.domain_min_expr
            && self.domain_max_expr == other.domain_max_expr
            && self.fill_color == other.fill_color
            && self.is_integral == other.is_integral
            && self.integral_var == other.integral_var
            && self.integral_lower == other.integral_lower
    }
}

impl FunctionObj {
    pub fn new(expr: impl Into<String>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.into(),
            color: Color::BLUE,
            visible: true,
            width: 2.0,
            domain_min: None,
            domain_max: None,
            domain_min_expr: None,
            domain_max_expr: None,
            fill_color: None,
            is_integral: false,
            integral_var: String::new(),
            integral_lower: 0.0,
            cached_key: Arc::new(RwLock::new(None)),
            cached_samples: Arc::new(RwLock::new(FunctionSamples::new())),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_fill(mut self, color: Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    pub fn as_integral(mut self, var: &str, lower: f64) -> Self {
        self.is_integral = true;
        self.integral_var = var.to_string();
        self.integral_lower = lower;
        self
    }

    /// Invalidate any cached samples for this function.
    pub fn invalidate_cache(&self) {
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
        self.cached_samples
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextObj {
    pub id: ObjectId,
    pub label: String,
    pub content: String,
    pub position: Point2,
    pub color: Color,
    pub visible: bool,
    pub font_size: f32,
}

impl TextObj {
    pub fn new(content: impl Into<String>, position: Point2) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            content: content.into(),
            position,
            color: Color::BLACK,
            visible: true,
            font_size: 14.0,
        }
    }
}

// ── 3D Objects ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point3DObj {
    pub id: ObjectId,
    pub label: String,
    pub position: Point3D,
    pub color: Color,
    pub visible: bool,
    pub size: f32,
}

impl Point3DObj {
    pub fn new(position: Point3D) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            position,
            color: Color::BLUE,
            visible: true,
            size: 8.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment3DObj {
    pub id: ObjectId,
    pub label: String,
    pub a: Point3D,
    pub b: Point3D,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl Segment3DObj {
    pub fn new(a: Point3D, b: Point3D) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            a,
            b,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sphere3DObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point3D,
    pub radius: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Sphere3DObj {
    pub fn new(center: Point3D, radius: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            radius,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cube3DObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point3D,
    pub size: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Cube3DObj {
    pub fn new(center: Point3D, size: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            size,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: None,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pyramid3DObj {
    pub id: ObjectId,
    pub label: String,
    pub base_center: Point3D,
    pub apex: Point3D,
    pub base_size: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Pyramid3DObj {
    pub fn new(base_center: Point3D, apex: Point3D, base_size: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            base_center,
            apex,
            base_size,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: None,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cone3DObj {
    pub id: ObjectId,
    pub label: String,
    pub base_center: Point3D,
    pub apex: Point3D,
    pub radius: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Cone3DObj {
    pub fn new(base_center: Point3D, apex: Point3D, radius: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            base_center,
            apex,
            radius,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: None,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cylinder3DObj {
    pub id: ObjectId,
    pub label: String,
    pub base_center: Point3D,
    pub top_center: Point3D,
    pub radius: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Cylinder3DObj {
    pub fn new(base_center: Point3D, top_center: Point3D, radius: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            base_center,
            top_center,
            radius,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: None,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Torus3DObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point3D,
    pub r_major: f64,
    pub r_minor: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl Torus3DObj {
    pub fn new(center: Point3D, r_major: f64, r_minor: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            r_major,
            r_minor,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoebiusStripObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point3D,
    pub radius: f64,
    pub width_r: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl MoebiusStripObj {
    pub fn new(center: Point3D, radius: f64, width_r: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            radius,
            width_r,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

// ── 3D Parametric Surface ──
#[derive(Debug, Serialize, Deserialize)]
pub struct Surface3DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_max_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_max_expr: Option<String>,
    /// Parametric surface: x(u,v), y(u,v), z(u,v)
    #[serde(default)]
    pub is_parametric: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expr_x: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expr_y: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expr_z: String,
    #[serde(default)]
    pub u_min: f64,
    #[serde(default)]
    pub u_max: f64,
    #[serde(default)]
    pub v_min: f64,
    #[serde(default)]
    pub v_max: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub solid: bool,
    pub mesh_res: usize,
    #[serde(skip)]
    pub cached_grid: Arc<RwLock<SurfaceSamples>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<SurfaceCacheKey>>>,
}

impl Clone for Surface3DObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr: self.expr.clone(),
            x_min: self.x_min,
            x_max: self.x_max,
            y_min: self.y_min,
            y_max: self.y_max,
            x_min_expr: self.x_min_expr.clone(),
            x_max_expr: self.x_max_expr.clone(),
            y_min_expr: self.y_min_expr.clone(),
            y_max_expr: self.y_max_expr.clone(),
            is_parametric: self.is_parametric,
            expr_x: self.expr_x.clone(),
            expr_y: self.expr_y.clone(),
            expr_z: self.expr_z.clone(),
            u_min: self.u_min,
            u_max: self.u_max,
            v_min: self.v_min,
            v_max: self.v_max,
            color: self.color,
            visible: self.visible,
            width: self.width,
            solid: self.solid,
            mesh_res: self.mesh_res,
            cached_grid: self.cached_grid.clone(),
            cached_key: self.cached_key.clone(),
        }
    }
}

impl PartialEq for Surface3DObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr == other.expr
            && self.x_min == other.x_min
            && self.x_max == other.x_max
            && self.y_min == other.y_min
            && self.y_max == other.y_max
            && self.x_min_expr == other.x_min_expr
            && self.x_max_expr == other.x_max_expr
            && self.y_min_expr == other.y_min_expr
            && self.y_max_expr == other.y_max_expr
            && self.is_parametric == other.is_parametric
            && self.expr_x == other.expr_x
            && self.expr_y == other.expr_y
            && self.expr_z == other.expr_z
            && self.u_min == other.u_min
            && self.u_max == other.u_max
            && self.v_min == other.v_min
            && self.v_max == other.v_max
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
            && self.solid == other.solid
            && self.mesh_res == other.mesh_res
    }
}

impl Surface3DObj {
    pub fn new(expr: impl Into<String>, xr: (f64, f64), yr: (f64, f64)) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.into(),
            x_min: xr.0,
            x_max: xr.1,
            y_min: yr.0,
            y_max: yr.1,
            x_min_expr: None,
            x_max_expr: None,
            y_min_expr: None,
            y_max_expr: None,
            is_parametric: false,
            expr_x: String::new(),
            expr_y: String::new(),
            expr_z: String::new(),
            u_min: 0.0,
            u_max: 0.0,
            v_min: 0.0,
            v_max: 0.0,
            color: Color::BLUE,
            visible: true,
            width: 1.0,
            solid: false,
            mesh_res: 30,
            cached_grid: Arc::new(RwLock::new(SurfaceSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new_parametric(
        expr_x: impl Into<String>,
        expr_y: impl Into<String>,
        expr_z: impl Into<String>,
        u_domain: (f64, f64),
        v_domain: (f64, f64),
    ) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: String::new(),
            x_min: 0.0,
            x_max: 0.0,
            y_min: 0.0,
            y_max: 0.0,
            x_min_expr: None,
            x_max_expr: None,
            y_min_expr: None,
            y_max_expr: None,
            is_parametric: true,
            expr_x: expr_x.into(),
            expr_y: expr_y.into(),
            expr_z: expr_z.into(),
            u_min: u_domain.0,
            u_max: u_domain.1,
            v_min: v_domain.0,
            v_max: v_domain.1,
            color: Color::BLUE,
            visible: true,
            width: 1.0,
            solid: false,
            mesh_res: 30,
            cached_grid: Arc::new(RwLock::new(SurfaceSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn as_solid(mut self) -> Self {
        self.solid = true;
        self
    }

    /// Invalidate any cached grid for this surface.
    pub fn invalidate_cache(&self) {
        self.cached_grid
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipseObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub rx: f64,
    pub ry: f64,
    pub angle: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl EllipseObj {
    pub fn new(center: Point2, rx: f64, ry: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            rx,
            ry,
            angle: 0.0,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParabolaObj {
    pub id: ObjectId,
    pub label: String,
    pub vertex: Point2,
    pub p: f64,
    pub vertical: bool,
    #[serde(default)]
    pub angle: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl ParabolaObj {
    pub fn new(vertex: Point2, p: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            vertex,
            p,
            vertical: true,
            angle: 0.0,
            color: Color::RED,
            visible: true,
            width: 2.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperbolaObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub a: f64,
    pub b: f64,
    pub horizontal: bool,
    #[serde(default)]
    pub angle: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl HyperbolaObj {
    pub fn new(center: Point2, a: f64, b: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            a,
            b,
            horizontal: true,
            angle: 0.0,
            color: Color::RED,
            visible: true,
            width: 2.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

// --------------------------------------------------------
// AM2, AM3, and 4D Structural Objects
// --------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ParametricCurve2DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_x: String,
    pub expr_y: String,
    pub t_min: f64,
    pub t_max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_max_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<Curve2DSamples>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<ParametricCacheKey>>>,
}

impl Clone for ParametricCurve2DObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr_x: self.expr_x.clone(),
            expr_y: self.expr_y.clone(),
            t_min: self.t_min,
            t_max: self.t_max,
            t_min_expr: self.t_min_expr.clone(),
            t_max_expr: self.t_max_expr.clone(),
            color: self.color,
            visible: self.visible,
            width: self.width,
            cached_samples: self.cached_samples.clone(),
            cached_key: self.cached_key.clone(),
        }
    }
}

impl PartialEq for ParametricCurve2DObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr_x == other.expr_x
            && self.expr_y == other.expr_y
            && self.t_min == other.t_min
            && self.t_max == other.t_max
            && self.t_min_expr == other.t_min_expr
            && self.t_max_expr == other.t_max_expr
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
    }
}

impl ParametricCurve2DObj {
    pub fn new(expr_x: &str, expr_y: &str, t_min: f64, t_max: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_x: expr_x.to_string(),
            expr_y: expr_y.to_string(),
            t_min,
            t_max,
            t_min_expr: None,
            t_max_expr: None,
            color: Color::BLUE,
            visible: true,
            width: 2.0,
            cached_samples: Arc::new(RwLock::new(Curve2DSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    /// Invalidate any cached samples for this curve.
    pub fn invalidate_cache(&self) {
        self.cached_samples
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParametricCurve3DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_x: String,
    pub expr_y: String,
    pub expr_z: String,
    pub t_min: f64,
    pub t_max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_max_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<Curve3DSamples>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<ParametricCacheKey>>>,
}

impl Clone for ParametricCurve3DObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr_x: self.expr_x.clone(),
            expr_y: self.expr_y.clone(),
            expr_z: self.expr_z.clone(),
            t_min: self.t_min,
            t_max: self.t_max,
            t_min_expr: self.t_min_expr.clone(),
            t_max_expr: self.t_max_expr.clone(),
            color: self.color,
            visible: self.visible,
            width: self.width,
            cached_samples: self.cached_samples.clone(),
            cached_key: self.cached_key.clone(),
        }
    }
}

impl PartialEq for ParametricCurve3DObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr_x == other.expr_x
            && self.expr_y == other.expr_y
            && self.expr_z == other.expr_z
            && self.t_min == other.t_min
            && self.t_max == other.t_max
            && self.t_min_expr == other.t_min_expr
            && self.t_max_expr == other.t_max_expr
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
    }
}

impl ParametricCurve3DObj {
    pub fn new(expr_x: &str, expr_y: &str, expr_z: &str, t_min: f64, t_max: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_x: expr_x.to_string(),
            expr_y: expr_y.to_string(),
            expr_z: expr_z.to_string(),
            t_min,
            t_max,
            t_min_expr: None,
            t_max_expr: None,
            color: Color::new(1.0, 0.0, 1.0, 1.0),
            visible: true,
            width: 2.0,
            cached_samples: Arc::new(RwLock::new(Curve3DSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    /// Invalidate any cached samples for this curve.
    pub fn invalidate_cache(&self) {
        self.cached_samples
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PolarCurveObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_r: String,
    pub t_min: f64,
    pub t_max: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_min_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_max_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<Curve2DSamples>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<ParametricCacheKey>>>,
}

impl Clone for PolarCurveObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr_r: self.expr_r.clone(),
            t_min: self.t_min,
            t_max: self.t_max,
            t_min_expr: self.t_min_expr.clone(),
            t_max_expr: self.t_max_expr.clone(),
            color: self.color,
            visible: self.visible,
            width: self.width,
            fill_color: self.fill_color,
            cached_samples: self.cached_samples.clone(),
            cached_key: self.cached_key.clone(),
        }
    }
}

impl PartialEq for PolarCurveObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr_r == other.expr_r
            && self.t_min == other.t_min
            && self.t_max == other.t_max
            && self.t_min_expr == other.t_min_expr
            && self.t_max_expr == other.t_max_expr
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
            && self.fill_color == other.fill_color
    }
}

impl PolarCurveObj {
    pub fn new(expr_r: &str, t_min: f64, t_max: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_r: expr_r.to_string(),
            t_min,
            t_max,
            t_min_expr: None,
            t_max_expr: None,
            color: Color::new(0.0, 0.7, 0.3, 1.0),
            visible: true,
            width: 2.0,
            fill_color: None,
            cached_samples: Arc::new(RwLock::new(Curve2DSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_fill(mut self, color: Color) -> Self {
        self.fill_color = Some(color);
        self
    }

    /// Invalidate any cached samples for this curve.
    pub fn invalidate_cache(&self) {
        self.cached_samples
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexGridObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub density: usize,
    pub color: Color,
    pub visible: bool,
    /// 0 = grid lines, 1 = domain coloring (complex), 2 = heat map (real f(x,y))
    pub render_mode: u8,
}
impl ComplexGridObj {
    pub fn new(expr: &str, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.to_string(),
            x_min,
            x_max,
            y_min,
            y_max,
            density: 10,
            color: Color::BLUE,
            visible: true,
            render_mode: 0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn as_domain_coloring(mut self) -> Self {
        self.render_mode = 1;
        self.density = self.density.max(200);
        self
    }
    pub fn as_heat_map(mut self) -> Self {
        self.render_mode = 2;
        self.density = self.density.max(150);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexMappingObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub target: ObjectId,
    pub color: Color,
    pub visible: bool,
    #[serde(skip)]
    pub conformal_cache: Option<ConformalMap>,
}
impl ComplexMappingObj {
    pub fn new(expr: &str, target: ObjectId) -> Self {
        let conformal_cache = ConformalMap::from_expr_string(expr);
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.to_string(),
            target,
            color: Color::new(0.5, 0.0, 0.5, 1.0),
            visible: true,
            conformal_cache,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn refresh_cache(&mut self) {
        self.conformal_cache = ConformalMap::from_expr_string(&self.expr);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VectorField2DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_u: String,
    pub expr_v: String,
    pub color: Color,
    pub visible: bool,
    pub density: usize,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<VectorFieldSamples>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<VectorFieldCacheKey>>>,
}

impl Clone for VectorField2DObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr_u: self.expr_u.clone(),
            expr_v: self.expr_v.clone(),
            color: self.color,
            visible: self.visible,
            density: self.density,
            cached_samples: self.cached_samples.clone(),
            cached_key: self.cached_key.clone(),
        }
    }
}

impl PartialEq for VectorField2DObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr_u == other.expr_u
            && self.expr_v == other.expr_v
            && self.color == other.color
            && self.visible == other.visible
            && self.density == other.density
    }
}

impl VectorField2DObj {
    pub fn new(expr_u: &str, expr_v: &str) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_u: expr_u.to_string(),
            expr_v: expr_v.to_string(),
            color: Color::new(0.8, 0.4, 0.0, 1.0),
            visible: true,
            density: 15,
            cached_samples: Arc::new(RwLock::new(VectorFieldSamples::new())),
            cached_key: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    /// Invalida cualquier caché de muestreo de este campo vectorial.
    pub fn invalidate_cache(&self) {
        self.cached_samples
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

/// Phase portrait for autonomous ODE system dx/dt = P(x,y), dy/dt = Q(x,y)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhasePortraitObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_dx: String,
    pub expr_dy: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub density: usize,
    pub color: Color,
    pub visible: bool,
}
impl PhasePortraitObj {
    pub fn new(
        expr_dx: &str,
        expr_dy: &str,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_dx: expr_dx.to_string(),
            expr_dy: expr_dy.to_string(),
            x_min,
            x_max,
            y_min,
            y_max,
            density: 20,
            color: Color::new(0.2, 0.2, 0.8, 1.0),
            visible: true,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RelationOperator {
    Eq,
    Less,
    Greater,
    LessEq,
    GreaterEq,
}

/// Cached (x, y) samples for a 1D function.
pub type FunctionSamples = Vec<(f64, Option<f64>)>;

/// Cached samples for a 2D parametric or polar curve.
pub type Curve2DSamples = Vec<(f64, f64)>;

/// Cached samples for a 3D parametric curve.
pub type Curve3DSamples = Vec<(f64, f64, f64)>;

/// Cached z-grid for a 3D parametric surface.
pub type SurfaceSamples = Vec<Vec<f64>>;

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCacheKey {
    pub expr: String,
    pub domain: (f64, f64),
    pub grid_size: usize,
    pub variables_hash: u64,
    pub is_integral: bool,
    pub integral_var: String,
    pub integral_lower: f64,
}

/// Cache key for parametric curves (2D, 3D, polar).
#[derive(Debug, Clone, PartialEq)]
pub struct ParametricCacheKey {
    pub t_domain: (f64, f64),
    pub steps: usize,
    pub variables_hash: u64,
}

/// Cache key for 3D parametric surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceCacheKey {
    pub x_domain: (f64, f64),
    pub y_domain: (f64, f64),
    pub res: usize,
    pub variables_hash: u64,
}

/// Cached (x, y, u, v) samples for a 2D vector field.
pub type VectorFieldSamples = Vec<(f64, f64, f64, f64)>;

/// Type alias for cached world-space region (x_min, x_max, y_min, y_max).
pub type CachedRegion = (f64, f64, f64, f64);

/// Cache key for 2D vector fields.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorFieldCacheKey {
    pub expr_u: String,
    pub expr_v: String,
    pub view_bounds: (f64, f64, f64, f64),
    pub grid_size: usize,
    pub variables_hash: u64,
}

/// World-space line segments grouped by contour level.
pub type ImplicitCurveSegments = Vec<(f64, Vec<(Point2, Point2)>)>;

#[derive(Debug, Clone, PartialEq)]
pub struct ImplicitCurveCacheKey {
    pub expr_lhs: String,
    pub expr_rhs: String,
    pub operator: RelationOperator,
    pub contour_levels_hash: u64,
    pub contour_colors_hash: u64,
    pub view_bounds: (f64, f64, f64, f64),
    pub grid_size: usize,
    pub variables_hash: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImplicitCurveObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_lhs: String,
    pub expr_rhs: String,
    pub operator: RelationOperator,
    pub color: Color,
    /// Color de relleno para regiones (Less/LessEq/Greater/GreaterEq) o para
    /// el interior de curvas cerradas (Eq). Si es `None`, no se rellena.
    pub fill_color: Option<Color>,
    pub visible: bool,
    pub width: f32,
    pub contour_levels: Option<Vec<f64>>,
    pub contour_colors: Option<Vec<Color>>,
    /// Cached geometry: one segment list per contour level (world-space).
    /// Wrapped in a lock so the GPU renderer can update it through a shared
    /// document reference.
    #[serde(skip)]
    pub cached_segments: Arc<RwLock<ImplicitCurveSegments>>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<ImplicitCurveCacheKey>>>,
    /// World-space region that was actually computed (padded/snapped view
    /// bounds). Used to decide whether a new view can reuse the cached
    /// geometry without re-evaluation.
    #[serde(skip)]
    pub cached_region: Arc<RwLock<Option<CachedRegion>>>,
    /// ASTs parseados de lhs y rhs, cacheados juntos. Se cachean porque
    /// el render de relleno llama `eval_2d` millones de veces por frame;
    /// parsear el AST en cada llamada era el cuello de botella que
    /// causaba lag/cuelgues con expresiones no triviales. La clave es el
    /// hash de **ambas** expresiones combinadas, así que no se confunden
    /// lhs y rhs (bug anterior: un solo cache se sobreescribía entre
    /// llamadas a lhs y rhs).
    #[serde(skip)]
    #[allow(private_interfaces)]
    pub cached_asts: Arc<RwLock<Option<CachedAsts>>>,
}

#[derive(Clone, Debug)]
struct CachedAsts {
    lhs: grafito_geometry::ast::Expr,
    rhs: grafito_geometry::ast::Expr,
    /// Hash de lhs + rhs + variables combinadas.
    hash: u64,
}

impl Clone for ImplicitCurveObj {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            label: self.label.clone(),
            expr_lhs: self.expr_lhs.clone(),
            expr_rhs: self.expr_rhs.clone(),
            operator: self.operator,
            color: self.color,
            fill_color: self.fill_color,
            visible: self.visible,
            width: self.width,
            contour_levels: self.contour_levels.clone(),
            contour_colors: self.contour_colors.clone(),
            cached_segments: self.cached_segments.clone(),
            cached_key: self.cached_key.clone(),
            cached_region: self.cached_region.clone(),
            cached_asts: self.cached_asts.clone(),
        }
    }
}

impl PartialEq for ImplicitCurveObj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.label == other.label
            && self.expr_lhs == other.expr_lhs
            && self.expr_rhs == other.expr_rhs
            && self.operator == other.operator
            && self.color == other.color
            && self.visible == other.visible
            && self.width == other.width
            && self.contour_levels == other.contour_levels
            && self.contour_colors == other.contour_colors
    }
}

impl ImplicitCurveObj {
    pub fn new(expr_lhs: &str, expr_rhs: &str, operator: RelationOperator) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_lhs: expr_lhs.to_string(),
            expr_rhs: expr_rhs.to_string(),
            operator,
            color: Color::new(0.6, 0.2, 0.8, 1.0),
            // Por defecto, regiones y curvas cerradas se rellenan con un
            // violeta claramente visible (alpha 0.5). El usuario puede
            // desactivarlo. Con alpha 0.2 el fill era casi invisible.
            fill_color: Some(Color::new(0.6, 0.2, 0.8, 0.5)),
            visible: true,
            width: 2.0,
            contour_levels: None,
            contour_colors: None,
            cached_segments: Arc::new(RwLock::new(ImplicitCurveSegments::new())),
            cached_key: Arc::new(RwLock::new(None)),
            cached_region: Arc::new(RwLock::new(None)),
            cached_asts: Arc::new(RwLock::new(None)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_fill(mut self, fill: Option<Color>) -> Self {
        self.fill_color = fill;
        self
    }

    /// Devuelve los ASTs parseados de `expr_lhs` y `expr_rhs`, cacheándolos
    /// juntos para no reparsear en cada frame. Devuelve `None` si alguna
    /// expresión no parsea (en cuyo caso el render debe omitir el objeto).
    ///
    /// La caché se invalida automáticamente cuando cambia el texto de las
    /// expresiones o las variables del documento.
    ///
    /// Importante: el cache es **combinado** para lhs y rhs (un solo slot)
    /// porque antes había un bug donde llamadas separadas a lhs y rhs se
    /// sobreescribían mutuamente, devolviendo el AST incorrecto.
    pub fn get_cached_asts(
        &self,
        variables: &HashMap<String, f64>,
        var_names: &[&str],
    ) -> Option<(grafito_geometry::ast::Expr, grafito_geometry::ast::Expr)> {
        // Hash combinado de lhs + rhs + variables.
        let mut hasher = DefaultHasher::new();
        self.expr_lhs.hash(&mut hasher);
        self.expr_rhs.hash(&mut hasher);
        for v in variables {
            v.0.hash(&mut hasher);
            v.1.to_bits().hash(&mut hasher);
        }
        let combined_hash = hasher.finish();

        // Verificar cache.
        if let Some(cached) = self
            .cached_asts
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
        {
            if cached.hash == combined_hash {
                return Some((cached.lhs, cached.rhs));
            }
        }

        // Re-parsear ambos juntos.
        let lhs =
            grafito_geometry::expr::prepare_function_ast(&self.expr_lhs, variables, var_names)
                .ok()?;
        let rhs =
            grafito_geometry::expr::prepare_function_ast(&self.expr_rhs, variables, var_names)
                .ok()?;

        let new_cache = CachedAsts {
            lhs: lhs.clone(),
            rhs: rhs.clone(),
            hash: combined_hash,
        };
        *self.cached_asts.write().unwrap_or_else(|p| p.into_inner()) = Some(new_cache);
        Some((lhs, rhs))
    }
}

fn is_variable_in_expr(var: &str, expr: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = expr[start..].find(var) {
        let actual_pos = start + pos;
        let before = if actual_pos == 0 {
            None
        } else {
            expr.as_bytes().get(actual_pos - 1).map(|&b| b as char)
        };
        let after = expr
            .as_bytes()
            .get(actual_pos + var.len())
            .map(|&b| b as char);

        let is_before_word = before.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let is_after_word = after.is_some_and(|c| c.is_alphanumeric() || c == '_');

        if !is_before_word && !is_after_word {
            return true;
        }
        start = actual_pos + 1;
    }
    false
}

impl ImplicitCurveObj {
    pub fn cache_key(
        &self,
        view_bounds: (f64, f64, f64, f64),
        grid_size: usize,
        variables: &HashMap<String, f64>,
    ) -> ImplicitCurveCacheKey {
        let mut hasher = DefaultHasher::new();
        if let Some(levels) = &self.contour_levels {
            for v in levels {
                v.to_bits().hash(&mut hasher);
            }
        }
        let contour_levels_hash = hasher.finish();

        let mut hasher = DefaultHasher::new();
        if let Some(colors) = &self.contour_colors {
            for c in colors {
                c.r.to_bits().hash(&mut hasher);
                c.g.to_bits().hash(&mut hasher);
                c.b.to_bits().hash(&mut hasher);
                c.a.to_bits().hash(&mut hasher);
            }
        }
        let contour_colors_hash = hasher.finish();

        let mut referenced = std::collections::HashSet::new();
        let lhs_clean = grafito_geometry::expr::preprocess_expr(&self.expr_lhs);
        if let Ok(ast_lhs) = grafito_geometry::ast::parse_ast(&lhs_clean) {
            ast_lhs.get_variables(&mut referenced);
        } else {
            for k in variables.keys() {
                if is_variable_in_expr(k, &self.expr_lhs) {
                    referenced.insert(k.clone());
                }
            }
        }

        let rhs_clean = grafito_geometry::expr::preprocess_expr(&self.expr_rhs);
        if let Ok(ast_rhs) = grafito_geometry::ast::parse_ast(&rhs_clean) {
            ast_rhs.get_variables(&mut referenced);
        } else {
            for k in variables.keys() {
                if is_variable_in_expr(k, &self.expr_rhs) {
                    referenced.insert(k.clone());
                }
            }
        }

        let mut hasher = DefaultHasher::new();
        let mut sorted_vars: Vec<(&String, &f64)> = variables
            .iter()
            .filter(|(k, _)| referenced.contains(*k))
            .collect();
        sorted_vars.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in sorted_vars {
            k.hash(&mut hasher);
            v.to_bits().hash(&mut hasher);
        }
        let variables_hash = hasher.finish();

        ImplicitCurveCacheKey {
            expr_lhs: self.expr_lhs.clone(),
            expr_rhs: self.expr_rhs.clone(),
            operator: self.operator,
            contour_levels_hash,
            contour_colors_hash,
            view_bounds,
            grid_size,
            variables_hash,
        }
    }

    /// Invalidate any cached geometry for this curve.
    pub fn invalidate_cache(&self) {
        self.cached_segments
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| p.into_inner()) = None;
        *self
            .cached_region
            .write()
            .unwrap_or_else(|p| p.into_inner()) = None;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attractor3DObj {
    pub id: ObjectId,
    pub label: String,
    pub attractor_type: String,
    pub params: Vec<f64>,
    pub x0: f64,
    pub y0: f64,
    pub z0: f64,
    pub dt: f64,
    pub steps: usize,
    pub skip: usize,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl Attractor3DObj {
    pub fn new(attractor_type: &str, params: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            attractor_type: attractor_type.to_string(),
            params,
            x0: 0.1,
            y0: 0.0,
            z0: 0.0,
            dt: 0.005,
            steps: 20000,
            skip: 100,
            color: Color::new(1.0, 0.3, 0.3, 1.0),
            visible: true,
            width: 1.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_initial(mut self, x: f64, y: f64, z: f64) -> Self {
        self.x0 = x;
        self.y0 = y;
        self.z0 = z;
        self
    }
    pub fn with_dt(mut self, dt: f64) -> Self {
        self.dt = dt;
        self
    }
    pub fn with_steps(mut self, steps: usize, skip: usize) -> Self {
        self.steps = steps;
        self.skip = skip;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fractal2DObj {
    pub id: ObjectId,
    pub label: String,
    pub fractal_type: String,
    pub params: Vec<f64>,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub resolution: usize,
    pub max_iter: u32,
    pub color: Color,
    pub visible: bool,
}
impl Fractal2DObj {
    pub fn mandelbrot() -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            fractal_type: "mandelbrot".to_string(),
            params: vec![],
            x_min: -2.5,
            x_max: 1.0,
            y_min: -1.25,
            y_max: 1.25,
            resolution: 200,
            max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0),
            visible: true,
        }
    }
    pub fn julia(cr: f64, ci: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            fractal_type: "julia".to_string(),
            params: vec![cr, ci],
            x_min: -2.0,
            x_max: 2.0,
            y_min: -2.0,
            y_max: 2.0,
            resolution: 200,
            max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0),
            visible: true,
        }
    }
    pub fn burning_ship() -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            fractal_type: "burning_ship".to_string(),
            params: vec![],
            x_min: -2.0,
            x_max: 1.0,
            y_min: -2.0,
            y_max: 1.0,
            resolution: 200,
            max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0),
            visible: true,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_resolution(mut self, res: usize) -> Self {
        self.resolution = res;
        self
    }
    pub fn with_max_iter(mut self, max_iter: u32) -> Self {
        self.max_iter = max_iter;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperSurface4DObj {
    pub id: ObjectId,
    pub label: String,
    pub surface_type: String,
    pub params: Vec<f64>,
    pub rotation_angles: Vec<f64>,
    pub resolution: usize,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl HyperSurface4DObj {
    pub fn hypercube() -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            surface_type: "hypercube".to_string(),
            params: vec![3.0],
            rotation_angles: vec![0.3, 0.5, 0.7],
            resolution: 16,
            color: Color::new(0.8, 0.2, 0.8, 1.0),
            visible: true,
            width: 1.5,
        }
    }
    pub fn hypersphere() -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            surface_type: "hypersphere".to_string(),
            params: vec![3.0],
            rotation_angles: vec![0.3, 0.5, 0.7],
            resolution: 20,
            color: Color::new(0.2, 0.8, 0.8, 1.0),
            visible: true,
            width: 1.5,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_rotation(mut self, angles: Vec<f64>) -> Self {
        self.rotation_angles = angles;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorField3DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_u: String,
    pub expr_v: String,
    pub expr_w: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub z_min: f64,
    pub z_max: f64,
    pub density: usize,
    pub color: Color,
    pub visible: bool,
}
impl VectorField3DObj {
    pub fn new(expr_u: &str, expr_v: &str, expr_w: &str) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr_u: expr_u.to_string(),
            expr_v: expr_v.to_string(),
            expr_w: expr_w.to_string(),
            x_min: -3.0,
            x_max: 3.0,
            y_min: -3.0,
            y_max: 3.0,
            z_min: -3.0,
            z_max: 3.0,
            density: 5,
            color: Color::new(0.8, 0.4, 0.0, 1.0),
            visible: true,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_bounds(mut self, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> Self {
        self.x_min = x.0;
        self.x_max = x.1;
        self.y_min = y.0;
        self.y_max = y.1;
        self.z_min = z.0;
        self.z_max = z.1;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramObj {
    pub id: ObjectId,
    pub label: String,
    pub data: Vec<f64>,
    pub bins: usize,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl HistogramObj {
    pub fn new(data: Vec<f64>, bins: usize) -> Self {
        let (x_min, x_max, y_max) = if data.is_empty() {
            (-5.0, 5.0, 5.0)
        } else {
            let lo = data.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let margin = if (hi - lo).abs() < 1e-12 {
                0.5
            } else {
                (hi - lo) * 0.05
            };
            let hist = grafito_geometry::statistics::histogram(&data, bins.max(1));
            let max_count = hist.iter().map(|(_, _, c)| *c).fold(0.0, f64::max);
            (lo - margin, hi + margin, max_count.max(1.0))
        };
        Self {
            id: ObjectId::new(),
            label: String::new(),
            data,
            bins,
            x_min,
            x_max,
            y_min: 0.0,
            y_max,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.4)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0;
        self.x_max = x.1;
        self.y_min = y.0;
        self.y_max = y.1;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterPlotObj {
    pub id: ObjectId,
    pub label: String,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub color: Color,
    pub visible: bool,
    pub point_size: f32,
}
impl ScatterPlotObj {
    pub fn new(xs: Vec<f64>, ys: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            xs,
            ys,
            x_min: -5.0,
            x_max: 5.0,
            y_min: -5.0,
            y_max: 5.0,
            color: Color::BLUE,
            visible: true,
            point_size: 5.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0;
        self.x_max = x.1;
        self.y_min = y.0;
        self.y_max = y.1;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxPlotObj {
    pub id: ObjectId,
    pub label: String,
    pub data: Vec<f64>,
    pub position: f64,
    pub width_box: f64,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl BoxPlotObj {
    pub fn new(data: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            data,
            position: 0.0,
            width_box: 1.0,
            x_min: -5.0,
            x_max: 5.0,
            y_min: -5.0,
            y_max: 5.0,
            color: Color::BLACK,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.3)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_position(mut self, pos: f64, w: f64) -> Self {
        self.position = pos;
        self.width_box = w;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegressionLineObj {
    pub id: ObjectId,
    pub label: String,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
    pub regression_type: String,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}
impl RegressionLineObj {
    pub fn linear(xs: Vec<f64>, ys: Vec<f64>, slope: f64, intercept: f64, r2: f64) -> Self {
        let (x_min, x_max) = if xs.is_empty() {
            (-5.0, 5.0)
        } else {
            let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let margin = if (hi - lo).abs() < 1e-12 {
                0.5
            } else {
                (hi - lo) * 0.05
            };
            (lo - margin, hi + margin)
        };
        let (y_min, y_max) = if ys.is_empty() {
            (-5.0, 5.0)
        } else {
            let lo = ys.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let margin = if (hi - lo).abs() < 1e-12 {
                0.5
            } else {
                (hi - lo) * 0.05
            };
            (lo - margin, hi + margin)
        };
        Self {
            id: ObjectId::new(),
            label: String::new(),
            xs,
            ys,
            slope,
            intercept,
            r_squared: r2,
            regression_type: "linear".to_string(),
            x_min,
            x_max,
            y_min,
            y_max,
            color: Color::RED,
            visible: true,
            width: 2.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0;
        self.x_max = x.1;
        self.y_min = y.0;
        self.y_max = y.1;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_implicit_curve_caches_asts() {
        // Llamar get_cached_asts dos veces con la misma expresión debe
        // devolver los mismos ASTs cacheados.
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
        let vars = HashMap::new();
        let (lhs1, rhs1) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        let (lhs2, rhs2) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        assert_eq!(lhs1, lhs2);
        assert_eq!(rhs1, rhs2);
    }

    #[test]
    fn test_implicit_curve_cache_does_not_mix_lhs_and_rhs() {
        // **Test de regresión crítico**: antes el cache se compartía entre
        // lhs y rhs y se sobreescribían. Verificamos que ahora cada slot
        // tiene el AST correcto.
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
        let vars = HashMap::new();
        let (lhs, rhs) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        // lhs debe evaluar como x²+y² (en (0,0) es 0).
        assert_eq!(lhs.eval_2d("x", 0.0, "y", 0.0), 0.0);
        assert_eq!(lhs.eval_2d("x", 1.0, "y", 0.0), 1.0);
        // rhs debe evaluar como 1 (constante).
        assert_eq!(rhs.eval_2d("x", 0.0, "y", 0.0), 1.0);
        assert_eq!(rhs.eval_2d("x", 100.0, "y", 200.0), 1.0);
    }

    #[test]
    fn test_implicit_curve_cache_invalidates_on_change() {
        // Cambiar la expresión debe reparsear.
        let mut ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Less);
        let vars = HashMap::new();
        let _ = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        ic.expr_lhs = "x^2 + y^2 + 1".to_string();
        let (lhs_new, _) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        // El nuevo lhs debe evaluar como x²+y²+1 (en (0,0) es 1, no 0).
        assert_eq!(lhs_new.eval_2d("x", 0.0, "y", 0.0), 1.0);
    }

    #[test]
    fn test_implicit_curve_cache_handles_eq_operator() {
        // **Test de regresión crítico**: para `x^2 + y^2 = 1` (Eq), el
        // scanline fill no debe ejecutarse (Eq es solo contorno). El cache
        // no debería romperse con esta configuración.
        let ic = ImplicitCurveObj::new("x^2 + y^2", "1", RelationOperator::Eq);
        let vars = HashMap::new();
        let (lhs, rhs) = ic.get_cached_asts(&vars, &["x", "y"]).unwrap();
        assert_eq!(lhs.eval_2d("x", 1.0, "y", 0.0), 1.0);
        assert_eq!(rhs.eval_2d("x", 1.0, "y", 0.0), 1.0);
    }
}
