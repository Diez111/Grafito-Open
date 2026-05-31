use grafito_geometry::{Color, Point2, Point3D, Circle as GeomCircle};
use serde::{Deserialize, Serialize};
use crate::id::ObjectId;

/// A geometric object in the document (2D and 3D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeoObject {
    // 2D
    Point(PointObj),
    Line(LineObj),
    Circle(CircleObj),
    Polygon(PolygonObj),
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
        }
    }

    pub fn color(&self) -> Color {
        match self {
            GeoObject::Point(o) => o.color,
            GeoObject::Line(o) => o.color,
            GeoObject::Circle(o) => o.color,
            GeoObject::Polygon(o) => o.color,
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
        }
    }

    pub fn set_color(&mut self, color: Color) {
        match self {
            GeoObject::Point(o) => o.color = color,
            GeoObject::Line(o) => o.color = color,
            GeoObject::Circle(o) => o.color = color,
            GeoObject::Polygon(o) => o.color = color,
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
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            GeoObject::Point(o) => o.visible,
            GeoObject::Line(o) => o.visible,
            GeoObject::Circle(o) => o.visible,
            GeoObject::Polygon(o) => o.visible,
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
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        match self {
            GeoObject::Point(o) => o.visible = visible,
            GeoObject::Line(o) => o.visible = visible,
            GeoObject::Circle(o) => o.visible = visible,
            GeoObject::Polygon(o) => o.visible = visible,
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
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            GeoObject::Point(_) => "Point",
            GeoObject::Line(_) => "Line",
            GeoObject::Circle(_) => "Circle",
            GeoObject::Polygon(_) => "Polygon",
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointObj {
    pub id: ObjectId,
    pub label: String,
    pub position: Point2,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineObj {
    pub id: ObjectId,
    pub label: String,
    pub start: Point2,
    pub end: Point2,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl LineObj {
    pub fn new(start: Point2, end: Point2) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            start,
            end,
            color: Color::BLACK,
            visible: true,
            width: 2.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn length(&self) -> f64 {
        self.start.distance(&self.end)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CircleObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub radius: f64,
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
            color: Color::BLACK,
            visible: true,
            width: 2.0,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.2)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub domain_min: Option<f64>,
    pub domain_max: Option<f64>,
    pub fill_color: Option<Color>,
    // Integral function: ∫_[integral_lower]^x expr(var) d(var)
    pub is_integral: bool,
    pub integral_var: String,
    pub integral_lower: f64,
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
            fill_color: None,
            is_integral: false,
            integral_var: String::new(),
            integral_lower: 0.0,
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
        Self { id: ObjectId::new(), label: String::new(), position, color: Color::BLUE, visible: true, size: 8.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment3DObj {
    pub id: ObjectId, pub label: String, pub a: Point3D, pub b: Point3D,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl Segment3DObj {
    pub fn new(a: Point3D, b: Point3D) -> Self {
        Self { id: ObjectId::new(), label: String::new(), a, b, color: Color::BLACK, visible: true, width: 2.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sphere3DObj {
    pub id: ObjectId, pub label: String, pub center: Point3D, pub radius: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl Sphere3DObj {
    pub fn new(center: Point3D, radius: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, radius, color: Color::BLACK, visible: true, width: 1.5, fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)) }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cube3DObj {
    pub id: ObjectId, pub label: String, pub center: Point3D, pub size: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl Cube3DObj {
    pub fn new(center: Point3D, size: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, size, color: Color::BLACK, visible: true, width: 1.5, fill_color: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pyramid3DObj {
    pub id: ObjectId, pub label: String, pub base_center: Point3D, pub apex: Point3D, pub base_size: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl Pyramid3DObj {
    pub fn new(base_center: Point3D, apex: Point3D, base_size: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), base_center, apex, base_size, color: Color::BLACK, visible: true, width: 1.5, fill_color: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cone3DObj {
    pub id: ObjectId, pub label: String, pub base_center: Point3D, pub apex: Point3D, pub radius: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl Cone3DObj {
    pub fn new(base_center: Point3D, apex: Point3D, radius: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), base_center, apex, radius, color: Color::BLACK, visible: true, width: 1.5, fill_color: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cylinder3DObj {
    pub id: ObjectId, pub label: String, pub base_center: Point3D, pub top_center: Point3D, pub radius: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl Cylinder3DObj {
    pub fn new(base_center: Point3D, top_center: Point3D, radius: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), base_center, top_center, radius, color: Color::BLACK, visible: true, width: 1.5, fill_color: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Torus3DObj {
    pub id: ObjectId, pub label: String,
    pub center: Point3D, pub r_major: f64, pub r_minor: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl Torus3DObj {
    pub fn new(center: Point3D, r_major: f64, r_minor: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, r_major, r_minor, color: Color::BLACK, visible: true, width: 1.5 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoebiusStripObj {
    pub id: ObjectId, pub label: String,
    pub center: Point3D, pub radius: f64, pub width_r: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl MoebiusStripObj {
    pub fn new(center: Point3D, radius: f64, width_r: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, radius, width_r, color: Color::BLACK, visible: true, width: 1.5 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

// ── 3D Parametric Surface ──
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface3DObj {
    pub id: ObjectId, pub label: String, pub expr: String,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
    pub solid: bool, pub mesh_res: usize,
}
impl Surface3DObj {
    pub fn new(expr: impl Into<String>, xr: (f64, f64), yr: (f64, f64)) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr: expr.into(),
               x_min: xr.0, x_max: xr.1, y_min: yr.0, y_max: yr.1,
               color: Color::BLUE, visible: true, width: 1.0, solid: false, mesh_res: 30 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn as_solid(mut self) -> Self { self.solid = true; self }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EllipseObj {
    pub id: ObjectId, pub label: String, pub center: Point2,
    pub rx: f64, pub ry: f64, pub angle: f64,
    pub color: Color, pub visible: bool, pub width: f32, pub fill_color: Option<Color>,
}
impl EllipseObj {
    pub fn new(center: Point2, rx: f64, ry: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, rx, ry, angle: 0.0,
               color: Color::BLACK, visible: true, width: 2.0, fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)) }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParabolaObj {
    pub id: ObjectId, pub label: String, pub vertex: Point2, pub p: f64, pub vertical: bool,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl ParabolaObj {
    pub fn new(vertex: Point2, p: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), vertex, p, vertical: true,
               color: Color::RED, visible: true, width: 2.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperbolaObj {
    pub id: ObjectId, pub label: String, pub center: Point2,
    pub a: f64, pub b: f64, pub horizontal: bool,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl HyperbolaObj {
    pub fn new(center: Point2, a: f64, b: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), center, a, b, horizontal: true,
               color: Color::RED, visible: true, width: 2.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

// --------------------------------------------------------
// AM2, AM3, and 4D Structural Objects
// --------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricCurve2DObj {
    pub id: ObjectId, pub label: String,
    pub expr_x: String, pub expr_y: String,
    pub t_min: f64, pub t_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl ParametricCurve2DObj {
    pub fn new(expr_x: &str, expr_y: &str, t_min: f64, t_max: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_x: expr_x.to_string(), expr_y: expr_y.to_string(), t_min, t_max, color: Color::BLUE, visible: true, width: 2.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParametricCurve3DObj {
    pub id: ObjectId, pub label: String,
    pub expr_x: String, pub expr_y: String, pub expr_z: String,
    pub t_min: f64, pub t_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl ParametricCurve3DObj {
    pub fn new(expr_x: &str, expr_y: &str, expr_z: &str, t_min: f64, t_max: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_x: expr_x.to_string(), expr_y: expr_y.to_string(), expr_z: expr_z.to_string(), t_min, t_max, color: Color::new(1.0, 0.0, 1.0, 1.0), visible: true, width: 2.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolarCurveObj {
    pub id: ObjectId, pub label: String,
    pub expr_r: String,
    pub t_min: f64, pub t_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
    pub fill_color: Option<Color>,
}
impl PolarCurveObj {
    pub fn new(expr_r: &str, t_min: f64, t_max: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_r: expr_r.to_string(), t_min, t_max, color: Color::new(0.0, 0.7, 0.3, 1.0), visible: true, width: 2.0, fill_color: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_fill(mut self, color: Color) -> Self { self.fill_color = Some(color); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexGridObj {
    pub id: ObjectId, pub label: String,
    pub expr: String,
    pub x_min: f64, pub x_max: f64,
    pub y_min: f64, pub y_max: f64,
    pub density: usize,
    pub color: Color, pub visible: bool,
    /// 0 = grid lines, 1 = domain coloring (complex), 2 = heat map (real f(x,y))
    pub render_mode: u8,
}
impl ComplexGridObj {
    pub fn new(expr: &str, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr: expr.to_string(), x_min, x_max, y_min, y_max, density: 10, color: Color::BLUE, visible: true, render_mode: 0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn as_domain_coloring(mut self) -> Self { self.render_mode = 1; self.density = self.density.max(200); self }
    pub fn as_heat_map(mut self) -> Self { self.render_mode = 2; self.density = self.density.max(150); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexMappingObj {
    pub id: ObjectId, pub label: String,
    pub expr: String,
    pub target: ObjectId, // ID of the region/polygon/curve to transform
    pub color: Color, pub visible: bool,
}
impl ComplexMappingObj {
    pub fn new(expr: &str, target: ObjectId) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr: expr.to_string(), target, color: Color::new(0.5, 0.0, 0.5, 1.0), visible: true }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorField2DObj {
    pub id: ObjectId, pub label: String,
    pub expr_u: String, pub expr_v: String,
    pub color: Color, pub visible: bool, pub density: usize,
}
impl VectorField2DObj {
    pub fn new(expr_u: &str, expr_v: &str) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_u: expr_u.to_string(), expr_v: expr_v.to_string(), color: Color::new(0.8, 0.4, 0.0, 1.0), visible: true, density: 15 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

/// Phase portrait for autonomous ODE system dx/dt = P(x,y), dy/dt = Q(x,y)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhasePortraitObj {
    pub id: ObjectId, pub label: String,
    pub expr_dx: String, pub expr_dy: String,
    pub x_min: f64, pub x_max: f64,
    pub y_min: f64, pub y_max: f64,
    pub density: usize,
    pub color: Color, pub visible: bool,
}
impl PhasePortraitObj {
    pub fn new(expr_dx: &str, expr_dy: &str, x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_dx: expr_dx.to_string(), expr_dy: expr_dy.to_string(), x_min, x_max, y_min, y_max, density: 20, color: Color::new(0.2, 0.2, 0.8, 1.0), visible: true }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RelationOperator {
    Eq,
    Less,
    Greater,
    LessEq,
    GreaterEq,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImplicitCurveObj {
    pub id: ObjectId, pub label: String,
    pub expr_lhs: String,
    pub expr_rhs: String,
    pub operator: RelationOperator,
    pub color: Color, pub visible: bool, pub width: f32,
    pub contour_levels: Option<Vec<f64>>,
    pub contour_colors: Option<Vec<Color>>,
}
impl ImplicitCurveObj {
    pub fn new(expr_lhs: &str, expr_rhs: &str, operator: RelationOperator) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr_lhs: expr_lhs.to_string(), expr_rhs: expr_rhs.to_string(), operator, color: Color::new(0.6, 0.2, 0.8, 1.0), visible: true, width: 2.0, contour_levels: None, contour_colors: None }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attractor3DObj {
    pub id: ObjectId, pub label: String,
    pub attractor_type: String,
    pub params: Vec<f64>,
    pub x0: f64, pub y0: f64, pub z0: f64,
    pub dt: f64, pub steps: usize, pub skip: usize,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl Attractor3DObj {
    pub fn new(attractor_type: &str, params: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            attractor_type: attractor_type.to_string(), params,
            x0: 0.1, y0: 0.0, z0: 0.0,
            dt: 0.005, steps: 20000, skip: 100,
            color: Color::new(1.0, 0.3, 0.3, 1.0), visible: true, width: 1.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_initial(mut self, x: f64, y: f64, z: f64) -> Self { self.x0 = x; self.y0 = y; self.z0 = z; self }
    pub fn with_dt(mut self, dt: f64) -> Self { self.dt = dt; self }
    pub fn with_steps(mut self, steps: usize, skip: usize) -> Self { self.steps = steps; self.skip = skip; self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fractal2DObj {
    pub id: ObjectId, pub label: String,
    pub fractal_type: String,
    pub params: Vec<f64>,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub resolution: usize, pub max_iter: u32,
    pub color: Color, pub visible: bool,
}
impl Fractal2DObj {
    pub fn mandelbrot() -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            fractal_type: "mandelbrot".to_string(), params: vec![],
            x_min: -2.5, x_max: 1.0, y_min: -1.25, y_max: 1.25,
            resolution: 200, max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0), visible: true,
        }
    }
    pub fn julia(cr: f64, ci: f64) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            fractal_type: "julia".to_string(), params: vec![cr, ci],
            x_min: -2.0, x_max: 2.0, y_min: -2.0, y_max: 2.0,
            resolution: 200, max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0), visible: true,
        }
    }
    pub fn burning_ship() -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            fractal_type: "burning_ship".to_string(), params: vec![],
            x_min: -2.0, x_max: 1.0, y_min: -2.0, y_max: 1.0,
            resolution: 200, max_iter: 256,
            color: Color::new(0.0, 0.0, 0.0, 1.0), visible: true,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_resolution(mut self, res: usize) -> Self { self.resolution = res; self }
    pub fn with_max_iter(mut self, max_iter: u32) -> Self { self.max_iter = max_iter; self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HyperSurface4DObj {
    pub id: ObjectId, pub label: String,
    pub surface_type: String,
    pub params: Vec<f64>,
    pub rotation_angles: Vec<f64>,
    pub resolution: usize,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl HyperSurface4DObj {
    pub fn hypercube() -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            surface_type: "hypercube".to_string(), params: vec![3.0],
            rotation_angles: vec![0.3, 0.5, 0.7],
            resolution: 16,
            color: Color::new(0.8, 0.2, 0.8, 1.0), visible: true, width: 1.5,
        }
    }
    pub fn hypersphere() -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            surface_type: "hypersphere".to_string(), params: vec![3.0],
            rotation_angles: vec![0.3, 0.5, 0.7],
            resolution: 20,
            color: Color::new(0.2, 0.8, 0.8, 1.0), visible: true, width: 1.5,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_rotation(mut self, angles: Vec<f64>) -> Self { self.rotation_angles = angles; self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorField3DObj {
    pub id: ObjectId, pub label: String,
    pub expr_u: String, pub expr_v: String, pub expr_w: String,
    pub x_min: f64, pub x_max: f64,
    pub y_min: f64, pub y_max: f64,
    pub z_min: f64, pub z_max: f64,
    pub density: usize,
    pub color: Color, pub visible: bool,
}
impl VectorField3DObj {
    pub fn new(expr_u: &str, expr_v: &str, expr_w: &str) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            expr_u: expr_u.to_string(), expr_v: expr_v.to_string(), expr_w: expr_w.to_string(),
            x_min: -3.0, x_max: 3.0, y_min: -3.0, y_max: 3.0, z_min: -3.0, z_max: 3.0,
            density: 5,
            color: Color::new(0.8, 0.4, 0.0, 1.0), visible: true,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_bounds(mut self, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> Self {
        self.x_min = x.0; self.x_max = x.1;
        self.y_min = y.0; self.y_max = y.1;
        self.z_min = z.0; self.z_max = z.1;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramObj {
    pub id: ObjectId, pub label: String,
    pub data: Vec<f64>,
    pub bins: usize,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
    pub fill_color: Option<Color>,
}
impl HistogramObj {
    pub fn new(data: Vec<f64>, bins: usize) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            data, bins,
            x_min: -5.0, x_max: 5.0, y_min: -5.0, y_max: 5.0,
            color: Color::BLACK, visible: true, width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.4)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0; self.x_max = x.1; self.y_min = y.0; self.y_max = y.1; self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScatterPlotObj {
    pub id: ObjectId, pub label: String,
    pub xs: Vec<f64>, pub ys: Vec<f64>,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool,
    pub point_size: f32,
}
impl ScatterPlotObj {
    pub fn new(xs: Vec<f64>, ys: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            xs, ys,
            x_min: -5.0, x_max: 5.0, y_min: -5.0, y_max: 5.0,
            color: Color::BLUE, visible: true, point_size: 5.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0; self.x_max = x.1; self.y_min = y.0; self.y_max = y.1; self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxPlotObj {
    pub id: ObjectId, pub label: String,
    pub data: Vec<f64>,
    pub position: f64,
    pub width_box: f64,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
    pub fill_color: Option<Color>,
}
impl BoxPlotObj {
    pub fn new(data: Vec<f64>) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            data, position: 0.0, width_box: 1.0,
            x_min: -5.0, x_max: 5.0, y_min: -5.0, y_max: 5.0,
            color: Color::BLACK, visible: true, width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.3)),
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_position(mut self, pos: f64, w: f64) -> Self { self.position = pos; self.width_box = w; self }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegressionLineObj {
    pub id: ObjectId, pub label: String,
    pub xs: Vec<f64>, pub ys: Vec<f64>,
    pub slope: f64, pub intercept: f64, pub r_squared: f64,
    pub regression_type: String,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl RegressionLineObj {
    pub fn linear(xs: Vec<f64>, ys: Vec<f64>, slope: f64, intercept: f64, r2: f64) -> Self {
        Self {
            id: ObjectId::new(), label: String::new(),
            xs, ys, slope, intercept, r_squared: r2,
            regression_type: "linear".to_string(),
            x_min: -5.0, x_max: 5.0, y_min: -5.0, y_max: 5.0,
            color: Color::RED, visible: true, width: 2.0,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
    pub fn with_view(mut self, x: (f64, f64), y: (f64, f64)) -> Self {
        self.x_min = x.0; self.x_max = x.1; self.y_min = y.0; self.y_max = y.1; self
    }
}
