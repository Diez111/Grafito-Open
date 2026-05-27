use grafito_geometry::{Color, Point2, Circle as GeomCircle};
use serde::{Deserialize, Serialize};
use crate::id::ObjectId;

/// A geometric object in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GeoObject {
    Point(PointObj),
    Line(LineObj),
    Circle(CircleObj),
    Polygon(PolygonObj),
    Function(FunctionObj),
    Text(TextObj),
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
            label: expr.into(),
            expr: String::new(),
            color: Color::BLUE,
            visible: true,
            width: 2.0,
            domain_min: None,
            domain_max: None,
        }
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
