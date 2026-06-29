//! Aplicación de escritorio Grafito — raíz de módulos.
//!
//! La lógica de la aplicación está dividida en módulos específicos:
//! `app`, `canvas`, `input`, `ui`, `commands` y `utils`. Los helpers de
//! renderizado legado y el despachador de herramientas permanecen como
//! módulos internos del crate.

pub(crate) mod algebra;
pub(crate) mod app;
pub(crate) mod canvas;
pub(crate) mod commands;
pub(crate) mod export;
pub(crate) mod input;
pub(crate) mod keyboard;
pub(crate) mod panels;
pub mod render_2d;
pub(crate) mod render_3d;
pub(crate) mod snap;
pub(crate) mod tool_dispatcher;
pub mod tools_panel;
pub(crate) mod ui;
pub(crate) mod utils;

#[cfg(test)]
mod tests;

/// Cantidad de muestras MSAA usada por el renderizador GPU y la superficie de eframe.
pub const MSAA_SAMPLES: u16 = 4;

/// Modo de vista 2D/3D actual.
///
/// Se conserva por compatibilidad con el código existente. Ahora se deriva
/// automáticamente de la [`Perspective`] activa mediante
/// [`Perspective::view_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    D2,
    D3,
}

// ─────────────────────────────────────────────────────────────────────────
// Sistema de Perspectivas (estilo GeoGebra Perspectives)
// ─────────────────────────────────────────────────────────────────────────

/// Las diez perspectivas de Grafito, análogas a las *Perspectives* de GeoGebra.
///
/// Cada perspectiva define un `layout` que controla el modo del canvas, los
/// paneles visibles, los grupos de herramientas de la toolbar, el teclado
/// matemático y la herramienta por defecto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Perspective {
    /// Geometría 2D euclidiana.
    #[default]
    Geometry2D,
    /// Sólidos y superficies 3D.
    Geometry3D,
    /// Álgebra y CAS simbólico.
    AlgebraCas,
    /// Cálculo: derivadas, integrales, límites.
    Calculus,
    /// Probabilidad y distribuciones.
    Probability,
    /// Estadística y regresión.
    Statistics,
    /// Números complejos y mapeos conformes.
    Complex,
    /// Fractales, atractores y EDOs.
    Dynamics,
    /// Hoja de cálculo + análisis de datos.
    DataAnalysis,
    /// Modo examen restringido.
    Exam,
}

impl Perspective {
    /// Devuelve todas las perspectivas en orden, útil para construir selectores.
    pub const ALL: [Perspective; 10] = [
        Perspective::Geometry2D,
        Perspective::Geometry3D,
        Perspective::AlgebraCas,
        Perspective::Calculus,
        Perspective::Probability,
        Perspective::Statistics,
        Perspective::Complex,
        Perspective::Dynamics,
        Perspective::DataAnalysis,
        Perspective::Exam,
    ];

    /// Nombre largo, legible para el usuario.
    pub const fn title(&self) -> &'static str {
        match self {
            Perspective::Geometry2D => "Geometría 2D",
            Perspective::Geometry3D => "Geometría 3D",
            Perspective::AlgebraCas => "Álgebra y CAS",
            Perspective::Calculus => "Cálculo",
            Perspective::Probability => "Probabilidad",
            Perspective::Statistics => "Estadística",
            Perspective::Complex => "Complejos",
            Perspective::Dynamics => "Dinámica",
            Perspective::DataAnalysis => "Análisis de datos",
            Perspective::Exam => "Examen",
        }
    }

    /// Etiqueta corta (2-3 caracteres) para el selector visual del sidebar.
    pub const fn short_label(&self) -> &'static str {
        match self {
            Perspective::Geometry2D => "G2",
            Perspective::Geometry3D => "G3",
            Perspective::AlgebraCas => "AL",
            Perspective::Calculus => "Cλ",
            Perspective::Probability => "P",
            Perspective::Statistics => "S",
            Perspective::Complex => "i",
            Perspective::Dynamics => "Dn",
            Perspective::DataAnalysis => "D",
            Perspective::Exam => "E",
        }
    }

    /// Atajo de teclado asociado (`Ctrl+Shift+N`) donde N es 1..9,0.
    pub const fn shortcut_number(&self) -> u8 {
        match self {
            Perspective::Geometry2D => 1,
            Perspective::Geometry3D => 2,
            Perspective::AlgebraCas => 3,
            Perspective::Calculus => 4,
            Perspective::Probability => 5,
            Perspective::Statistics => 6,
            Perspective::Complex => 7,
            Perspective::Dynamics => 8,
            Perspective::DataAnalysis => 9,
            Perspective::Exam => 0,
        }
    }

    /// Modo del canvas (2D, 3D o 2D reducido) asociado a la perspectiva.
    pub const fn canvas_mode(&self) -> CanvasMode {
        match self {
            Perspective::Geometry2D => CanvasMode::D2,
            Perspective::Geometry3D => CanvasMode::D3,
            Perspective::AlgebraCas => CanvasMode::SmallD2,
            Perspective::Calculus => CanvasMode::D2,
            Perspective::Probability => CanvasMode::SmallD2,
            Perspective::Statistics => CanvasMode::D2,
            Perspective::Complex => CanvasMode::D2,
            Perspective::Dynamics => CanvasMode::D3,
            Perspective::DataAnalysis => CanvasMode::D2,
            Perspective::Exam => CanvasMode::D2,
        }
    }

    /// Deriva el `ViewMode` (D2/D3) usado por el resto de la aplicación.
    pub const fn view_mode(&self) -> ViewMode {
        match self.canvas_mode() {
            CanvasMode::D2 | CanvasMode::SmallD2 => ViewMode::D2,
            CanvasMode::D3 => ViewMode::D3,
        }
    }

    /// Construye el [`PerspectiveLayout`] que define qué mostrar para esta
    /// perspectiva.
    pub fn layout(&self) -> PerspectiveLayout {
        use grafito_ui::toolbar::ToolGroupId as G;
        use grafito_ui::Tool;
        match self {
            Perspective::Geometry2D => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Algebra,
                right_panel: Some(RightPanelContent::ConstructionProtocol),
                visible_tool_groups: &[
                    G::Move,
                    G::Point,
                    G::Line,
                    G::Circle,
                    G::Polygon,
                    G::Pencil,
                    G::Eraser,
                    G::Conic,
                    G::Measure,
                    G::Constraint,
                    G::Boolean,
                ],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::Geometry3D => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D3,
                left_panel: LeftPanelContent::Algebra,
                right_panel: Some(RightPanelContent::Properties),
                visible_tool_groups: &[G::Move, G::ThreeD, G::Curve, G::Pencil, G::Eraser],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::AlgebraCas => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::SmallD2,
                left_panel: LeftPanelContent::AlgebraAndCas,
                right_panel: Some(RightPanelContent::Table),
                visible_tool_groups: &[G::Move, G::Curve, G::Analysis],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Function,
            },
            Perspective::Calculus => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Cas,
                right_panel: Some(RightPanelContent::Table),
                visible_tool_groups: &[G::Move, G::Curve, G::Analysis, G::Circle],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Function,
            },
            Perspective::Probability => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::SmallD2,
                left_panel: LeftPanelContent::Stats,
                right_panel: Some(RightPanelContent::Data),
                visible_tool_groups: &[G::Move, G::Advanced],
                show_math_keyboard: false,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::Statistics => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Stats,
                right_panel: Some(RightPanelContent::Regression),
                visible_tool_groups: &[G::Move, G::Advanced, G::Measure],
                show_math_keyboard: false,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::Complex => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Complex,
                right_panel: Some(RightPanelContent::DomainColoring),
                visible_tool_groups: &[G::Move, G::Advanced],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::Dynamics => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D3,
                left_panel: LeftPanelContent::Attractor,
                right_panel: Some(RightPanelContent::Parameters),
                visible_tool_groups: &[G::Move, G::Advanced, G::ThreeD],
                show_math_keyboard: true,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::DataAnalysis => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Spreadsheet,
                right_panel: Some(RightPanelContent::Regression),
                visible_tool_groups: &[G::Move, G::Advanced],
                show_math_keyboard: false,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
            Perspective::Exam => PerspectiveLayout {
                title: Self::title(self),
                icon: Self::short_label(self),
                canvas_mode: CanvasMode::D2,
                left_panel: LeftPanelContent::Algebra,
                right_panel: None,
                visible_tool_groups: &[G::Move, G::Point, G::Line, G::Circle, G::Polygon],
                show_math_keyboard: false,
                show_input_bar: true,
                default_tool: Tool::Select,
            },
        }
    }
}

/// Modo del canvas asociado a una perspectiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasMode {
    /// Canvas 2D a pantalla completa.
    D2,
    /// Canvas 3D a pantalla completa.
    D3,
    /// Canvas 2D reducido (comparte espacio con paneles, p.ej. Álgebra/CAS).
    SmallD2,
}

/// Contenido del panel izquierdo según la perspectiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftPanelContent {
    /// Vista de álgebra (objetos, variables, comandos).
    Algebra,
    /// Panel CAS (cálculo simbólico).
    Cas,
    /// Álgebra + CAS combinados (álgebra como tab por defecto).
    AlgebraAndCas,
    /// Estadística / datos.
    Stats,
    /// Números complejos / mapeos conformes.
    Complex,
    /// Atractores / parámetros dinámicos.
    Attractor,
    /// Hoja de cálculo.
    Spreadsheet,
    /// Herramientas de construcción.
    Tools,
}

impl LeftPanelContent {
    /// Mapea el contenido declarado al índice del tab del sidebar (6 tabs
    /// armonizados: 0=Álgebra, 1=Herram., 2=CAS, 3=Tabla, 4=Hoja, 5=Vista).
    /// El tab activo elige el drawer genérico; la perspectiva activa decide
    /// si el drawer muestra contenido específico (Stats en Tabla, Complejos
    /// en Álgebra, Atractores en Herram.).
    pub const fn default_sidebar_tab(self) -> usize {
        match self {
            LeftPanelContent::Algebra
            | LeftPanelContent::AlgebraAndCas
            | LeftPanelContent::Complex => 0,
            LeftPanelContent::Tools | LeftPanelContent::Attractor => 1,
            LeftPanelContent::Cas => 2,
            LeftPanelContent::Stats => 3,
            LeftPanelContent::Spreadsheet => 4,
        }
    }
}

/// Contenido del panel derecho según la perspectiva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightPanelContent {
    /// Propiedades del objeto seleccionado.
    Properties,
    /// Protocolo de construcción.
    ConstructionProtocol,
    /// Tabla de valores x|f(x).
    Table,
    /// Datos / hoja de cálculo lateral.
    Data,
    /// Regresión y ajustes.
    Regression,
    /// Domain coloring / visualización compleja.
    DomainColoring,
    /// Parámetros de simulación dinámica.
    Parameters,
    /// Animación trigonométrica (círculo unitario).
    TrigAnimation,
    /// Hoja de cálculo lateral.
    Spreadsheet,
}

/// Definición estática de qué mostrar para una [`Perspective`].
///
/// `visible_tool_groups` usa [`grafito_ui::toolbar::ToolGroupId`] en lugar de
/// una slice anidada de `ToolEntry` para preservar la asociación entre cada
/// grupo y su icono vectorial, manteniendo además el renderizado libre de
/// asignaciones.
pub struct PerspectiveLayout {
    /// Título legible de la perspectiva.
    pub title: &'static str,
    /// Etiqueta corta / icono textual.
    pub icon: &'static str,
    /// Modo del canvas.
    pub canvas_mode: CanvasMode,
    /// Contenido del panel izquierdo.
    pub left_panel: LeftPanelContent,
    /// Contenido del panel derecho (`None` lo oculta).
    pub right_panel: Option<RightPanelContent>,
    /// Grupos de herramienta visibles en la toolbar.
    pub visible_tool_groups: &'static [grafito_ui::toolbar::ToolGroupId],
    /// Si se muestra el teclado matemático virtual.
    pub show_math_keyboard: bool,
    /// Si se muestra la barra de entrada.
    pub show_input_bar: bool,
    /// Herramienta por defecto al entrar en la perspectiva.
    pub default_tool: grafito_ui::Tool,
}

pub use app::run_app;
pub(crate) use app::GrafitoApp;
pub(crate) use app::PendingAction;
pub(crate) use utils::to_color32;
