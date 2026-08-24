//! Construcción de contexto inmutable para propuestas del asistente.

use crate::command_registry::{self, ArgumentKind, MutationClass};
use grafito_assistant_types::{AssistantFocus, DocumentContextObject, ImmutableDocumentContext};
use grafito_core::{Document, GeoObject, ObjectId};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Vista que debe abrirse después de confirmar una propuesta gráfica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantGraphView {
    TwoD,
    ThreeD,
}

/// Ruta de render que comprueba el preflight antes de confirmar un comando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantGraphProof {
    StaticTwoD,
    WorldMeshThreeD,
    CpuOverlayThreeD,
}

/// Contrato de un comando gráfico que puede producir el asistente sin depender
/// de etiquetas, archivos ni estado implícito del documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssistantGraphCapability {
    pub canonical: &'static str,
    pub syntax: &'static str,
    pub help: &'static str,
    pub keywords: &'static [&'static str],
    pub min_args: usize,
    pub max_args: usize,
    pub view: AssistantGraphView,
    pub proof: AssistantGraphProof,
}

const ASSISTANT_GRAPH_CAPABILITIES: &[AssistantGraphCapability] = &[
    AssistantGraphCapability {
        canonical: "Function",
        syntax: "Function[expr]",
        help: "Grafica una funcion real y=f(x), incluida una suma parcial de Fourier finita.",
        keywords: &[
            "funcion",
            "grafica",
            "grafico",
            "curva",
            "parabola",
            "fourier",
            "furier",
            "armonica",
            "armónica",
        ],
        min_args: 1,
        max_args: 1,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Piecewise",
        syntax: "Piecewise[condicion1, valor1, valor_por_defecto]",
        help: "Grafica una funcion definida por partes.",
        keywords: &["tramos", "partes", "piecewise"],
        min_args: 3,
        max_args: 64,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "ParametricCurve2D",
        syntax: "ParametricCurve2D[x(t), y(t), tmin, tmax]",
        help: "Grafica una curva parametrica 2D.",
        keywords: &["parametrica", "parametrico"],
        min_args: 4,
        max_args: 4,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "PolarCurve",
        syntax: "PolarCurve[r(t), tmin, tmax]",
        help: "Grafica una curva polar.",
        keywords: &["polar", "rosa", "espiral"],
        min_args: 3,
        max_args: 3,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "ImplicitCurve",
        syntax: "ImplicitCurve[f(x, y) = c]",
        help: "Grafica una curva o region implicita, por ejemplo un corazon.",
        keywords: &[
            "implicita",
            "implícita",
            "implicit",
            "nivel",
            "corazon",
            "corazón",
            "region",
            "región",
            "cardioide",
        ],
        min_args: 1,
        max_args: 3,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Contour",
        syntax: "Contour[f(x, y), xmin, xmax, ymin, ymax, nivel]",
        help: "Grafica curvas de nivel 2D.",
        keywords: &["contorno", "nivel", "contour"],
        min_args: 6,
        max_args: 13,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "VectorField2D",
        syntax: "VectorField2D[u(x, y), v(x, y)]",
        help: "Grafica un campo vectorial 2D.",
        keywords: &["campo", "vectorial", "vectores"],
        min_args: 2,
        max_args: 2,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "PhasePortrait",
        syntax: "PhasePortrait[dxdt, dydt]",
        help: "Grafica un retrato de fase 2D.",
        keywords: &["fase", "edo", "dinamico", "dinamica"],
        min_args: 2,
        max_args: 2,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "DomainColoring",
        syntax: "DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]",
        help: "Visualiza fase y modulo de una funcion compleja. La resolución opcional debe ser un entero literal entre 16 y 300; por defecto es 200.",
        keywords: &["compleja", "complejo", "dominio", "fase"],
        min_args: 1,
        max_args: 6,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "ComplexGrid",
        syntax: "ComplexGrid[expr, xmin, xmax, ymin, ymax, density]",
        help: "Grafica una rejilla transformada por una funcion compleja.",
        keywords: &["compleja", "complejo", "rejilla", "mapeo"],
        min_args: 1,
        max_args: 6,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "HeatMap",
        syntax: "HeatMap[f(x, y), xmin, xmax, ymin, ymax, resolution]",
        help: "Grafica un mapa de calor 2D.",
        keywords: &["calor", "densidad", "heatmap"],
        min_args: 1,
        max_args: 6,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Quadrants",
        syntax: "Quadrants[xmin, xmax, ymin, ymax]",
        help: "Muestra los cuadrantes del plano complejo.",
        keywords: &["cuadrantes", "complejo"],
        min_args: 0,
        max_args: 4,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Histogram",
        syntax: "Histogram[{datos}, bins]",
        help: "Grafica un histograma.",
        keywords: &["histograma", "datos", "estadistica"],
        min_args: 1,
        max_args: 2,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "ScatterPlot",
        syntax: "ScatterPlot[{xs}, {ys}]",
        help: "Grafica una nube de puntos.",
        keywords: &["dispersion", "scatter", "datos"],
        min_args: 2,
        max_args: 2,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "BoxPlot",
        syntax: "BoxPlot[{datos}]",
        help: "Grafica un diagrama de caja.",
        keywords: &["caja", "boxplot", "estadistica"],
        min_args: 1,
        max_args: 1,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "LinearRegression",
        syntax: "LinearRegression[{xs}, {ys}]",
        help: "Grafica una recta de regresion.",
        keywords: &["regresion", "ajuste", "datos"],
        min_args: 2,
        max_args: 2,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Mandelbrot",
        syntax: "Mandelbrot[max_iter]",
        help: "Grafica el fractal de Mandelbrot.",
        keywords: &["mandelbrot", "fractal"],
        min_args: 0,
        max_args: 1,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Julia",
        syntax: "Julia[cr, ci, max_iter]",
        help: "Grafica un conjunto de Julia.",
        keywords: &["julia", "fractal"],
        min_args: 2,
        max_args: 3,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "BurningShip",
        syntax: "BurningShip[]",
        help: "Grafica el fractal Burning Ship.",
        keywords: &["burning", "ship", "fractal"],
        min_args: 0,
        max_args: 0,
        view: AssistantGraphView::TwoD,
        proof: AssistantGraphProof::StaticTwoD,
    },
    AssistantGraphCapability {
        canonical: "Point3D",
        syntax: "Point3D[x, y, z]",
        help: "Crea un punto 3D.",
        keywords: &["punto", "3d"],
        min_args: 3,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::CpuOverlayThreeD,
    },
    AssistantGraphCapability {
        canonical: "Segment3D",
        syntax: "Segment3D[x1, y1, z1, x2, y2, z2]",
        help: "Crea un segmento 3D.",
        keywords: &["segmento", "arista", "3d"],
        min_args: 6,
        max_args: 6,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Line3D",
        syntax: "Line3D[x0, y0, z0, dx, dy, dz]",
        help: "Crea una recta 3D por punto y direccion.",
        keywords: &["recta", "linea", "3d"],
        min_args: 6,
        max_args: 6,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Plane3D",
        syntax: "Plane3D[a, b, c, d]",
        help: "Crea un plano 3D.",
        keywords: &["plano", "3d"],
        min_args: 4,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Sphere",
        syntax: "Sphere[x, y, z, radius]",
        help: "Crea una esfera 3D.",
        keywords: &["esfera", "sphere", "3d"],
        min_args: 4,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Cube",
        syntax: "Cube[x, y, z, size]",
        help: "Crea un cubo 3D.",
        keywords: &["cubo", "cube", "3d"],
        min_args: 4,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Tetrahedron",
        syntax: "Tetrahedron[x, y, z, edge]",
        help: "Crea un tetraedro regular 3D sólido.",
        keywords: &["tetraedro", "tetrahedron", "3d"],
        min_args: 4,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Cylinder",
        syntax: "Cylinder[x, y, z, radius, height]",
        help: "Crea un cilindro 3D.",
        keywords: &["cilindro", "cylinder", "3d"],
        min_args: 5,
        max_args: 5,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Cone",
        syntax: "Cone[x, y, z, radius, height]",
        help: "Crea un cono 3D.",
        keywords: &["cono", "cone", "3d"],
        min_args: 5,
        max_args: 5,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Torus",
        syntax: "Torus[x, y, z, major_radius, minor_radius]",
        help: "Crea un toro 3D.",
        keywords: &["toro", "torus", "3d"],
        min_args: 5,
        max_args: 5,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Moebius",
        syntax: "Moebius[radius, width]",
        help: "Crea una banda de Moebius 3D.",
        keywords: &["moebius", "mobius", "banda", "3d"],
        min_args: 2,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Curve3D",
        syntax: "Curve3D[(x(t), y(t), z(t)), tmin, tmax]",
        help: "Grafica una curva parametrica 3D.",
        keywords: &["curva", "parametrica", "3d"],
        min_args: 3,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Surface3D",
        syntax: "Surface3D[f(x, y), xmin, xmax, ymin, ymax]",
        help: "Grafica una superficie 3D explicita o parametrica.",
        keywords: &["superficie", "surface", "3d"],
        min_args: 5,
        max_args: 7,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "ComplexSurface",
        syntax: "ComplexSurface[expr, xmin, xmax, ymin, ymax, resolution]",
        help: "Grafica el modulo de una funcion compleja como superficie 3D.",
        keywords: &["compleja", "superficie", "3d"],
        min_args: 1,
        max_args: 6,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "VectorField3D",
        syntax: "VectorField3D[u, v, w]",
        help: "Grafica un campo vectorial 3D.",
        keywords: &["campo", "vectorial", "3d"],
        min_args: 3,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Lorenz",
        syntax: "Lorenz[sigma, rho, beta]",
        help: "Grafica el atractor de Lorenz.",
        keywords: &["lorenz", "atractor", "caos"],
        min_args: 0,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Rossler",
        syntax: "Rossler[a, b, c]",
        help: "Grafica el atractor de Rossler.",
        keywords: &["rossler", "atractor", "caos"],
        min_args: 0,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Thomas",
        syntax: "Thomas[steps]",
        help: "Grafica el atractor de Thomas.",
        keywords: &["thomas", "atractor", "caos"],
        min_args: 0,
        max_args: 1,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Aizawa",
        syntax: "Aizawa[]",
        help: "Grafica el atractor de Aizawa.",
        keywords: &["aizawa", "atractor", "caos"],
        min_args: 0,
        max_args: 6,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Chen",
        syntax: "Chen[]",
        help: "Grafica el atractor de Chen.",
        keywords: &["chen", "atractor", "caos"],
        min_args: 0,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Halvorsen",
        syntax: "Halvorsen[a]",
        help: "Grafica el atractor de Halvorsen.",
        keywords: &["halvorsen", "atractor", "caos"],
        min_args: 0,
        max_args: 1,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Dadras",
        syntax: "Dadras[]",
        help: "Grafica el atractor de Dadras.",
        keywords: &["dadras", "atractor", "caos"],
        min_args: 0,
        max_args: 5,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Chua",
        syntax: "Chua[]",
        help: "Grafica el atractor de Chua.",
        keywords: &["chua", "atractor", "caos"],
        min_args: 0,
        max_args: 4,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Pentachoron4D",
        syntax: "Pentachoron4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el 5-celda regular 4D.",
        keywords: &["pentachoron", "pentacoron", "5cell", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Tesseract4D",
        syntax: "Tesseract4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el tesseract regular 4D.",
        keywords: &["tesseract", "hipercubo", "hypercube", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "SixteenCell4D",
        syntax: "SixteenCell4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el 16-celda regular 4D.",
        keywords: &["16cell", "sixteen", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "TwentyFourCell4D",
        syntax: "TwentyFourCell4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el 24-celda regular 4D.",
        keywords: &["24cell", "twenty", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "OneTwentyCell4D",
        syntax: "OneTwentyCell4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el 120-celda regular 4D.",
        keywords: &["120cell", "one", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "SixHundredCell4D",
        syntax: "SixHundredCell4D[scale, {xy, xz, xw, yz, yw, zw}]",
        help: "Crea el 600-celda regular 4D.",
        keywords: &["600cell", "sixhundred", "4d"],
        min_args: 0,
        max_args: 2,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "SimplexND",
        syntax: "SimplexND[n, scale, {lexicographic-plane angles}]",
        help: "Crea un simplex regular en R^n para n entre 3 y 10.",
        keywords: &["simplex", "nd", "dimension", "4d"],
        min_args: 1,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "HypercubeND",
        syntax: "HypercubeND[n, scale, {lexicographic-plane angles}]",
        help: "Crea un hipercubo regular en R^n para n entre 3 y 10.",
        keywords: &["hypercube", "hipercubo", "nd", "dimension", "4d"],
        min_args: 1,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "CrossPolytopeND",
        syntax: "CrossPolytopeND[n, scale, {lexicographic-plane angles}]",
        help: "Crea un politopo cruzado regular en R^n para n entre 3 y 10.",
        keywords: &["cross", "cruzado", "nd", "dimension", "4d"],
        min_args: 1,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::WorldMeshThreeD,
    },
    AssistantGraphCapability {
        canonical: "Hypercube",
        syntax: "Hypercube[a_xy, a_xz, a_xw]",
        help: "Grafica una proyeccion 4D de un hipercubo.",
        keywords: &["hipercubo", "tesseract", "4d"],
        min_args: 0,
        max_args: 3,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::CpuOverlayThreeD,
    },
    AssistantGraphCapability {
        canonical: "Hypersphere",
        syntax: "Hypersphere[]",
        help: "Grafica una proyeccion 4D de una hiperesfera.",
        keywords: &["hiperesfera", "4d"],
        min_args: 0,
        max_args: 0,
        view: AssistantGraphView::ThreeD,
        proof: AssistantGraphProof::CpuOverlayThreeD,
    },
];

const EXPRESSION_TRIGONOMETRY_FORMS: &[&str] = &[
    "sin(x)", "cos(x)", "tan(x)", "asin(x)", "acos(x)", "atan(x)", "sinh(x)", "cosh(x)", "tanh(x)",
];

const EXPRESSION_SPECIAL_FORMS: &[&str] = &[
    "erf(x)",
    "erfc(x)",
    "gamma(x)",
    "lngamma(x)",
    "digamma(x)",
    "trigamma(x)",
    "beta(a, b)",
    "besselj(n, x)",
    "bessely(n, x)",
    "besseli(n, x)",
];

const EXPRESSION_ALGEBRA_FORMS: &[&str] = &[
    "exp(x)",
    "ln(x)",
    "log(x)",
    "sqrt(x)",
    "cbrt(x)",
    "abs(x)",
    "min(a, b)",
    "max(a, b)",
    "clamp(x, min, max)",
];

struct ExpressionReferenceSeed {
    canonical: &'static str,
    help: &'static str,
    keywords: &'static [&'static str],
    forms: &'static [&'static str],
}

const EXPRESSION_REFERENCE_SEEDS: &[ExpressionReferenceSeed] = &[
    ExpressionReferenceSeed {
        canonical: "Funciones trigonométricas de expresiones",
        help: "Usa nombres en minúscula y paréntesis en las expresiones.",
        keywords: &[
            "expresion",
            "funcion",
            "sintaxis",
            "trigonometria",
            "seno",
            "coseno",
        ],
        forms: EXPRESSION_TRIGONOMETRY_FORMS,
    },
    ExpressionReferenceSeed {
        canonical: "Funciones especiales de expresiones",
        help: "Usa nombres en minúscula y paréntesis en las expresiones.",
        keywords: &[
            "expresion",
            "funcion",
            "sintaxis",
            "especial",
            "gamma",
            "bessel",
        ],
        forms: EXPRESSION_SPECIAL_FORMS,
    },
    ExpressionReferenceSeed {
        canonical: "Funciones algebraicas de expresiones",
        help: "Usa nombres en minúscula y paréntesis en las expresiones.",
        keywords: &["expresion", "funcion", "sintaxis", "raiz", "logaritmo"],
        forms: EXPRESSION_ALGEBRA_FORMS,
    },
];

/// Grafo inmutable de conocimiento que vincula el registro de comandos con la
/// política de preflight del asistente. Se construye una sola vez por proceso.
#[derive(Debug)]
pub struct AssistantKnowledgeGraph {
    nodes: Vec<AssistantKnowledgeNode>,
}

#[derive(Debug)]
struct AssistantKnowledgeNode {
    canonical: &'static str,
    aliases: &'static [&'static str],
    category: &'static str,
    help: &'static str,
    syntax_forms: Vec<AssistantKnowledgeSyntax>,
    registry_spec: Option<&'static command_registry::CommandSpec>,
    executable_policy: Option<&'static AssistantGraphCapability>,
    keywords: &'static [&'static str],
    reference_only: bool,
    expression_reference: bool,
}

#[derive(Debug)]
struct AssistantKnowledgeSyntax {
    syntax: String,
    argument_kinds: Vec<ArgumentKind>,
    min_args: usize,
    max_args: usize,
}

impl AssistantKnowledgeSyntax {
    fn from_command_signature(
        signature: &command_registry::CommandSignature,
        capability: Option<&AssistantGraphCapability>,
    ) -> Self {
        let mut argument_kinds = signature
            .arguments
            .iter()
            .map(|argument| argument.kind)
            .collect::<Vec<_>>();
        let mut min_args = signature
            .arguments
            .iter()
            .filter(|argument| !argument.optional)
            .count();
        let mut max_args = argument_kinds.len();
        let mut syntax = signature.syntax.to_owned();

        // Keep a registry-backed variadic tail typed when the assistant policy
        // deliberately bounds it more tightly than the handler.
        if let Some(capability) = capability.filter(|capability| capability.max_args > max_args) {
            if let Some(last_kind) = argument_kinds.last().copied() {
                argument_kinds.resize(capability.max_args, last_kind);
                max_args = capability.max_args;
            }
        }

        // Avoid advertising handler parameters that the assistant policy excludes.
        if let Some(capability) = capability.filter(|capability| {
            capability.max_args < max_args && capability.syntax != signature.syntax
        }) {
            syntax = capability.syntax.to_owned();
            argument_kinds.truncate(capability.max_args);
            min_args = min_args.max(capability.min_args);
            max_args = capability.max_args;
        }

        Self {
            syntax,
            max_args,
            argument_kinds,
            min_args,
        }
    }

    fn reference(syntax: impl Into<String>) -> Self {
        Self {
            syntax: syntax.into(),
            argument_kinds: Vec::new(),
            min_args: 0,
            max_args: 0,
        }
    }

    fn accepts_argument_count(&self, count: usize) -> bool {
        (self.min_args..=self.max_args).contains(&count)
    }

    fn is_literal_safe(&self) -> bool {
        self.argument_kinds.iter().all(|kind| {
            matches!(
                kind,
                ArgumentKind::Expression
                    | ArgumentKind::ComplexExpression
                    | ArgumentKind::Variable
                    | ArgumentKind::Number
                    | ArgumentKind::Integer
                    | ArgumentKind::Point
                    | ArgumentKind::Vector
                    | ArgumentKind::Relation
                    | ArgumentKind::ParameterList
            )
        })
    }
}

impl AssistantKnowledgeNode {
    fn has_literal_safe_form(&self) -> bool {
        self.literal_safe_forms().next().is_some()
    }

    fn literal_safe_forms(&self) -> impl Iterator<Item = &AssistantKnowledgeSyntax> {
        self.syntax_forms.iter().filter(|syntax| {
            !self.reference_only
                && self.executable_policy.is_some_and(|policy| {
                    syntax.is_literal_safe()
                        && (syntax.min_args..=syntax.max_args)
                            .any(|count| (policy.min_args..=policy.max_args).contains(&count))
                })
        })
    }

    fn accepts_literal_safe_arity(&self, count: usize) -> bool {
        let Some(policy) = self.executable_policy else {
            return false;
        };
        self.registry_spec
            .is_some_and(|spec| spec.accepts_argument_count(count))
            && (policy.min_args..=policy.max_args).contains(&count)
            && self
                .literal_safe_forms()
                .any(|syntax| syntax.accepts_argument_count(count))
    }

    fn relevance_score(&self, terms: &[String]) -> usize {
        score_named_terms(self.canonical, terms, 120, 260)
            + self
                .aliases
                .iter()
                .map(|alias| score_named_terms(alias, terms, 110, 280))
                .sum::<usize>()
            + score_text_terms(self.category, terms, 25)
            + score_text_terms(self.help, terms, 35)
            + self
                .syntax_forms
                .iter()
                .map(|syntax| score_text_terms(&syntax.syntax, terms, 45))
                .sum::<usize>()
            + self
                .keywords
                .iter()
                .map(|keyword| score_named_terms(keyword, terms, 80, 180))
                .sum::<usize>()
    }

    fn render_catalog_entry(&self) -> (bool, String) {
        let executable_forms = self
            .literal_safe_forms()
            .map(|syntax| syntax.syntax.as_str())
            .collect::<Vec<_>>();
        if !executable_forms.is_empty() {
            return (
                true,
                format!(
                    "- [EJECUTABLE] `{}`: {}",
                    executable_forms.join("`, `"),
                    self.help
                ),
            );
        }

        let forms = self
            .syntax_forms
            .iter()
            .map(|syntax| syntax.syntax.as_str())
            .collect::<Vec<_>>();
        let entry = if self.expression_reference {
            format!(
                "- [REFERENCIA] {}: {} Sintaxis: {}. No lo emitas como bloque grafito.",
                self.canonical,
                self.help,
                forms.join(", ")
            )
        } else {
            format!(
                "- [REFERENCIA] `{}`: {} No lo emitas como bloque grafito.",
                forms.join("`, `"),
                self.help
            )
        };
        (false, entry)
    }
}

impl AssistantKnowledgeGraph {
    fn build() -> Self {
        let mut nodes = command_registry::all()
            .iter()
            .map(|spec| {
                let executable_policy = assistant_graph_capability_by_canonical(spec.canonical);
                AssistantKnowledgeNode {
                    canonical: spec.canonical,
                    aliases: spec.aliases,
                    category: spec.category,
                    help: spec.help,
                    syntax_forms: spec
                        .signatures
                        .iter()
                        .map(|signature| {
                            AssistantKnowledgeSyntax::from_command_signature(
                                signature,
                                executable_policy,
                            )
                        })
                        .collect(),
                    registry_spec: Some(spec),
                    executable_policy,
                    keywords: executable_policy
                        .map(|capability| capability.keywords)
                        .unwrap_or_default(),
                    reference_only: spec.mutation == MutationClass::LoadsExternalData,
                    expression_reference: false,
                }
            })
            .collect::<Vec<_>>();

        for seed in EXPRESSION_REFERENCE_SEEDS {
            let syntax_forms = seed
                .forms
                .iter()
                .filter(|syntax| grafito_geometry::ast::parse_ast(syntax).is_ok())
                .map(|syntax| AssistantKnowledgeSyntax::reference(*syntax))
                .collect::<Vec<_>>();
            if syntax_forms.is_empty() {
                continue;
            }
            nodes.push(AssistantKnowledgeNode {
                canonical: seed.canonical,
                aliases: &[],
                category: "Expresiones",
                help: seed.help,
                syntax_forms,
                registry_spec: None,
                executable_policy: None,
                keywords: seed.keywords,
                reference_only: true,
                expression_reference: true,
            });
        }

        Self { nodes }
    }

    fn node_for(&self, command: &str) -> Option<&AssistantKnowledgeNode> {
        self.nodes.iter().find(|node| {
            node.canonical.eq_ignore_ascii_case(command)
                || node
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(command))
        })
    }
}

/// Devuelve el grafo de conocimiento local construido una vez por proceso.
pub fn assistant_knowledge_graph() -> &'static AssistantKnowledgeGraph {
    static GRAPH: OnceLock<AssistantKnowledgeGraph> = OnceLock::new();
    GRAPH.get_or_init(AssistantKnowledgeGraph::build)
}

fn assistant_graph_capability_by_canonical(
    command: &str,
) -> Option<&'static AssistantGraphCapability> {
    ASSISTANT_GRAPH_CAPABILITIES
        .iter()
        .find(|capability| capability.canonical.eq_ignore_ascii_case(command))
}

fn score_named_terms(
    value: &str,
    terms: &[String],
    partial_weight: usize,
    exact_weight: usize,
) -> usize {
    let value = value.to_lowercase();
    terms
        .iter()
        .map(|term| {
            if value == *term {
                exact_weight
            } else if value.contains(term) {
                partial_weight
            } else {
                0
            }
        })
        .sum()
}

fn score_text_terms(value: &str, terms: &[String], weight: usize) -> usize {
    let value = value.to_lowercase();
    terms
        .iter()
        .filter(|term| value.contains(term.as_str()))
        .count()
        * weight
}

/// Devuelve el contrato gráfico permitido para un comando canónico.
pub fn assistant_graph_capability(command: &str) -> Option<&'static AssistantGraphCapability> {
    let canonical = command_registry::resolve(command)
        .map(|spec| spec.canonical)
        .unwrap_or(command);
    assistant_graph_capability_by_canonical(canonical)
}

/// Indica si una propuesta remota puede verificarse localmente como un gráfico
/// 2D independiente antes de modificar el documento.
pub fn is_assistant_proposable(command: &str) -> bool {
    assistant_knowledge_graph()
        .node_for(command)
        .is_some_and(AssistantKnowledgeNode::has_literal_safe_form)
}

/// Indica si una propuesta tiene una forma literal segura y compatible con la
/// política de ejecución, el registro y la aridad publicada.
pub fn assistant_command_has_literal_safe_form(command: &str, argument_count: usize) -> bool {
    assistant_knowledge_graph()
        .node_for(command)
        .is_some_and(|node| node.accepts_literal_safe_arity(argument_count))
}

/// Comprueba restricciones literales adicionales que el registro expresa en la
/// sintaxis pero que no se pueden representar sólo con la aridad.
pub fn assistant_command_has_literal_safe_arguments(command: &str, arguments: &[String]) -> bool {
    let canonical = command_registry::resolve(command)
        .map(|spec| spec.canonical)
        .unwrap_or(command);
    match canonical {
        "DomainColoring" => match arguments.get(5) {
            None => true,
            Some(resolution) => resolution
                .trim()
                .parse::<usize>()
                .is_ok_and(|value| (16..=300).contains(&value)),
        },
        _ => true,
    }
}

/// Aclara restricciones literales que el asistente debe repetir al reparar un
/// comando rechazado, sin incluir el texto remoto original.
pub fn assistant_literal_argument_guidance(command: &str) -> Option<&'static str> {
    let canonical = command_registry::resolve(command)
        .map(|spec| spec.canonical)
        .unwrap_or(command);
    match canonical {
        "DomainColoring" => Some("resolution: literal integer 16..=300 (default 200)"),
        _ => None,
    }
}

/// Devuelve únicamente las sintaxis registradas que el asistente puede proponer.
pub fn assistant_executable_syntaxes(command: &str) -> Vec<String> {
    assistant_knowledge_graph()
        .node_for(command)
        .map(|node| {
            node.literal_safe_forms()
                .map(|syntax| syntax.syntax.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Construye una proyección breve y determinista del grafo para una consulta.
/// Nunca incluye formas no tipadas como acciones remotas y respeta `max_bytes`.
pub fn assistant_tool_catalog(problem: &str, max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }

    let normalized_problem = problem.to_lowercase();
    let complex_request =
        normalized_problem.contains("complej") || normalized_problem.contains("complex");
    let terms = normalized_problem
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() >= 3 || matches!(*term, "3d" | "4d"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let generic_graph_request = ["grafic", "graf", "mostr", "dibuj", "visualiz"]
        .iter()
        .any(|term| normalized_problem.contains(term));
    let catalog_overview_request = ["herramient", "comando", "sintaxis", "puedo hacer"]
        .iter()
        .any(|term| normalized_problem.contains(term));
    let specialized_request = [
        "param",
        "polar",
        "implic",
        "corazon",
        "corazón",
        "cardioide",
        "region",
        "región",
        "superfic",
        "3d",
        "4d",
        "vectorial",
        "campo",
        "atractor",
        "fractal",
        "histogram",
        "regresion",
        "dcolor",
    ]
    .iter()
    .any(|term| normalized_problem.contains(term));
    // Para ejemplos de cálculo (integral/derivada/taylor) queremos ofrecer Function como EJECUTABLE
    // aunque no haya pedido gráfico explícito, para que haya bloque Aplicar y no solo LaTeX.
    let calculus_example_request = [
        "integral",
        "derivada",
        "derivar",
        "deriva",
        "primitiva",
        "taylor",
        "serie",
        "polinomio",
        "aproxima",
    ]
    .iter()
    .any(|term| normalized_problem.contains(term));
    let mut candidates = Vec::new();
    for node in &assistant_knowledge_graph().nodes {
        let default_function = generic_graph_request
            && !complex_request
            && !specialized_request
            && node.canonical == "Function";
        let calculus_boost = calculus_example_request && node.canonical == "Function";
        let score = node.relevance_score(&terms)
            + usize::from(default_function) * 250
            + usize::from(calculus_boost) * 200
            + usize::from(catalog_overview_request);
        if score == 0 {
            continue;
        }
        let (executable, entry) = node.render_catalog_entry();
        candidates.push((score, executable, node.canonical, entry));
    }

    candidates.sort_by(
        |(left_score, left_executable, left_name, _),
         (right_score, right_executable, right_name, _)| {
            right_score
                .cmp(left_score)
                .then_with(|| right_executable.cmp(left_executable))
                .then_with(|| left_name.cmp(right_name))
        },
    );

    let mut catalog = String::new();
    for (_, _, _, entry) in candidates {
        let separator = usize::from(!catalog.is_empty());
        if catalog
            .len()
            .saturating_add(separator)
            .saturating_add(entry.len())
            > max_bytes
        {
            break;
        }
        if !catalog.is_empty() {
            catalog.push('\n');
        }
        catalog.push_str(&entry);
    }
    catalog
}

/// Extrae únicamente variables y metadatos visibles, sin rutas, archivos ni caches.
pub fn document_context(document: &Document) -> ImmutableDocumentContext {
    let variables = document
        .variables()
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect::<BTreeMap<_, _>>();
    let objects = document
        .objects_iter()
        .filter(|(_, object)| object.is_visible() && !object.contains_private_data())
        .map(|(_, object)| DocumentContextObject {
            label: object.label().to_string(),
            kind: object.name().to_string(),
            fingerprint: serde_json::to_string(object)
                .unwrap_or_else(|_| format!("{}:{}", object.name(), object.label())),
        })
        .collect();
    ImmutableDocumentContext::from_parts(document.version, variables, objects)
}

/// Resume la función seleccionada para una consulta del asistente.
///
/// El resumen evita serializar el objeto completo y sólo se toma al enviar la
/// consulta, para que no quede ligado a una selección anterior.
pub fn selected_function_focus(
    document: &Document,
    selected: Option<ObjectId>,
) -> Option<AssistantFocus> {
    let GeoObject::Function(function) = document.get_object(selected?)? else {
        return None;
    };
    if function.fit.is_some() {
        return None;
    }
    Some(AssistantFocus::function(
        function.label.clone(),
        function.expr.clone(),
        function.domain_min,
        function.domain_max,
        function.is_integral,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{CasWorksheetStatus, DataTableObj, Document, GeoObject, PointObj};
    use grafito_geometry::Point2;

    #[test]
    fn context_digest_changes_with_document_revision_or_variables() {
        let mut document = Document::new();
        let before = document_context(&document);
        document.set_variable("a".into(), 2.0);
        let after = document_context(&document);

        assert_ne!(before.digest, after.digest);
        assert_ne!(before.revision, after.revision);
    }

    #[test]
    fn assistant_context_omits_local_data_rows_and_diagnostics() {
        let mut document = Document::new();
        document.add_object(GeoObject::DataTable(DataTableObj::new(
            "time",
            "distance",
            vec![0.0, 1.0],
            vec![1.0, 3.0],
        )));
        document.add_object(GeoObject::ScatterPlot(grafito_core::ScatterPlotObj::new(
            vec![0.0, 1.0],
            vec![1.0, 3.0],
        )));

        let context = document_context(&document);

        assert!(context.objects.is_empty());
        assert!(!serde_json::to_string(&context)
            .unwrap()
            .contains("distance"));
    }

    #[test]
    fn assistant_context_omits_persisted_dynamic_locus_samples() {
        let mut document = Document::new();
        let driver = document.add_object(GeoObject::Point(
            PointObj::new(Point2::new(0.0, 0.0)).with_label("A"),
        ));
        let target = document.add_object(GeoObject::Point(
            PointObj::new(Point2::new(1.0, 0.0)).with_label("B"),
        ));
        let (locus, _) = document
            .try_add_locus(driver, target)
            .expect("fixture locus should be valid");

        let context = document_context(&document);

        assert!(context.objects.iter().any(|object| object.label == "A"));
        assert!(context.objects.iter().any(|object| object.label == "B"));
        assert!(context
            .objects
            .iter()
            .all(|object| object.label != document.get_object(locus).unwrap().label()));
        assert!(!serde_json::to_string(&context)
            .unwrap()
            .contains("locus_binding"));
    }

    #[test]
    fn assistant_context_omits_persisted_cas_worksheet_cells() {
        let mut document = Document::new();
        document
            .try_append_cas_worksheet_cell(
                "Solve[private_equation(x), x]".to_string(),
                "private diagnostic".to_string(),
                CasWorksheetStatus::Error,
            )
            .expect("fixture worksheet cell is valid");

        let context = document_context(&document);
        let encoded = serde_json::to_string(&context).expect("context serializes");

        assert!(!encoded.contains("private_equation"));
        assert!(!encoded.contains("private diagnostic"));
    }

    #[test]
    fn selected_function_focus_contains_expression_and_domain_without_document_json() {
        let mut document = Document::new();
        let mut function = grafito_core::FunctionObj::new("sin(x)").with_label("f");
        function.domain_min = Some(-3.0);
        function.domain_max = Some(3.0);
        let id = document.add_object(grafito_core::GeoObject::Function(function));

        let focus = selected_function_focus(&document, Some(id)).expect("function focus");

        assert_eq!(focus.label, "f");
        assert_eq!(focus.kind, "Function");
        assert!(focus.summary.contains("f(x) = sin(x)"));
        assert!(focus.summary.contains("[-3, 3]"));
        assert!(!focus.summary.contains("cached_samples"));
    }

    #[test]
    fn selected_fitted_function_never_becomes_remote_assistant_focus() {
        let mut document = Document::new();
        let source =
            grafito_core::DataTableObj::new("x", "y", vec![0.0, 1.0, 2.0], vec![1.0, 3.0, 5.0]);
        let source_id = source.id;
        document.add_object(GeoObject::DataTable(source));
        let fit = grafito_geometry::statistics::fit_xy(
            grafito_geometry::statistics::FitKind::Linear,
            &[0.0, 1.0, 2.0],
            &[1.0, 3.0, 5.0],
        )
        .expect("fixture fit succeeds");
        let id = document.add_object(GeoObject::Function(
            grafito_core::FunctionObj::new(fit.expression())
                .with_fit(grafito_core::FitMetadata::from_result(source_id, fit)),
        ));

        assert!(selected_function_focus(&document, Some(id)).is_none());
    }

    #[test]
    fn tool_catalog_includes_relevant_documented_commands_within_its_budget() {
        let catalog = assistant_tool_catalog("graficá y = sin(x)", 512);

        assert!(catalog.contains("Function[expr]"));
        assert!(!catalog.contains("PolarCurve[r(t), t0, t1]"));
        assert!(!catalog.contains("Analyze[objeto]"));
        assert!(catalog.len() <= 512);
        assert!(!catalog.contains("Image[ruta/al/archivo.png]"));
    }

    #[test]
    fn fourier_requests_retrieve_the_safe_function_capability() {
        let catalog = assistant_tool_catalog("serie de Fourier finita", 512);

        assert!(
            catalog.contains("[EJECUTABLE] `Function[expr]`"),
            "{catalog}"
        );
    }

    #[test]
    fn complex_graph_requests_offer_domain_coloring_not_label_dependent_mappings() {
        let catalog = assistant_tool_catalog("graficá la función compleja 1/z", 512);

        assert!(catalog.contains("DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]"));
        assert!(
            catalog.contains("entero literal entre 16 y 300"),
            "{catalog}"
        );
        assert!(!catalog.contains("Function[expr]"));
        assert!(catalog.contains("ComplexGrid[expr, xmin, xmax, ymin, ymax, density]"));
        assert!(catalog.contains("[EJECUTABLE] `ComplexGrid"));
        assert!(!catalog.contains("ComplexMapping"));
    }

    #[test]
    fn specialized_graph_requests_receive_their_verified_capabilities() {
        let polar = assistant_tool_catalog("graficá una curva polar", 512);
        let surface = assistant_tool_catalog("mostrá una superficie 3d", 512);

        assert!(polar.contains("PolarCurve[r(t), t0, t1]"), "{polar}");
        assert!(surface.contains("Surface3D[f(x, y), xmin, xmax, ymin, ymax]"));
        assert!(!surface.contains("Function[expr]"));
    }

    #[test]
    fn tetrahedron_requests_offer_the_native_solid_without_polyhedron_syntax() {
        let catalog = assistant_tool_catalog("construí un tetraedro regular", 512);

        assert!(crate::command_registry::resolve("Tetrahedron").is_some());
        assert!(catalog.contains("Tetrahedron[x, y, z, edge]"));
        assert!(!catalog.contains("Polyhedron"));
        assert!(catalog.contains("EJECUTABLE"));
    }

    #[test]
    fn heart_requests_receive_the_implicit_curve_capability() {
        let catalog = assistant_tool_catalog("dibujá un corazon", 512);

        assert!(catalog.contains("ImplicitCurve[f(x, y) = c]"));
        assert!(!catalog.contains("Function[expr]"));
    }

    #[test]
    fn knowledge_graph_retrieves_canonical_commands_for_registered_aliases() {
        let catalog = assistant_tool_catalog("usá dcolor para visualizar 1/z", 1_024);

        assert!(
            catalog.contains(
                "[EJECUTABLE] `DomainColoring[expr, xmin, xmax, ymin, ymax, resolution]`"
            ),
            "{catalog}"
        );
        assert!(!catalog.contains("`dcolor["), "{catalog}");
    }

    #[test]
    fn knowledge_graph_renders_all_safe_implicit_and_surface_forms() {
        let implicit = assistant_tool_catalog("curva implicita con lhs rhs y relacion", 1_536);
        let surface = assistant_tool_catalog("superficie parametrica 3d", 1_536);

        assert!(
            implicit.contains("ImplicitCurve[f(x, y) = c]"),
            "{implicit}"
        );
        assert!(
            implicit.contains("ImplicitCurve[lhs, rhs, relacion]"),
            "{implicit}"
        );
        assert!(
            surface.contains("Surface3D[f(x, y), xmin, xmax, ymin, ymax]"),
            "{surface}"
        );
        assert!(
            surface.contains("Surface3D[(x(u,v), y(u,v), z(u,v)), umin, umax, vmin, vmax]"),
            "{surface}"
        );
        assert!(
            surface.contains("Surface3D[x(u,v), y(u,v), z(u,v), umin, umax, vmin, vmax]"),
            "{surface}"
        );
    }

    #[test]
    fn knowledge_graph_never_marks_data_or_object_label_forms_executable() {
        let data = assistant_tool_catalog("creá un histograma con datos", 1_024);
        let line = assistant_tool_catalog("dibujá una recta3d", 1_024);

        assert!(!data.contains("[EJECUTABLE] `Histogram"), "{data}");
        assert!(
            line.contains("[EJECUTABLE] `Line3D[x0, y0, z0, dx, dy, dz]`"),
            "{line}"
        );
        assert!(!line.contains("[EJECUTABLE] `Line3D[p1, p2]`"), "{line}");
    }

    #[test]
    fn knowledge_graph_catalog_order_is_deterministic_and_prefix_bounded() {
        let problem = "superficie parametrica 3d";
        let first = assistant_tool_catalog(problem, 512);
        let second = assistant_tool_catalog(problem, 512);

        assert_eq!(first, second);
        assert!(first.len() <= 512);
        assert!(first.starts_with("- [EJECUTABLE] `Surface3D"), "{first}");
    }

    #[test]
    fn knowledge_graph_exposes_parser_backed_expression_functions_as_reference_only() {
        let catalog = assistant_tool_catalog("sintaxis de funciones gamma bessel sin cos", 1_024);

        assert!(catalog.contains("[REFERENCIA]"), "{catalog}");
        assert!(catalog.contains("sin(x)"), "{catalog}");
        assert!(catalog.contains("cos(x)"), "{catalog}");
        assert!(catalog.contains("gamma(x)"), "{catalog}");
        assert!(catalog.contains("besselj(n, x)"), "{catalog}");
        assert!(!catalog.contains("[EJECUTABLE] `gamma"), "{catalog}");
    }

    #[test]
    fn graph_capabilities_describe_safe_2d_and_3d_execution_routes() {
        let function = assistant_graph_capability("Function").expect("function capability");
        let surface = assistant_graph_capability("Surface3D").expect("surface capability");
        let implicit = assistant_graph_capability("ImplicitCurve").expect("implicit capability");
        let hypercube = assistant_graph_capability("Hypercube").expect("4d capability");

        assert_eq!(function.view, AssistantGraphView::TwoD);
        assert_eq!(surface.proof, AssistantGraphProof::WorldMeshThreeD);
        assert_eq!(implicit.min_args, 1);
        assert_eq!(implicit.max_args, 3);
        assert_eq!(hypercube.proof, AssistantGraphProof::CpuOverlayThreeD);
        assert!(assistant_graph_capability("Script").is_none());
    }

    #[test]
    fn contour_literal_safe_form_obeys_the_effective_work_limited_arity() {
        let contour = assistant_graph_capability("Contour").expect("contour capability");
        let catalog = assistant_tool_catalog("graficá curvas de contorno", 512);

        assert_eq!(contour.min_args, 6);
        assert_eq!(contour.max_args, 13);
        assert!(assistant_command_has_literal_safe_form("Contour", 13));
        assert!(!assistant_command_has_literal_safe_form("Contour", 14));
        assert_eq!(
            assistant_executable_syntaxes("Contour"),
            vec!["Contour[f(x, y), xmin, xmax, ymin, ymax, nivel, ...]".to_owned()]
        );
        assert!(
            catalog.contains("[EJECUTABLE] `Contour[f(x, y), xmin, xmax, ymin, ymax, nivel, ...]`"),
            "{catalog}"
        );
    }

    #[test]
    fn assistant_attractor_and_hypersphere_forms_obey_effective_arities() {
        let catalog = assistant_tool_catalog("chen halvorsen dadras chua hypersphere", 4_096);

        for (canonical, maximum, syntax) in [
            ("Chen", 3, "Chen[a, b, c]"),
            ("Halvorsen", 1, "Halvorsen[a]"),
            ("Dadras", 5, "Dadras[p, q, r, s, e]"),
            ("Chua", 4, "Chua[alpha, beta, m0, m1]"),
            ("Hypersphere", 0, "Hypersphere[]"),
        ] {
            let capability = assistant_graph_capability(canonical).expect("capability");

            assert_eq!(capability.max_args, maximum, "{canonical}");
            assert!(
                assistant_command_has_literal_safe_form(canonical, maximum),
                "{canonical} maximum arity must remain assistant-safe"
            );
            assert!(
                !assistant_command_has_literal_safe_form(canonical, maximum + 1),
                "{canonical} must reject one argument above its effective maximum"
            );
            assert_eq!(
                assistant_executable_syntaxes(canonical),
                vec![syntax.to_owned()],
                "{canonical}"
            );
            assert!(
                catalog.contains(&format!("[EJECUTABLE] `{syntax}`")),
                "{catalog}"
            );
        }

        assert!(!catalog.contains("Halvorsen[a, p2, p3, p4]"), "{catalog}");
    }

    #[test]
    fn registered_capability_maxima_do_not_exceed_registry_forms() {
        for capability in ASSISTANT_GRAPH_CAPABILITIES {
            let spec = command_registry::resolve(capability.canonical)
                .expect("every assistant capability must have registered metadata");
            assert!(
                spec.accepts_argument_count(capability.max_args),
                "{} assistant maximum {} exceeds registered arity",
                capability.canonical,
                capability.max_args
            );
        }
    }

    #[test]
    fn regular_polytope_capabilities_use_complete_world_mesh_proofs() {
        for (canonical, min_args, max_args) in [
            ("Pentachoron4D", 0, 2),
            ("Tesseract4D", 0, 2),
            ("SixteenCell4D", 0, 2),
            ("TwentyFourCell4D", 0, 2),
            ("OneTwentyCell4D", 0, 2),
            ("SixHundredCell4D", 0, 2),
            ("SimplexND", 1, 3),
            ("HypercubeND", 1, 3),
            ("CrossPolytopeND", 1, 3),
        ] {
            let capability = assistant_graph_capability(canonical)
                .expect("each named regular polytope is assistant-proposable");
            assert_eq!(capability.view, AssistantGraphView::ThreeD);
            assert_eq!(capability.proof, AssistantGraphProof::WorldMeshThreeD);
            assert_eq!(capability.min_args, min_args);
            assert_eq!(capability.max_args, max_args);
        }

        assert_eq!(
            assistant_graph_capability("Hypercube")
                .expect("legacy capability")
                .proof,
            AssistantGraphProof::CpuOverlayThreeD
        );
        assert_eq!(
            assistant_graph_capability("Hypersphere")
                .expect("legacy capability")
                .proof,
            AssistantGraphProof::CpuOverlayThreeD
        );
    }
}
