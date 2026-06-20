//! DRY macro for GeoObject field accessors.
//! Eliminates repeated match arms across id(), label(), color(), etc.

#[macro_export]
macro_rules! geo_match {
    // Match for field access (fn id, label, color, is_visible)
    ($self:expr, $field:ident, $ret:ty) => {
        match $self {
            GeoObject::Point(o) => o.$field,
            GeoObject::Line(o) => o.$field,
            GeoObject::Circle(o) => o.$field,
            GeoObject::Polygon(o) => o.$field,
            GeoObject::Function(o) => o.$field,
            GeoObject::Text(o) => o.$field,
            GeoObject::Ellipse(o) => o.$field,
            GeoObject::Parabola(o) => o.$field,
            GeoObject::Hyperbola(o) => o.$field,
            GeoObject::Point3D(o) => o.$field,
            GeoObject::Segment3D(o) => o.$field,
            GeoObject::Sphere3D(o) => o.$field,
            GeoObject::Cube3D(o) => o.$field,
            GeoObject::Pyramid3D(o) => o.$field,
            GeoObject::Cone3D(o) => o.$field,
            GeoObject::Cylinder3D(o) => o.$field,
            GeoObject::Surface3D(o) => o.$field,
        }
    };
    // Match for field assign (fn set_color)
    ($self:expr, $field:ident = $value:expr) => {
        match $self {
            GeoObject::Point(o) => o.$field = $value,
            GeoObject::Line(o) => o.$field = $value,
            GeoObject::Circle(o) => o.$field = $value,
            GeoObject::Polygon(o) => o.$field = $value,
            GeoObject::Function(o) => o.$field = $value,
            GeoObject::Text(o) => o.$field = $value,
            GeoObject::Ellipse(o) => o.$field = $value,
            GeoObject::Parabola(o) => o.$field = $value,
            GeoObject::Hyperbola(o) => o.$field = $value,
            GeoObject::Point3D(o) => o.$field = $value,
            GeoObject::Segment3D(o) => o.$field = $value,
            GeoObject::Sphere3D(o) => o.$field = $value,
            GeoObject::Cube3D(o) => o.$field = $value,
            GeoObject::Pyramid3D(o) => o.$field = $value,
            GeoObject::Cone3D(o) => o.$field = $value,
            GeoObject::Cylinder3D(o) => o.$field = $value,
            GeoObject::Surface3D(o) => o.$field = $value,
        }
    };
    // Match for auto-label assign (fn add_object)
    ($self:expr, $label:ident = $value:expr) => {
        match $self {
            GeoObject::Point(o) => o.$label = $value,
            GeoObject::Line(o) => o.$label = $value,
            GeoObject::Circle(o) => o.$label = $value,
            GeoObject::Polygon(o) => o.$label = $value,
            GeoObject::Function(o) => o.$label = $value,
            GeoObject::Text(o) => o.$label = $value,
            GeoObject::Ellipse(o) => o.$label = $value,
            GeoObject::Parabola(o) => o.$label = $value,
            GeoObject::Hyperbola(o) => o.$label = $value,
            GeoObject::Point3D(o) => o.$label = $value,
            GeoObject::Segment3D(o) => o.$label = $value,
            GeoObject::Sphere3D(o) => o.$label = $value,
            GeoObject::Cube3D(o) => o.$label = $value,
            GeoObject::Pyramid3D(o) => o.$label = $value,
            GeoObject::Cone3D(o) => o.$label = $value,
            GeoObject::Cylinder3D(o) => o.$label = $value,
            GeoObject::Surface3D(o) => o.$label = $value,
        }
    };
}
