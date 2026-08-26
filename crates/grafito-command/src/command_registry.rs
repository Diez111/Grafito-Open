//! Catalogo declarativo de comandos de texto estables.
//!
//! El despacho permanece en `commands.rs`. `dispatch_key` documenta el handler
//! actual, pero no sustituye su normalizacion amplia ni sus fallbacks.

/// Tipo de valor que acepta un argumento de un comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    /// Valor no clasificado. No se permite en especificaciones estables.
    Unspecified,
    Expression,
    ComplexExpression,
    Variable,
    Number,
    Integer,
    Point,
    Object,
    ObjectLabel,
    Vector,
    Curve,
    Matrix,
    Data,
    Path,
    Domain,
    Relation,
    ParameterList,
}

impl ArgumentKind {
    /// Nombre estable que se muestra en la referencia Markdown.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "sin especificar",
            Self::Expression => "expresion",
            Self::ComplexExpression => "expresion compleja",
            Self::Variable => "variable",
            Self::Number => "numero",
            Self::Integer => "entero",
            Self::Point => "punto",
            Self::Object => "objeto",
            Self::ObjectLabel => "etiqueta de objeto",
            Self::Vector => "vector",
            Self::Curve => "curva",
            Self::Matrix => "matriz",
            Self::Data => "datos",
            Self::Path => "ruta",
            Self::Domain => "dominio",
            Self::Relation => "relacion",
            Self::ParameterList => "lista de parametros",
        }
    }
}

/// Un argumento documentado dentro de una firma de comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgumentSpec {
    pub name: &'static str,
    pub kind: ArgumentKind,
    pub optional: bool,
}

/// Una forma valida y documentada de invocar un comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSignature {
    pub syntax: &'static str,
    pub arguments: &'static [ArgumentSpec],
}

/// Efecto persistente esperado de ejecutar un comando correctamente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationClass {
    /// Valor no clasificado. No se permite en especificaciones estables.
    Unclassified,
    ReadOnly,
    CreatesObject,
    AddsConstraint,
    TransformsObject,
    LoadsExternalData,
}

impl MutationClass {
    /// Etiqueta para documentacion de usuario.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unclassified => "sin clasificar",
            Self::ReadOnly => "solo consulta",
            Self::CreatesObject => "crea objetos",
            Self::AddsConstraint => "agrega restricciones",
            Self::TransformsObject => "transforma objetos",
            Self::LoadsExternalData => "carga datos externos",
        }
    }
}

/// Riesgo operativo de una ejecucion valida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Valor no clasificado. No se permite en especificaciones estables.
    Unclassified,
    Low,
    Medium,
    High,
}

impl RiskLevel {
    /// Etiqueta para documentacion de usuario.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unclassified => "sin clasificar",
            Self::Low => "bajo",
            Self::Medium => "medio",
            Self::High => "alto",
        }
    }
}

/// Metadatos de un comando de texto expuesto a usuarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    /// Identificador estable para documentacion e integraciones.
    pub id: &'static str,
    /// Nombre canonico que recibe el dispatcher existente.
    pub canonical: &'static str,
    /// Formas alternativas, en ingles o espanol, aceptadas por el parser.
    pub aliases: &'static [&'static str],
    /// Firmas tipadas que se muestran al usuario.
    pub signatures: &'static [CommandSignature],
    /// Explicacion corta de la operacion.
    pub help: &'static str,
    /// Agrupacion de la paleta y la documentacion.
    pub category: &'static str,
    /// Texto que debe insertarse al escoger el comando desde la UI.
    pub insertion: &'static str,
    /// Clave del `match` de handlers durante la migracion gradual.
    pub dispatch_key: &'static str,
    /// Efecto persistente esperado para una ejecucion valida.
    pub mutation: MutationClass,
    /// Riesgo de recursos o datos asociado a la ejecucion.
    pub risk: RiskLevel,
    /// Si se muestra en la paleta de comandos.
    pub palette_visible: bool,
    /// Etiqueta de paleta; normalmente coincide con `canonical`.
    pub palette_label: &'static str,
}

impl CommandSpec {
    /// Indica si la cantidad de argumentos coincide con una firma publicada.
    pub fn accepts_argument_count(&self, count: usize) -> bool {
        match self.id {
            "geometry.polygon" => return count >= 3,
            "geometry.bezier-curve" | "geometry.spline" => return count >= 2,
            "graph.piecewise" => return count >= 3,
            "graph.contour" => return count >= 6,
            "graph.phase-portrait" => return count >= 2,
            "complex.complex-grid" | "graph.heat-map" => return count >= 1,
            "am2.triple-integral" => return count == 11,
            "am2.flux" | "am2.green" => return count == 8,
            "am2.gauss-ostrogradski" => return count == 11,
            "discrete.convex-hull"
            | "discrete.delaunay"
            | "discrete.voronoi"
            | "discrete.mst"
            | "discrete.tsp" => return (1..=10000).contains(&count),
            "discrete.shortest-distance" => return count == 2,
            "statistics.anova" => return (2..=100).contains(&count),
            _ => {}
        }

        self.signatures.iter().any(|signature| {
            let minimum = signature
                .arguments
                .iter()
                .filter(|argument| !argument.optional)
                .count();
            (minimum..=signature.arguments.len()).contains(&count)
        })
    }
}

macro_rules! arguments {
    ($($name:literal : $kind:ident $required:ident),* $(,)?) => {
        &[$(ArgumentSpec {
            name: $name,
            kind: ArgumentKind::$kind,
            optional: arguments!(@optional $required),
        }),*]
    };
    (@optional required) => { false };
    (@optional optional) => { true };
}

macro_rules! signature {
    ($syntax:literal; $($name:literal : $kind:ident $required:ident),* $(,)?) => {
        CommandSignature {
            syntax: $syntax,
            arguments: arguments!($($name : $kind $required),*),
        }
    };
}

macro_rules! command {
    (
        $id:literal, $canonical:literal, [$($alias:literal),* $(,)?],
        $category:literal, $help:literal, $mutation:ident, $risk:ident,
        $palette_visible:expr, $palette_label:literal, [$($signature:expr),+ $(,)?]
    ) => {
        CommandSpec {
            id: $id,
            canonical: $canonical,
            aliases: &[$($alias),*],
            signatures: &[$($signature),+],
            help: $help,
            category: $category,
            insertion: concat!($canonical, "["),
            dispatch_key: $canonical,
            mutation: MutationClass::$mutation,
            risk: RiskLevel::$risk,
            palette_visible: $palette_visible,
            palette_label: $palette_label,
        }
    };
}

const COMMANDS: &[CommandSpec] = &[
    // Crear
    command!(
        "geometry.point",
        "Point",
        [],
        "Crear",
        "Crea un punto libre.",
        CreatesObject,
        Low,
        false,
        "Point",
        [signature!("Point[(x, y)]"; "punto": Point required)]
    ),
    command!(
        "geometry.circle",
        "Circle",
        [],
        "Crear",
        "Crea una circunferencia.",
        CreatesObject,
        Low,
        false,
        "Circle",
        [signature!("Circle[centro, radio]"; "centro": Point required, "radio": Number required)]
    ),
    command!(
        "geometry.polygon",
        "Polygon",
        [],
        "Crear",
        "Crea un poligono cerrado.",
        CreatesObject,
        Low,
        false,
        "Polygon",
        [signature!("Polygon[(x1, y1), ...]"; "vertices": Point required)]
    ),
    command!(
        "geometry.function",
        "Function",
        ["func"],
        "Crear",
        "Grafica una funcion explicita.",
        CreatesObject,
        Low,
        false,
        "Function",
        [signature!("Function[expr]"; "expr": Expression required)]
    ),
    command!(
        "dynamic.animate",
        "Animate",
        ["animar"],
        "Dinamica",
        "Anima un parametro local; sin argumentos crea una fase ciclica.",
        TransformsObject,
        Low,
        true,
        "Animate",
        [
            signature!("Animate[]";),
            signature!("Animate[variable]"; "variable": Variable required),
            signature!("Animate[variable, minimo, maximo, velocidad]"; "variable": Variable required, "minimo": Number required, "maximo": Number required, "velocidad": Number required)
        ]
    ),
    command!(
        "animation.generate",
        "GenerateAnimation",
        ["GenerateAnimation"],
        "Animaciones",
        "Genera una animación didáctica (placeholder o Manim) para el concepto dado.",
        CreatesObject,
        Low,
        true,
        "GenerateAnimation",
        [
            signature!("GenerateAnimation[template, concepto]"; "template": Expression required, "concepto": Expression required),
            signature!("GenerateAnimation[template]"; "template": Expression required),
            signature!("GenerateAnimation[]";)
        ]
    ),
    command!(
        "complex.domain-coloring",
        "DomainColoring",
        ["domain_coloring", "dcolor"],
        "Complejos",
        "Visualiza fase y módulo de una función compleja en el plano 2D; límites opcionales y una resolución que debe ser un entero literal entre 16 y 300 (200 por defecto).",
        CreatesObject,
        Medium,
        true,
        "DomainColoring",
        [
            signature!("DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]"; "expr": ComplexExpression required, "xmin": Number optional, "xmax": Number optional, "ymin": Number optional, "ymax": Number optional, "resolution": Integer optional)
        ]
    ),
    command!(
        "graph.piecewise",
        "Piecewise",
        ["pw"],
        "Crear",
        "Crea una funcion definida por partes.",
        CreatesObject,
        Medium,
        true,
        "Piecewise",
        [
            signature!("Piecewise[condicion1, valor1, valor_por_defecto, ...]"; "condicion1": Expression required, "valor1": Expression required, "valor_por_defecto": Expression required)
        ]
    ),
    command!(
        "graph.contour",
        "Contour",
        ["contourlines", "contour_lines"],
        "Crear",
        "Crea curvas de nivel 2D con uno a dieciseis niveles finitos.",
        CreatesObject,
        High,
        true,
        "Contour",
        [
            signature!("Contour[f(x, y), xmin, xmax, ymin, ymax, nivel, ...]"; "f(x, y)": Expression required, "xmin": Number required, "xmax": Number required, "ymin": Number required, "ymax": Number required, "nivel": Number required)
        ]
    ),
    command!(
        "graph.phase-portrait",
        "PhasePortrait",
        ["phase_portrait", "phase"],
        "Crear",
        "Crea un retrato de fase 2D.",
        CreatesObject,
        High,
        true,
        "PhasePortrait",
        [
            signature!("PhasePortrait[dxdt, dydt]"; "dxdt": Expression required, "dydt": Expression required)
        ]
    ),
    command!(
        "complex.complex-grid",
        "ComplexGrid",
        ["complex_grid", "cgrid"],
        "Complejos",
        "Visualiza una rejilla compleja transformada; limites y densidad son opcionales.",
        CreatesObject,
        Medium,
        true,
        "ComplexGrid",
        [
            signature!("ComplexGrid[expr, xmin, xmax, ymin, ymax, density]"; "expr": ComplexExpression required, "xmin": Number optional, "xmax": Number optional, "ymin": Number optional, "ymax": Number optional, "density": Integer optional)
        ]
    ),
    command!(
        "graph.heat-map",
        "HeatMap",
        ["heat_map", "hmap"],
        "Crear",
        "Crea un mapa de calor 2D; limites y resolucion son opcionales.",
        CreatesObject,
        High,
        true,
        "HeatMap",
        [
            signature!("HeatMap[f(x, y), xmin, xmax, ymin, ymax, resolution]"; "f(x, y)": Expression required, "xmin": Number optional, "xmax": Number optional, "ymin": Number optional, "ymax": Number optional, "resolution": Integer optional)
        ]
    ),
    command!(
        "complex.quadrants",
        "Quadrants",
        ["cuadrantes"],
        "Complejos",
        "Muestra los cuadrantes del plano complejo con limites opcionales.",
        CreatesObject,
        Low,
        true,
        "Quadrants",
        [
            signature!("Quadrants[xmin, xmax, ymin, ymax]"; "xmin": Number optional, "xmax": Number optional, "ymin": Number optional, "ymax": Number optional)
        ]
    ),
    command!(
        "geometry.ellipse",
        "Ellipse",
        [],
        "Crear",
        "Crea una elipse por centro y semiejes.",
        CreatesObject,
        Low,
        true,
        "Ellipse",
        [
            signature!("Ellipse[(cx, cy), rx, ry]"; "centro": Point required, "rx": Number required, "ry": Number required)
        ]
    ),
    command!(
        "geometry.parabola",
        "Parabola",
        [],
        "Crear",
        "Crea una parabola por vertice y parametro.",
        CreatesObject,
        Low,
        true,
        "Parabola",
        [signature!("Parabola[(vx, vy), p]"; "vertice": Point required, "p": Number required)]
    ),
    command!(
        "geometry.hyperbola",
        "Hyperbola",
        [],
        "Crear",
        "Crea una hiperbola por centro y semiejes.",
        CreatesObject,
        Low,
        true,
        "Hyperbola",
        [
            signature!("Hyperbola[(cx, cy), a, b]"; "centro": Point required, "a": Number required, "b": Number required)
        ]
    ),
    command!(
        "geometry.regular-polygon",
        "RegularPolygon",
        ["regular_polygon"],
        "Crear",
        "Crea un poligono regular.",
        CreatesObject,
        Low,
        true,
        "RegularPolygon",
        [
            signature!("RegularPolygon[(cx, cy), n, r]"; "centro": Point required, "n": Integer required, "r": Number required)
        ]
    ),
    command!(
        "geometry.sampled-graph",
        "SampledGraph",
        [],
        "Crear",
        "Muestrea y=f(x) en 201 abscisas uniformes de [-range, range] y crea un poligono estatico cerrado con las muestras finitas; no es un lugar geometrico dinamico.",
        CreatesObject,
        Medium,
        true,
        "SampledGraph",
        [signature!("SampledGraph[expr, range]"; "expr": Expression required, "range": Number required)]
    ),
    command!(
        "dynamic.locus",
        "Locus",
        ["lugar"],
        "Dinamica",
        "Crea un lugar geometrico persistente: registra el objetivo despues de cada actualizacion local valida del driver, sin eventos de puntero ni tiempo.",
        AddsConstraint,
        Medium,
        true,
        "Locus",
        [signature!("Locus[driver, target]"; "driver": ObjectLabel required, "target": ObjectLabel required)]
    ),
    command!(
        "geometry.parametric-curve-2d",
        "ParametricCurve2D",
        ["parametric_curve_2d", "param2d"],
        "Crear",
        "Crea una curva parametrica 2D.",
        CreatesObject,
        Medium,
        true,
        "ParametricCurve2D",
        [
            signature!("ParametricCurve2D[x(t), y(t), t0, t1]"; "x(t)": Expression required, "y(t)": Expression required, "t0": Number required, "t1": Number required)
        ]
    ),
    command!(
        "geometry.polar-curve",
        "PolarCurve",
        ["polar_curve", "polar"],
        "Crear",
        "Crea una curva polar.",
        CreatesObject,
        Medium,
        true,
        "PolarCurve",
        [
            signature!("PolarCurve[r(t), t0, t1]"; "r(t)": Expression required, "t0": Number required, "t1": Number required)
        ]
    ),
    command!(
        "geometry.implicit-curve",
        "ImplicitCurve",
        ["ImplicitRegion"],
        "Crear",
        "Crea una curva implicita.",
        CreatesObject,
        High,
        true,
        "ImplicitCurve",
        [
            signature!("ImplicitCurve[f(x, y) = c]"; "ecuacion": Expression required),
            signature!("ImplicitCurve[lhs, rhs, relacion]"; "lhs": Expression required, "rhs": Expression required, "relacion": Relation required)
        ]
    ),
    command!(
        "geometry.vector-field-2d",
        "VectorField2D",
        ["vector_field_2d", "vf2d"],
        "Crear",
        "Crea un campo vectorial 2D.",
        CreatesObject,
        High,
        true,
        "VectorField2D",
        [
            signature!("VectorField2D[u(x, y), v(x, y)]"; "u": Expression required, "v": Expression required)
        ]
    ),
    // Construir
    command!(
        "construction.perpendicular",
        "Perpendicular",
        [],
        "Construir",
        "Crea una recta perpendicular.",
        CreatesObject,
        Low,
        false,
        "Perpendicular",
        [
            signature!("Perpendicular[punto, recta]"; "punto": Object required, "recta": Object required)
        ]
    ),
    command!(
        "construction.parallel",
        "Parallel",
        [],
        "Construir",
        "Crea una recta paralela.",
        CreatesObject,
        Low,
        false,
        "Parallel",
        [signature!("Parallel[punto, recta]"; "punto": Point required, "recta": Object required)]
    ),
    command!(
        "construction.tangent",
        "Tangent",
        [],
        "Construir",
        "Construye o restringe una tangencia segun los argumentos.",
        AddsConstraint,
        Medium,
        true,
        "Tangent",
        [
            signature!("Tangent[obj1, obj2]"; "obj1": Object required, "obj2": Object required),
            signature!("Tangent[centro, radio, punto]"; "centro": Point required, "radio": Number required, "punto": Point required)
        ]
    ),
    command!(
        "construction.perpendicular-bisector",
        "PerpendicularBisector",
        [],
        "Construir",
        "Crea la mediatriz de dos puntos.",
        CreatesObject,
        Low,
        true,
        "PerpendicularBisector",
        [
            signature!("PerpendicularBisector[(x1, y1), (x2, y2)]"; "a": Point required, "b": Point required)
        ]
    ),
    command!(
        "construction.angle-bisector",
        "AngleBisector",
        [],
        "Construir",
        "Crea la bisectriz de un angulo.",
        CreatesObject,
        Low,
        true,
        "AngleBisector",
        [
            signature!("AngleBisector[p1, vertice, p2]"; "p1": Point required, "vertice": Point required, "p2": Point required)
        ]
    ),
    command!(
        "construction.midpoint",
        "Midpoint",
        [],
        "Construir",
        "Crea el punto medio.",
        CreatesObject,
        Low,
        true,
        "Midpoint",
        [signature!("Midpoint[A, B]"; "A": Object required, "B": Object required)]
    ),
    command!(
        "construction.line",
        "Line",
        [],
        "Construir",
        "Crea una recta por dos puntos.",
        CreatesObject,
        Low,
        true,
        "Line",
        [signature!("Line[(x1, y1), (x2, y2)]"; "a": Point required, "b": Point required)]
    ),
    command!(
        "construction.segment",
        "Segment",
        [],
        "Construir",
        "Crea un segmento por dos puntos.",
        CreatesObject,
        Low,
        true,
        "Segment",
        [signature!("Segment[(x1, y1), (x2, y2)]"; "a": Point required, "b": Point required)]
    ),
    command!(
        "construction.vector",
        "Vector",
        [],
        "Construir",
        "Crea un vector por dos puntos.",
        CreatesObject,
        Low,
        true,
        "Vector",
        [
            signature!("Vector[(x1, y1), (x2, y2)]"; "origen": Point required, "extremo": Point required)
        ]
    ),
    command!(
        "construction.ray",
        "Ray",
        [],
        "Construir",
        "Crea una semirrecta por dos puntos.",
        CreatesObject,
        Low,
        true,
        "Ray",
        [
            signature!("Ray[(x1, y1), (x2, y2)]"; "origen": Point required, "direccion": Point required)
        ]
    ),
    // Transformar
    command!(
        "transform.translate",
        "Translate",
        [],
        "Transformar",
        "Traslada un objeto.",
        TransformsObject,
        Medium,
        true,
        "Translate",
        [
            signature!("Translate[punto, (dx, dy)]"; "punto": Point required, "desplazamiento": Vector required)
        ]
    ),
    command!(
        "transform.rotate",
        "Rotate",
        [],
        "Transformar",
        "Rota un objeto.",
        TransformsObject,
        Medium,
        true,
        "Rotate",
        [
            signature!("Rotate[punto, centro, angulo]"; "punto": Point required, "centro": Point required, "angulo": Number required),
            signature!("Rotate[punto, angulo]"; "punto": Point required, "angulo": Number required)
        ]
    ),
    command!(
        "transform.dilate",
        "Dilate",
        [],
        "Transformar",
        "Aplica una homotecia.",
        TransformsObject,
        Medium,
        true,
        "Dilate",
        [
            signature!("Dilate[punto, factor, centro]"; "punto": Point required, "factor": Number required, "centro": Point required)
        ]
    ),
    command!(
        "transform.reflect",
        "Reflect",
        ["mirror"],
        "Transformar",
        "Refleja un objeto respecto a un eje (linea) o a un circulo (inversion).",
        TransformsObject,
        Medium,
        true,
        "Reflect",
        [
            signature!("Reflect[obj, punto_a, punto_b]"; "obj": Object required, "punto_a": Point required, "punto_b": Point required),
            signature!("Reflect[obj, circulo]"; "obj": Object required, "circulo": Object required)
        ]
    ),
    command!(
        "transform.shear",
        "Shear",
        ["cizalla", "trasquilacion"],
        "Transformar",
        "Aplica cizallamiento afin: x' = x + k*y con k = tan(angulo).",
        TransformsObject,
        Medium,
        false,
        "Shear",
        [
            signature!("Shear[objeto, angulo, eje]"; "objeto": Object required, "angulo": Number required, "eje": Expression optional),
            signature!("Shear[objeto, angulo]"; "objeto": Object required, "angulo": Number required)
        ]
    ),
    command!(
        "transform.stretch",
        "Stretch",
        ["estirar", "estiramiento"],
        "Transformar",
        "Aplica estiramiento afin: x' = factor*x (o y' = factor*y segun eje).",
        TransformsObject,
        Medium,
        false,
        "Stretch",
        [
            signature!("Stretch[objeto, factor, eje]"; "objeto": Object required, "factor": Number required, "eje": Expression optional),
            signature!("Stretch[objeto, factor]"; "objeto": Object required, "factor": Number required)
        ]
    ),
    command!(
        "text.fraction",
        "FractionText",
        ["fraccion", "fraction"],
        "Crear",
        "Crea texto con valor fraccionario: FractionText[0.5] -> \"1/2\".",
        CreatesObject,
        Low,
        false,
        "FractionText",
        [
            signature!("FractionText[valor]"; "valor": Number required),
            signature!("FractionText[valor, punto]"; "valor": Number required, "punto": Point optional)
        ]
    ),
    command!(
        "text.surd",
        "SurdText",
        ["surd", "raiztexto"],
        "Crear",
        "Crea texto con surd: SurdText[1.414] -> \"√2\".",
        CreatesObject,
        Low,
        false,
        "SurdText",
        [
            signature!("SurdText[valor]"; "valor": Number required),
            signature!("SurdText[valor, punto]"; "valor": Number required, "punto": Point optional)
        ]
    ),
    command!(
        "spreadsheet.fill-column",
        "FillColumn",
        ["fill_column", "fillcol"],
        "Estadistica",
        "Rellena una columna de la hoja iterando filas y escribiendo valor; respeta MAX_SPREADSHEET_ROWS/COLS/RECOMPUTE.",
        CreatesObject,
        Medium,
        true,
        "FillColumn",
        [
            signature!("FillColumn[col, valor]"; "col": Expression required, "valor": Expression optional),
            signature!("FillColumn[col, inicio, fin, valor]"; "col": Expression required, "inicio": Integer optional, "fin": Integer optional, "valor": Expression optional)
        ]
    ),
    command!(
        "spreadsheet.fill-cells",
        "FillCells",
        ["fill_cells", "fillcells", "rellenar"],
        "Estadistica",
        "Rellena un rango rectangular de celdas con un valor; respeta presupuestos de spreadsheet.",
        CreatesObject,
        Medium,
        true,
        "FillCells",
        [
            signature!("FillCells[rango, valor]"; "rango": Expression required, "valor": Expression optional),
            signature!("FillCells[a1, b2, valor]"; "a1": Expression required, "b2": Expression required, "valor": Expression optional)
        ]
    ),
    command!(
        "spreadsheet.cell-range",
        "CellRange",
        ["cell_range", "rango"],
        "Estadistica",
        "Resuelve un rango A1:B2 a array de valores evaluados; soporta A1:B2 o A1,B2.",
        ReadOnly,
        Low,
        true,
        "CellRange",
        [
            signature!("CellRange[a1, b2]"; "a1": Expression required, "b2": Expression optional),
            signature!("CellRange[rango]"; "rango": Expression required)
        ]
    ),
    // Restricciones, conicas y booleanas documentadas
    command!(
        "constraint.distance",
        "Distance",
        ["dist"],
        "Restricciones",
        "Impone una distancia entre objetos.",
        AddsConstraint,
        Medium,
        false,
        "Distance",
        [
            signature!("Distance[A, B, valor]"; "A": Object required, "B": Object required, "valor": Number optional)
        ]
    ),
    command!(
        "constraint.angle",
        "Angle",
        [],
        "Restricciones",
        "Impone un angulo entre objetos.",
        AddsConstraint,
        Medium,
        false,
        "Angle",
        [
            signature!("Angle[l1, l2, grados]"; "l1": Object required, "l2": Object required, "grados": Number optional)
        ]
    ),
    command!(
        "constraint.coincident",
        "Coincident",
        [],
        "Restricciones",
        "Hace coincidir dos puntos.",
        AddsConstraint,
        Medium,
        false,
        "Coincident",
        [signature!("Coincident[A, B]"; "A": Object required, "B": Object required)]
    ),
    command!(
        "constraint.horizontal",
        "Horizontal",
        [],
        "Restricciones",
        "Fuerza una orientacion horizontal.",
        AddsConstraint,
        Medium,
        false,
        "Horizontal",
        [signature!("Horizontal[obj]"; "obj": Object required)]
    ),
    command!(
        "constraint.vertical",
        "Vertical",
        [],
        "Restricciones",
        "Fuerza una orientacion vertical.",
        AddsConstraint,
        Medium,
        false,
        "Vertical",
        [signature!("Vertical[obj]"; "obj": Object required)]
    ),
    command!(
        "constraint.equal-length",
        "EqualLength",
        ["equal_length", "eqlength"],
        "Restricciones",
        "Iguala longitudes.",
        AddsConstraint,
        Medium,
        false,
        "EqualLength",
        [signature!("EqualLength[s1, s2]"; "s1": Object required, "s2": Object required)]
    ),
    command!(
        "constraint.symmetry",
        "Symmetry",
        [],
        "Restricciones",
        "Impone simetria respecto a un eje.",
        AddsConstraint,
        Medium,
        false,
        "Symmetry",
        [
            signature!("Symmetry[P, Q, eje]"; "P": Object required, "Q": Object required, "eje": Object required)
        ]
    ),
    command!(
        "conic.ellipse-by-foci",
        "EllipseByFoci",
        ["ellipse_by_foci"],
        "Conicas",
        "Construye una elipse por focos.",
        AddsConstraint,
        Medium,
        false,
        "EllipseByFoci",
        [
            signature!("EllipseByFoci[F1, F2, P]"; "F1": Object required, "F2": Object required, "P": Object required)
        ]
    ),
    command!(
        "conic.parabola-by-focus-directrix",
        "ParabolaByFocusDirectrix",
        ["parabola_by_focus_directrix"],
        "Conicas",
        "Construye una parabola por foco y directriz.",
        AddsConstraint,
        Medium,
        false,
        "ParabolaByFocusDirectrix",
        [signature!("ParabolaByFocusDirectrix[F, d]"; "F": Object required, "d": Object required)]
    ),
    command!(
        "conic.hyperbola-by-foci",
        "HyperbolaByFoci",
        ["hyperbola_by_foci"],
        "Conicas",
        "Construye una hiperbola por focos.",
        AddsConstraint,
        Medium,
        false,
        "HyperbolaByFoci",
        [
            signature!("HyperbolaByFoci[F1, F2, P]"; "F1": Object required, "F2": Object required, "P": Object required)
        ]
    ),
    command!(
        "conic.by-five-points",
        "ConicByFivePoints",
        ["conic_by_five_points"],
        "Conicas",
        "Ajusta una conica por cinco puntos.",
        AddsConstraint,
        High,
        false,
        "ConicByFivePoints",
        [
            signature!("ConicByFivePoints[A, B, C, D, E]"; "A": Object required, "B": Object required, "C": Object required, "D": Object required, "E": Object required)
        ]
    ),
    command!(
        "boolean.union",
        "PolygonUnion",
        ["polyunion"],
        "Booleanas",
        "Une dos poligonos.",
        CreatesObject,
        High,
        false,
        "PolygonUnion",
        [
            signature!("PolygonUnion[poly1, poly2]"; "poly1": Object required, "poly2": Object required)
        ]
    ),
    command!(
        "boolean.intersection",
        "PolygonIntersection",
        ["polyintersection"],
        "Booleanas",
        "Interseca dos poligonos.",
        CreatesObject,
        High,
        false,
        "PolygonIntersection",
        [
            signature!("PolygonIntersection[poly1, poly2]"; "poly1": Object required, "poly2": Object required)
        ]
    ),
    command!(
        "boolean.difference",
        "PolygonDifference",
        ["polydifference"],
        "Booleanas",
        "Resta dos poligonos.",
        CreatesObject,
        High,
        false,
        "PolygonDifference",
        [
            signature!("PolygonDifference[poly1, poly2]"; "poly1": Object required, "poly2": Object required)
        ]
    ),
    command!(
        "boolean.xor",
        "PolygonXor",
        ["polyxor"],
        "Booleanas",
        "Calcula la diferencia simetrica.",
        CreatesObject,
        High,
        false,
        "PolygonXor",
        [
            signature!("PolygonXor[poly1, poly2]"; "poly1": Object required, "poly2": Object required)
        ]
    ),
    command!(
        "binding.point",
        "PointExpr",
        [],
        "Expresiones",
        "Crea un punto ligado a expresiones.",
        CreatesObject,
        Low,
        false,
        "PointExpr",
        [
            signature!("PointExpr[x_expr, y_expr]"; "x_expr": Expression required, "y_expr": Expression required)
        ]
    ),
    command!(
        "binding.circle",
        "CircleExpr",
        [],
        "Expresiones",
        "Crea un circulo con radio ligado a una expresion.",
        CreatesObject,
        Low,
        false,
        "CircleExpr",
        [
            signature!("CircleExpr[centro, radius_expr]"; "centro": Point required, "radius_expr": Expression required)
        ]
    ),
    // CAS y analisis
    command!(
        "cas.derivative",
        "Derivative",
        ["derivada", "deriv", "diff"],
        "CAS",
        "Deriva simbolicamente una expresion.",
        CreatesObject,
        Low,
        true,
        "Derivative",
        [
            signature!("Derivative[expr, variable]"; "expr": Expression required, "variable": Variable optional)
        ]
    ),
    command!(
        "cas.integral",
        "Integral",
        ["integrar", "int"],
        "CAS",
        "Calcula una integral simbolica o definida.",
        CreatesObject,
        Medium,
        true,
        "Integral",
        [
            signature!("Integral[expr]"; "expr": Expression required),
            signature!("Integral[expr, variable]"; "expr": Expression required, "variable": Variable required),
            signature!("Integral[expr, a, b]"; "expr": Expression required, "a": Number required, "b": Number required),
            signature!("Integral[expr, variable, a, b]"; "expr": Expression required, "variable": Variable required, "a": Number required, "b": Number required)
        ]
    ),
    command!(
        "cas.solve",
        "Solve",
        ["nsolve", "resolver"],
        "CAS",
        "Resuelve una ecuacion en la variable indicada.",
        CreatesObject,
        Medium,
        true,
        "Solve",
        [
            signature!("Solve[expr, variable, minimo, maximo]"; "expr": Expression required, "variable": Variable optional, "minimo": Number optional, "maximo": Number optional)
        ]
    ),
    command!(
        "cas.limit",
        "Limit",
        ["limite", "lim"],
        "CAS",
        "Estima un limite bilateral finito.",
        ReadOnly,
        Medium,
        true,
        "Limit",
        [
            signature!("Limit[expr, variable, punto]"; "expr": Expression required, "variable": Variable required, "punto": Number required)
        ]
    ),
    command!(
        "cas.limit-above",
        "LimitAbove",
        ["limite_superior", "limite_derecho", "limitabove"],
        "CAS",
        "Estima un límite lateral por la derecha (x→a⁺).",
        ReadOnly,
        Medium,
        true,
        "LimitAbove",
        [
            signature!("LimitAbove[expr, variable, punto]"; "expr": Expression required, "variable": Variable required, "punto": Number required)
        ]
    ),
    command!(
        "cas.limit-below",
        "LimitBelow",
        ["limite_inferior", "limite_izquierdo", "limitbelow"],
        "CAS",
        "Estima un límite lateral por la izquierda (x→a⁻).",
        ReadOnly,
        Medium,
        true,
        "LimitBelow",
        [
            signature!("LimitBelow[expr, variable, punto]"; "expr": Expression required, "variable": Variable required, "punto": Number required)
        ]
    ),
    command!(
        "cas.parametric-derivative",
        "ParametricDerivative",
        [
            "derivada_parametrica",
            "derivadaParametrica",
            "parametricderivative"
        ],
        "CAS",
        "Deriva paramétrica dy/dx = (dy/dt)/(dx/dt) simbólicamente.",
        ReadOnly,
        Low,
        true,
        "ParametricDerivative",
        [
            signature!("ParametricDerivative[x(t), y(t), variable]"; "x(t)": Expression required, "y(t)": Expression required, "variable": Variable required),
            signature!("ParametricDerivative[x(t), y(t)]"; "x(t)": Expression required, "y(t)": Expression required)
        ]
    ),
    command!(
        "cas.asymptote",
        "Asymptote",
        ["asintota", "asíntota", "asymptote"],
        "CAS",
        "Calcula asíntota oblicua y = m·x + b con m = lim f/x, b = lim f−m·x.",
        ReadOnly,
        Medium,
        true,
        "Asymptote",
        [
            signature!("Asymptote[expr]"; "expr": Expression required),
            signature!("Asymptote[expr, variable]"; "expr": Expression required, "variable": Variable required)
        ]
    ),
    command!(
        "cas.groebner-degrevlex",
        "GroebnerDegRevLex",
        [
            "groebner",
            "groebnerbasis",
            "groebnerlex",
            "groebner_basis",
            "groebnerDegRevLex"
        ],
        "CAS",
        "Base de Groebner (stub: no implementado, use Eliminate).",
        ReadOnly,
        Low,
        true,
        "GroebnerDegRevLex",
        [
            signature!("GroebnerDegRevLex[polinomios]"; "polinomios": Expression required),
            signature!("GroebnerDegRevLex[polinomios, variables]"; "polinomios": Expression required, "variables": ParameterList optional)
        ]
    ),
    command!(
        "cas.factor",
        "Factor",
        ["factorizar"],
        "CAS",
        "Factoriza polinomios equivalentes.",
        ReadOnly,
        Low,
        true,
        "Factor",
        [
            signature!("Factor[expr, variable]"; "expr": Expression required, "variable": Variable optional)
        ]
    ),
    command!(
        "cas.expand",
        "Expand",
        ["expandir"],
        "CAS",
        "Expande productos y potencias algebraicas.",
        ReadOnly,
        Low,
        true,
        "Expand",
        [signature!("Expand[expr]"; "expr": Expression required)]
    ),
    command!(
        "cas.simplify",
        "Simplify",
        ["simplificar"],
        "CAS",
        "Simplifica una expresion mediante reglas seguras.",
        ReadOnly,
        Low,
        true,
        "Simplify",
        [signature!("Simplify[expr]"; "expr": Expression required)]
    ),
    command!(
        "cas.taylor",
        "Taylor",
        [],
        "CAS",
        "Construye una serie de Taylor finita.",
        CreatesObject,
        Medium,
        true,
        "Taylor",
        [
            signature!("Taylor[expr, variable, centro, orden]"; "expr": Expression required, "variable": Variable required, "centro": Number optional, "orden": Integer optional)
        ]
    ),
    command!(
        "cas.complete-square",
        "CompleteSquare",
        [
            "complete_square",
            "completesquare",
            "completarCuadrado",
            "completar_cuadrado"
        ],
        "CAS",
        "Completa cuadrado: convierte a*x^2+b*x+c a a*(x+b/2a)^2 + (c - b^2/4a).",
        ReadOnly,
        Low,
        true,
        "CompleteSquare",
        [
            signature!("CompleteSquare[expr, variable]"; "expr": Expression required, "variable": Variable required),
            signature!("CompleteSquare[expr]"; "expr": Expression required)
        ]
    ),
    command!(
        "cas.prime-factors",
        "PrimeFactors",
        ["prime_factors", "primefactors", "factoresPrimos", "factores_primos"],
        "CAS",
        "Factoriza un entero n (2 <= n <= 1e12) en primos por trial division.",
        ReadOnly,
        Low,
        true,
        "PrimeFactors",
        [signature!("PrimeFactors[n]"; "n": Integer required)]
    ),
    command!(
        "cas.ifactor",
        "IFactor",
        ["ifactor", "ifactorizar", "factorEntero", "factor_entero"],
        "CAS",
        "Factorización entera: si es entero usa PrimeFactors, si es polinomio extrae contenido entero y lo factoriza.",
        ReadOnly,
        Low,
        true,
        "IFactor",
        [
            signature!("IFactor[expr]"; "expr": Expression required),
            signature!("IFactor[expr, variable]"; "expr": Expression required, "variable": Variable required)
        ]
    ),
    command!(
        "cas.assume",
        "Assume",
        ["assume", "asumir", "suponer", "supone"],
        "CAS",
        "Almacena hipótesis como x>0 (positive), x!=0 (nonzero), x real/integer; guarda en Document.variables_assumptions.",
        ReadOnly,
        Low,
        true,
        "Assume",
        [signature!("Assume[predicado]"; "predicado": Expression required)]
    ),
    command!(
        "analysis.root",
        "Root",
        ["raiz", "raices"],
        "Analisis",
        "Busca raices de una funcion.",
        CreatesObject,
        Medium,
        true,
        "Root",
        [signature!("Root[f]"; "f": Object required)]
    ),
    command!(
        "analysis.extremum",
        "Extremum",
        ["extremos", "max", "min"],
        "Analisis",
        "Busca extremos locales.",
        CreatesObject,
        Medium,
        true,
        "Extremum",
        [signature!("Extremum[f]"; "f": Object required)]
    ),
    command!(
        "analysis.inflection",
        "Inflection",
        ["inflexion"],
        "Analisis",
        "Busca puntos de inflexion.",
        CreatesObject,
        Medium,
        true,
        "Inflection",
        [signature!("Inflection[f]"; "f": Object required)]
    ),
    command!(
        "analysis.y-intercept",
        "YIntercept",
        ["interceptoy", "intercepto_y"],
        "Analisis",
        "Calcula el intercepto con el eje Y.",
        CreatesObject,
        Low,
        true,
        "YIntercept",
        [signature!("YIntercept[f]"; "f": Object required)]
    ),
    command!(
        "analysis.x-intercept",
        "XIntercept",
        ["interceptox", "intercepto_x"],
        "Analisis",
        "Calcula los interceptos con el eje X.",
        CreatesObject,
        Medium,
        true,
        "XIntercept",
        [signature!("XIntercept[f]"; "f": Object required)]
    ),
    command!(
        "analysis.intersect",
        "Intersect",
        ["interseccion"],
        "Analisis",
        "Calcula intersecciones entre curvas.",
        CreatesObject,
        Medium,
        true,
        "Intersect",
        [signature!("Intersect[a, b]"; "a": Object required, "b": Object required)]
    ),
    command!(
        "analysis.analyze",
        "Analyze",
        ["analizar", "analisis"],
        "Analisis",
        "Ejecuta el analisis disponible de una funcion.",
        CreatesObject,
        Medium,
        true,
        "Analyze",
        [signature!("Analyze[f]"; "f": Object required)]
    ),
    // Complejos y calculo multivariable
    command!(
        "complex.mapping",
        "ComplexMapping",
        ["complex_mapping", "mapeocomplejo"],
        "Complejos",
        "Aplica un mapeo complejo a un objetivo.",
        CreatesObject,
        High,
        true,
        "ComplexMapping",
        [
            signature!("ComplexMapping[expr_compleja, target]"; "expr_compleja": ComplexExpression required, "target": Object required)
        ]
    ),
    command!(
        "complex.gauss",
        "Gauss",
        ["residuos", "residue"],
        "Complejos",
        "Calcula una integral compleja por residuos.",
        CreatesObject,
        High,
        true,
        "Gauss",
        [
            signature!("Gauss[expr_compleja, curva]"; "expr_compleja": ComplexExpression required, "curva": Curve required)
        ]
    ),
    command!(
        "complex.integral",
        "ComplexIntegral",
        ["integralcompleja", "contourintegral"],
        "Complejos",
        "Calcula una integral compleja sobre una curva.",
        CreatesObject,
        High,
        true,
        "ComplexIntegral",
        [
            signature!("ComplexIntegral[expr_compleja, curva]"; "expr_compleja": ComplexExpression required, "curva": Curve required)
        ]
    ),
    command!(
        "am1.riemann-sum",
        "RiemannSum",
        [],
        "AM1",
        "Calcula una suma de Riemann.",
        ReadOnly,
        Medium,
        true,
        "RiemannSum",
        [
            signature!("RiemannSum[f, x, a, b, n, metodo]"; "f": Expression required, "x": Variable required, "a": Number required, "b": Number required, "n": Integer required, "metodo": ParameterList optional)
        ]
    ),
    command!(
        "am1.bolzano",
        "BolzanoCheck",
        [],
        "AM1",
        "Verifica condiciones del teorema de Bolzano.",
        ReadOnly,
        Medium,
        true,
        "BolzanoCheck",
        [
            signature!("BolzanoCheck[f, x, a, b]"; "f": Expression required, "x": Variable required, "a": Number required, "b": Number required)
        ]
    ),
    command!(
        "am1.lhopital",
        "LHopital",
        [],
        "AM1",
        "Aplica pasos de la regla de L'Hopital.",
        ReadOnly,
        Medium,
        true,
        "LHopital",
        [
            signature!("LHopital[num, den, x, a, max_steps]"; "num": Expression required, "den": Expression required, "x": Variable required, "a": Number required, "max_steps": Integer optional)
        ]
    ),
    command!(
        "am2.jacobian",
        "JacobianMatrix",
        [],
        "AM2",
        "Calcula una matriz Jacobiana.",
        ReadOnly,
        Medium,
        true,
        "JacobianMatrix",
        [
            signature!("JacobianMatrix[[f1, f2], [x, y]]"; "funciones": ParameterList required, "variables": ParameterList required)
        ]
    ),
    command!(
        "am2.hessian",
        "Hessian",
        [],
        "AM2",
        "Calcula una matriz Hessiana.",
        ReadOnly,
        Medium,
        true,
        "Hessian",
        [
            signature!("Hessian[f, [x, y]]"; "f": Expression required, "variables": ParameterList required)
        ]
    ),
    command!(
        "am2.line-integral-vector",
        "LineIntegralVector",
        [],
        "AM2",
        "Calcula una integral de linea vectorial.",
        ReadOnly,
        High,
        true,
        "LineIntegralVector",
        [
            signature!("LineIntegralVector[[P, Q], [x(t), y(t)], t, a, b, n]"; "campo": ParameterList required, "curva": ParameterList required, "t": Variable required, "a": Number required, "b": Number required, "n": Integer required)
        ]
    ),
    command!(
        "am2.triple-integral",
        "TripleIntegral",
        [],
        "AM2",
        "Calcula una integral triple numerica.",
        ReadOnly,
        High,
        true,
        "TripleIntegral",
        [
            signature!("TripleIntegral[f, x, a, b, y, c, d, z, e, f, n]"; "integrando": Expression required, "dominio": Domain required, "n": Integer required)
        ]
    ),
    command!(
        "am2.flux",
        "Flux",
        [],
        "AM2",
        "Calcula el flujo de un campo vectorial.",
        ReadOnly,
        High,
        true,
        "Flux",
        [
            signature!("Flux[[P, Q, R], superficie, [u, v], u0, u1, v0, v1, n]"; "campo": ParameterList required, "superficie": ParameterList required, "dominio": Domain required, "n": Integer required)
        ]
    ),
    command!(
        "am2.green",
        "GreenTheorem",
        [],
        "AM2",
        "Calcula una verificacion del teorema de Green.",
        ReadOnly,
        High,
        true,
        "GreenTheorem",
        [
            signature!("GreenTheorem[[P, Q], x, a, b, y, c, d, n]"; "campo": ParameterList required, "dominio": Domain required, "n": Integer required)
        ]
    ),
    command!(
        "am2.gauss-ostrogradski",
        "GaussOstrogradski",
        [],
        "AM2",
        "Calcula una verificacion de Gauss-Ostrogradski.",
        ReadOnly,
        High,
        true,
        "GaussOstrogradski",
        [
            signature!("GaussOstrogradski[[P, Q, R], x, a, b, y, c, d, z, e, f, n]"; "campo": ParameterList required, "dominio": Domain required, "n": Integer required)
        ]
    ),
    // Matrices, probabilidad y estadistica
    command!(
        "matrix.determinant",
        "Determinant",
        ["det"],
        "Matrices",
        "Calcula un determinante.",
        ReadOnly,
        Medium,
        true,
        "Determinant",
        [signature!("Determinant[[a, b], [c, d]]"; "matriz": Matrix required)]
    ),
    command!(
        "matrix.inverse",
        "Inverse",
        ["inversa"],
        "Matrices",
        "Calcula una matriz inversa.",
        ReadOnly,
        Medium,
        true,
        "Inverse",
        [signature!("Inverse[[a, b], [c, d]]"; "matriz": Matrix required)]
    ),
    CommandSpec {
        dispatch_key: "LinearSolve",
        ..command!(
            "matrix.solve-system",
            "SolveSystem",
            ["linearsolve", "linsolve", "sistema"],
            "Matrices",
            "Resuelve un sistema lineal.",
            ReadOnly,
            Medium,
            true,
            "SolveSystem",
            [signature!("SolveSystem[A, b]"; "A": Matrix required, "b": Vector required)]
        )
    },
    command!(
        "matrix.gauss-jordan",
        "GaussJordan",
        [],
        "Matrices",
        "Reduce una matriz por Gauss-Jordan.",
        ReadOnly,
        Medium,
        true,
        "GaussJordan",
        [signature!("GaussJordan[A]"; "A": Matrix required)]
    ),
    command!(
        "matrix.cramer",
        "Cramer",
        [],
        "Matrices",
        "Resuelve un sistema por Cramer.",
        ReadOnly,
        Medium,
        true,
        "Cramer",
        [signature!("Cramer[A, b]"; "A": Matrix required, "b": Vector required)]
    ),
    command!(
        "matrix.change-of-basis",
        "ChangeOfBasis",
        [],
        "Matrices",
        "Cambia coordenadas entre bases.",
        ReadOnly,
        Medium,
        true,
        "ChangeOfBasis",
        [
            signature!("ChangeOfBasis[v, B_from, B_to]"; "v": Vector required, "B_from": Matrix required, "B_to": Matrix required)
        ]
    ),
    command!(
        "matrix.diagonalization",
        "Diagonalization",
        [],
        "Matrices",
        "Intenta diagonalizar una matriz.",
        ReadOnly,
        High,
        true,
        "Diagonalization",
        [signature!("Diagonalization[A]"; "A": Matrix required)]
    ),
    command!(
        "probability.normal",
        "Normal",
        [],
        "Probabilidad",
        "Evalua o crea una distribucion normal.",
        ReadOnly,
        Low,
        true,
        "Normal",
        [signature!("Normal[mu, sigma]"; "mu": Number required, "sigma": Number required)]
    ),
    command!(
        "probability.binomial",
        "Binomial",
        [],
        "Probabilidad",
        "Evalua una distribucion binomial.",
        ReadOnly,
        Low,
        true,
        "Binomial",
        [
            signature!("Binomial[n, p, k]"; "n": Integer required, "p": Number required, "k": Integer required)
        ]
    ),
    command!(
        "probability.poisson",
        "Poisson",
        [],
        "Probabilidad",
        "Evalua una distribucion de Poisson.",
        ReadOnly,
        Low,
        true,
        "Poisson",
        [signature!("Poisson[lambda, k]"; "lambda": Number required, "k": Integer required)]
    ),
    command!(
        "statistics.histogram",
        "Histogram",
        ["histograma"],
        "Estadistica",
        "Crea un histograma.",
        CreatesObject,
        Medium,
        true,
        "Histogram",
        [signature!("Histogram[{data}, bins]"; "data": Data required, "bins": Integer optional)]
    ),
    command!(
        "statistics.scatter-plot",
        "ScatterPlot",
        ["scatter"],
        "Estadistica",
        "Crea un grafico de dispersion.",
        CreatesObject,
        Medium,
        true,
        "ScatterPlot",
        [signature!("ScatterPlot[{xs}, {ys}]"; "xs": Data required, "ys": Data required)]
    ),
    command!(
        "statistics.box-plot",
        "BoxPlot",
        [],
        "Estadistica",
        "Crea un diagrama de caja.",
        CreatesObject,
        Medium,
        true,
        "BoxPlot",
        [signature!("BoxPlot[{data}]"; "data": Data required)]
    ),
    command!(
        "statistics.linear-regression",
        "LinearRegression",
        ["regression", "regresion"],
        "Estadistica",
        "Calcula una regresion lineal.",
        CreatesObject,
        Medium,
        true,
        "LinearRegression",
        [signature!("LinearRegression[{xs}, {ys}]"; "xs": Data required, "ys": Data required)]
    ),
    command!(
        "statistics.data-table",
        "DataTable",
        ["datos", "tabla"],
        "Estadistica",
        "Crea una tabla local de pares x/y y un gráfico de dispersión enlazado.",
        CreatesObject,
        Medium,
        true,
        "DataTable",
        [signature!("DataTable[{xs}, {ys}]"; "xs": Data required, "ys": Data required)]
    ),
    command!(
        "statistics.fit-linear",
        "FitLinear",
        ["ajuste lineal"],
        "Estadistica",
        "Ajusta una recta a una tabla local y muestra RMSE y R².",
        CreatesObject,
        Medium,
        true,
        "FitLinear",
        [signature!("FitLinear[tabla]"; "tabla": Object required)]
    ),
    command!(
        "statistics.fit-polynomial",
        "FitPoly",
        ["ajuste polinomico"],
        "Estadistica",
        "Ajusta un polinomio de grado elegido a una tabla local.",
        CreatesObject,
        Medium,
        true,
        "FitPoly",
        [signature!("FitPoly[tabla, grado]"; "tabla": Object required, "grado": Integer required)]
    ),
    command!(
        "statistics.fit-exponential",
        "FitExp",
        ["ajuste exponencial"],
        "Estadistica",
        "Ajusta y = a exp(bx) a una tabla local con y positiva.",
        CreatesObject,
        Medium,
        true,
        "FitExp",
        [signature!("FitExp[tabla]"; "tabla": Object required)]
    ),
    command!(
        "statistics.fit-logarithmic",
        "FitLog",
        ["ajuste logaritmico"],
        "Estadistica",
        "Ajusta y = a ln(x) + b a una tabla local con x positiva.",
        CreatesObject,
        Medium,
        true,
        "FitLog",
        [signature!("FitLog[tabla]"; "tabla": Object required)]
    ),
    command!(
        "statistics.fit-power",
        "FitPow",
        ["ajuste potencia"],
        "Estadistica",
        "Ajusta y = a x^b a una tabla local con x e y positivas.",
        CreatesObject,
        Medium,
        true,
        "FitPow",
        [signature!("FitPow[tabla]"; "tabla": Object required)]
    ),
    command!(
        "statistics.fit-sinusoidal",
        "FitSin",
        ["ajuste sinusoidal"],
        "Estadistica",
        "Ajusta una senoide local con una búsqueda de frecuencia acotada.",
        CreatesObject,
        High,
        true,
        "FitSin",
        [signature!("FitSin[tabla]"; "tabla": Object required)]
    ),
    command!(
        "statistics.mean",
        "Mean",
        ["media"],
        "Estadistica",
        "Calcula la media.",
        ReadOnly,
        Low,
        true,
        "Mean",
        [signature!("Mean[{data}]"; "data": Data required)]
    ),
    command!(
        "statistics.median",
        "Median",
        ["mediana"],
        "Estadistica",
        "Calcula la mediana.",
        ReadOnly,
        Low,
        true,
        "Median",
        [signature!("Median[{data}]"; "data": Data required)]
    ),
    command!(
        "statistics.std-dev",
        "StdDev",
        ["desviacion"],
        "Estadistica",
        "Calcula el desvio estandar.",
        ReadOnly,
        Low,
        true,
        "StdDev",
        [signature!("StdDev[{data}]"; "data": Data required)]
    ),
    command!(
        "statistics.correlation",
        "Correlation",
        ["correlacion"],
        "Estadistica",
        "Calcula una correlacion.",
        ReadOnly,
        Low,
        true,
        "Correlation",
        [signature!("Correlation[{xs}, {ys}]"; "xs": Data required, "ys": Data required)]
    ),
    command!(
        "probability.inverse-normal",
        "InverseNormal",
        [
            "inverse_normal",
            "inversenormal",
            "cuantilnormal",
            "cuantil_normal"
        ],
        "Probabilidad",
        "Cuantil normal: InverseNormal[p, mu, sigma] (p en (0,1), sigma>0); con un arg usa N(0,1).",
        ReadOnly,
        Low,
        true,
        "InverseNormal",
        [
            signature!("InverseNormal[p]"; "p": Number required),
            signature!("InverseNormal[p, mu, sigma]"; "p": Number required, "mu": Number required, "sigma": Number required)
        ]
    ),
    command!(
        "probability.inverse-t",
        "InverseT",
        ["inverse_t", "inverset", "cuantilt", "cuantil_t"],
        "Probabilidad",
        "Cuantil t-Student: InverseT[p, df] (p en (0,1), df>0).",
        ReadOnly,
        Low,
        true,
        "InverseT",
        [signature!("InverseT[p, df]"; "p": Number required, "df": Number required)]
    ),
    command!(
        "probability.inverse-chi-squared",
        "InverseChiSquared",
        [
            "inverse_chi_squared",
            "inversechisquared",
            "inversachicuadrado",
            "cuantilchicuadrado"
        ],
        "Probabilidad",
        "Cuantil chi-cuadrado: InverseChiSquared[p, df] (p en (0,1), df>0).",
        ReadOnly,
        Low,
        true,
        "InverseChiSquared",
        [signature!("InverseChiSquared[p, df]"; "p": Number required, "df": Number required)]
    ),
    command!(
        "probability.inverse-f",
        "InverseF",
        ["inverse_f", "inversef", "cuantilf", "cuantil_f"],
        "Probabilidad",
        "Cuantil F de Fisher: InverseF[p, df1, df2] (p en (0,1), df1>0, df2>0).",
        ReadOnly,
        Low,
        true,
        "InverseF",
        [signature!("InverseF[p, df1, df2]"; "p": Number required, "df1": Number required, "df2": Number required)]
    ),
    command!(
        "statistics.frequency-table",
        "FrequencyTable",
        ["frequency_table", "frecuencia", "tabl frecuencias"],
        "Estadistica",
        "Tabla de frecuencias: FrequencyTable[{datos}].",
        ReadOnly,
        Low,
        true,
        "FrequencyTable",
        [signature!("FrequencyTable[{datos}]"; "datos": Data required)]
    ),
    command!(
        "statistics.stem-plot",
        "StemPlot",
        ["stem_plot", "stemleaf", "tallo_hoja", "diagrama_tallo"],
        "Estadistica",
        "Diagrama tallo-hoja: StemPlot[{datos}] texto.",
        ReadOnly,
        Low,
        true,
        "StemPlot",
        [signature!("StemPlot[{datos}]"; "datos": Data required)]
    ),
    command!(
        "statistics.residual-plot",
        "ResidualPlot",
        ["residual_plot", "residuos", "grafico_residuos"],
        "Estadistica",
        "Residuos de regresión lineal: ResidualPlot[{xs}, {ys}] o ResidualPlot[tabla].",
        ReadOnly,
        Low,
        true,
        "ResidualPlot",
        [
            signature!("ResidualPlot[{xs}, {ys}]"; "xs": Data required, "ys": Data required),
            signature!("ResidualPlot[tabla]"; "tabla": Object required)
        ]
    ),
    command!(
        "statistics.t-test",
        "TTest",
        ["ttest", "t_test", "prueba_t"],
        "Estadistica",
        "Prueba t de una muestra: TTest[{datos}, mu0].",
        ReadOnly,
        Low,
        true,
        "TTest",
        [signature!("TTest[{datos}, mu0]"; "datos": Data required, "mu0": Number required)]
    ),
    command!(
        "statistics.t-test-two-sample",
        "TTest2",
        ["ttest2", "t_test2", "prueba_t2"],
        "Estadistica",
        "Prueba t de dos muestras independientes: TTest2[{a}, {b}].",
        ReadOnly,
        Low,
        true,
        "TTest2",
        [signature!("TTest2[{a}, {b}]"; "a": Data required, "b": Data required)]
    ),
    command!(
        "statistics.t-test-paired",
        "TTestPaired",
        ["ttest_paired", "t_paired", "prueba_t_pareada", "ttestpareado"],
        "Estadistica",
        "Prueba t pareada: TTestPaired[{antes}, {despues}].",
        ReadOnly,
        Low,
        true,
        "TTestPaired",
        [signature!("TTestPaired[{a}, {b}]"; "a": Data required, "b": Data required)]
    ),
    command!(
        "statistics.z-test",
        "ZTest",
        ["ztest", "z_test", "prueba_z"],
        "Estadistica",
        "Prueba z de una muestra con sigma conocido: ZTest[{datos}, mu0, sigma].",
        ReadOnly,
        Low,
        true,
        "ZTest",
        [signature!("ZTest[{datos}, mu0, sigma]"; "datos": Data required, "mu0": Number required, "sigma": Number required)]
    ),
    command!(
        "statistics.chi-squared-test",
        "ChiSqTest",
        ["chisqtest", "chi2test", "prueba_chi2", "chi_cuadrado"],
        "Estadistica",
        "Prueba chi-cuadrado de bondad de ajuste: ChiSqTest[{obs}, {esp}].",
        ReadOnly,
        Low,
        true,
        "ChiSqTest",
        [signature!("ChiSqTest[{obs}, {esp}]"; "obs": Data required, "esp": Data required)]
    ),
    command!(
        "statistics.anova",
        "ANOVA",
        ["anova", "anova_oneway"],
        "Estadistica",
        "ANOVA de un factor: ANOVA[{g1}, {g2}, ...].",
        ReadOnly,
        Low,
        true,
        "ANOVA",
        [signature!("ANOVA[{g1}, {g2}]"; "g1": Data required, "g2": Data required)]
    ),
    command!(
        "finance.rate",
        "Rate",
        ["rate", "tasa", "tipo"],
        "Financiera",
        "Calcula la tasa periodica (tipo 0=anual) resolviendo TVM con exp/log; 4-5 args.",
        ReadOnly,
        Low,
        true,
        "Rate",
        [
            signature!("Rate[nper, pmt, pv, fv]"; "nper": Number required, "pmt": Number required, "pv": Number required, "fv": Number required),
            signature!("Rate[nper, pmt, pv, fv, tipo]"; "nper": Number required, "pmt": Number required, "pv": Number required, "fv": Number required, "tipo": Integer optional)
        ]
    ),
    command!(
        "finance.nper",
        "Nper",
        ["nper", "n_per", "periodos", "plazo"],
        "Financiera",
        "Calcula numero de periodos via TVM con exp/log; usa log((pmt*(1+r*tipo)-fv*r)/(pmt*(1+r*tipo)+pv*r))/log(1+r).",
        ReadOnly,
        Low,
        true,
        "Nper",
        [
            signature!("Nper[rate, pmt, pv, fv]"; "rate": Number required, "pmt": Number required, "pv": Number required, "fv": Number required),
            signature!("Nper[rate, pmt, pv, fv, tipo]"; "rate": Number required, "pmt": Number required, "pv": Number required, "fv": Number required, "tipo": Integer optional)
        ]
    ),
    command!(
        "finance.pmt",
        "Pmt",
        ["pmt", "pago", "cuota"],
        "Financiera",
        "Calcula el pago periodico TVM; 4-5 args con tipo 0/1.",
        ReadOnly,
        Low,
        true,
        "Pmt",
        [
            signature!("Pmt[rate, nper, pv, fv]"; "rate": Number required, "nper": Number required, "pv": Number required, "fv": Number required),
            signature!("Pmt[rate, nper, pv, fv, tipo]"; "rate": Number required, "nper": Number required, "pv": Number required, "fv": Number required, "tipo": Integer optional)
        ]
    ),
    command!(
        "finance.pv",
        "PV",
        ["pv", "va", "valoractual", "presentvalue"],
        "Financiera",
        "Calcula valor presente TVM; usa exp/log para (1+rate)^nper.",
        ReadOnly,
        Low,
        true,
        "PV",
        [
            signature!("PV[rate, nper, pmt, fv]"; "rate": Number required, "nper": Number required, "pmt": Number required, "fv": Number required),
            signature!("PV[rate, nper, pmt, fv, tipo]"; "rate": Number required, "nper": Number required, "pmt": Number required, "fv": Number required, "tipo": Integer optional)
        ]
    ),
    command!(
        "finance.fv",
        "FV",
        ["fv", "vf", "valorfuturo", "futurevalue"],
        "Financiera",
        "Calcula valor futuro TVM; usa exp/log para (1+rate)^nper.",
        ReadOnly,
        Low,
        true,
        "FV",
        [
            signature!("FV[rate, nper, pmt, pv]"; "rate": Number required, "nper": Number required, "pmt": Number required, "pv": Number required),
            signature!("FV[rate, nper, pmt, pv, tipo]"; "rate": Number required, "nper": Number required, "pmt": Number required, "pv": Number required, "tipo": Integer optional)
        ]
    ),
    // Sistemas dinamicos, fractales y dimensiones superiores
    command!(
        "dynamics.lorenz",
        "Lorenz",
        [],
        "Atractores",
        "Crea el atractor de Lorenz.",
        CreatesObject,
        High,
        true,
        "Lorenz",
        [
            signature!("Lorenz[sigma, rho, beta]"; "sigma": Number optional, "rho": Number optional, "beta": Number optional)
        ]
    ),
    command!(
        "dynamics.rossler",
        "Rossler",
        ["rossler", "rossler"],
        "Atractores",
        "Crea el atractor de Rossler.",
        CreatesObject,
        High,
        true,
        "Rossler",
        [
            signature!("Rossler[a, b, c]"; "a": Number optional, "b": Number optional, "c": Number optional)
        ]
    ),
    command!(
        "dynamics.thomas",
        "Thomas",
        ["butterfly"],
        "Atractores",
        "Crea el atractor de Thomas.",
        CreatesObject,
        High,
        true,
        "Thomas (Butterfly)",
        [signature!("Thomas[pasos]"; "pasos": Integer optional)]
    ),
    command!(
        "dynamics.aizawa",
        "Aizawa",
        [],
        "Atractores",
        "Crea el atractor de Aizawa.",
        CreatesObject,
        High,
        true,
        "Aizawa",
        [signature!("Aizawa[a, b, c, d, e, f]"; "a": Number optional, "b": Number optional, "c": Number optional, "d": Number optional, "e": Number optional, "f": Number optional)]
    ),
    command!(
        "dynamics.chen",
        "Chen",
        [],
        "Atractores",
        "Crea el atractor de Chen.",
        CreatesObject,
        High,
        true,
        "Chen",
        [signature!("Chen[a, b, c]"; "a": Number optional, "b": Number optional, "c": Number optional)]
    ),
    command!(
        "dynamics.halvorsen",
        "Halvorsen",
        [],
        "Atractores",
        "Crea el atractor de Halvorsen.",
        CreatesObject,
        High,
        true,
        "Halvorsen",
        [signature!("Halvorsen[a, p2, p3, p4]"; "a": Number optional, "p2": Number optional, "p3": Number optional, "p4": Number optional)]
    ),
    command!(
        "dynamics.dadras",
        "Dadras",
        [],
        "Atractores",
        "Crea el atractor de Dadras.",
        CreatesObject,
        High,
        true,
        "Dadras",
        [signature!("Dadras[p, q, r, s, e]"; "p": Number optional, "q": Number optional, "r": Number optional, "s": Number optional, "e": Number optional)]
    ),
    command!(
        "dynamics.chua",
        "Chua",
        [],
        "Atractores",
        "Crea el atractor de Chua.",
        CreatesObject,
        High,
        true,
        "Chua",
        [signature!("Chua[alpha, beta, m0, m1]"; "alpha": Number optional, "beta": Number optional, "m0": Number optional, "m1": Number optional)]
    ),
    command!(
        "fractal.mandelbrot",
        "Mandelbrot",
        [],
        "Fractales",
        "Crea el fractal de Mandelbrot.",
        CreatesObject,
        High,
        true,
        "Mandelbrot",
        [signature!("Mandelbrot[max_iter]"; "max_iter": Integer optional)]
    ),
    command!(
        "fractal.julia",
        "Julia",
        [],
        "Fractales",
        "Crea un fractal de Julia.",
        CreatesObject,
        High,
        true,
        "Julia",
        [
            signature!("Julia[cr, ci, max_iter]"; "cr": Number required, "ci": Number required, "max_iter": Integer optional)
        ]
    ),
    command!(
        "fractal.burning-ship",
        "BurningShip",
        ["burning_ship"],
        "Fractales",
        "Crea el fractal Burning Ship.",
        CreatesObject,
        High,
        true,
        "BurningShip",
        [signature!("BurningShip[]";)]
    ),
    command!(
        "geometry.hypercube",
        "Hypercube",
        ["tesseract"],
        "4D",
        "Crea una proyeccion de hipercubo.",
        CreatesObject,
        High,
        true,
        "Hypercube",
        [
            signature!("Hypercube[a1, a2, a3]"; "a1": Number optional, "a2": Number optional, "a3": Number optional)
        ]
    ),
    command!(
        "geometry.hypersphere",
        "Hypersphere",
        [],
        "4D",
        "Crea una proyeccion de hiperesfera.",
        CreatesObject,
        High,
        true,
        "Hypersphere",
        [signature!("Hypersphere[]";)]
    ),
    command!(
        "geometry.pentachoron-4d",
        "Pentachoron4D",
        ["fivecell4d", "5cell4d"],
        "4D",
        "Crea el 5-celda regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "Pentachoron4D",
        [
            signature!("Pentachoron4D[]";),
            signature!("Pentachoron4D[scale]"; "scale": Number required),
            signature!("Pentachoron4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.tesseract-4d",
        "Tesseract4D",
        ["hypercube4d"],
        "4D",
        "Crea el hipercubo regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "Tesseract4D",
        [
            signature!("Tesseract4D[]";),
            signature!("Tesseract4D[scale]"; "scale": Number required),
            signature!("Tesseract4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.sixteen-cell-4d",
        "SixteenCell4D",
        ["16cell4d"],
        "4D",
        "Crea el 16-celda regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "SixteenCell4D",
        [
            signature!("SixteenCell4D[]";),
            signature!("SixteenCell4D[scale]"; "scale": Number required),
            signature!("SixteenCell4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.twenty-four-cell-4d",
        "TwentyFourCell4D",
        ["24cell4d"],
        "4D",
        "Crea el 24-celda regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "TwentyFourCell4D",
        [
            signature!("TwentyFourCell4D[]";),
            signature!("TwentyFourCell4D[scale]"; "scale": Number required),
            signature!("TwentyFourCell4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.one-twenty-cell-4d",
        "OneTwentyCell4D",
        ["120cell4d"],
        "4D",
        "Crea el 120-celda regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "OneTwentyCell4D",
        [
            signature!("OneTwentyCell4D[]";),
            signature!("OneTwentyCell4D[scale]"; "scale": Number required),
            signature!("OneTwentyCell4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.six-hundred-cell-4d",
        "SixHundredCell4D",
        ["600cell4d"],
        "4D",
        "Crea el 600-celda regular 4D con escala y seis rotaciones opcionales.",
        CreatesObject,
        High,
        true,
        "SixHundredCell4D",
        [
            signature!("SixHundredCell4D[]";),
            signature!("SixHundredCell4D[scale]"; "scale": Number required),
            signature!("SixHundredCell4D[scale, {xy, xz, xw, yz, yw, zw}]"; "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.simplex-nd",
        "SimplexND",
        ["simplex_nd"],
        "4D",
        "Crea un simplex regular en R^n para n entre 3 y 10.",
        CreatesObject,
        High,
        true,
        "SimplexND",
        [
            signature!("SimplexND[n]"; "n": Integer required),
            signature!("SimplexND[n, scale]"; "n": Integer required, "scale": Number required),
            signature!("SimplexND[n, scale, {lexicographic-plane angles}]"; "n": Integer required, "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.hypercube-nd",
        "HypercubeND",
        ["hypercube_nd"],
        "4D",
        "Crea un hipercubo regular en R^n para n entre 3 y 10.",
        CreatesObject,
        High,
        true,
        "HypercubeND",
        [
            signature!("HypercubeND[n]"; "n": Integer required),
            signature!("HypercubeND[n, scale]"; "n": Integer required, "scale": Number required),
            signature!("HypercubeND[n, scale, {lexicographic-plane angles}]"; "n": Integer required, "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.cross-polytope-nd",
        "CrossPolytopeND",
        ["cross_polytope_nd"],
        "4D",
        "Crea un politopo cruzado regular en R^n para n entre 3 y 10.",
        CreatesObject,
        High,
        true,
        "CrossPolytopeND",
        [
            signature!("CrossPolytopeND[n]"; "n": Integer required),
            signature!("CrossPolytopeND[n, scale]"; "n": Integer required, "scale": Number required),
            signature!("CrossPolytopeND[n, scale, {lexicographic-plane angles}]"; "n": Integer required, "scale": Number required, "rotaciones": ParameterList required)
        ]
    ),
    command!(
        "geometry.point-3d",
        "Point3D",
        [],
        "3D",
        "Crea un punto 3D.",
        CreatesObject,
        Low,
        false,
        "Point3D",
        [signature!("Point3D[x, y, z]"; "x": Number required, "y": Number required, "z": Number required)]
    ),
    command!(
        "geometry.segment-3d",
        "Segment3D",
        [],
        "3D",
        "Crea un segmento 3D.",
        CreatesObject,
        Low,
        false,
        "Segment3D",
        [signature!("Segment3D[x1, y1, z1, x2, y2, z2]"; "x1": Number required, "y1": Number required, "z1": Number required, "x2": Number required, "y2": Number required, "z2": Number required)]
    ),
    command!(
        "geometry.line-3d",
        "Line3D",
        ["line3", "recta3d", "recta"],
        "3D",
        "Crea una recta 3D por punto y direccion o por dos puntos.",
        CreatesObject,
        Medium,
        false,
        "Line3D",
        [
            signature!("Line3D[x0, y0, z0, dx, dy, dz]"; "x0": Number required, "y0": Number required, "z0": Number required, "dx": Number required, "dy": Number required, "dz": Number required),
            signature!("Line3D[p1, p2]"; "p1": ObjectLabel required, "p2": ObjectLabel required)
        ]
    ),
    command!(
        "geometry.plane-3d",
        "Plane3D",
        ["plane", "plano", "plano3d"],
        "3D",
        "Crea un plano 3D por ecuacion o por tres puntos.",
        CreatesObject,
        Medium,
        false,
        "Plane3D",
        [
            signature!("Plane3D[a, b, c, d]"; "a": Number required, "b": Number required, "c": Number required, "d": Number required),
            signature!("Plane3D[p1, p2, p3]"; "p1": ObjectLabel required, "p2": ObjectLabel required, "p3": ObjectLabel required)
        ]
    ),
    command!(
        "geometry.sphere-3d",
        "Sphere",
        [],
        "3D",
        "Crea una esfera 3D.",
        CreatesObject,
        Medium,
        false,
        "Sphere",
        [signature!("Sphere[x, y, z, radius]"; "x": Number required, "y": Number required, "z": Number required, "radius": Number required)]
    ),
    command!(
        "geometry.cube-3d",
        "Cube",
        [],
        "3D",
        "Crea un cubo 3D.",
        CreatesObject,
        Medium,
        false,
        "Cube",
        [signature!("Cube[x, y, z, size]"; "x": Number required, "y": Number required, "z": Number required, "size": Number required)]
    ),
    command!(
        "geometry.tetrahedron-3d",
        "Tetrahedron",
        [],
        "3D",
        "Crea un tetraedro regular 3D sólido.",
        CreatesObject,
        Medium,
        false,
        "Tetrahedron",
        [signature!("Tetrahedron[x, y, z, edge]"; "x": Number required, "y": Number required, "z": Number required, "edge": Number required)]
    ),
    command!(
        "geometry.cylinder-3d",
        "Cylinder",
        [],
        "3D",
        "Crea un cilindro 3D vertical.",
        CreatesObject,
        Medium,
        false,
        "Cylinder",
        [signature!("Cylinder[x, y, z, radius, height]"; "x": Number required, "y": Number required, "z": Number required, "radius": Number required, "height": Number required)]
    ),
    command!(
        "geometry.cone-3d",
        "Cone",
        [],
        "3D",
        "Crea un cono 3D vertical.",
        CreatesObject,
        Medium,
        false,
        "Cone",
        [signature!("Cone[x, y, z, radius, height]"; "x": Number required, "y": Number required, "z": Number required, "radius": Number required, "height": Number required)]
    ),
    command!(
        "geometry.torus-3d",
        "Torus",
        [],
        "3D",
        "Crea un toro 3D.",
        CreatesObject,
        High,
        false,
        "Torus",
        [signature!("Torus[x, y, z, major_radius, minor_radius]"; "x": Number required, "y": Number required, "z": Number required, "major_radius": Number required, "minor_radius": Number required)]
    ),
    command!(
        "geometry.moebius-3d",
        "Moebius",
        ["mobius"],
        "3D",
        "Crea una banda de Moebius 3D.",
        CreatesObject,
        High,
        false,
        "Moebius",
        [signature!("Moebius[radius, width]"; "radius": Number required, "width": Number required)]
    ),
    command!(
        "geometry.curve-3d",
        "Curve3D",
        [],
        "3D",
        "Crea una curva parametrica 3D.",
        CreatesObject,
        High,
        true,
        "Curve3D",
        [
            signature!("Curve3D[(x(t), y(t), z(t)), t, tmin, tmax]"; "curva": ParameterList required, "t": Variable required, "tmin": Number required, "tmax": Number required),
            signature!("Curve3D[(x(t), y(t), z(t)), tmin, tmax]"; "curva": ParameterList required, "tmin": Number required, "tmax": Number required)
        ]
    ),
    command!(
        "geometry.surface-3d",
        "Surface3D",
        [],
        "3D",
        "Crea una superficie 3D parametrica o explicita.",
        CreatesObject,
        High,
        true,
        "Surface3D",
        [
            signature!("Surface3D[f(x, y), xmin, xmax, ymin, ymax]"; "f(x, y)": Expression required, "xmin": Number required, "xmax": Number required, "ymin": Number required, "ymax": Number required),
            signature!("Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]"; "superficie": ParameterList required, "umin": Number required, "umax": Number required, "vmin": Number required, "vmax": Number required),
            signature!("Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]"; "x(u,v)": Expression required, "y(u,v)": Expression required, "z(u,v)": Expression required, "umin": Number required, "umax": Number required, "vmin": Number required, "vmax": Number required)
        ]
    ),
    command!(
        "geometry.complex-surface",
        "ComplexSurface",
        ["complexsurface", "complex_surface", "csurface"],
        "3D",
        "Grafica el modulo de una funcion compleja como superficie 3D.",
        CreatesObject,
        High,
        false,
        "ComplexSurface",
        [signature!("ComplexSurface[expr, xmin, xmax, ymin, ymax, resolution]"; "expr": ComplexExpression required, "xmin": Number optional, "xmax": Number optional, "ymin": Number optional, "ymax": Number optional, "resolution": Integer optional)]
    ),
    command!(
        "geometry.extrude",
        "Extrude",
        [],
        "3D",
        "Extruye un poligono a un solido.",
        CreatesObject,
        High,
        true,
        "Extrude",
        [
            signature!("Extrude[polygon_label, height]"; "polygon_label": ObjectLabel required, "height": Number required)
        ]
    ),
    command!(
        "geometry.vector-field-3d",
        "VectorField3D",
        ["vectorfield"],
        "3D",
        "Crea un campo vectorial 3D.",
        CreatesObject,
        High,
        true,
        "VectorField3D",
        [
            signature!("VectorField3D[u, v, w]"; "u": Expression required, "v": Expression required, "w": Expression required)
        ]
    ),
    command!(
        "geometry.prism-3d",
        "Prism",
        ["prism", "prisma"],
        "3D",
        "Crea un prisma extruyendo un polígono base por un vector (altura en Z o dx,dy,dz).",
        CreatesObject,
        Medium,
        true,
        "Prism",
        [
            signature!("Prism[poligono, altura]"; "poligono": ObjectLabel required, "altura": Number required),
            signature!("Prism[poligono, dx, dy, dz]"; "poligono": ObjectLabel required, "dx": Number required, "dy": Number required, "dz": Number required)
        ]
    ),
    command!(
        "geometry.net-3d",
        "Net",
        ["net", "desarrollo", "desplegado", "unwrap"],
        "3D",
        "Genera el desarrollo 2D de un poliedro (stub: informa disponibilidad).",
        ReadOnly,
        Low,
        true,
        "Net",
        [
            signature!("Net[poliedro]"; "poliedro": ObjectLabel required),
            signature!("Net[poliedro, escala]"; "poliedro": ObjectLabel required, "escala": Number optional)
        ]
    ),
    command!(
        "geometry.quadric-3d",
        "Quadric",
        ["quadric", "cuadrica", "cuádrica"],
        "3D",
        "Crea una cuádrica general a*x²+b*y²+c*z²+d*xy+e*yz+f*zx+g*x+h*y+i*z+j=0.",
        CreatesObject,
        Medium,
        true,
        "Quadric",
        [
            signature!("Quadric[a, b, c, d, e, f, g, h, i, j]"; "a": Number required, "b": Number required, "c": Number required, "d": Number required, "e": Number required, "f": Number required, "g": Number required, "h": Number required, "i": Number required, "j": Number required)
        ]
    ),
    command!(
        "geometry.intersection-3d",
        "Intersection3D",
        ["intersection3d", "intersect3d", "interseccion3d", "intersección3d"],
        "3D",
        "Calcula intersecciones 3D: Plano-Plano, Recta-Plano, Recta-Recta, Plano-Esfera (círculo) o Plano-Poliedro (stub).",
        CreatesObject,
        Medium,
        true,
        "Intersection3D",
        [
            signature!("Intersection3D[a, b]"; "a": ObjectLabel required, "b": ObjectLabel required),
            signature!("Intersection3D[a, b, c]"; "a": ObjectLabel required, "b": ObjectLabel required, "c": ObjectLabel required)
        ]
    ),
    command!(
        "geometry.arc",
        "Arc",
        ["arc", "arco"],
        "Crear",
        "Crea un arco por centro/radio/ángulos o por tres puntos.",
        CreatesObject,
        Low,
        true,
        "Arc",
        [
            signature!("Arc[centro, radio, inicio, fin]"; "centro": Point required, "radio": Number required, "inicio": Number required, "fin": Number required),
            signature!("Arc[P1, P2, P3]"; "P1": Point required, "P2": Point required, "P3": Point required)
        ]
    ),
    command!(
        "geometry.sector",
        "Sector",
        ["sector"],
        "Crear",
        "Crea un sector circular con relleno.",
        CreatesObject,
        Low,
        true,
        "Sector",
        [
            signature!("Sector[centro, radio, angulo]"; "centro": Point required, "radio": Number required, "angulo": Number required),
            signature!("Sector[centro, radio, inicio, fin]"; "centro": Point required, "radio": Number required, "inicio": Number required, "fin": Number required)
        ]
    ),
    command!(
        "geometry.semicircle",
        "Semicircle",
        ["semicircle", "semicirculo"],
        "Crear",
        "Crea un semicírculo por centro/radio o por tres puntos.",
        CreatesObject,
        Low,
        true,
        "Semicircle",
        [
            signature!("Semicircle[centro, radio]"; "centro": Point required, "radio": Number required),
            signature!("Semicircle[P1, P2, P3]"; "P1": Point required, "P2": Point required, "P3": Point required)
        ]
    ),
    command!(
        "geometry.bezier-curve",
        "BezierCurve",
        ["bezier", "bezier_curve"],
        "Crear",
        "Crea una curva de Bézier por 2..64 puntos de control.",
        CreatesObject,
        Medium,
        true,
        "BezierCurve",
        [
            signature!("BezierCurve[P1, P2, ...]"; "P1": Point required, "P2": Point required)
        ]
    ),
    command!(
        "geometry.spline",
        "Spline",
        ["spline"],
        "Crear",
        "Crea una spline Catmull-Rom por 2..64 puntos.",
        CreatesObject,
        Medium,
        true,
        "Spline",
        [
            signature!("Spline[P1, P2, ...]"; "P1": Point required, "P2": Point required)
        ]
    ),
    command!(
        "geometry.compasses",
        "Compasses",
        ["compass", "compas"],
        "Construir",
        "Traza un círculo con compás: centro y punto o radio.",
        CreatesObject,
        Low,
        true,
        "Compasses",
        [
            signature!("Compasses[centro, punto]"; "centro": Point required, "punto": Point required),
            signature!("Compasses[centro, radio]"; "centro": Point required, "radio": Number required)
        ]
    ),
    command!(
        "geometry.incircle",
        "Incircle",
        ["incircle", "incirculo"],
        "Construir",
        "Crea el incírculo de un triángulo ABC.",
        CreatesObject,
        Medium,
        true,
        "Incircle",
        [
            signature!("Incircle[A, B, C]"; "A": Point required, "B": Point required, "C": Point required)
        ]
    ),
    command!(
        "geometry.circumcircle",
        "Circumcircle",
        ["circumcircle", "circuncirculo"],
        "Construir",
        "Crea el circuncírculo de un triángulo ABC.",
        CreatesObject,
        Medium,
        true,
        "Circumcircle",
        [
            signature!("Circumcircle[A, B, C]"; "A": Point required, "B": Point required, "C": Point required)
        ]
    ),
    command!(
        "discrete.convex-hull",
        "ConvexHull",
        ["convex_hull", "convexhull", "envolventeconvexa", "envolvente"],
        "Discreta",
        "Calcula la envolvente convexa de un conjunto de puntos con monotone chain; respeta MAX_POLYGON_VERTICES 8192 y MAX_DISCRETE_COUNT 10000.",
        CreatesObject,
        Medium,
        true,
        "ConvexHull",
        [
            signature!("ConvexHull[puntos]"; "puntos": Data required),
            signature!("ConvexHull[{p1, p2, ...}]"; "p1": Point required)
        ]
    ),
    command!(
        "discrete.delaunay",
        "DelaunayTriangulation",
        ["delaunay", "delaunaytriangulation", "triangulaciondelaunay"],
        "Discreta",
        "Triangulación Delaunay aproximada por abanico (fan) desde el primer punto; stub que no falla y respeta límites discretos.",
        CreatesObject,
        Medium,
        true,
        "DelaunayTriangulation",
        [signature!("DelaunayTriangulation[puntos]"; "puntos": Data required)]
    ),
    command!(
        "discrete.voronoi",
        "Voronoi",
        ["voronoi", "cellsvoronoi", "diagramaVoronoi"],
        "Discreta",
        "Diagrama de Voronoi aproximado: genera círculos stub en cada sitio cuando no hay motor exacto disponible.",
        CreatesObject,
        Medium,
        true,
        "Voronoi",
        [signature!("Voronoi[puntos]"; "puntos": Data required)]
    ),
    command!(
        "discrete.mst",
        "MinimumSpanningTree",
        ["mst", "minimumspanningtree", "arbolminimo", "kruskal"],
        "Discreta",
        "Árbol de expansión mínima por Prim euclídeo O(n²); crea segmentos entre puntos.",
        CreatesObject,
        Medium,
        true,
        "MinimumSpanningTree",
        [signature!("MinimumSpanningTree[puntos]"; "puntos": Data required)]
    ),
    command!(
        "discrete.tsp",
        "TravelingSalesman",
        ["tsp", "travelingsalesman", "viajante", "travellingsalesman"],
        "Discreta",
        "Tour del viajante aproximado por vecino más cercano (greedy) empezando en el primer punto.",
        CreatesObject,
        Medium,
        true,
        "TravelingSalesman",
        [signature!("TravelingSalesman[puntos]"; "puntos": Data required)]
    ),
    command!(
        "discrete.shortest-distance",
        "ShortestDistance",
        ["shortestdistance", "distanciaminima", "closestdistance", "distanciamínima"],
        "Discreta",
        "Distancia euclídea mínima entre un punto y un objeto (punto/segmento/círculo/polígono). Valida finitud y límites.",
        ReadOnly,
        Low,
        true,
        "ShortestDistance",
        [
            signature!("ShortestDistance[punto, objeto]"; "punto": Point required, "objeto": Object required)
        ]
    ),
    // ---- Lista funcional (P2.5) — operaciones puras sin tocar Document ----
    command!(
        "list.sequence",
        "Sequence",
        ["seq", "secuencia", "rango", "table"],
        "Lista",
        "Genera lista {expr(var=start)...expr(var=end)} evaluando expr con var entera; valida MAX_ARRAY_LENGTH 200k y MAX_DISCRETE_COUNT 10k.",
        ReadOnly,
        Low,
        true,
        "Sequence",
        [signature!("Sequence[expr, var, start, end]"; "expr": Expression required, "var": Variable required, "start": Number required, "end": Number required)]
    ),
    command!(
        "list.zip",
        "Zip",
        ["zip", "emparejar", "cremallera"],
        "Lista",
        "Empareja dos listas en lista de pares {{a1,b1},…}; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Zip",
        [signature!("Zip[list1, list2]"; "list1": Data required, "list2": Data required)]
    ),
    command!(
        "list.flatten",
        "Flatten",
        ["flatten", "aplanar", "aplanado"],
        "Lista",
        "Aplana un nivel de anidamiento {{1,2},{3,4}}→{1,2,3,4}; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Flatten",
        [signature!("Flatten[list]"; "list": Data required)]
    ),
    command!(
        "list.sort",
        "Sort",
        ["sort", "ordenar", "orden"],
        "Lista",
        "Ordena ascendentemente una lista plana numérica; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Sort",
        [signature!("Sort[list]"; "list": Data required)]
    ),
    command!(
        "list.reverse",
        "Reverse",
        ["reverse", "invertir", "reversa"],
        "Lista",
        "Invierte el orden de una lista; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Reverse",
        [signature!("Reverse[list]"; "list": Data required)]
    ),
    command!(
        "list.join",
        "Join",
        ["join", "unir", "concat", "concatenar"],
        "Lista",
        "Concatena dos listas; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Join",
        [signature!("Join[list1, list2]"; "list1": Data required, "list2": Data required)]
    ),
    command!(
        "list.append",
        "Append",
        ["append", "anexar", "agregar"],
        "Lista",
        "Añade un elemento al final de la lista; valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Append",
        [signature!("Append[list, elem]"; "list": Data required, "elem": Number required)]
    ),
    command!(
        "list.first",
        "First",
        ["first", "primero", "head"],
        "Lista",
        "Primer elemento de la lista.",
        ReadOnly,
        Low,
        true,
        "First",
        [signature!("First[list]"; "list": Data required)]
    ),
    command!(
        "list.last",
        "Last",
        ["last", "ultimo", "último", "tail"],
        "Lista",
        "Último elemento de la lista.",
        ReadOnly,
        Low,
        true,
        "Last",
        [signature!("Last[list]"; "list": Data required)]
    ),
    command!(
        "list.take",
        "Take",
        ["take", "tomar", "coger"],
        "Lista",
        "Primeros n elementos de la lista; valida 0≤n≤len y MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "Take",
        [signature!("Take[list, n]"; "list": Data required, "n": Integer required)]
    ),
    command!(
        "list.keep-if",
        "KeepIf",
        ["keepif", "keep_if", "filtrar", "selectif", "filter"],
        "Lista",
        "Filtra con predicado simple sobre x (ej x>2); valida MAX_ARRAY_LENGTH.",
        ReadOnly,
        Low,
        true,
        "KeepIf",
        [signature!("KeepIf[list, predicado]"; "list": Data required, "predicado": Expression required)]
    ),
    command!(
        "list.count-if",
        "CountIf",
        ["countif", "count_if", "contarsi", "contar_si"],
        "Lista",
        "Cuenta elementos que cumplen predicado simple sobre x; valida longitud.",
        ReadOnly,
        Low,
        true,
        "CountIf",
        [signature!("CountIf[list, predicado]"; "list": Data required, "predicado": Expression required)]
    ),
];

/// Returns every registered stable text command.
pub fn all() -> &'static [CommandSpec] {
    COMMANDS
}

/// Returns commands that the command palette should display.
pub fn palette_commands() -> impl Iterator<Item = &'static CommandSpec> {
    COMMANDS.iter().filter(|spec| spec.palette_visible)
}

/// Finds a command by its stable identifier.
pub fn by_id(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

/// Resolves a canonical command name or one of its aliases.
pub fn resolve(name: &str) -> Option<&'static CommandSpec> {
    let name = name.trim();
    COMMANDS.iter().find(|spec| {
        spec.canonical.eq_ignore_ascii_case(name)
            || spec
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(name))
    })
}

/// Returns the existing dispatcher key for a registered command.
pub fn canonicalize(name: &str) -> Option<&'static str> {
    resolve(name).map(|spec| spec.dispatch_key)
}

/// Renders the deterministic Markdown command reference.
pub fn render_markdown() -> String {
    let mut markdown = String::from(
        "# Referencia de Comandos de Grafito\n\n<!-- Generated from crates/grafito-command/src/command_registry.rs; do not edit manually. -->\n\nEsta referencia se genera desde el registro de comandos estable. El parser y sus fallbacks siguen en `commands.rs`; el registro documenta sus metadatos, no reemplaza el despacho.\n\n",
    );
    let mut category = "";

    for spec in COMMANDS {
        if spec.category != category {
            category = spec.category;
            markdown.push_str("## ");
            markdown.push_str(category);
            markdown.push_str("\n\n");
        }

        let signature = spec.signatures[0];
        markdown.push_str("- `");
        markdown.push_str(signature.syntax);
        markdown.push_str("`: ");
        markdown.push_str(spec.help);
        markdown.push_str(" Mutacion: ");
        markdown.push_str(spec.mutation.label());
        markdown.push_str(". Riesgo: ");
        markdown.push_str(spec.risk.label());
        markdown.push('.');
        if spec.signatures.len() > 1 {
            markdown.push_str(" Formas alternativas: `");
            markdown.push_str(
                &spec.signatures[1..]
                    .iter()
                    .map(|signature| signature.syntax)
                    .collect::<Vec<_>>()
                    .join("`, `"),
            );
            markdown.push_str("`.");
        }
        if !spec.aliases.is_empty() {
            markdown.push_str(" Alias: `");
            markdown.push_str(&spec.aliases.join("`, `"));
            markdown.push_str("`.");
        }
        markdown.push('\n');
    }

    markdown
}
