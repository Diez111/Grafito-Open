//! Data Transfer Objects para el bridge FFI

/// Color RGBA normalizado (0.0 - 1.0)
#[derive(uniffi::Record, Clone, Debug)]
pub struct ColorDto {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Propiedad editable de un objeto
#[derive(uniffi::Record, Clone, Debug)]
pub struct PropertyDto {
    pub name: String,
    pub value: String,
    pub editable: bool,
}

/// Representación plana de un GeoObject para FFI
#[derive(uniffi::Record, Clone, Debug)]
pub struct ObjectDto {
    pub id: String,
    pub label: String,
    pub object_type: String,
    pub visible: bool,
    pub color: ColorDto,
    pub properties: Vec<PropertyDto>,
    pub summary: String,
}

/// Variable con slider
#[derive(uniffi::Record, Clone, Debug)]
pub struct VariableDto {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
}

/// Snapshot completo del documento
#[derive(uniffi::Record, Clone, Debug)]
pub struct DocumentSnapshot {
    pub objects: Vec<ObjectDto>,
    pub variables: Vec<VariableDto>,
    pub selected_id: Option<String>,
    pub view_mode: String,
    pub undo_available: bool,
    pub redo_available: bool,
}

/// Resultado de procesar un comando
#[derive(uniffi::Record, Clone, Debug)]
pub struct CommandResult {
    pub success: bool,
    pub message: Option<String>,
    pub new_object_id: Option<String>,
}

/// Herramienta de dibujo activa
#[derive(uniffi::Enum, Clone, Debug)]
pub enum ToolDto {
    Select,
    Point,
    Line,
    Circle,
    Polygon,
    Function,
    Point3D,
    Sphere3D,
    Cube3D,
    Attractor,
    Fractal,
    Histogram,
    ScatterPlot,
    Tangent,
    Perpendicular,
    Locus,
    Midpoint,
    Distance,
    Angle,
    Area,
    Slope,
    Slider,
    Button,
    Image,
    DomainColoring,
    HeatMap,
    ComplexGrid,
}

/// Resultado de operación CAS
#[derive(uniffi::Record, Clone, Debug)]
pub struct CasResult {
    pub expression: String,
    pub latex: Option<String>,
}

/// Hoja de cálculo
#[derive(uniffi::Record, Clone, Debug)]
pub struct SpreadsheetDto {
    pub rows: u32,
    pub cols: u32,
    pub cells: Vec<CellDto>,
}

/// Celda de hoja de cálculo
#[derive(uniffi::Record, Clone, Debug)]
pub struct CellDto {
    pub row: u32,
    pub col: u32,
    pub value: String,
    pub evaluated: Option<f64>,
}

/// Comando de la paleta
#[derive(uniffi::Record, Clone, Debug)]
pub struct PaletteCommandDto {
    pub name: String,
    pub category: String,
    pub syntax_hint: String,
}
