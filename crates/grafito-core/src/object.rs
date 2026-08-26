use crate::id::ObjectId;
use crate::pencil::PencilObj;
use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_geometry::statistics::{FitDiagnostics, FitKind, FitResult};
use grafito_geometry::{
    Circle as GeomCircle, Color, Point2, Point3D, RegularPolychoron, RegularPolytopeFamily, AABB,
    MAX_REGULAR_POLYTOPE_DIMENSION, MIN_REGULAR_POLYTOPE_DIMENSION,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, RwLock};

/// A geometric object in the document (2D and 3D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
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
    Arc(ArcObj),
    Sector(SectorObj),
    BezierCurve(BezierCurveObj),
    Spline(SplineObj),
    // 3D
    Point3D(Point3DObj),
    Segment3D(Segment3DObj),
    Plane3D(Plane3DObj),
    Line3D(Line3DObj),
    Sphere3D(Sphere3DObj),
    Cube3D(Cube3DObj),
    Tetrahedron3D(Tetrahedron3DObj),
    Pyramid3D(Pyramid3DObj),
    Cone3D(Cone3DObj),
    Cylinder3D(Cylinder3DObj),
    Torus3D(Torus3DObj),
    MoebiusStrip(MoebiusStripObj),
    Surface3D(Surface3DObj),
    Prism3D(Prism3DObj),
    Quadric3D(Quadric3DObj),

    // AM2/AM3 Advanced
    ParametricCurve2D(ParametricCurve2DObj),
    ParametricCurve3D(ParametricCurve3DObj),
    PolarCurve(PolarCurveObj),
    ImplicitCurve(ImplicitCurveObj),
    VectorField2D(VectorField2DObj),
    ComplexGrid(ComplexGridObj),
    ComplexMapping(ComplexMappingObj),
    ComplexIntegral(ComplexIntegralObj),

    // AM4 Advanced: Attractors, Fractals, 4D, Statistics
    Attractor3D(Attractor3DObj),
    Fractal2D(Fractal2DObj),
    RegularPolychoron4D(RegularPolychoron4DObj),
    RegularPolytopeND(RegularPolytopeNDObj),
    HyperSurface4D(HyperSurface4DObj),
    VectorField3D(VectorField3DObj),
    Histogram(HistogramObj),
    ScatterPlot(ScatterPlotObj),
    BoxPlot(BoxPlotObj),
    RegressionLine(RegressionLineObj),
    DataTable(DataTableObj),
    PhasePortrait(PhasePortraitObj),

    // Transformed Wrapper
    Transformed(TransformedObj),
}

/// Espacio principal donde se renderiza un objeto del documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderSpace {
    D2,
    D3,
}

impl GeoObject {
    pub fn render_space(&self) -> RenderSpace {
        match self {
            GeoObject::Point(_)
            | GeoObject::Line(_)
            | GeoObject::Circle(_)
            | GeoObject::Polygon(_)
            | GeoObject::Pencil(_)
            | GeoObject::Function(_)
            | GeoObject::Text(_)
            | GeoObject::Ellipse(_)
            | GeoObject::Parabola(_)
            | GeoObject::Hyperbola(_)
            | GeoObject::Arc(_)
            | GeoObject::Sector(_)
            | GeoObject::BezierCurve(_)
            | GeoObject::Spline(_)
            | GeoObject::ParametricCurve2D(_)
            | GeoObject::PolarCurve(_)
            | GeoObject::ImplicitCurve(_)
            | GeoObject::VectorField2D(_)
            | GeoObject::ComplexGrid(_)
            | GeoObject::ComplexMapping(_)
            | GeoObject::ComplexIntegral(_)
            | GeoObject::Fractal2D(_)
            | GeoObject::Histogram(_)
            | GeoObject::ScatterPlot(_)
            | GeoObject::BoxPlot(_)
            | GeoObject::RegressionLine(_)
            | GeoObject::DataTable(_)
            | GeoObject::PhasePortrait(_) => RenderSpace::D2,
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
            | GeoObject::Prism3D(_)
            | GeoObject::Quadric3D(_)
            | GeoObject::ParametricCurve3D(_)
            | GeoObject::Attractor3D(_)
            | GeoObject::RegularPolychoron4D(_)
            | GeoObject::RegularPolytopeND(_)
            | GeoObject::HyperSurface4D(_)
            | GeoObject::VectorField3D(_) => RenderSpace::D3,
            GeoObject::Transformed(o) => o.inner.render_space(),
        }
    }

    pub fn is_3d(&self) -> bool {
        self.render_space() == RenderSpace::D3
    }

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
            GeoObject::Arc(o) => o.id,
            GeoObject::Sector(o) => o.id,
            GeoObject::BezierCurve(o) => o.id,
            GeoObject::Spline(o) => o.id,
            GeoObject::Point3D(o) => o.id,
            GeoObject::Segment3D(o) => o.id,
            GeoObject::Plane3D(o) => o.id,
            GeoObject::Line3D(o) => o.id,
            GeoObject::Sphere3D(o) => o.id,
            GeoObject::Cube3D(o) => o.id,
            GeoObject::Tetrahedron3D(o) => o.id,
            GeoObject::Pyramid3D(o) => o.id,
            GeoObject::Cone3D(o) => o.id,
            GeoObject::Cylinder3D(o) => o.id,
            GeoObject::Torus3D(o) => o.id,
            GeoObject::MoebiusStrip(o) => o.id,
            GeoObject::Surface3D(o) => o.id,
            GeoObject::Prism3D(o) => o.id,
            GeoObject::Quadric3D(o) => o.id,
            GeoObject::ParametricCurve2D(o) => o.id,
            GeoObject::ParametricCurve3D(o) => o.id,
            GeoObject::PolarCurve(o) => o.id,
            GeoObject::VectorField2D(o) => o.id,
            GeoObject::ComplexGrid(o) => o.id,
            GeoObject::ComplexMapping(o) => o.id,
            GeoObject::ComplexIntegral(o) => o.id,
            GeoObject::ImplicitCurve(o) => o.id,
            GeoObject::Attractor3D(o) => o.id,
            GeoObject::Fractal2D(o) => o.id,
            GeoObject::RegularPolychoron4D(o) => o.id,
            GeoObject::RegularPolytopeND(o) => o.id,
            GeoObject::HyperSurface4D(o) => o.id,
            GeoObject::VectorField3D(o) => o.id,
            GeoObject::Histogram(o) => o.id,
            GeoObject::ScatterPlot(o) => o.id,
            GeoObject::BoxPlot(o) => o.id,
            GeoObject::RegressionLine(o) => o.id,
            GeoObject::DataTable(o) => o.id,
            GeoObject::PhasePortrait(o) => o.id,
            GeoObject::Pencil(o) => o.id,
            GeoObject::Transformed(o) => o.inner.id(),
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
            GeoObject::Arc(o) => &o.label,
            GeoObject::Sector(o) => &o.label,
            GeoObject::BezierCurve(o) => &o.label,
            GeoObject::Spline(o) => &o.label,
            GeoObject::Point3D(o) => &o.label,
            GeoObject::Segment3D(o) => &o.label,
            GeoObject::Plane3D(o) => &o.label,
            GeoObject::Line3D(o) => &o.label,
            GeoObject::Sphere3D(o) => &o.label,
            GeoObject::Cube3D(o) => &o.label,
            GeoObject::Tetrahedron3D(o) => &o.label,
            GeoObject::Pyramid3D(o) => &o.label,
            GeoObject::Cone3D(o) => &o.label,
            GeoObject::Cylinder3D(o) => &o.label,
            GeoObject::Torus3D(o) => &o.label,
            GeoObject::MoebiusStrip(o) => &o.label,
            GeoObject::Surface3D(o) => &o.label,
            GeoObject::Prism3D(o) => &o.label,
            GeoObject::Quadric3D(o) => &o.label,
            GeoObject::ParametricCurve2D(o) => &o.label,
            GeoObject::ParametricCurve3D(o) => &o.label,
            GeoObject::PolarCurve(o) => &o.label,
            GeoObject::VectorField2D(o) => &o.label,
            GeoObject::ComplexGrid(o) => &o.label,
            GeoObject::ComplexMapping(o) => &o.label,
            GeoObject::ComplexIntegral(o) => &o.label,
            GeoObject::ImplicitCurve(o) => &o.label,
            GeoObject::Attractor3D(o) => &o.label,
            GeoObject::Fractal2D(o) => &o.label,
            GeoObject::RegularPolychoron4D(o) => &o.label,
            GeoObject::RegularPolytopeND(o) => &o.label,
            GeoObject::HyperSurface4D(o) => &o.label,
            GeoObject::VectorField3D(o) => &o.label,
            GeoObject::Histogram(o) => &o.label,
            GeoObject::ScatterPlot(o) => &o.label,
            GeoObject::BoxPlot(o) => &o.label,
            GeoObject::RegressionLine(o) => &o.label,
            GeoObject::DataTable(o) => &o.label,
            GeoObject::PhasePortrait(o) => &o.label,
            GeoObject::Pencil(o) => &o.label,
            GeoObject::Transformed(o) => o.inner.label(),
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
            GeoObject::Arc(o) => o.label = label,
            GeoObject::Sector(o) => o.label = label,
            GeoObject::BezierCurve(o) => o.label = label,
            GeoObject::Spline(o) => o.label = label,
            GeoObject::Point3D(o) => o.label = label,
            GeoObject::Segment3D(o) => o.label = label,
            GeoObject::Plane3D(o) => o.label = label,
            GeoObject::Line3D(o) => o.label = label,
            GeoObject::Sphere3D(o) => o.label = label,
            GeoObject::Cube3D(o) => o.label = label,
            GeoObject::Tetrahedron3D(o) => o.label = label,
            GeoObject::Pyramid3D(o) => o.label = label,
            GeoObject::Cone3D(o) => o.label = label,
            GeoObject::Cylinder3D(o) => o.label = label,
            GeoObject::Torus3D(o) => o.label = label.clone(),
            GeoObject::MoebiusStrip(o) => o.label = label.clone(),
            GeoObject::Surface3D(o) => o.label = label.clone(),
            GeoObject::Prism3D(o) => o.label = label.clone(),
            GeoObject::Quadric3D(o) => o.label = label.clone(),
            GeoObject::ParametricCurve2D(o) => o.label = label,
            GeoObject::ParametricCurve3D(o) => o.label = label,
            GeoObject::PolarCurve(o) => o.label = label,
            GeoObject::VectorField2D(o) => o.label = label,
            GeoObject::ComplexGrid(o) => o.label = label,
            GeoObject::ComplexMapping(o) => o.label = label,
            GeoObject::ComplexIntegral(o) => o.label = label,
            GeoObject::ImplicitCurve(o) => o.label = label,
            GeoObject::Attractor3D(o) => o.label = label,
            GeoObject::Fractal2D(o) => o.label = label,
            GeoObject::RegularPolychoron4D(o) => o.label = label,
            GeoObject::RegularPolytopeND(o) => o.label = label,
            GeoObject::HyperSurface4D(o) => o.label = label,
            GeoObject::VectorField3D(o) => o.label = label,
            GeoObject::Histogram(o) => o.label = label,
            GeoObject::ScatterPlot(o) => o.label = label,
            GeoObject::BoxPlot(o) => o.label = label,
            GeoObject::RegressionLine(o) => o.label = label,
            GeoObject::DataTable(o) => o.label = label,
            GeoObject::PhasePortrait(o) => o.label = label,
            GeoObject::Pencil(o) => o.label = label,
            GeoObject::Transformed(o) => o.inner.set_label(label),
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
            GeoObject::Arc(o) => o.color,
            GeoObject::Sector(o) => o.color,
            GeoObject::BezierCurve(o) => o.color,
            GeoObject::Spline(o) => o.color,
            GeoObject::Point3D(o) => o.color,
            GeoObject::Segment3D(o) => o.color,
            GeoObject::Plane3D(o) => o.color,
            GeoObject::Line3D(o) => o.color,
            GeoObject::Sphere3D(o) => o.color,
            GeoObject::Cube3D(o) => o.color,
            GeoObject::Tetrahedron3D(o) => o.color,
            GeoObject::Pyramid3D(o) => o.color,
            GeoObject::Cone3D(o) => o.color,
            GeoObject::Cylinder3D(o) => o.color,
            GeoObject::Torus3D(o) => o.color,
            GeoObject::MoebiusStrip(o) => o.color,
            GeoObject::Surface3D(o) => o.color,
            GeoObject::Prism3D(o) => o.color,
            GeoObject::Quadric3D(o) => o.color,
            GeoObject::ParametricCurve2D(o) => o.color,
            GeoObject::ParametricCurve3D(o) => o.color,
            GeoObject::PolarCurve(o) => o.color,
            GeoObject::VectorField2D(o) => o.color,
            GeoObject::ComplexGrid(o) => o.color,
            GeoObject::ComplexMapping(o) => o.color,
            GeoObject::ComplexIntegral(o) => o.color,
            GeoObject::ImplicitCurve(o) => o.color,
            GeoObject::Attractor3D(o) => o.color,
            GeoObject::Fractal2D(o) => o.color,
            GeoObject::RegularPolychoron4D(o) => o.color,
            GeoObject::RegularPolytopeND(o) => o.color,
            GeoObject::HyperSurface4D(o) => o.color,
            GeoObject::VectorField3D(o) => o.color,
            GeoObject::Histogram(o) => o.color,
            GeoObject::ScatterPlot(o) => o.color,
            GeoObject::BoxPlot(o) => o.color,
            GeoObject::RegressionLine(o) => o.color,
            GeoObject::DataTable(o) => o.color,
            GeoObject::PhasePortrait(o) => o.color,
            GeoObject::Transformed(o) => o.inner.color(),
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
            GeoObject::Arc(o) => o.color = color,
            GeoObject::Sector(o) => o.color = color,
            GeoObject::BezierCurve(o) => o.color = color,
            GeoObject::Spline(o) => o.color = color,
            GeoObject::Point3D(o) => o.color = color,
            GeoObject::Segment3D(o) => o.color = color,
            GeoObject::Plane3D(o) => o.color = color,
            GeoObject::Line3D(o) => o.color = color,
            GeoObject::Sphere3D(o) => o.color = color,
            GeoObject::Cube3D(o) => o.color = color,
            GeoObject::Tetrahedron3D(o) => o.color = color,
            GeoObject::Pyramid3D(o) => o.color = color,
            GeoObject::Cone3D(o) => o.color = color,
            GeoObject::Cylinder3D(o) => o.color = color,
            GeoObject::Torus3D(o) => o.color = color,
            GeoObject::MoebiusStrip(o) => o.color = color,
            GeoObject::Surface3D(o) => o.color = color,
            GeoObject::Prism3D(o) => o.color = color,
            GeoObject::Quadric3D(o) => o.color = color,
            GeoObject::ParametricCurve2D(o) => o.color = color,
            GeoObject::ParametricCurve3D(o) => o.color = color,
            GeoObject::PolarCurve(o) => o.color = color,
            GeoObject::VectorField2D(o) => o.color = color,
            GeoObject::ComplexGrid(o) => o.color = color,
            GeoObject::ComplexMapping(o) => o.color = color,
            GeoObject::ComplexIntegral(o) => o.color = color,
            GeoObject::ImplicitCurve(o) => o.color = color,
            GeoObject::Attractor3D(o) => o.color = color,
            GeoObject::Fractal2D(o) => o.color = color,
            GeoObject::RegularPolychoron4D(o) => o.color = color,
            GeoObject::RegularPolytopeND(o) => o.color = color,
            GeoObject::HyperSurface4D(o) => o.color = color,
            GeoObject::VectorField3D(o) => o.color = color,
            GeoObject::Histogram(o) => o.color = color,
            GeoObject::ScatterPlot(o) => o.color = color,
            GeoObject::BoxPlot(o) => o.color = color,
            GeoObject::RegressionLine(o) => o.color = color,
            GeoObject::DataTable(o) => o.color = color,
            GeoObject::PhasePortrait(o) => o.color = color,
            GeoObject::Transformed(o) => o.inner.set_color(color),
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
            GeoObject::Arc(o) => o.visible,
            GeoObject::Sector(o) => o.visible,
            GeoObject::BezierCurve(o) => o.visible,
            GeoObject::Spline(o) => o.visible,
            GeoObject::Point3D(o) => o.visible,
            GeoObject::Segment3D(o) => o.visible,
            GeoObject::Plane3D(o) => o.visible,
            GeoObject::Line3D(o) => o.visible,
            GeoObject::Sphere3D(o) => o.visible,
            GeoObject::Cube3D(o) => o.visible,
            GeoObject::Tetrahedron3D(o) => o.visible,
            GeoObject::Pyramid3D(o) => o.visible,
            GeoObject::Cone3D(o) => o.visible,
            GeoObject::Cylinder3D(o) => o.visible,
            GeoObject::Torus3D(o) => o.visible,
            GeoObject::MoebiusStrip(o) => o.visible,
            GeoObject::Surface3D(o) => o.visible,
            GeoObject::Prism3D(o) => o.visible,
            GeoObject::Quadric3D(o) => o.visible,
            GeoObject::ParametricCurve2D(o) => o.visible,
            GeoObject::ParametricCurve3D(o) => o.visible,
            GeoObject::PolarCurve(o) => o.visible,
            GeoObject::VectorField2D(o) => o.visible,
            GeoObject::ComplexGrid(o) => o.visible,
            GeoObject::ComplexMapping(o) => o.visible,
            GeoObject::ComplexIntegral(o) => o.visible,
            GeoObject::ImplicitCurve(o) => o.visible,
            GeoObject::Attractor3D(o) => o.visible,
            GeoObject::Fractal2D(o) => o.visible,
            GeoObject::RegularPolychoron4D(o) => o.visible,
            GeoObject::RegularPolytopeND(o) => o.visible,
            GeoObject::HyperSurface4D(o) => o.visible,
            GeoObject::VectorField3D(o) => o.visible,
            GeoObject::Histogram(o) => o.visible,
            GeoObject::ScatterPlot(o) => o.visible,
            GeoObject::BoxPlot(o) => o.visible,
            GeoObject::RegressionLine(o) => o.visible,
            // Las tablas no tienen geometría de canvas y nunca participan del
            // render/export genérico aunque un documento legado marque visible.
            GeoObject::DataTable(_) => false,
            GeoObject::PhasePortrait(o) => o.visible,
            GeoObject::Transformed(o) => o.inner.is_visible(),
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
            GeoObject::Arc(o) => o.visible = visible,
            GeoObject::Sector(o) => o.visible = visible,
            GeoObject::BezierCurve(o) => o.visible = visible,
            GeoObject::Spline(o) => o.visible = visible,
            GeoObject::Point3D(o) => o.visible = visible,
            GeoObject::Segment3D(o) => o.visible = visible,
            GeoObject::Plane3D(o) => o.visible = visible,
            GeoObject::Line3D(o) => o.visible = visible,
            GeoObject::Sphere3D(o) => o.visible = visible,
            GeoObject::Cube3D(o) => o.visible = visible,
            GeoObject::Tetrahedron3D(o) => o.visible = visible,
            GeoObject::Pyramid3D(o) => o.visible = visible,
            GeoObject::Cone3D(o) => o.visible = visible,
            GeoObject::Cylinder3D(o) => o.visible = visible,
            GeoObject::Torus3D(o) => o.visible = visible,
            GeoObject::MoebiusStrip(o) => o.visible = visible,
            GeoObject::Surface3D(o) => o.visible = visible,
            GeoObject::Prism3D(o) => o.visible = visible,
            GeoObject::Quadric3D(o) => o.visible = visible,
            GeoObject::ParametricCurve2D(o) => o.visible = visible,
            GeoObject::ParametricCurve3D(o) => o.visible = visible,
            GeoObject::PolarCurve(o) => o.visible = visible,
            GeoObject::VectorField2D(o) => o.visible = visible,
            GeoObject::ComplexGrid(o) => o.visible = visible,
            GeoObject::ComplexMapping(o) => o.visible = visible,
            GeoObject::ComplexIntegral(o) => o.visible = visible,
            GeoObject::ImplicitCurve(o) => o.visible = visible,
            GeoObject::Attractor3D(o) => o.visible = visible,
            GeoObject::Fractal2D(o) => o.visible = visible,
            GeoObject::RegularPolychoron4D(o) => o.visible = visible,
            GeoObject::RegularPolytopeND(o) => o.visible = visible,
            GeoObject::HyperSurface4D(o) => o.visible = visible,
            GeoObject::VectorField3D(o) => o.visible = visible,
            GeoObject::Histogram(o) => o.visible = visible,
            GeoObject::ScatterPlot(o) => o.visible = visible,
            GeoObject::BoxPlot(o) => o.visible = visible,
            GeoObject::RegressionLine(o) => o.visible = visible,
            GeoObject::DataTable(_) => {}
            GeoObject::PhasePortrait(o) => o.visible = visible,
            GeoObject::Transformed(o) => o.inner.set_visible(visible),
        }
    }

    pub fn invalidate_cache(&self) {
        match self {
            GeoObject::Function(o) => o.invalidate_cache(),
            GeoObject::Surface3D(o) => o.invalidate_cache(),
            GeoObject::ParametricCurve2D(o) => o.invalidate_cache(),
            GeoObject::ParametricCurve3D(o) => o.invalidate_cache(),
            GeoObject::PolarCurve(o) => o.invalidate_cache(),
            GeoObject::VectorField2D(o) => o.invalidate_cache(),
            GeoObject::ImplicitCurve(o) => o.invalidate_cache(),
            GeoObject::Transformed(o) => o.inner.invalidate_cache(),
            _ => {}
        }
    }

    /// Drops runtime-only caches before a document is used as a transaction
    /// staging area. Clones normally share these `Arc` caches with the live
    /// document, so staging must detach them before a failed operation can
    /// invalidate a cache visible to the caller.
    pub(crate) fn detach_runtime_caches(&mut self) {
        let mut pending = vec![self];
        while let Some(object) = pending.pop() {
            match object {
                GeoObject::Function(o) => {
                    o.cached_key = Default::default();
                    o.cached_samples = Default::default();
                }
                GeoObject::Surface3D(o) => {
                    o.cached_grid = Default::default();
                    o.cached_key = Default::default();
                }
                GeoObject::ParametricCurve2D(o) => {
                    o.cached_samples = Default::default();
                    o.cached_key = Default::default();
                }
                GeoObject::ParametricCurve3D(o) => {
                    o.cached_samples = Default::default();
                    o.cached_key = Default::default();
                }
                GeoObject::PolarCurve(o) => {
                    o.cached_samples = Default::default();
                    o.cached_key = Default::default();
                }
                GeoObject::VectorField2D(o) => {
                    o.cached_samples = Default::default();
                    o.cached_key = Default::default();
                }
                GeoObject::ImplicitCurve(o) => {
                    o.cached_segments = Default::default();
                    o.cached_key = Default::default();
                    o.cached_region = Default::default();
                    o.cached_asts = Default::default();
                }
                GeoObject::Transformed(o) => pending.push(o.inner.as_mut()),
                _ => {}
            }
        }
    }

    /// Returns every document object referenced by this object's semantic data.
    /// Runtime caches and nested rendering state are intentionally excluded.
    pub fn referenced_object_ids(&self) -> Vec<ObjectId> {
        match self {
            GeoObject::Pencil(object) => object
                .locus_binding()
                .map(|binding| vec![binding.driver, binding.target])
                .unwrap_or_default(),
            GeoObject::Function(object) => object
                .fit
                .as_ref()
                .map(|fit| vec![fit.source])
                .unwrap_or_default(),
            GeoObject::ScatterPlot(object) => object
                .source_data
                .map(|source| vec![source])
                .unwrap_or_default(),
            GeoObject::ComplexMapping(object) => vec![object.target],
            GeoObject::ComplexIntegral(object) => vec![object.target],
            GeoObject::Transformed(object) => object.inner.referenced_object_ids(),
            _ => Vec::new(),
        }
    }

    /// True when serializing this object would disclose local measurements or
    /// diagnostics rather than ordinary geometric metadata.
    pub fn contains_private_data(&self) -> bool {
        match self {
            GeoObject::Pencil(pencil) => pencil.is_dynamic_locus(),
            GeoObject::DataTable(_)
            | GeoObject::Histogram(_)
            | GeoObject::ScatterPlot(_)
            | GeoObject::BoxPlot(_)
            | GeoObject::RegressionLine(_) => true,
            GeoObject::Function(function) => function.fit.is_some(),
            GeoObject::Transformed(object) => object.inner.contains_private_data(),
            _ => false,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            GeoObject::Point(_) => "Point",
            GeoObject::Line(_) => "Line",
            GeoObject::Circle(_) => "Circle",
            GeoObject::Polygon(_) => "Polygon",
            GeoObject::Pencil(pencil) if pencil.is_dynamic_locus() => "Locus",
            GeoObject::Pencil(_) => "Pencil",
            GeoObject::Function(_) => "Function",
            GeoObject::Text(_) => "Text",
            GeoObject::Ellipse(_) => "Ellipse",
            GeoObject::Parabola(_) => "Parabola",
            GeoObject::Hyperbola(_) => "Hyperbola",
            GeoObject::Arc(_) => "Arc",
            GeoObject::Sector(_) => "Sector",
            GeoObject::BezierCurve(_) => "BezierCurve",
            GeoObject::Spline(_) => "Spline",
            GeoObject::Point3D(_) => "Point3D",
            GeoObject::Segment3D(_) => "Segment3D",
            GeoObject::Plane3D(_) => "Plane3D",
            GeoObject::Line3D(_) => "Line3D",
            GeoObject::Sphere3D(_) => "Sphere3D",
            GeoObject::Cube3D(_) => "Cube3D",
            GeoObject::Tetrahedron3D(_) => "Tetrahedron3D",
            GeoObject::Pyramid3D(_) => "Pyramid3D",
            GeoObject::Cone3D(_) => "Cone3D",
            GeoObject::Cylinder3D(_) => "Cylinder3D",
            GeoObject::Torus3D(_) => "Torus3D",
            GeoObject::MoebiusStrip(_) => "MoebiusStrip",
            GeoObject::Surface3D(_) => "Surface3D",
            GeoObject::Prism3D(_) => "Prism3D",
            GeoObject::Quadric3D(_) => "Quadric3D",
            GeoObject::ParametricCurve2D(_) => "ParametricCurve2D",
            GeoObject::ParametricCurve3D(_) => "ParametricCurve3D",
            GeoObject::PolarCurve(_) => "PolarCurve",
            GeoObject::VectorField2D(_) => "VectorField2D",
            GeoObject::ComplexGrid(_) => "ComplexGrid",
            GeoObject::ComplexMapping(_) => "ComplexMapping",
            GeoObject::ComplexIntegral(_) => "ComplexIntegral",
            GeoObject::ImplicitCurve(_) => "ImplicitCurve",
            GeoObject::Attractor3D(_) => "Attractor3D",
            GeoObject::Fractal2D(_) => "Fractal2D",
            GeoObject::RegularPolychoron4D(_) => "RegularPolychoron4D",
            GeoObject::RegularPolytopeND(_) => "RegularPolytopeND",
            GeoObject::HyperSurface4D(_) => "HyperSurface4D",
            GeoObject::VectorField3D(_) => "VectorField3D",
            GeoObject::Histogram(_) => "Histogram",
            GeoObject::ScatterPlot(_) => "ScatterPlot",
            GeoObject::BoxPlot(_) => "BoxPlot",
            GeoObject::RegressionLine(_) => "RegressionLine",
            GeoObject::DataTable(_) => "DataTable",
            GeoObject::PhasePortrait(_) => "PhasePortrait",
            GeoObject::Transformed(_) => "Transformed",
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

pub use grafito_geometry::LineKind;

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
            color: Color::DEFAULT_STROKE,
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
        self.kind.contains_t(t)
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
    #[serde(default)]
    pub is_integral: bool,
    #[serde(default = "default_integral_var")]
    pub integral_var: String,
    #[serde(default)]
    pub integral_lower: f64,
    /// Metadatos de un ajuste local persistente, si esta función fue generada
    /// desde una tabla de datos. Los samples siguen usando el pipeline normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<FitMetadata>,
    #[serde(skip)]
    pub cached_key: Arc<RwLock<Option<FunctionCacheKey>>>,
    #[serde(skip)]
    pub cached_samples: Arc<RwLock<FunctionSamples>>,
}

fn default_integral_var() -> String {
    "x".to_string()
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
            fit: self.fit.clone(),
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
            && self.fit == other.fit
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
            fit: None,
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

    /// Asocia la función renderizable a una tabla local y sus diagnósticos.
    pub fn with_fit(mut self, fit: FitMetadata) -> Self {
        self.fit = Some(fit);
        self
    }

    /// Invalidate any cached samples for this function.
    pub fn invalidate_cache(&self) {
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
        self.cached_samples
            .write()
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
pub struct Plane3DObj {
    pub id: ObjectId,
    pub label: String,
    /// Coeficientes de la ecuación `ax + by + cz + d = 0`.
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    /// Expresiones opcionales vinculantes (como en Surface3DObj).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub c_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub d_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    /// Opacidad del relleno (0.0 = transparente, 1.0 = opaco).
    #[serde(default = "default_plane_opacity")]
    pub opacity: f32,
}

fn default_plane_opacity() -> f32 {
    0.25
}

impl Plane3DObj {
    /// Crea un plano a partir de los coeficientes `ax + by + cz + d = 0`.
    pub fn from_equation(a: f64, b: f64, c: f64, d: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            a,
            b,
            c,
            d,
            a_expr: None,
            b_expr: None,
            c_expr: None,
            d_expr: None,
            color: Color::new(0.3, 0.6, 0.9, 1.0),
            visible: true,
            opacity: default_plane_opacity(),
        }
    }

    /// Crea un plano a partir de tres puntos.
    pub fn from_three_points(p1: Point3D, p2: Point3D, p3: Point3D) -> Option<Self> {
        let v1 = (p2.x - p1.x, p2.y - p1.y, p2.z - p1.z);
        let v2 = (p3.x - p1.x, p3.y - p1.y, p3.z - p1.z);
        // cross product v1 × v2
        let a = v1.1 * v2.2 - v1.2 * v2.1;
        let b = v1.2 * v2.0 - v1.0 * v2.2;
        let c = v1.0 * v2.1 - v1.1 * v2.0;
        if a.hypot(b).hypot(c) <= crate::validation::GEOM_EPS {
            return None;
        }
        let d = -(a * p1.x + b * p1.y + c * p1.z);
        Some(Self::from_equation(a, b, c, d))
    }

    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line3DObj {
    pub id: ObjectId,
    pub label: String,
    /// Punto de paso de la recta.
    pub point: Point3D,
    /// Vector dirección.
    pub direction: Point3D,
    /// Expresiones opcionales vinculantes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub px_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub py_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pz_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dx_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dy_expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dz_expr: Option<String>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl Line3DObj {
    /// Crea una recta a partir de un punto y un vector dirección.
    pub fn from_point_and_direction(point: Point3D, direction: Point3D) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            point,
            direction,
            px_expr: None,
            py_expr: None,
            pz_expr: None,
            dx_expr: None,
            dy_expr: None,
            dz_expr: None,
            color: Color::new(0.9, 0.3, 0.3, 1.0),
            visible: true,
            width: 2.0,
        }
    }

    /// Crea una recta a partir de dos puntos.
    pub fn from_two_points(a: Point3D, b: Point3D) -> Self {
        Self::from_point_and_direction(a, Point3D::new(b.x - a.x, b.y - a.y, b.z - a.z))
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
pub struct Tetrahedron3DObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point3D,
    pub edge_length: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}
impl Tetrahedron3DObj {
    pub fn new(center: Point3D, edge_length: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            edge_length,
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 1.0)),
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
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
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
        }
    }
    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }
}

// ── Prism y Quadric (P1.4) ──
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prism3DObj {
    pub id: ObjectId,
    pub label: String,
    /// Vértices de la base en 3D (al menos 3).
    pub base_vertices: Vec<Point3D>,
    /// Vector de extrusión.
    pub direction: Point3D,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}

impl Prism3DObj {
    /// Crea un prisma a partir de vértices base y vector dirección.
    pub fn new(base_vertices: Vec<Point3D>, direction: Point3D) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            base_vertices,
            direction,
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.15)),
        }
    }

    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    /// Vértices de la tapa superior (base + dirección).
    pub fn top_vertices(&self) -> Vec<Point3D> {
        self.base_vertices
            .iter()
            .map(|p| {
                Point3D::new(
                    p.x + self.direction.x,
                    p.y + self.direction.y,
                    p.z + self.direction.z,
                )
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quadric3DObj {
    pub id: ObjectId,
    pub label: String,
    /// Coeficientes de `a*x^2 + b*y^2 + c*z^2 + d*xy + e*yz + f*zx + g*x + h*y + i*z + j = 0`.
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
    pub g: f64,
    pub h: f64,
    pub i: f64,
    pub j: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl Quadric3DObj {
    /// Crea una cuádrica general a partir de 10 coeficientes.
    pub fn from_coeffs(coeffs: [f64; 10]) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            a: coeffs[0],
            b: coeffs[1],
            c: coeffs[2],
            d: coeffs[3],
            e: coeffs[4],
            f: coeffs[5],
            g: coeffs[6],
            h: coeffs[7],
            i: coeffs[8],
            j: coeffs[9],
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
        }
    }

    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    /// Evalúa la cuádrica en un punto 3D.
    pub fn eval_at(&self, p: Point3D) -> f64 {
        self.a * p.x * p.x
            + self.b * p.y * p.y
            + self.c * p.z * p.z
            + self.d * p.x * p.y
            + self.e * p.y * p.z
            + self.f * p.z * p.x
            + self.g * p.x
            + self.h * p.y
            + self.i * p.z
            + self.j
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
    /// Si es true, `expr` es una expresión compleja f(z) y la altura z = |f(z)|.
    #[serde(default)]
    pub is_complex: bool,
    /// Compatibilidad para superficies explícitas/complex de schema v1, cuyo
    /// resultado se dibujaba como `(x, f(x,y), y)`.
    #[serde(default)]
    pub legacy_axis_swap: bool,
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
            is_complex: self.is_complex,
            legacy_axis_swap: self.legacy_axis_swap,
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
            && self.is_complex == other.is_complex
            && self.legacy_axis_swap == other.legacy_axis_swap
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
            is_complex: false,
            legacy_axis_swap: false,
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
            is_complex: false,
            legacy_axis_swap: false,
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

    /// Crea una superficie 3D que visualiza |f(z)| sobre el plano complejo.
    /// `expr` es una expresión compleja f(z), la altura z = |f(x + iy)|.
    pub fn new_complex(expr: impl Into<String>, xr: (f64, f64), yr: (f64, f64)) -> Self {
        let mut s = Self::new(expr, xr, yr);
        s.is_complex = true;
        s
    }

    /// Convierte una muestra explícita `(x, y, f(x,y))` a coordenadas de
    /// documento, preservando la orientación de archivos schema v1.
    pub fn explicit_sample_point(&self, x: f64, y: f64, value: f64) -> Point3D {
        if self.legacy_axis_swap {
            Point3D::new(x, value, y)
        } else {
            Point3D::new(x, y, value)
        }
    }

    /// Invalidate any cached grid for this surface.
    pub fn invalidate_cache(&self) {
        self.cached_grid
            .write()
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
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
            color: Color::DEFAULT_STROKE,
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

// Objetos P1.1: arcos, sectores, bezier y spline 2D.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArcObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub radius: f64,
    /// Ángulo inicial en radianes.
    pub start_angle: f64,
    /// Ángulo final en radianes.
    pub end_angle: f64,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl ArcObj {
    pub fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            radius,
            start_angle,
            end_angle,
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 2.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Crea un arco por tres puntos no colineales; el arco va de p1 a p3 pasando por p2.
    pub fn from_three_points(p1: Point2, p2: Point2, p3: Point2) -> Option<Self> {
        let center = circumcenter(p1, p2, p3)?;
        let radius = center.distance(&p1);
        if !radius.is_finite() || radius <= 1e-12 {
            return None;
        }
        let a1 = (p1.y - center.y).atan2(p1.x - center.x);
        let a2 = (p2.y - center.y).atan2(p2.x - center.x);
        let a3 = (p3.y - center.y).atan2(p3.x - center.x);
        // Determina dirección: si p2 está entre p1 y p3 en sentido antihorario, conserva orden; si no, invierte.
        let ccw = is_angle_between_ccw(a1, a3, a2);
        let (start_angle, end_angle) = if ccw { (a1, a3) } else { (a3, a1) };
        // Ajusta end para que el barrido sea el menor que contiene a2.
        let mut end = end_angle;
        let mut start = start_angle;
        // Normaliza para que el arco contenga a2.
        if ccw {
            if !is_angle_between_ccw(start, end, a2) {
                // Si no contiene, invierte dirección.
                std::mem::swap(&mut start, &mut end);
            }
        } else if !is_angle_between_ccw(end, start, a2) {
            std::mem::swap(&mut start, &mut end);
        }
        Some(Self::new(center, radius, start, end))
    }

    /// Longitud del arco.
    pub fn length(&self) -> f64 {
        let delta = (self.end_angle - self.start_angle).abs();
        // Normaliza delta al intervalo [0, 2π] eligiendo el menor recorrido que contiene el arco dibujado.
        // Para arcos > π el usuario espera el recorrido directo, no el complementario.
        let mut d = delta % (2.0 * std::f64::consts::PI);
        if d > std::f64::consts::PI && delta <= std::f64::consts::PI {
            d = 2.0 * std::f64::consts::PI - d;
        }
        self.radius * d
    }

    /// Muestrea el arco en `steps` segmentos (steps+1 puntos).
    pub fn sample_points(&self, steps: usize) -> Vec<Point2> {
        let steps = steps.clamp(1, 256);
        let mut pts = Vec::with_capacity(steps + 1);
        let delta = self.end_angle - self.start_angle;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let ang = self.start_angle + delta * t;
            pts.push(Point2::new(
                self.center.x + self.radius * ang.cos(),
                self.center.y + self.radius * ang.sin(),
            ));
        }
        pts
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorObj {
    pub id: ObjectId,
    pub label: String,
    pub center: Point2,
    pub radius: f64,
    pub start_angle: f64,
    pub end_angle: f64,
    pub color: Color,
    pub fill_color: Option<Color>,
    pub visible: bool,
    pub width: f32,
}

impl SectorObj {
    pub fn new(center: Point2, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            center,
            radius,
            start_angle,
            end_angle,
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 2.0,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 0.25)),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn area(&self) -> f64 {
        let delta = (self.end_angle - self.start_angle).abs() % (2.0 * std::f64::consts::PI);
        0.5 * self.radius * self.radius * delta
    }

    /// Vértices del sector como polígono cerrado (centro + arco).
    pub fn polygon_vertices(&self, steps: usize) -> Vec<Point2> {
        let steps = steps.clamp(8, 256);
        let mut verts = Vec::with_capacity(steps + 2);
        verts.push(self.center);
        let delta = self.end_angle - self.start_angle;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let ang = self.start_angle + delta * t;
            verts.push(Point2::new(
                self.center.x + self.radius * ang.cos(),
                self.center.y + self.radius * ang.sin(),
            ));
        }
        verts
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BezierCurveObj {
    pub id: ObjectId,
    pub label: String,
    pub control_points: Vec<Point2>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl BezierCurveObj {
    pub fn new(control_points: Vec<Point2>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            control_points,
            color: Color::BLUE,
            visible: true,
            width: 2.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Evalúa la curva de Bézier en t ∈ [0,1] usando De Casteljau.
    pub fn point_at(&self, t: f64) -> Option<Point2> {
        bezier_point(&self.control_points, t)
    }

    pub fn sample_points(&self, steps: usize) -> Vec<Point2> {
        let steps = steps.clamp(1, 512);
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            if let Some(p) = self.point_at(t) {
                pts.push(p);
            }
        }
        pts
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplineObj {
    pub id: ObjectId,
    pub label: String,
    pub points: Vec<Point2>,
    #[serde(default)]
    pub closed: bool,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
}

impl SplineObj {
    pub fn new(points: Vec<Point2>) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            points,
            closed: false,
            color: Color::new(0.2, 0.7, 0.3, 1.0),
            visible: true,
            width: 2.0,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Muestrea spline Catmull-Rom; `steps_per_segment` controla densidad.
    pub fn sample_points(&self, steps_per_segment: usize) -> Vec<Point2> {
        catmull_rom_sample(&self.points, self.closed, steps_per_segment)
    }
}

// Helpers geométricos compartidos por Arc/Spline/Bezier.

fn circumcenter(a: Point2, b: Point2, c: Point2) -> Option<Point2> {
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() < 1e-12 || !d.is_finite() {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let ux = (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d;
    let uy = (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d;
    if ux.is_finite() && uy.is_finite() {
        Some(Point2::new(ux, uy))
    } else {
        None
    }
}

fn is_angle_between_ccw(start: f64, end: f64, mid: f64) -> bool {
    let two_pi = 2.0 * std::f64::consts::PI;
    let norm = |a: f64| ((a % two_pi) + two_pi) % two_pi;
    let s = norm(start);
    let e = norm(end);
    let m = norm(mid);
    if s <= e {
        s <= m && m <= e
    } else {
        m >= s || m <= e
    }
}

fn bezier_point(control: &[Point2], t: f64) -> Option<Point2> {
    if control.is_empty() || !t.is_finite() {
        return None;
    }
    if control.len() == 1 {
        return Some(control[0]);
    }
    let mut tmp: Vec<Point2> = control.to_vec();
    let n = tmp.len();
    for r in 1..n {
        for i in 0..n - r {
            let x = (1.0 - t) * tmp[i].x + t * tmp[i + 1].x;
            let y = (1.0 - t) * tmp[i].y + t * tmp[i + 1].y;
            tmp[i] = Point2::new(x, y);
        }
    }
    let p = tmp[0];
    if p.x.is_finite() && p.y.is_finite() {
        Some(p)
    } else {
        None
    }
}

fn catmull_rom_sample(points: &[Point2], closed: bool, steps_per_segment: usize) -> Vec<Point2> {
    if points.len() < 2 {
        return points.to_vec();
    }
    if points.len() == 2 {
        return points.to_vec();
    }
    let steps = steps_per_segment.clamp(4, 64);
    let mut out = Vec::new();
    let n = points.len();
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let (p0, p1, p2, p3) = if closed {
            (
                points[(i + n - 1) % n],
                points[i],
                points[(i + 1) % n],
                points[(i + 2) % n],
            )
        } else {
            let p0 = if i == 0 { points[0] } else { points[i - 1] };
            let p1 = points[i];
            let p2 = points[i + 1];
            let p3 = if i + 2 < n {
                points[i + 2]
            } else {
                points[n - 1]
            };
            (p0, p1, p2, p3)
        };
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let x = 0.5
                * ((2.0 * p1.x)
                    + (-p0.x + p2.x) * t
                    + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                    + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3);
            let y = 0.5
                * ((2.0 * p1.y)
                    + (-p0.y + p2.y) * t
                    + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                    + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3);
            if x.is_finite() && y.is_finite() {
                // Evita duplicar el punto inicial de cada segmento salvo el primero.
                if !(i > 0 && s == 0) {
                    out.push(Point2::new(x, y));
                }
            }
        }
    }
    // Añade el punto final exacto para curvas abiertas.
    if !closed {
        if let Some(last) = points.last().copied() {
            out.push(last);
        }
    } else if let Some(first) = out.first().copied() {
        out.push(first);
    }
    out
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
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
    }
}

fn default_curve_3d_parameter() -> String {
    "t".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParametricCurve3DObj {
    pub id: ObjectId,
    pub label: String,
    pub expr_x: String,
    pub expr_y: String,
    pub expr_z: String,
    #[serde(default = "default_curve_3d_parameter")]
    pub parameter: String,
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
            parameter: self.parameter.clone(),
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
            && self.parameter == other.parameter
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
            parameter: default_curve_3d_parameter(),
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

    pub fn with_parameter(mut self, parameter: impl Into<String>) -> Self {
        self.parameter = parameter.into();
        self
    }

    /// Invalidate any cached samples for this curve.
    pub fn invalidate_cache(&self) {
        self.cached_samples
            .write()
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
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
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
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
    #[serde(default)]
    pub domain_coloring_mode: u8,
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
            domain_coloring_mode: 0,
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
    #[serde(default)]
    pub animate_homotopy: bool,
    #[serde(default)]
    pub homotopy_speed: f32, // speed factor, e.g. 1.0
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
            animate_homotopy: false,
            homotopy_speed: 1.0,
            conformal_cache,
        }
    }

    pub fn new_with_symbol(expr: &str, target: ObjectId, symbol: &str) -> Self {
        let conformal_cache = conformal_map_from_expr(expr, symbol);
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.to_string(),
            target,
            color: Color::new(0.5, 0.0, 0.5, 1.0),
            visible: true,
            animate_homotopy: false,
            homotopy_speed: 1.0,
            conformal_cache,
        }
    }

    pub fn conformal_map(&self, symbol: &str) -> Option<ConformalMap> {
        conformal_map_from_expr(&self.expr, symbol).or(self.conformal_cache)
    }

    pub fn with_label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    pub fn refresh_cache(&mut self) {
        self.conformal_cache = ConformalMap::from_expr_string(&self.expr);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplexIntegralObj {
    pub id: ObjectId,
    pub label: String,
    pub expr: String,
    pub target: ObjectId, // the contour to integrate over
    pub color: Color,
    pub visible: bool,
    pub compute_residue: bool, // If true, computes sum of residues instead of raw integral
}
impl ComplexIntegralObj {
    pub fn new(expr: &str, target: ObjectId, compute_residue: bool) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            expr: expr.to_string(),
            target,
            color: Color::new(0.8, 0.2, 0.2, 1.0),
            visible: true,
            compute_residue,
        }
    }
}

fn conformal_map_from_expr(expr: &str, symbol: &str) -> Option<ConformalMap> {
    let normalized = normalize_complex_symbol(expr, symbol);
    ConformalMap::from_expr_string(&normalized)
}

fn normalize_complex_symbol(expr: &str, symbol: &str) -> String {
    if symbol.is_empty() || symbol == "z" {
        return expr.to_string();
    }

    let mut out = String::with_capacity(expr.len());
    let mut rest = expr;
    while let Some(pos) = rest.find(symbol) {
        let before = &rest[..pos];
        let after = &rest[pos + symbol.len()..];
        let prev_ident = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let next_ident = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        out.push_str(before);
        if prev_ident || next_ident {
            out.push_str(symbol);
        } else {
            out.push('z');
        }
        rest = after;
    }
    out.push_str(rest);
    out
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
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
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

/// Cached world-space point grid for a 3D surface in document `(x, y, z)` order.
pub type SurfaceSamples = Vec<Vec<Point3D>>;

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
    pub expr_hash: u64,
    pub variables_hash: u64,
}

/// Cache key for 3D parametric surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceCacheKey {
    pub x_domain: (f64, f64),
    pub y_domain: (f64, f64),
    pub res: usize,
    pub is_parametric: bool,
    pub expr_hash: u64,
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
        // Hash combinado de lhs + rhs + variables (orden determinista).
        let mut hasher = DefaultHasher::new();
        self.expr_lhs.hash(&mut hasher);
        self.expr_rhs.hash(&mut hasher);
        let mut sorted_vars: Vec<_> = variables.iter().collect();
        sorted_vars.sort_by(|a, b| a.0.cmp(b.0));
        for (key, value) in sorted_vars {
            key.hash(&mut hasher);
            value.to_bits().hash(&mut hasher);
        }
        let combined_hash = hasher.finish();

        // Verificar cache.
        if let Some(cached) = self
            .cached_asts
            .read()
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
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
        *self.cached_asts.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = Some(new_cache);
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
            .unwrap_or_else(|p| {
                log::warn!("cache lock envenenado; recuperando estado parcial");
                p.into_inner()
            })
            .clear();
        *self.cached_key.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
        *self.cached_region.write().unwrap_or_else(|p| {
            log::warn!("cache lock envenenado; recuperando estado parcial");
            p.into_inner()
        }) = None;
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

    pub fn model(&self) -> grafito_geometry::attractors::AttractorType {
        use grafito_geometry::attractors::AttractorType;

        match self.attractor_type.as_str() {
            "lorenz" => AttractorType::Lorenz {
                sigma: self.params.first().copied().unwrap_or(10.0),
                rho: self.params.get(1).copied().unwrap_or(28.0),
                beta: self.params.get(2).copied().unwrap_or(8.0 / 3.0),
            },
            "rossler" => AttractorType::Rossler {
                a: self.params.first().copied().unwrap_or(0.2),
                b: self.params.get(1).copied().unwrap_or(0.2),
                c: self.params.get(2).copied().unwrap_or(5.7),
            },
            "thomas" => AttractorType::Thomas {
                b: self.params.first().copied().unwrap_or(0.208186),
            },
            "aizawa" => AttractorType::Aizawa {
                a: self.params.first().copied().unwrap_or(0.95),
                b: self.params.get(1).copied().unwrap_or(0.7),
                c: self.params.get(2).copied().unwrap_or(0.6),
                d: self.params.get(3).copied().unwrap_or(3.5),
                e: self.params.get(4).copied().unwrap_or(0.25),
                f: self.params.get(5).copied().unwrap_or(0.1),
            },
            "chen" => AttractorType::Chen {
                a: self.params.first().copied().unwrap_or(35.0),
                b: self.params.get(1).copied().unwrap_or(3.0),
                c: self.params.get(2).copied().unwrap_or(28.0),
            },
            "halvorsen" => AttractorType::Halvorsen {
                a: self.params.first().copied().unwrap_or(1.89),
            },
            "dadras" => AttractorType::Dadras {
                p: self.params.first().copied().unwrap_or(3.0),
                q: self.params.get(1).copied().unwrap_or(2.7),
                r: self.params.get(2).copied().unwrap_or(1.7),
                s: self.params.get(3).copied().unwrap_or(2.0),
                e: self.params.get(4).copied().unwrap_or(9.0),
            },
            "chua" => AttractorType::Chua {
                alpha: self.params.first().copied().unwrap_or(15.6),
                beta: self.params.get(1).copied().unwrap_or(28.0),
                m0: self.params.get(2).copied().unwrap_or(-1.143),
                m1: self.params.get(3).copied().unwrap_or(-0.714),
            },
            _ => AttractorType::lorenz(),
        }
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
        self.max_iter = max_iter.min(grafito_geometry::fractals::MAX_FRACTAL_ITER);
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

/// Selector y presentación de un politopo regular convexo de cuatro dimensiones.
///
/// La topología canónica se deriva desde [`RegularPolychoron`] al renderizarse;
/// nunca se duplica en documentos persistidos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegularPolychoron4DObj {
    pub id: ObjectId,
    pub label: String,
    pub kind: RegularPolychoron,
    pub scale: f64,
    /// Ángulos para los planos indicados por [`Self::ROTATION_PLANES`].
    pub rotation_angles: [f64; 6],
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}

impl RegularPolychoron4DObj {
    /// Orden canónico de los seis planos coordenados de SO(4):
    /// `xy`, `xz`, `xw`, `yz`, `yw`, `zw`.
    pub const ROTATION_PLANES: [(usize, usize); 6] =
        [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

    /// Crea un selector de politopo 4D con una presentación renderizable por defecto.
    pub fn new(kind: RegularPolychoron) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            kind,
            scale: 1.0,
            rotation_angles: [0.0; 6],
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 1.0)),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// Selector y presentación de una familia regular genérica en R^n.
///
/// La topología se deriva desde [`RegularPolytopeFamily`] al renderizarse. Para
/// una dimensión `n`, `rotation_angles` sigue el orden lexicográfico de todos
/// los planos coordenados: `(0, 1)`, `(0, 2)`, ..., `(0, n - 1)`, `(1, 2)`,
/// ..., `(n - 2, n - 1)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegularPolytopeNDObj {
    pub id: ObjectId,
    pub label: String,
    pub family: RegularPolytopeFamily,
    pub dimension: usize,
    pub scale: f64,
    pub rotation_angles: Vec<f64>,
    pub color: Color,
    pub visible: bool,
    pub width: f32,
    pub fill_color: Option<Color>,
}

impl RegularPolytopeNDObj {
    /// Devuelve `n(n - 1) / 2` solo para las dimensiones publicadas por geometría.
    pub fn expected_rotation_angle_count(dimension: usize) -> Option<usize> {
        if !(MIN_REGULAR_POLYTOPE_DIMENSION..=MAX_REGULAR_POLYTOPE_DIMENSION).contains(&dimension) {
            return None;
        }

        Some(dimension * (dimension - 1) / 2)
    }

    /// Itera los planos coordenados en el mismo orden que `rotation_angles`.
    pub fn rotation_plane_pairs(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.dimension)
            .flat_map(move |first| ((first + 1)..self.dimension).map(move |second| (first, second)))
    }

    /// Crea un selector N-D con ángulos nulos para cada plano válido publicado.
    ///
    /// Las dimensiones fuera del intervalo admitido conservan un vector vacío
    /// para no reservar según una entrada inválida; `Document::try_add_object`
    /// las rechaza antes de crear topología.
    pub fn new(family: RegularPolytopeFamily, dimension: usize) -> Self {
        let rotation_angle_count = Self::expected_rotation_angle_count(dimension).unwrap_or(0);
        Self {
            id: ObjectId::new(),
            label: String::new(),
            family,
            dimension,
            scale: 1.0,
            rotation_angles: vec![0.0; rotation_angle_count],
            color: Color::DEFAULT_STROKE,
            visible: true,
            width: 1.5,
            fill_color: Some(Color::new(0.2, 0.5, 0.9, 1.0)),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
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
        let data: Vec<f64> = data.into_iter().filter(|value| value.is_finite()).collect();
        let bins = bins.clamp(1, grafito_geometry::statistics::MAX_HISTOGRAM_BINS);
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
            let hist = grafito_geometry::statistics::histogram(&data, bins);
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
            color: Color::DEFAULT_STROKE,
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
    /// Tabla local de la que se creó este gráfico, si existe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_data: Option<ObjectId>,
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
            source_data: None,
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

    /// Mantiene una referencia semántica a la tabla de la que provienen los puntos.
    pub fn linked_to(mut self, source_data: ObjectId) -> Self {
        self.source_data = Some(source_data);
        self
    }
}

/// Tabla local de dos columnas para análisis y ajustes, sin ruta de origen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataTableObj {
    pub id: ObjectId,
    pub label: String,
    pub x_name: String,
    pub y_name: String,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub color: Color,
    pub visible: bool,
}

impl DataTableObj {
    pub fn new(
        x_name: impl Into<String>,
        y_name: impl Into<String>,
        xs: Vec<f64>,
        ys: Vec<f64>,
    ) -> Self {
        Self {
            id: ObjectId::new(),
            label: String::new(),
            x_name: x_name.into(),
            y_name: y_name.into(),
            xs,
            ys,
            color: Color::BLUE,
            // La tabla alimenta análisis y no tiene geometría de canvas.
            visible: false,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
}

/// Enlace persistente entre una función ajustada y su fuente local de datos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitMetadata {
    pub source: ObjectId,
    pub kind: FitKind,
    pub coefficients: Vec<f64>,
    #[serde(default)]
    pub x_offset: f64,
    #[serde(default = "default_fit_metadata_x_scale")]
    pub x_scale: f64,
    pub diagnostics: FitDiagnostics,
}

fn default_fit_metadata_x_scale() -> f64 {
    1.0
}

impl FitMetadata {
    pub fn from_result(source: ObjectId, result: FitResult) -> Self {
        Self {
            source,
            kind: result.kind,
            coefficients: result.coefficients,
            x_offset: result.x_offset,
            x_scale: result.x_scale,
            diagnostics: result.diagnostics,
        }
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
            color: Color::DEFAULT_STROKE,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransformedObj {
    pub inner: Box<GeoObject>,
    pub complex_expr: String,
    pub compiled_expr: Option<String>,
}

impl TransformedObj {
    pub fn new(inner: GeoObject, expr: &str) -> Self {
        Self {
            inner: Box::new(inner),
            complex_expr: expr.to_string(),
            compiled_expr: None,
        }
    }

    /// Crea un Transformed validado (type-safe). Valida que la expresión sea parseable
    /// y que no sea singular trivial (p. ej. "0" que colapsa el objeto).
    pub fn try_new(inner: GeoObject, expr: &str) -> Result<Self, String> {
        if expr.len() > crate::validation::MAX_EXPR_LENGTH {
            return Err(format!(
                "complex_expr exceeds {} chars",
                crate::validation::MAX_EXPR_LENGTH
            ));
        }
        if expr.is_empty() {
            return Err("complex_expr cannot be empty".into());
        }
        // Validar que la expresión sea sintácticamente válida como función de z
        // y detectar singularidades que colapsan todo a un punto.
        let ast = grafito_geometry::expr::prepare_function_ast(
            expr,
            &std::collections::HashMap::new(),
            &["z"],
        )
        .map_err(|reason| format!("complex_expr inválida: {reason}"))?;
        // Detecta 0*z, (z-z), sin(0)*z, constantes, etc. Evaluando en dos puntos.
        let v0 = ast.eval_at("z", 0.0);
        let v1 = ast.eval_at("z", 1.0);
        if v0.is_finite() && v1.is_finite() {
            let max_abs = v0.abs().max(v1.abs());
            if max_abs < crate::validation::GEOM_EPS || (v1 - v0).abs() < 1e-12 {
                return Err("complex_expr singular: colapsa el objeto".into());
            }
        } else if !v0.is_finite() && !v1.is_finite() {
            // Ambas evaluaciones no finitas -> también singular para el muestreo.
            return Err("complex_expr singular: colapsa el objeto".into());
        }
        // Validación Jacobiana no trivial: compila como expresión compleja y
        // evalúa det(J) en muestreo. Si |det|<GEOM_EPS o no finito -> singular.
        crate::validation::validate_transformed_jacobian(expr)?;
        // Validar anidamiento: Document::validate ya limita MAX_TRANSFORM_DEPTH, aquí solo check básico
        Ok(Self {
            inner: Box::new(inner),
            complex_expr: expr.to_string(),
            compiled_expr: None,
        })
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

    #[test]
    fn render_space_classifies_2d_and_3d_objects() {
        let point = GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0)));
        let plane = GeoObject::Plane3D(Plane3DObj::from_equation(0.0, 0.0, 1.0, 0.0));
        let line = GeoObject::Line3D(Line3DObj::from_point_and_direction(
            Point3D::new(0.0, 0.0, 0.0),
            Point3D::new(1.0, 0.0, 0.0),
        ));
        let transformed = GeoObject::Transformed(TransformedObj::new(line.clone(), "z"));
        let polychoron = GeoObject::RegularPolychoron4D(RegularPolychoron4DObj::new(
            grafito_geometry::RegularPolychoron::Tesseract,
        ));
        let polytope = GeoObject::RegularPolytopeND(RegularPolytopeNDObj::new(
            grafito_geometry::RegularPolytopeFamily::Hypercube,
            5,
        ));

        assert_eq!(point.render_space(), RenderSpace::D2);
        assert_eq!(plane.render_space(), RenderSpace::D3);
        assert_eq!(line.render_space(), RenderSpace::D3);
        assert_eq!(transformed.render_space(), RenderSpace::D3);
        assert_eq!(polychoron.render_space(), RenderSpace::D3);
        assert_eq!(polytope.render_space(), RenderSpace::D3);
        assert!(plane.is_3d());
        assert!(polychoron.is_3d());
        assert!(polytope.is_3d());
    }

    #[test]
    fn regular_polytope_rotation_angles_use_canonical_coordinate_plane_order() {
        assert_eq!(
            RegularPolychoron4DObj::ROTATION_PLANES,
            [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
        );

        let polytope =
            RegularPolytopeNDObj::new(grafito_geometry::RegularPolytopeFamily::Hypercube, 4);
        assert_eq!(polytope.rotation_angles.len(), 6);
        assert_eq!(
            polytope.rotation_plane_pairs().collect::<Vec<_>>(),
            vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]
        );
    }

    #[test]
    fn histogram_constructor_discards_non_finite_values_from_bounds_and_data() {
        let histogram = HistogramObj::new(vec![1.0, f64::NAN, f64::INFINITY, 3.0], 2);

        assert_eq!(histogram.data, vec![1.0, 3.0]);
        assert!(histogram.x_min.is_finite() && histogram.x_max.is_finite());
    }
}
