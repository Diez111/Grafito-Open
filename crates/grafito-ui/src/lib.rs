//! Grafito UI — Componentes y paneles de interfaz construidos con egui.
//!
//! Provee la toolbar, la paleta de comandos, el panel de álgebra, el selector
//! de color, temas y la enumeración [`Tool`] que sincroniza el modo de
//! interacción del canvas.
//!
//! # Ejemplo mínimo
//!
//! ```
//! use grafito_ui::Tool;
//!
//! let mut tool = Tool::default();
//! assert_eq!(tool, Tool::Select);
//!
//! tool = Tool::Point;
//! assert_eq!(tool.cursor_icon(), egui::CursorIcon::Crosshair);
//! ```

pub mod animation;
pub mod color_picker;
pub mod command_palette;
pub mod icons;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod toolbar;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Tool {
    // Basic 2D tools
    #[default]
    Select,
    Point,
    Line,
    Circle,
    Polygon,
    Pencil,
    Function,
    // 3D tools
    Point3D,
    Sphere3D,
    Cube3D,
    // Advanced tools
    Attractor,
    Fractal,
    Histogram,
    ScatterPlot,
    Root,
    Extremum,
    Inflection,
    YIntercept,
    XIntercept,
    Analyze,
    Intersect,
    // Curve creators
    ParametricCurve2D,
    PolarCurve,
    ImplicitCurve,
    VectorField2D,
    // Construction tools
    Segment,
    Ray,
    Vector,
    RegularPolygon,
    Tangent,
    Perpendicular,
    Locus,
    Midpoint,
    // Measurement tools
    Distance,
    Angle,
    Area,
    Slope,
    // Control tools
    Slider,
    Button,
    Image,
    Eraser,
    // Complex & Visualization
    DomainColoring,
    HeatMap,
    ComplexGrid,
    // Numeric constraints
    DistanceConstraint,
    AngleConstraint,
    Coincident,
    Horizontal,
    Vertical,
    EqualLength,
    Symmetry,
    // Conic constructions
    EllipseByFoci,
    ParabolaByFocusDirectrix,
    HyperbolaByFoci,
    ConicByFivePoints,
    // Polygon booleans
    PolygonUnion,
    PolygonIntersection,
    PolygonDifference,
    PolygonXor,
}

impl Tool {
    pub fn name(&self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Point => "Point",
            Tool::Line => "Line",
            Tool::Circle => "Circle",
            Tool::Polygon => "Polygon",
            Tool::Pencil => "Pencil",
            Tool::Function => "Function",
            Tool::Point3D => "Point3D",
            Tool::Sphere3D => "Sphere3D",
            Tool::Cube3D => "Cube3D",
            Tool::Attractor => "Attractor",
            Tool::Fractal => "Fractal",
            Tool::Histogram => "Histogram",
            Tool::ScatterPlot => "ScatterPlot",
            Tool::Root => "Root",
            Tool::Extremum => "Extremum",
            Tool::Inflection => "Inflection",
            Tool::YIntercept => "YIntercept",
            Tool::XIntercept => "XIntercept",
            Tool::Analyze => "Analyze",
            Tool::Intersect => "Intersect",
            Tool::ParametricCurve2D => "ParametricCurve2D",
            Tool::PolarCurve => "PolarCurve",
            Tool::ImplicitCurve => "ImplicitCurve",
            Tool::VectorField2D => "VectorField2D",
            Tool::Segment => "Segment",
            Tool::Ray => "Ray",
            Tool::Vector => "Vector",
            Tool::RegularPolygon => "RegularPolygon",
            Tool::Tangent => "Tangent",
            Tool::Perpendicular => "Perpendicular",
            Tool::Locus => "Locus",
            Tool::Midpoint => "Midpoint",
            Tool::Distance => "Distance",
            Tool::Angle => "Angle",
            Tool::Area => "Area",
            Tool::Slope => "Slope",
            Tool::Slider => "Slider",
            Tool::Button => "Button",
            Tool::Image => "Image",
            Tool::DomainColoring => "DomainColoring",
            Tool::HeatMap => "HeatMap",
            Tool::ComplexGrid => "ComplexGrid",
            Tool::DistanceConstraint => "DistanceConstraint",
            Tool::AngleConstraint => "AngleConstraint",
            Tool::Coincident => "Coincident",
            Tool::Horizontal => "Horizontal",
            Tool::Vertical => "Vertical",
            Tool::EqualLength => "EqualLength",
            Tool::Symmetry => "Symmetry",
            Tool::EllipseByFoci => "EllipseByFoci",
            Tool::ParabolaByFocusDirectrix => "ParabolaByFocusDirectrix",
            Tool::HyperbolaByFoci => "HyperbolaByFoci",
            Tool::ConicByFivePoints => "ConicByFivePoints",
            Tool::PolygonUnion => "PolygonUnion",
            Tool::PolygonIntersection => "PolygonIntersection",
            Tool::PolygonDifference => "PolygonDifference",
            Tool::PolygonXor => "PolygonXor",
            Tool::Eraser => "Eraser",
        }
    }

    pub fn cursor_icon(&self) -> egui::CursorIcon {
        match self {
            Tool::Select => egui::CursorIcon::Default,
            Tool::Point | Tool::Point3D => egui::CursorIcon::Crosshair,
            Tool::Line
            | Tool::Circle
            | Tool::Polygon
            | Tool::Pencil
            | Tool::Function
            | Tool::Sphere3D
            | Tool::Cube3D
            | Tool::Attractor
            | Tool::Fractal
            | Tool::Histogram
            | Tool::ScatterPlot
            | Tool::Root
            | Tool::Extremum
            | Tool::Inflection
            | Tool::YIntercept
            | Tool::XIntercept
            | Tool::Analyze
            | Tool::Intersect
            | Tool::ParametricCurve2D
            | Tool::PolarCurve
            | Tool::ImplicitCurve
            | Tool::VectorField2D
            | Tool::Segment
            | Tool::Ray
            | Tool::Vector
            | Tool::RegularPolygon
            | Tool::Tangent
            | Tool::Perpendicular
            | Tool::Locus
            | Tool::Midpoint
            | Tool::Distance
            | Tool::Angle
            | Tool::Area
            | Tool::Slope
            | Tool::Slider
            | Tool::Button
            | Tool::Image
            | Tool::Eraser
            | Tool::DomainColoring
            | Tool::HeatMap
            | Tool::ComplexGrid
            | Tool::Coincident
            | Tool::Horizontal
            | Tool::Vertical
            | Tool::EqualLength
            | Tool::Symmetry
            | Tool::EllipseByFoci
            | Tool::ParabolaByFocusDirectrix
            | Tool::HyperbolaByFoci
            | Tool::ConicByFivePoints
            | Tool::PolygonUnion
            | Tool::PolygonIntersection
            | Tool::PolygonDifference
            | Tool::PolygonXor => egui::CursorIcon::Crosshair,
            Tool::DistanceConstraint | Tool::AngleConstraint => egui::CursorIcon::Crosshair,
        }
    }
}
