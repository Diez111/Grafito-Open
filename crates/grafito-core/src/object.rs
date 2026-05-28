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
    Surface3D(Surface3DObj),
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
            GeoObject::Surface3D(o) => o.id,
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
            GeoObject::Surface3D(o) => &o.label,
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
            GeoObject::Surface3D(o) => o.color,
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
            GeoObject::Surface3D(o) => o.color = color,
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
            GeoObject::Surface3D(o) => o.visible,
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
            GeoObject::Surface3D(o) => o.visible = visible,
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
            GeoObject::Surface3D(_) => "Surface3D",
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
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
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

// ── 3D Parametric Surface ──
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface3DObj {
    pub id: ObjectId, pub label: String, pub expr: String,
    pub x_min: f64, pub x_max: f64, pub y_min: f64, pub y_max: f64,
    pub color: Color, pub visible: bool, pub width: f32,
}
impl Surface3DObj {
    pub fn new(expr: impl Into<String>, xr: (f64, f64), yr: (f64, f64)) -> Self {
        Self { id: ObjectId::new(), label: String::new(), expr: expr.into(),
               x_min: xr.0, x_max: xr.1, y_min: yr.0, y_max: yr.1,
               color: Color::BLUE, visible: true, width: 1.0 }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self { self.label = l.into(); self }
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
