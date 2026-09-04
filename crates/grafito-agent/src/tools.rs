//! Herramientas seguras del agente (3 base + 6 pedagógicas = 9).
//!
//! Implementación autocontenida dentro de `grafito-agent`: no depende de
//! `grafito-assistant`, `grafito-pedagogy`, `grafito-geometry` ni `grafito-anim`
//! para mantener el DAG como hoja y ownership exclusivo en este crate.
//! Los datos curriculares replican los IDs/títulos reales del `Curriculum`
//! (`grafito-pedagogy/src/curriculum.rs`) y la lógica determinista
//! (wyhash, tolerancia 2 %, plantillas nativas) replica el comportamiento
//! verificado en `grafito-pedagogy` y `grafito-anim/src/protocol.rs`.

use crate::loop_engine::ToolDispatcher;
use crate::schema::{ToolCall, ToolResult, ToolSchema};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt;

// ── Constantes ──────────────────────────────────────────────────────────────

/// Límite de bytes por argumento de texto (mitiga DoS).
pub const MAX_ARG_BYTES: usize = 2_000;
/// Versión del protocolo de animación que `generate_animation` declara.
pub const ANIM_PROTOCOL_VERSION: u32 = 1;
/// Plantillas válidas del motor nativo (`anim_native` + protocolo).
pub const KNOWN_TEMPLATES: &[&str] = &[
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "euler",
    "fourier",
];
const MIN_CANVAS: u64 = 64;
const MAX_CANVAS: u64 = 4096;
const DEFAULT_CANVAS: (u32, u32) = (640, 480);
const DEFAULT_DURATION_MS: u64 = 2000;
const MIN_DURATION_MS: u64 = 100;
const MAX_DURATION_MS: u64 = 30_000;

// ── Error tipado en español ─────────────────────────────────────────────────

/// Error tipado de las herramientas, siempre en español con código claro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// Falta un campo obligatorio.
    FaltaCampo { campo: &'static str },
    /// Campo presente pero inválido.
    CampoInvalido { campo: &'static str, motivo: String },
    /// LO / concepto no encontrado en el currículum.
    NoEncontrado(String),
    /// Fallo de evaluación matemática o de dominio.
    EvaluacionFallida(String),
    /// Presupuesto excedido (tamaño, rango, etc.).
    PresupuestoExcedido(String),
}

impl ToolError {
    /// Código estable para aserciones y logs.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::FaltaCampo { .. } => "E_FALTA_CAMPO",
            Self::CampoInvalido { .. } => "E_CAMPO_INVALIDO",
            Self::NoEncontrado(_) => "E_NO_ENCONTRADO",
            Self::EvaluacionFallida(_) => "E_EVALUACION",
            Self::PresupuestoExcedido(_) => "E_PRESUPUESTO",
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FaltaCampo { campo } => {
                write!(f, "falta el campo obligatorio '{campo}' ({})", self.code())
            }
            Self::CampoInvalido { campo, motivo } => {
                write!(f, "campo '{campo}' inválido: {motivo} ({})", self.code())
            }
            Self::NoEncontrado(detalle) => {
                write!(f, "no encontrado: {detalle} ({})", self.code())
            }
            Self::EvaluacionFallida(detalle) => {
                write!(f, "evaluación fallida: {detalle} ({})", self.code())
            }
            Self::PresupuestoExcedido(detalle) => {
                write!(f, "presupuesto excedido: {detalle} ({})", self.code())
            }
        }
    }
}

impl std::error::Error for ToolError {}

fn err_result(call_id: &str, err: ToolError) -> ToolResult {
    ToolResult::text(call_id, false, err.to_string())
}

// ── Helpers de argumentos ───────────────────────────────────────────────────

fn string_arg(call: &ToolCall, key: &str) -> Option<String> {
    call.arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty() && v.len() <= MAX_ARG_BYTES)
        .map(str::to_owned)
}

/// Rechaza cualquier string >2000 bytes (recursivo en objetos/arrays).
fn reject_oversized_string_args(call: &ToolCall) -> Option<ToolResult> {
    fn check(call_id: &str, key: &str, value: &Value) -> Option<ToolResult> {
        if let Some(text) = value.as_str() {
            if text.len() > MAX_ARG_BYTES {
                return Some(ToolResult::text(
                    call_id,
                    false,
                    ToolError::PresupuestoExcedido(format!(
                        "el argumento '{key}' excede el límite de {MAX_ARG_BYTES} bytes (E_PRESUPUESTO)"
                    ))
                    .to_string(),
                ));
            }
        } else if let Some(map) = value.as_object() {
            for (k, v) in map {
                if let Some(r) = check(call_id, k, v) {
                    return Some(r);
                }
            }
        } else if let Some(arr) = value.as_array() {
            for item in arr {
                if let Some(text) = item.as_str() {
                    if text.len() > MAX_ARG_BYTES {
                        return Some(ToolResult::text(
                            call_id,
                            false,
                            ToolError::PresupuestoExcedido(format!(
                                "un elemento de '{key}' excede el límite de {MAX_ARG_BYTES} bytes (E_PRESUPUESTO)"
                            ))
                            .to_string(),
                        ));
                    }
                }
            }
        }
        None
    }
    if let Some(map) = call.arguments.as_object() {
        for (k, v) in map {
            if let Some(r) = check(&call.id, k, v) {
                return Some(r);
            }
        }
    }
    None
}

// ── Nivel pedagógico ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Primary,
    Secondary,
    University,
    UtmAm1,
    UtmAm2,
    UtmAlgebra,
    UtmProb,
}

fn parse_level(raw: Option<&str>) -> Level {
    let Some(text) = raw.map(|s| s.trim().to_lowercase()) else {
        return Level::Secondary;
    };
    match text.as_str() {
        "primary" | "primaria" | "primario" => Level::Primary,
        "secondary" | "secundaria" | "secundario" => Level::Secondary,
        "university" | "universidad" | "universitario" => Level::University,
        "utn_am1" | "utnam1" | "am1" | "utn am1" => Level::UtmAm1,
        "utn_am2" | "utnam2" | "am2" | "utn am2" => Level::UtmAm2,
        "utn_algebra" | "algebra" | "álgebra" => Level::UtmAlgebra,
        "utn_probabilidad" | "probabilidad" | "prob" | "utn_prob" => Level::UtmProb,
        _ => Level::Secondary,
    }
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::Primary => "Primaria",
        Level::Secondary => "Secundaria",
        Level::University => "Universidad",
        Level::UtmAm1 => "UTN AM1",
        Level::UtmAm2 => "UTN AM2",
        Level::UtmAlgebra => "UTN Álgebra",
        Level::UtmProb => "UTN Probabilidad",
    }
}

fn level_difficulty(level: Level) -> &'static str {
    match level {
        Level::Primary => "Easy",
        Level::Secondary => "Medium",
        _ => "Hard",
    }
}

// ── Currículum real (43 LOs, replica curriculum.rs) ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct Lo {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    program: Option<&'static str>,
    level_min: u32,
    requires: &'static [&'static str],
    tags: &'static [&'static str],
    hours: f32,
}

fn all_los() -> Vec<Lo> {
    vec![
        // Primaria 5
        Lo {
            id: "pri-conteo",
            title: "Conteo",
            description: "Conteo, números naturales, orden y comparación",
            program: None,
            level_min: 1,
            requires: &[],
            tags: &["conteo", "numeros", "orden", "primaria"],
            hours: 2.0,
        },
        Lo {
            id: "pri-fracc-vis",
            title: "Fracciones visuales",
            description: "Fracciones con dibujos, mitad y cuarto, representación pictórica",
            program: None,
            level_min: 1,
            requires: &["pri-conteo"],
            tags: &["fraccion", "visual", "mitad", "primaria"],
            hours: 2.5,
        },
        Lo {
            id: "pri-perim-area",
            title: "Perímetro y área",
            description: "Perímetro y área de figuras simples, cuadrados y rectángulos",
            program: None,
            level_min: 2,
            requires: &["pri-conteo"],
            tags: &["perimetro", "area", "geometria", "primaria"],
            hours: 3.0,
        },
        Lo {
            id: "pri-proporciones",
            title: "Proporciones simples",
            description: "Doble, mitad, proporcionalidad simple con ejemplos concretos",
            program: None,
            level_min: 2,
            requires: &["pri-fracc-vis"],
            tags: &["proporcion", "doble", "mitad", "primaria"],
            hours: 2.0,
        },
        Lo {
            id: "pri-datos",
            title: "Datos simples",
            description: "Tablas, gráficos de barras, promedio simple y recolección de datos",
            program: None,
            level_min: 2,
            requires: &["pri-conteo"],
            tags: &["datos", "tablas", "barras", "primaria", "estadistica"],
            hours: 2.0,
        },
        // Secundaria 11
        Lo {
            id: "sec-fracc",
            title: "Fracciones",
            description: "Operaciones con fracciones, simplificación, fracciones equivalentes",
            program: None,
            level_min: 4,
            requires: &["pri-fracc-vis"],
            tags: &["fraccion", "simplificacion", "equivalente", "secundaria"],
            hours: 3.0,
        },
        Lo {
            id: "sec-prop",
            title: "Proporciones",
            description: "Razones, proporciones, regla de tres, porcentaje",
            program: None,
            level_min: 5,
            requires: &["sec-fracc"],
            tags: &["proporcion", "razon", "regla de tres", "porcentaje"],
            hours: 3.0,
        },
        Lo {
            id: "sec-ec",
            title: "Ecuaciones",
            description: "Ecuaciones lineales y cuadráticas, sistemas de ecuaciones",
            program: None,
            level_min: 6,
            requires: &["sec-prop"],
            tags: &["ecuacion", "lineal", "cuadratica", "sistema"],
            hours: 4.0,
        },
        Lo {
            id: "sec-lineal",
            title: "Funciones lineales",
            description: "Recta, pendiente, ordenada al origen, gráfica de función lineal",
            program: None,
            level_min: 6,
            requires: &["sec-ec"],
            tags: &["funcion", "lineal", "recta", "pendiente", "grafica"],
            hours: 4.0,
        },
        Lo {
            id: "sec-cuad",
            title: "Funciones cuadráticas",
            description: "Parábola, vértice, raíces, discriminante, gráfica cuadrática",
            program: None,
            level_min: 7,
            requires: &["sec-lineal"],
            tags: &["funcion", "cuadratica", "parabola", "vertice", "raiz"],
            hours: 4.0,
        },
        Lo {
            id: "sec-pend",
            title: "Pendiente",
            description: "Pendiente de recta, tangente intuitiva, inclinación",
            program: None,
            level_min: 6,
            requires: &["sec-lineal"],
            tags: &["pendiente", "recta", "tangente", "inclinacion"],
            hours: 2.0,
        },
        Lo {
            id: "sec-area",
            title: "Área",
            description: "Área bajo curva, aproximación, área de figuras",
            program: None,
            level_min: 6,
            requires: &["pri-perim-area"],
            tags: &["area", "curva", "aproximacion", "figura"],
            hours: 2.5,
        },
        Lo {
            id: "sec-trig",
            title: "Trigonometría",
            description: "Seno, coseno, círculo unitario, identidades trigonométricas",
            program: None,
            level_min: 7,
            requires: &["sec-lineal"],
            tags: &["trigonometria", "seno", "coseno", "circulo", "identidad"],
            hours: 4.0,
        },
        Lo {
            id: "sec-vect",
            title: "Vectores intro",
            description: "Vectores, componentes, suma, noción geométrica",
            program: None,
            level_min: 6,
            requires: &["sec-pend"],
            tags: &["vector", "componente", "suma", "geometrico"],
            hours: 3.0,
        },
        Lo {
            id: "sec-prob",
            title: "Probabilidad básica secundaria",
            description: "Eventos, probabilidad simple, diagramas, frecuencia",
            program: None,
            level_min: 5,
            requires: &["sec-prop", "pri-datos"],
            tags: &[
                "probabilidad",
                "evento",
                "diagrama",
                "frecuencia",
                "secundaria",
            ],
            hours: 3.0,
        },
        Lo {
            id: "sec-pitagoras",
            title: "Teorema de Pitágoras",
            description:
                "Triángulo rectángulo, catetos e hipotenusa, c²=a²+b², demostración y aplicaciones",
            program: None,
            level_min: 8,
            requires: &["sec-area"],
            tags: &[
                "pitagoras",
                "triangulo",
                "hipotenusa",
                "cateto",
                "secundaria",
                "geometria",
            ],
            hours: 3.0,
        },
        // AM1 8
        Lo {
            id: "am1-func",
            title: "Funciones",
            description: "Dominio, imagen, composición, inversa, clasificación",
            program: Some("UTN AM1"),
            level_min: 10,
            requires: &[],
            tags: &["funcion", "dominio", "imagen", "composicion", "inversa"],
            hours: 6.0,
        },
        Lo {
            id: "am1-lim",
            title: "Límites",
            description: "Límites laterales, indeterminaciones, asintotas, límites infinitos",
            program: Some("UTN AM1"),
            level_min: 11,
            requires: &["am1-func"],
            tags: &["limite", "asintota", "indeterminacion", "continuidad"],
            hours: 5.0,
        },
        Lo {
            id: "am1-cont",
            title: "Continuidad",
            description: "Continuidad, teorema de Bolzano, clasificación de discontinuidades",
            program: Some("UTN AM1"),
            level_min: 11,
            requires: &["am1-lim"],
            tags: &["continuidad", "bolzano", "discontinuidad", "limite"],
            hours: 4.0,
        },
        Lo {
            id: "am1-der",
            title: "Derivadas",
            description: "Definición, reglas, recta tangente, extremos, derivada",
            program: Some("UTN AM1"),
            level_min: 12,
            requires: &["am1-cont"],
            tags: &["derivada", "tangente", "extremos", "reglas"],
            hours: 7.0,
        },
        Lo {
            id: "am1-der-aplic",
            title: "Aplicaciones de derivadas",
            description: "Crecimiento, concavidad, máximos y mínimos, L'Hôpital, optimización",
            program: Some("UTN AM1"),
            level_min: 12,
            requires: &["am1-der"],
            tags: &[
                "derivada",
                "optimizacion",
                "extremos",
                "concavidad",
                "lhopital",
            ],
            hours: 6.0,
        },
        Lo {
            id: "am1-int",
            title: "Integrales",
            description: "Primitivas, área, Barrow, impropias, integral definida",
            program: Some("UTN AM1"),
            level_min: 12,
            requires: &["am1-der"],
            tags: &["integral", "primitiva", "barrow", "area", "impropia"],
            hours: 7.0,
        },
        Lo {
            id: "am1-int-aplic",
            title: "Aplicaciones de integrales",
            description: "Área entre curvas, volumen de revolución, longitud de arco",
            program: Some("UTN AM1"),
            level_min: 12,
            requires: &["am1-int"],
            tags: &["integral", "area", "volumen", "revolucion", "arco"],
            hours: 5.0,
        },
        Lo {
            id: "am1-sucesiones",
            title: "Sucesiones",
            description: "Sucesiones numéricas, convergencia, criterio, límite de sucesión",
            program: Some("UTN AM1"),
            level_min: 11,
            requires: &["am1-lim"],
            tags: &["sucesion", "convergencia", "limite", "numerica"],
            hours: 4.0,
        },
        // AM2 7
        Lo {
            id: "am2-edo",
            title: "EDO",
            description: "Variables separables, lineales, aplicaciones, ecuaciones diferenciales",
            program: Some("UTN AM2"),
            level_min: 13,
            requires: &["am1-der", "am1-int"],
            tags: &["edo", "diferencial", "separable", "lineal"],
            hours: 8.0,
        },
        Lo {
            id: "am2-series",
            title: "Series numéricas",
            description: "Criterios de convergencia, series alternadas, geométricas",
            program: Some("UTN AM2"),
            level_min: 13,
            requires: &["am1-sucesiones"],
            tags: &["serie", "convergencia", "numerica", "criterio"],
            hours: 6.0,
        },
        Lo {
            id: "am2-taylor",
            title: "Taylor y Fourier",
            description: "Series de Taylor, Fourier, aproximación, convergencia",
            program: Some("UTN AM2"),
            level_min: 14,
            requires: &["am2-series"],
            tags: &["taylor", "fourier", "serie", "aproximacion"],
            hours: 6.0,
        },
        Lo {
            id: "am2-multivariable",
            title: "Cálculo multivariable",
            description: "Funciones de varias variables, derivadas parciales, gradiente",
            program: Some("UTN AM2"),
            level_min: 13,
            requires: &["am1-der"],
            tags: &["multivariable", "parcial", "gradiente", "varias variables"],
            hours: 7.0,
        },
        Lo {
            id: "am2-int-multi",
            title: "Integrales dobles y triples",
            description: "Integrales dobles, triples, cambio de variables, Jacobiano",
            program: Some("UTN AM2"),
            level_min: 14,
            requires: &["am2-multivariable"],
            tags: &[
                "integral",
                "doble",
                "triple",
                "jacobiano",
                "cambio variable",
            ],
            hours: 7.0,
        },
        Lo {
            id: "am2-campos",
            title: "Campos vectoriales",
            description: "Campos vectoriales, rotacional, divergencia, potencial",
            program: Some("UTN AM2"),
            level_min: 14,
            requires: &["am2-multivariable", "alg-vectores"],
            tags: &[
                "campo",
                "vectorial",
                "rotacional",
                "divergencia",
                "gradiente",
            ],
            hours: 6.0,
        },
        Lo {
            id: "am2-teoremas",
            title: "Teoremas integrales",
            description: "Green, Stokes, Gauss (divergencia), aplicaciones",
            program: Some("UTN AM2"),
            level_min: 14,
            requires: &["am2-campos", "am2-int-multi"],
            tags: &["green", "stokes", "gauss", "teorema", "integral"],
            hours: 6.0,
        },
        // Álgebra 6
        Lo {
            id: "alg-vectores",
            title: "Vectores",
            description: "Vectores en R2/R3, producto escalar y vectorial, norma",
            program: Some("UTN Álgebra"),
            level_min: 11,
            requires: &["sec-vect"],
            tags: &["vector", "escalar", "vectorial", "norma", "r2", "r3"],
            hours: 5.0,
        },
        Lo {
            id: "alg-rectas-planos",
            title: "Rectas y planos",
            description: "Ecuaciones de rectas y planos, posiciones relativas, distancias",
            program: Some("UTN Álgebra"),
            level_min: 12,
            requires: &["alg-vectores"],
            tags: &["recta", "plano", "ecuacion", "posicion", "distancia"],
            hours: 5.0,
        },
        Lo {
            id: "alg-matrices",
            title: "Matrices",
            description: "Operaciones, rango, sistemas lineales, Gauss-Jordan",
            program: Some("UTN Álgebra"),
            level_min: 11,
            requires: &["sec-ec"],
            tags: &["matriz", "rango", "sistema", "gauss", "lineal"],
            hours: 6.0,
        },
        Lo {
            id: "alg-determinantes",
            title: "Determinantes",
            description: "Propiedades, cálculo, matriz inversa, regla de Cramer",
            program: Some("UTN Álgebra"),
            level_min: 12,
            requires: &["alg-matrices"],
            tags: &["determinante", "inversa", "cramer", "matriz"],
            hours: 4.0,
        },
        Lo {
            id: "alg-conicas",
            title: "Cónicas",
            description: "Circunferencia, elipse, parábola, hipérbola, ecuaciones canónicas",
            program: Some("UTN Álgebra"),
            level_min: 12,
            requires: &["alg-rectas-planos"],
            tags: &[
                "conica",
                "elipse",
                "parabola",
                "hiperbola",
                "circunferencia",
            ],
            hours: 5.0,
        },
        Lo {
            id: "alg-transformaciones",
            title: "Transformaciones lineales",
            description: "Núcleo, imagen, matriz asociada, autovalores y autovectores",
            program: Some("UTN Álgebra"),
            level_min: 13,
            requires: &["alg-matrices", "alg-determinantes"],
            tags: &["transformacion", "lineal", "nucleo", "imagen", "autovalor"],
            hours: 6.0,
        },
        // Probabilidad 6
        Lo {
            id: "prob-basica",
            title: "Probabilidad básica",
            description: "Espacio muestral, eventos, probabilidad condicional, Bayes",
            program: Some("UTN Probabilidad"),
            level_min: 11,
            requires: &["sec-prob"],
            tags: &["probabilidad", "muestral", "bayes", "condicional", "evento"],
            hours: 5.0,
        },
        Lo {
            id: "prob-var",
            title: "Variables aleatorias",
            description: "Variables aleatorias discretas y continuas, esperanza, varianza",
            program: Some("UTN Probabilidad"),
            level_min: 12,
            requires: &["prob-basica"],
            tags: &["variable", "aleatoria", "esperanza", "varianza", "discreta"],
            hours: 5.0,
        },
        Lo {
            id: "prob-distribuciones",
            title: "Distribuciones",
            description: "Binomial, Poisson, Normal, exponencial, propiedades",
            program: Some("UTN Probabilidad"),
            level_min: 12,
            requires: &["prob-var"],
            tags: &[
                "distribucion",
                "binomial",
                "poisson",
                "normal",
                "exponencial",
            ],
            hours: 6.0,
        },
        Lo {
            id: "prob-inferencia",
            title: "Inferencia estadística",
            description: "Estimación puntual, intervalos de confianza, test de hipótesis",
            program: Some("UTN Probabilidad"),
            level_min: 13,
            requires: &["prob-distribuciones"],
            tags: &[
                "inferencia",
                "estimacion",
                "confianza",
                "hipotesis",
                "intervalo",
            ],
            hours: 6.0,
        },
        Lo {
            id: "prob-regresion",
            title: "Regresión",
            description: "Regresión lineal, correlación, mínimos cuadrados, predicción",
            program: Some("UTN Probabilidad"),
            level_min: 13,
            requires: &["prob-distribuciones"],
            tags: &["regresion", "correlacion", "lineal", "minimos cuadrados"],
            hours: 5.0,
        },
        Lo {
            id: "prob-muestreo",
            title: "Muestreo",
            description:
                "Técnicas de muestreo, teorema central del límite, distribuciones muestrales",
            program: Some("UTN Probabilidad"),
            level_min: 13,
            requires: &["prob-inferencia"],
            tags: &["muestreo", "central limite", "muestral", "tecnica"],
            hours: 4.0,
        },
    ]
}

fn curriculum_get(id: &str) -> Option<Lo> {
    all_los().into_iter().find(|lo| lo.id == id)
}

fn curriculum_find(concept: &str) -> Vec<Lo> {
    let q = concept.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, Lo)> = all_los()
        .into_iter()
        .filter_map(|lo| {
            let mut score = 0usize;
            if lo.title.to_lowercase().contains(&q) {
                score += 1;
            }
            if lo.description.to_lowercase().contains(&q) {
                score += 1;
            }
            if lo.id.to_lowercase().contains(&q) {
                score += 1;
            }
            score += lo
                .tags
                .iter()
                .filter(|t| t.to_lowercase().contains(&q))
                .count();
            if score > 0 {
                Some((score, lo))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(b.1.title)));
    scored.into_iter().map(|(_, lo)| lo).collect()
}

fn lo_to_json(lo: &Lo) -> Value {
    json!({
        "id": lo.id,
        "title": lo.title,
        "description": lo.description,
        "program": lo.program,
        "level_min": lo.level_min,
        "requires": lo.requires,
        "tags": lo.tags,
        "estimated_hours": lo.hours,
    })
}

// ── Ejercicios deterministas (wyhash, replica pedagogy) ─────────────────────

const WY_CONST: u64 = 0x9E37_79B9_7F4A_7C15;
const WY_CONST2: u64 = 0xBF58_476D_1CE4_E5B9;

fn wyhash(seed: u64) -> u64 {
    seed.wrapping_mul(WY_CONST)
}

fn wyhash2(seed: u64) -> u64 {
    seed.wrapping_mul(WY_CONST).wrapping_add(WY_CONST2)
}

struct GeneratedExercise {
    prompt: String,
    solution: String,
    kind: &'static str,
    validator: String,
    params: BTreeMap<String, f64>,
}

fn generate_exercise_inner(lo_id: &str, seed: u64) -> GeneratedExercise {
    let h0 = wyhash(seed);
    let h1 = wyhash2(h0);
    let h2 = wyhash(h1);
    match lo_id {
        "am1-der" => {
            let a = 1 + (h0 % 3);
            let b = 1 + (h1 % 3);
            let prompt = format!("Deriva f(x)={a}*x^2 + {b}*x en x=1");
            let solution = (2 * a + b).to_string();
            let mut params = BTreeMap::new();
            params.insert("a".to_string(), a as f64);
            params.insert("b".to_string(), b as f64);
            GeneratedExercise {
                prompt,
                solution,
                kind: "Symbolic",
                validator: "NumericTol(0.02)".to_string(),
                params,
            }
        }
        "am1-int" => {
            let a = 1 + (h0 % 3);
            let prompt = format!("Calcula ∫₀¹ {a}*x^2 dx");
            let val = a as f64 / 3.0;
            let solution = format!("{val}");
            let mut params = BTreeMap::new();
            params.insert("a".to_string(), a as f64);
            GeneratedExercise {
                prompt,
                solution,
                kind: "Symbolic",
                validator: "NumericTol(0.02)".to_string(),
                params,
            }
        }
        "sec-trig" => {
            let k = seed % 4;
            let (prompt, solution) = match k {
                0 => ("¿Cuánto vale sin(0)?".to_string(), "0".to_string()),
                1 => ("¿Cuánto vale sin(π/2)?".to_string(), "1".to_string()),
                2 => ("¿Cuánto vale sin(π)?".to_string(), "0".to_string()),
                _ => ("¿Cuánto vale sin(3·π/2)?".to_string(), "-1".to_string()),
            };
            let mut params = BTreeMap::new();
            params.insert("k".to_string(), k as f64);
            params.insert(
                "angle_rad".to_string(),
                k as f64 * std::f64::consts::FRAC_PI_2,
            );
            GeneratedExercise {
                prompt,
                solution,
                kind: "Numeric",
                validator: "NumericTol(0.02)".to_string(),
                params,
            }
        }
        _ => {
            let a = 1 + (h0 % 5);
            let b = 1 + (h1 % 5);
            let c = 1 + (h2 % 5);
            let prompt = format!("Si f(x)={a}*x+{b}, evalúa en x={c}");
            let solution = (a * c + b).to_string();
            let mut params = BTreeMap::new();
            params.insert("a".to_string(), a as f64);
            params.insert("b".to_string(), b as f64);
            params.insert("c".to_string(), c as f64);
            let kind = if lo_id == "am1-lim" || lo_id == "am1-cont" {
                "Symbolic"
            } else if lo_id.starts_with("sec-") || lo_id.starts_with("am1-") {
                "Numeric"
            } else {
                "Graphical"
            };
            GeneratedExercise {
                prompt,
                solution,
                kind,
                validator: "NumericTol(0.02)".to_string(),
                params,
            }
        }
    }
}

// ── Feedback (bien / parcial / mal, replica pedagogy) ───────────────────────

fn normalize_answer(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .replace(' ', "")
        .replace(['·', '×'], "*")
}

fn parse_numeric(raw: &str) -> Option<f64> {
    let t = raw.trim().to_lowercase().replace(' ', "").replace(',', ".");
    if t.is_empty() {
        return None;
    }
    let t = if t.contains('=') {
        t.split('=').next_back().unwrap_or(&t).to_string()
    } else {
        t
    };
    if t.contains('/') {
        let parts: Vec<&str> = t.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                if b.abs() > 1e-12 {
                    return Some(a / b);
                }
            }
        }
        return None;
    }
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    if t == "pi" || t == "π" {
        return Some(std::f64::consts::PI);
    }
    None
}

fn numeric_close(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let diff = (a - b).abs();
    if b.abs() < 1e-9 {
        diff <= 0.02
    } else {
        diff <= 0.02 * b.abs()
    }
}

fn numeric_close_loose(a: f64, b: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let diff = (a - b).abs();
    if b.abs() < 1e-9 {
        diff <= 0.15
    } else {
        diff <= 0.15 * b.abs()
    }
}

fn is_domain_phrase(s: &str) -> bool {
    let low = s.to_lowercase();
    let norm = normalize_answer(s);
    low.contains("no existe")
        || norm.contains("noexiste")
        || norm.contains("infinito")
        || norm.contains("indeterminado")
        || low.contains("no está definida")
        || low.contains("no esta definida")
}

fn diagnose_misconception(prompt: &str, solution: &str, answer: &str) -> &'static str {
    let sol_norm = normalize_answer(solution);
    let ans_norm = normalize_answer(answer);
    if sol_norm.contains('-') != ans_norm.contains('-') {
        return "Sign";
    }
    if let (Some(sv), Some(av)) = (parse_numeric(solution), parse_numeric(answer)) {
        if sv != 0.0 && av != 0.0 && sv.signum() != av.signum() {
            return "Sign";
        }
    }
    if answer.contains('/') && solution.contains('/') {
        match (parse_numeric(solution), parse_numeric(answer)) {
            (Some(sv), Some(av)) => {
                if !numeric_close(av, sv) {
                    return "Fraction";
                }
            }
            _ => return "Fraction",
        }
    }
    let sol_low = solution.to_lowercase();
    let ans_low = answer.to_lowercase();
    let prompt_low = prompt.to_lowercase();
    if prompt_low.contains('(') && prompt_low.contains('+') {
        return "Distributive";
    }
    if ans_norm.contains('+')
        && ans_norm.contains('*')
        && !ans_norm.contains('(')
        && sol_norm.contains('(')
    {
        return "Distributive";
    }
    let sol_trig = sol_low.contains("cos") || sol_low.contains("sin");
    let ans_trig = ans_low.contains("cos") || ans_low.contains("sin");
    if sol_trig && !ans_trig {
        return "ChainRule";
    }
    if is_domain_phrase(answer) != is_domain_phrase(solution) {
        return "Domain";
    }
    let notation = (sol_low.contains('^')
        && !ans_low.contains('^')
        && !ans_low.contains("**")
        && !ans_low.contains('²'))
        || (!sol_low.contains('^') && ans_low.contains('^'))
        || (sol_low.contains("sin") && ans_low.contains("sen"))
        || (sol_low.contains("√") && ans_low.contains("sqrt"))
        || (sol_low.contains("sqrt") && ans_low.contains('√'));
    if notation {
        return "Notation";
    }
    if sol_low.contains("x^2") && ans_low == "x2" {
        return "Notation";
    }
    "Concept"
}

fn misconception_feedback(misconception: &str, solution: &str) -> (String, String) {
    match misconception {
        "Sign" => (
            "Revisá el signo: ¿es positivo o negativo? Cuidado con los menos al distribuir.".to_string(),
            "Repasá am1-der (reglas de signos) y sec-ec.".to_string(),
        ),
        "Fraction" => (
            "Revisá fracciones: no se suman numeradores y denominadores por separado. Buscá común denominador.".to_string(),
            "Repasá sec-fracc (operaciones con fracciones).".to_string(),
        ),
        "Distributive" => (
            "Revisá la propiedad distributiva: (a+b)·c = a·c + b·c, no a + b·c.".to_string(),
            "Repasá sec-prop (propiedad distributiva).".to_string(),
        ),
        "ChainRule" => (
            "Revisá la regla de la cadena: deriva afuera y adentro.".to_string(),
            "Repasá am1-der (regla de la cadena).".to_string(),
        ),
        "Domain" => (
            "Revisá el dominio: ¿la función está definida ahí? ¿existe el límite?".to_string(),
            "Repasá am1-func (dominio) y am1-lim.".to_string(),
        ),
        "Notation" => (
            "Revisá la notación: usá ^ para potencias (x^2), sin/cos en minúscula y paréntesis claros.".to_string(),
            "Repasá notación matemática básica y am1-func.".to_string(),
        ),
        _ => (
            format!("Casi. Esperaba '{solution}', revisá el procedimiento paso a paso."),
            "Repasá la pista socrática y pedí la animación correspondiente.".to_string(),
        ),
    }
}

struct Assessment {
    correct: bool,
    verdict: &'static str,
    misconception: &'static str,
    message: String,
    next_step: String,
}

fn assess_inner(prompt: &str, solution: &str, answer: &str) -> Assessment {
    let sol_norm = normalize_answer(solution);
    let ans_norm = normalize_answer(answer);
    if sol_norm == ans_norm {
        return Assessment {
            correct: true,
            verdict: "bien",
            misconception: "None",
            message: "¡Correcto! Bien razonado.".to_string(),
            next_step: "Probá el siguiente nivel o pedí una animación.".to_string(),
        };
    }
    if let (Some(sv), Some(av)) = (parse_numeric(solution), parse_numeric(answer)) {
        if numeric_close(av, sv) {
            return Assessment {
                correct: true,
                verdict: "bien",
                misconception: "None",
                message: "¡Correcto! Bien razonado.".to_string(),
                next_step: "Probá el siguiente nivel o pedí una animación.".to_string(),
            };
        }
        if numeric_close_loose(av, sv) {
            let misc = diagnose_misconception(prompt, solution, answer);
            let (mut message, next_step) = misconception_feedback(misc, solution);
            message = format!("Parcial: estás cerca ({answer} vs {solution}). {message}");
            return Assessment {
                correct: false,
                verdict: "parcial",
                misconception: misc,
                message,
                next_step,
            };
        }
    }
    let misc = diagnose_misconception(prompt, solution, answer);
    // Notación cercana (x2 vs x^2) se considera parcial, no mal total.
    if misc == "Notation" {
        let (base, next_step) = misconception_feedback(misc, solution);
        return Assessment {
            correct: false,
            verdict: "parcial",
            misconception: misc,
            message: format!("Parcial: la idea es correcta pero la notación no. {base}"),
            next_step,
        };
    }
    let (message, next_step) = misconception_feedback(misc, solution);
    Assessment {
        correct: false,
        verdict: "mal",
        misconception: misc,
        message,
        next_step,
    }
}

// ── Scaffold ────────────────────────────────────────────────────────────────

fn scaffold_inner(concept: &str, level: Level) -> (String, Option<String>, String) {
    match level {
        Level::Primary => (
            format!("¿Te imaginás {concept} como algo que ves todos los días? ¿Qué forma tiene?"),
            Some(format!("Pensá en {concept} con un dibujo. ¿Sube o baja?")),
            format!("{concept} es como describir cómo cambia algo. En primaria lo vemos con ejemplos y gráficos simples."),
        ),
        Level::Secondary => (
            format!("¿Qué representa {concept} en el gráfico de y = x²?"),
            Some("Mirá la pendiente de la tangente o el área bajo la curva".to_string()),
            format!("En secundaria, {concept} aparece como pendiente (derivada) o área (integral). Lo vemos en el canvas y con animación."),
        ),
        _ => (
            format!("¿Cómo definirías {concept} formalmente con límites?"),
            Some("Recordá la definición por límite: f'(x)=lim_{{h→0}} [f(x+h)-f(x)]/h".to_string()),
            format!("Formalmente, {concept} se define vía límites y se demuestra con el teorema del valor medio. Grafito puede mostrar la derivada, la tangente y la serie de Taylor."),
        ),
    }
}

// ── Animación (replica protocol.rs) ─────────────────────────────────────────

fn normalize_concept(concept: &str) -> String {
    let tmp = concept.trim().replace(['\n', '\r', '\t'], " ");
    let mut out = String::with_capacity(tmp.len());
    let mut prev_space = false;
    for ch in tmp.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if out.trim().is_empty() {
        return "matemática".to_string();
    }
    if out.len() > 500 {
        out.chars().take(500).collect()
    } else {
        out
    }
}

fn template_for_concept(concept: &str) -> &'static str {
    let c = concept.to_lowercase();
    if c.contains("pitágoras")
        || c.contains("pitagoras")
        || c.contains("pythag")
        || (c.contains("triang") && (c.contains("rect") || c.contains("hipoten")))
    {
        return "pitagoras";
    }
    if c.contains("integral")
        || c.contains("área")
        || (c.contains("area")
            && (c.contains("bajo") || c.contains("curva") || c.contains("riemann")))
    {
        return "integral-area";
    }
    if c.contains("taylor")
        || c.contains("maclaurin")
        || (c.contains("serie") && (c.contains("potencia") || c.contains("aprox")))
        || c.contains("aproxima")
    {
        return "taylor-series";
    }
    if c.contains("conformal")
        || c.contains("conforme")
        || c.contains("complej")
        || c.contains("complex")
        || c.contains("fractal")
        || c.contains("mandelb")
    {
        return "conformal-map";
    }
    if c.contains("deriv")
        || c.contains("pendiente")
        || c.contains("tangente")
        || c.contains("slope")
    {
        return "derivative-slope";
    }
    if c.contains("vector") {
        return "conformal-map";
    }
    if c.contains("euler") || c.contains("exponencial") || c.contains("exp(") {
        return "euler";
    }
    if c.contains("fourier") || c.contains("armónico") || c.contains("armonico") {
        return "fourier";
    }
    if c.contains("probab") || c.contains("binom") || c.contains("distrib") || c.contains("estad") {
        return "integral-area";
    }
    if c.contains("sin(") || c.contains("cos(") || c.contains("seno") || c.contains("coseno") {
        return "taylor-series";
    }
    "derivative-slope"
}

fn sanitize_template(template: &str, concept: &str) -> String {
    let t = template.trim().to_lowercase();
    match t.as_str() {
        "derivative-slope" | "integral-area" | "taylor-series" | "conformal-map" | "pitagoras"
        | "euler" | "fourier" => t,
        "pythagoras" => "pitagoras".to_string(),
        "" | "universal" | "auto" => template_for_concept(concept).to_string(),
        _ => template_for_concept(concept).to_string(),
    }
}

/// Valida resolución 64..=4096 por lado (igual que `Resolution::try_new`).
pub fn is_valid_resolution(width: u32, height: u32) -> bool {
    (MIN_CANVAS..=MAX_CANVAS).contains(&(width as u64))
        && (MIN_CANVAS..=MAX_CANVAS).contains(&(height as u64))
}

/// Valida duración 0.1..=30 s (igual que `AnimDuration::try_new`).
pub fn is_valid_duration_secs(secs: f64) -> bool {
    secs.is_finite() && (0.1..=30.0).contains(&secs)
}

/// Comprueba que la plantilla existe en el motor nativo.
pub fn is_known_template(template: &str) -> bool {
    KNOWN_TEMPLATES.contains(&template)
}

fn canvas_from_call(call: &ToolCall) -> (u32, u32) {
    if let Some(arr) = call.arguments.get("canvas").and_then(Value::as_array) {
        if arr.len() == 2 {
            if let (Some(w), Some(h)) = (arr[0].as_u64(), arr[1].as_u64()) {
                if (MIN_CANVAS..=MAX_CANVAS).contains(&w) && (MIN_CANVAS..=MAX_CANVAS).contains(&h)
                {
                    return (w as u32, h as u32);
                }
            }
        }
    }
    let mut w_opt = call
        .arguments
        .get("width")
        .and_then(Value::as_u64)
        .filter(|v| (MIN_CANVAS..=MAX_CANVAS).contains(v))
        .map(|v| v as u32);
    let mut h_opt = call
        .arguments
        .get("height")
        .and_then(Value::as_u64)
        .filter(|v| (MIN_CANVAS..=MAX_CANVAS).contains(v))
        .map(|v| v as u32);
    if let Some(obj) = call.arguments.get("params").and_then(Value::as_object) {
        if w_opt.is_none() {
            w_opt = obj
                .get("width")
                .or_else(|| obj.get("canvas_width"))
                .and_then(Value::as_u64)
                .filter(|v| (MIN_CANVAS..=MAX_CANVAS).contains(v))
                .map(|v| v as u32);
        }
        if h_opt.is_none() {
            h_opt = obj
                .get("height")
                .or_else(|| obj.get("canvas_height"))
                .and_then(Value::as_u64)
                .filter(|v| (MIN_CANVAS..=MAX_CANVAS).contains(v))
                .map(|v| v as u32);
        }
    }
    match (w_opt, h_opt) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, DEFAULT_CANVAS.1),
        (None, Some(h)) => (DEFAULT_CANVAS.0, h),
        (None, None) => DEFAULT_CANVAS,
    }
}

// ── Evaluador de expresiones ────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    LParen,
    RParen,
    Comma,
}

fn tokenize(expr: &str) -> Result<Vec<Token>, ToolError> {
    if expr.trim().is_empty() {
        return Err(ToolError::FaltaCampo {
            campo: "expression",
        });
    }
    if expr.len() > MAX_ARG_BYTES {
        return Err(ToolError::PresupuestoExcedido(format!(
            "la expresión excede {MAX_ARG_BYTES} bytes"
        )));
    }
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                let mut j = i + 1;
                if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
                    j += 1;
                }
                if j < chars.len() && chars[j].is_ascii_digit() {
                    i = j;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = chars[start..i].iter().collect();
            match text.parse::<f64>() {
                Ok(v) if v.is_finite() => tokens.push(Token::Number(v)),
                Ok(_) => {
                    return Err(ToolError::CampoInvalido {
                        campo: "expression",
                        motivo: "literal numérico no finito".to_string(),
                    });
                }
                Err(_) => {
                    return Err(ToolError::CampoInvalido {
                        campo: "expression",
                        motivo: format!("número inválido '{text}'"),
                    });
                }
            }
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == 'π' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == 'π')
            {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident(name));
            continue;
        }
        match c {
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '^' => tokens.push(Token::Caret),
            '%' => tokens.push(Token::Percent),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            ',' => tokens.push(Token::Comma),
            _ => {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: format!("carácter inválido '{c}'"),
                });
            }
        }
        i += 1;
    }
    if tokens.is_empty() {
        return Err(ToolError::FaltaCampo {
            campo: "expression",
        });
    }
    Ok(tokens)
}

struct ExprParser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    vars: &'a BTreeMap<String, f64>,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next_is_lparen(&self) -> bool {
        matches!(self.tokens.get(self.pos + 1), Some(Token::LParen))
    }

    fn parse_expr(&mut self) -> Result<f64, ToolError> {
        let mut val = self.parse_term()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.pos += 1;
                    val += self.parse_term()?;
                }
                Some(Token::Minus) => {
                    self.pos += 1;
                    val -= self.parse_term()?;
                }
                _ => break,
            }
        }
        Ok(val)
    }

    fn parse_term(&mut self) -> Result<f64, ToolError> {
        let mut val = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(Token::Star) => {
                    self.pos += 1;
                    val *= self.parse_factor()?;
                }
                Some(Token::Slash) => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    if rhs == 0.0 {
                        return Err(ToolError::EvaluacionFallida(
                            "división por cero".to_string(),
                        ));
                    }
                    val /= rhs;
                }
                Some(Token::Percent) => {
                    self.pos += 1;
                    let rhs = self.parse_factor()?;
                    if rhs == 0.0 {
                        return Err(ToolError::EvaluacionFallida("módulo por cero".to_string()));
                    }
                    val %= rhs;
                }
                _ => break,
            }
        }
        Ok(val)
    }

    fn parse_factor(&mut self) -> Result<f64, ToolError> {
        let base = self.parse_unary()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.pos += 1;
            let exp = self.parse_factor()?;
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<f64, ToolError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.pos += 1;
                Ok(-self.parse_unary()?)
            }
            Some(Token::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<f64, ToolError> {
        match self.peek().cloned() {
            Some(Token::Number(v)) => {
                self.pos += 1;
                Ok(v)
            }
            Some(Token::Ident(name)) => {
                if self.next_is_lparen() {
                    self.parse_call(&name)
                } else {
                    self.pos += 1;
                    if let Some(v) = self.vars.get(&name) {
                        if v.is_finite() {
                            Ok(*v)
                        } else {
                            Err(ToolError::CampoInvalido {
                                campo: "variables",
                                motivo: format!("variable '{name}' no finita"),
                            })
                        }
                    } else if name.eq_ignore_ascii_case("pi") || name == "π" {
                        Ok(std::f64::consts::PI)
                    } else if name == "e" || name == "E" {
                        Ok(std::f64::consts::E)
                    } else {
                        Err(ToolError::CampoInvalido {
                            campo: "expression",
                            motivo: format!("variable desconocida '{name}'"),
                        })
                    }
                }
            }
            Some(Token::LParen) => {
                self.pos += 1;
                let v = self.parse_expr()?;
                match self.peek() {
                    Some(Token::RParen) => {
                        self.pos += 1;
                        Ok(v)
                    }
                    _ => Err(ToolError::CampoInvalido {
                        campo: "expression",
                        motivo: "falta ')' de cierre".to_string(),
                    }),
                }
            }
            _ => Err(ToolError::CampoInvalido {
                campo: "expression",
                motivo: "se esperaba número, variable o '('".to_string(),
            }),
        }
    }

    fn parse_call(&mut self, name: &str) -> Result<f64, ToolError> {
        // consume ident + '('
        self.pos += 1;
        match self.peek() {
            Some(Token::LParen) => self.pos += 1,
            _ => {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: format!("llamada a '{name}' sin '('"),
                });
            }
        }
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Token::RParen)) {
            self.pos += 1;
        } else {
            loop {
                args.push(self.parse_expr()?);
                match self.peek() {
                    Some(Token::Comma) => {
                        self.pos += 1;
                    }
                    Some(Token::RParen) => {
                        self.pos += 1;
                        break;
                    }
                    _ => {
                        return Err(ToolError::CampoInvalido {
                            campo: "expression",
                            motivo: format!("llamada a '{name}' sin ')' de cierre"),
                        });
                    }
                }
                if args.len() > 4 {
                    return Err(ToolError::CampoInvalido {
                        campo: "expression",
                        motivo: format!("demasiados argumentos en '{name}'"),
                    });
                }
            }
        }
        eval_function(name, &args)
    }
}

fn eval_function(name: &str, args: &[f64]) -> Result<f64, ToolError> {
    let lower = name.to_lowercase();
    let one = |f: fn(f64) -> f64| -> Result<f64, ToolError> {
        if args.len() != 1 {
            return Err(ToolError::CampoInvalido {
                campo: "expression",
                motivo: format!("la función '{name}' espera 1 argumento"),
            });
        }
        Ok(f(args[0]))
    };
    match lower.as_str() {
        "sin" | "sen" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "asin" | "arcsin" => one(f64::asin),
        "acos" | "arccos" => one(f64::acos),
        "atan" | "arctan" => one(f64::atan),
        "sinh" => one(f64::sinh),
        "cosh" => one(f64::cosh),
        "tanh" => one(f64::tanh),
        "sqrt" => {
            if args.len() != 1 {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'sqrt' espera 1 argumento".to_string(),
                });
            }
            if args[0] < 0.0 {
                return Err(ToolError::EvaluacionFallida(
                    "sqrt de número negativo (dominio)".to_string(),
                ));
            }
            Ok(args[0].sqrt())
        }
        "cbrt" => one(|v| v.cbrt()),
        "exp" => one(f64::exp),
        "ln" | "log" => {
            if args.len() == 2 {
                // log(valor, base)
                if args[0] <= 0.0 || args[1] <= 0.0 || args[1] == 1.0 {
                    return Err(ToolError::EvaluacionFallida(
                        "log con dominio inválido".to_string(),
                    ));
                }
                Ok(args[0].ln() / args[1].ln())
            } else if args.len() == 1 {
                if args[0] <= 0.0 {
                    return Err(ToolError::EvaluacionFallida(
                        "ln de número no positivo (dominio)".to_string(),
                    ));
                }
                Ok(args[0].ln())
            } else {
                Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'log' espera 1 o 2 argumentos".to_string(),
                })
            }
        }
        "log10" => {
            if args.len() != 1 {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'log10' espera 1 argumento".to_string(),
                });
            }
            if args[0] <= 0.0 {
                return Err(ToolError::EvaluacionFallida(
                    "log10 de número no positivo (dominio)".to_string(),
                ));
            }
            Ok(args[0].log10())
        }
        "abs" => one(f64::abs),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "round" => one(f64::round),
        "trunc" => one(f64::trunc),
        "pow" => {
            if args.len() != 2 {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'pow' espera 2 argumentos".to_string(),
                });
            }
            Ok(args[0].powf(args[1]))
        }
        "max" => {
            if args.len() != 2 {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'max' espera 2 argumentos".to_string(),
                });
            }
            Ok(args[0].max(args[1]))
        }
        "min" => {
            if args.len() != 2 {
                return Err(ToolError::CampoInvalido {
                    campo: "expression",
                    motivo: "la función 'min' espera 2 argumentos".to_string(),
                });
            }
            Ok(args[0].min(args[1]))
        }
        _ => Err(ToolError::CampoInvalido {
            campo: "expression",
            motivo: format!("función desconocida '{name}'"),
        }),
    }
}

fn evaluate_expression(expr: &str, vars: &BTreeMap<String, f64>) -> Result<f64, ToolError> {
    let tokens = tokenize(expr)?;
    let mut parser = ExprParser {
        tokens,
        pos: 0,
        vars,
    };
    let value = parser.parse_expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(ToolError::CampoInvalido {
            campo: "expression",
            motivo: "símbolo inesperado al final".to_string(),
        });
    }
    if !value.is_finite() {
        return Err(ToolError::EvaluacionFallida(
            "la expresión dio un valor no finito".to_string(),
        ));
    }
    Ok(value)
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

// ── Catálogo de docs ────────────────────────────────────────────────────────

struct DocEntry {
    canonical: &'static str,
    syntax: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
}

const DOC_CATALOG: &[DocEntry] = &[
    DocEntry {
        canonical: "Function",
        syntax: "y = x^2 - 1",
        description: "Grafica una función explícita y = f(x) en 2D",
        keywords: &["funcion", "grafic", "curva", "polinomio"],
    },
    DocEntry {
        canonical: "ParametricCurve2D",
        syntax: "x = cos(t), y = sin(t), t in [0, 2*pi]",
        description: "Curva paramétrica 2D con parámetro t",
        keywords: &["parametrica", "curva", "trayectoria"],
    },
    DocEntry {
        canonical: "PolarCurve",
        syntax: "r = 1 + cos(theta)",
        description: "Curva polar r(theta)",
        keywords: &["polar", "rosa", "cardioide", "espiral"],
    },
    DocEntry {
        canonical: "ImplicitCurve",
        syntax: "x^2 + y^2 = 1",
        description: "Curva implícita lhs = rhs",
        keywords: &["implicita", "circunferencia", "elipse", "conica"],
    },
    DocEntry {
        canonical: "VectorField2D",
        syntax: "F(x,y) = (-y, x)",
        description: "Campo vectorial 2D",
        keywords: &["vectorial", "campo", "flujo", "fase"],
    },
    DocEntry {
        canonical: "DomainColoring",
        syntax: "dcolor(f(z) = 1/z)",
        description: "Coloreado de dominio para funciones complejas",
        keywords: &["complejo", "dcolor", "dominio", "conforme"],
    },
    DocEntry {
        canonical: "Point",
        syntax: "P = (1, 2)",
        description: "Punto 2D",
        keywords: &["punto", "coordenada", "marca"],
    },
    DocEntry {
        canonical: "Circle",
        syntax: "Circle(C=(0,0), r=1)",
        description: "Circunferencia por centro y radio",
        keywords: &["circulo", "circunferencia", "radio", "conica"],
    },
    DocEntry {
        canonical: "Line",
        syntax: "Line(A=(0,0), B=(1,1))",
        description: "Recta por dos puntos",
        keywords: &["recta", "linea", "pendiente"],
    },
    DocEntry {
        canonical: "Segment",
        syntax: "Segment(A=(0,0), B=(2,0))",
        description: "Segmento entre dos puntos",
        keywords: &["segmento", "distancia", "lado"],
    },
    DocEntry {
        canonical: "Vector",
        syntax: "Vector(origen=(0,0), fin=(1,2))",
        description: "Vector geométrico con origen y fin",
        keywords: &["vector", "flecha", "componente"],
    },
    DocEntry {
        canonical: "Tangent",
        syntax: "Tangent(curva, punto)",
        description: "Recta tangente a una curva en un punto",
        keywords: &["tangente", "derivada", "pendiente"],
    },
    DocEntry {
        canonical: "Histogram",
        syntax: "Histogram(datos=[1,2,2,3])",
        description: "Histograma de datos univariados",
        keywords: &["histograma", "datos", "estadistica", "distribucion"],
    },
    DocEntry {
        canonical: "ScatterPlot",
        syntax: "Scatter(puntos=[(0,0),(1,1)])",
        description: "Nube de puntos",
        keywords: &["dispersion", "puntos", "datos", "regresion"],
    },
    DocEntry {
        canonical: "LinearRegression",
        syntax: "Regresion(puntos=[(0,0),(1,1)])",
        description: "Recta de regresión lineal por mínimos cuadrados",
        keywords: &["regresion", "lineal", "correlacion", "ajuste"],
    },
    DocEntry {
        canonical: "Surface3D",
        syntax: "z = sin(x)*cos(y)",
        description: "Superficie 3D z = f(x,y)",
        keywords: &["superficie", "3d", "malla", "relieve"],
    },
    DocEntry {
        canonical: "Sphere",
        syntax: "Sphere(C=(0,0,0), r=1)",
        description: "Esfera 3D",
        keywords: &["esfera", "3d", "volumen"],
    },
    DocEntry {
        canonical: "Mandelbrot",
        syntax: "Mandelbrot(iter=100)",
        description: "Conjunto de Mandelbrot (fractal)",
        keywords: &["fractal", "mandelbrot", "complejo", "julia"],
    },
];

fn docs_catalog_search(query: &str, max_bytes: usize) -> String {
    let q = query.to_lowercase();
    let terms: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3 || *t == "3d" || *t == "4d")
        .map(str::to_owned)
        .collect();
    if terms.is_empty() {
        return String::new();
    }
    let mut scored: Vec<(usize, &DocEntry)> = DOC_CATALOG
        .iter()
        .filter_map(|entry| {
            let haystack = format!(
                "{} {} {} {}",
                entry.canonical.to_lowercase(),
                entry.syntax.to_lowercase(),
                entry.description.to_lowercase(),
                entry.keywords.join(" ")
            );
            let mut score = 0usize;
            for term in &terms {
                if haystack.contains(term) {
                    score += 1;
                }
            }
            // Boost cálculo -> Function ejecutable
            if [
                "integral", "derivada", "derivar", "deriva", "taylor", "serie",
            ]
            .iter()
            .any(|t| q.contains(*t))
                && entry.canonical == "Function"
            {
                score += 2;
            }
            if score > 0 {
                Some((score, entry))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.canonical.cmp(b.1.canonical)));
    let mut out = String::new();
    for (_, entry) in scored {
        let line = format!(
            "{}: {} — {}",
            entry.canonical, entry.syntax, entry.description
        );
        let sep = usize::from(!out.is_empty());
        if out.len() + sep + line.len() > max_bytes {
            break;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }
    out
}

// ── Tools base ──────────────────────────────────────────────────────────────

fn evaluate_expr_tool(call: &ToolCall) -> ToolResult {
    let Some(expression) = string_arg(call, "expression") else {
        return err_result(
            &call.id,
            ToolError::FaltaCampo {
                campo: "expression",
            },
        );
    };
    let mut vars = BTreeMap::new();
    if let Some(obj) = call.arguments.get("variables").and_then(Value::as_object) {
        for (k, v) in obj {
            if let Some(n) = v.as_f64() {
                if n.is_finite() {
                    vars.insert(k.clone(), n);
                }
            }
        }
    }
    match evaluate_expression(&expression, &vars) {
        Ok(v) => ToolResult::text(&call.id, true, format_number(v)),
        Err(e) => err_result(&call.id, e),
    }
}

fn grafito_docs_tool(call: &ToolCall) -> ToolResult {
    let Some(query) = string_arg(call, "query") else {
        return err_result(&call.id, ToolError::FaltaCampo { campo: "query" });
    };
    let catalog = docs_catalog_search(&query, 2_048);
    if catalog.trim().is_empty() {
        return err_result(
            &call.id,
            ToolError::NoEncontrado(format!("sin comandos catalogados para '{query}'")),
        );
    }
    ToolResult::text(&call.id, true, catalog)
}

fn ask_user_tool(call: &ToolCall) -> ToolResult {
    let Some(question) = string_arg(call, "question") else {
        return err_result(&call.id, ToolError::FaltaCampo { campo: "question" });
    };
    let _ = question;
    ToolResult::text(
        &call.id,
        false,
        "ask_user requiere una respuesta explícita del usuario en el chat de Grafito; repítela como pregunta de aclaración en lugar de ejecutarla en silencio (E_CONSENTIMIENTO)".to_string(),
    )
}

// ── Tools pedagógicas ───────────────────────────────────────────────────────

fn scaffold_tool(call: &ToolCall) -> ToolResult {
    let Some(concept) = string_arg(call, "concept") else {
        return err_result(&call.id, ToolError::FaltaCampo { campo: "concept" });
    };
    let level = parse_level(string_arg(call, "level").as_deref());
    let (question, hint, explanation) = scaffold_inner(concept.trim(), level);
    let payload = json!({
        "concept": concept.trim(),
        "level": level_label(level),
        "question": question,
        "hint": hint,
        "explanation": explanation,
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

fn resolve_lo(call: &ToolCall) -> Result<Lo, ToolError> {
    let lo_id = string_arg(call, "lo_id")
        .or_else(|| string_arg(call, "learning_objective_id"))
        .or_else(|| string_arg(call, "exercise_id"))
        .or_else(|| string_arg(call, "id"))
        .or_else(|| string_arg(call, "concept"));
    let Some(lo_id) = lo_id else {
        return Err(ToolError::FaltaCampo { campo: "lo_id" });
    };
    if let Some(lo) = curriculum_get(lo_id.trim()) {
        return Ok(lo);
    }
    let mut candidates = curriculum_find(&lo_id);
    if candidates.is_empty() {
        return Err(ToolError::NoEncontrado(format!(
            "LearningObjective no encontrado: '{lo_id}'"
        )));
    }
    Ok(candidates.remove(0))
}

fn generate_exercise_tool(call: &ToolCall) -> ToolResult {
    let lo = match resolve_lo(call) {
        Ok(lo) => lo,
        Err(e) => return err_result(&call.id, e),
    };
    let level = parse_level(string_arg(call, "level").as_deref());
    let seed = call
        .arguments
        .get("seed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let gen = generate_exercise_inner(lo.id, seed);
    if gen.prompt.trim().is_empty() || gen.solution.trim().is_empty() {
        return err_result(
            &call.id,
            ToolError::EvaluacionFallida("ejercicio incompleto".to_string()),
        );
    }
    if gen.prompt.len() > 500 || gen.solution.len() > 500 {
        return err_result(
            &call.id,
            ToolError::PresupuestoExcedido("ejercicio demasiado largo".to_string()),
        );
    }
    let payload = json!({
        "lo_id": lo.id,
        "prompt": gen.prompt,
        "solution": gen.solution,
        "kind": gen.kind,
        "difficulty": level_difficulty(level),
        "params": gen.params,
        "seed": seed,
        "validator": gen.validator,
        "level": level_label(level),
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

fn assess_answer_tool(call: &ToolCall) -> ToolResult {
    let Some(answer) = string_arg(call, "answer") else {
        return err_result(&call.id, ToolError::FaltaCampo { campo: "answer" });
    };
    let lo_id_opt = string_arg(call, "exercise_id")
        .or_else(|| string_arg(call, "lo_id"))
        .or_else(|| string_arg(call, "learning_objective_id"))
        .or_else(|| string_arg(call, "id"));
    if let Some(lo_id) = lo_id_opt {
        let lo = if let Some(found) = curriculum_get(lo_id.trim()) {
            found
        } else {
            let mut c = curriculum_find(&lo_id);
            if c.is_empty() {
                return err_result(
                    &call.id,
                    ToolError::NoEncontrado(format!(
                        "LearningObjective no encontrado para assess: '{lo_id}'"
                    )),
                );
            }
            c.remove(0)
        };
        let seed = call
            .arguments
            .get("seed")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let level = parse_level(string_arg(call, "level").as_deref());
        let _ = level;
        let gen = generate_exercise_inner(lo.id, seed);
        let a = assess_inner(&gen.prompt, &gen.solution, &answer);
        let payload = json!({
            "lo_id": lo.id,
            "exercise_prompt": gen.prompt,
            "expected": gen.solution,
            "answer": answer,
            "correct": a.correct,
            "veredicto": a.verdict,
            "misconception": a.misconception,
            "message": a.message,
            "next_step": a.next_step,
        });
        ToolResult::text(&call.id, true, payload.to_string())
    } else {
        let payload = json!({
            "answer": answer,
            "correct": false,
            "veredicto": "mal",
            "misconception": "Concept",
            "message": "No se proporcionó exercise_id/lo_id; no se puede validar contra un ejercicio concreto. Provee lo_id para evaluación precisa.",
            "next_step": "Provee lo_id (ej. am1-der) junto a answer para evaluación exacta.",
            "hint": "Ejemplo: assess_answer {\"lo_id\": \"am1-der\", \"answer\": \"2*x+1\"}"
        });
        ToolResult::text(&call.id, false, payload.to_string())
    }
}

fn get_curriculum_tool(call: &ToolCall) -> ToolResult {
    let query = string_arg(call, "query")
        .or_else(|| string_arg(call, "concept"))
        .or_else(|| string_arg(call, "q"))
        .unwrap_or_default();
    if query.trim().is_empty() {
        return err_result(&call.id, ToolError::FaltaCampo { campo: "query" });
    }
    let results = curriculum_find(&query);
    if results.is_empty() {
        return err_result(
            &call.id,
            ToolError::NoEncontrado(format!("sin resultados para '{query}'")),
        );
    }
    let items: Vec<Value> = results
        .into_iter()
        .take(5)
        .map(|lo| lo_to_json(&lo))
        .collect();
    let payload = json!({
        "query": query,
        "count": items.len(),
        "results": items,
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

fn suggest_next_tool(call: &ToolCall) -> ToolResult {
    let _ = string_arg(call, "branch_id");
    // Perfil mock determinista ordenado por mastery ascendente (más débil primero),
    // enlazado a LOs reales del currículum.
    let mut branches: Vec<(&str, &str, f64, bool, u32, u64)> = vec![
        ("am1-func", "Funciones", 0.6, false, 2, 0),
        ("am1-lim", "Límites", 0.2, false, 1, 0),
        ("am1-der", "Derivadas", 0.0, false, 1, 0),
    ];
    branches.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let items: Vec<Value> = branches
        .iter()
        .take(3)
        .map(|(id, name, mastery, covered, box_level, next_review)| {
            let lo_title = curriculum_get(id)
                .map(|lo| lo.title.to_string())
                .unwrap_or_else(|| (*name).to_string());
            let lo_desc = curriculum_get(id)
                .map(|lo| lo.description.to_string())
                .unwrap_or_default();
            json!({
                "id": id,
                "name": name,
                "title": lo_title,
                "description": lo_desc,
                "mastery": mastery,
                "covered": covered,
                "box_level": box_level,
                "next_review_epoch": next_review,
            })
        })
        .collect();
    if items.is_empty() {
        return err_result(
            &call.id,
            ToolError::NoEncontrado("perfil mock vacío; sin sugerencias".to_string()),
        );
    }
    let payload = json!({
        "mock": true,
        "count": items.len(),
        "next": items,
        "note": "perfil mock puro; en la app real se usa StudentProfile persistido (recommend_next)",
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

fn generate_animation_tool(call: &ToolCall) -> ToolResult {
    let template_raw = string_arg(call, "template").unwrap_or_default();
    let concept_raw = string_arg(call, "concept").unwrap_or_default();
    let mut params_map = BTreeMap::new();
    if let Some(obj) = call.arguments.get("params").and_then(Value::as_object) {
        for (k, v) in obj {
            if let Some(n) = v.as_f64() {
                if n.is_finite() {
                    params_map.insert(k.clone(), n);
                }
            }
        }
    }
    if template_raw.trim().is_empty() && concept_raw.trim().is_empty() {
        return err_result(
            &call.id,
            ToolError::FaltaCampo {
                campo: "concept/template",
            },
        );
    }
    let concept = if concept_raw.trim().is_empty() {
        template_raw.clone()
    } else {
        concept_raw.clone()
    };
    let template = sanitize_template(&template_raw, &concept);
    if !is_known_template(&template) {
        return err_result(
            &call.id,
            ToolError::CampoInvalido {
                campo: "template",
                motivo: format!("plantilla desconocida '{template}'"),
            },
        );
    }
    let concept_norm = normalize_concept(&concept);
    if template.len() > 64 {
        return err_result(
            &call.id,
            ToolError::PresupuestoExcedido("plantilla excede 64 caracteres".to_string()),
        );
    }
    let canvas = canvas_from_call(call);
    if !is_valid_resolution(canvas.0, canvas.1) {
        return err_result(
            &call.id,
            ToolError::CampoInvalido {
                campo: "canvas",
                motivo: format!("resolución {}x{} fuera de 64..=4096", canvas.0, canvas.1),
            },
        );
    }
    // Duración fija válida 2.0 s (0.1..=30) -> 2000 ms (100..=30000).
    if !is_valid_duration_secs(2.0)
        || DEFAULT_DURATION_MS < MIN_DURATION_MS
        || DEFAULT_DURATION_MS > MAX_DURATION_MS
    {
        return err_result(
            &call.id,
            ToolError::CampoInvalido {
                campo: "duration",
                motivo: "duración fuera de rango".to_string(),
            },
        );
    }
    let payload = json!({
        "template": template,
        "concept": concept_norm,
        "params": params_map,
        "export": "gif",
        "canvas": [canvas.0, canvas.1],
        "duration_ms": DEFAULT_DURATION_MS,
        "protocol_version": ANIM_PROTOCOL_VERSION,
        "note": "solicitud validada; el motor de animación se ejecuta en la capa UI tras aprobación explícita"
    });
    ToolResult::text(&call.id, true, payload.to_string())
}

// ── Dispatchers ─────────────────────────────────────────────────────────────

/// Despachador completo (3 base + 6 pedagógicas).
#[derive(Debug, Default, Clone, Copy)]
pub struct SafeGrafitoDispatcher;

impl ToolDispatcher for SafeGrafitoDispatcher {
    fn dispatch(&self, call: &ToolCall) -> ToolResult {
        dispatch_safe_tool(call)
    }
}

/// Despachador solo pedagógico (6 tools).
#[derive(Debug, Default, Clone, Copy)]
pub struct PedagogyDispatcher;

impl ToolDispatcher for PedagogyDispatcher {
    fn dispatch(&self, call: &ToolCall) -> ToolResult {
        dispatch_pedagogy_tool(call)
    }
}

fn dispatch_safe_tool(call: &ToolCall) -> ToolResult {
    if let Some(rejected) = reject_oversized_string_args(call) {
        return rejected;
    }
    match call.name.as_str() {
        "evaluate_expr" => evaluate_expr_tool(call),
        "grafito_docs" => grafito_docs_tool(call),
        "ask_user" => ask_user_tool(call),
        "scaffold" => scaffold_tool(call),
        "generate_exercise" => generate_exercise_tool(call),
        "assess_answer" => assess_answer_tool(call),
        "get_curriculum" => get_curriculum_tool(call),
        "suggest_next" => suggest_next_tool(call),
        "generate_animation" => generate_animation_tool(call),
        unknown => ToolResult::text(
            &call.id,
            false,
            format!("tool '{unknown}' no disponible en esta sesión (E_NO_ENCONTRADO)"),
        ),
    }
}

fn dispatch_pedagogy_tool(call: &ToolCall) -> ToolResult {
    if let Some(rejected) = reject_oversized_string_args(call) {
        return rejected;
    }
    match call.name.as_str() {
        "scaffold" => scaffold_tool(call),
        "generate_exercise" => generate_exercise_tool(call),
        "assess_answer" => assess_answer_tool(call),
        "get_curriculum" => get_curriculum_tool(call),
        "suggest_next" => suggest_next_tool(call),
        "generate_animation" => generate_animation_tool(call),
        unknown => ToolResult::text(
            &call.id,
            false,
            format!("pedagogy tool '{unknown}' no disponible; tools válidas: scaffold, generate_exercise, assess_answer, get_curriculum, suggest_next, generate_animation (E_NO_ENCONTRADO)"),
        ),
    }
}

// ── Schemas OpenAI-compat ───────────────────────────────────────────────────

/// Schema de `evaluate_expr`.
#[must_use]
pub fn evaluate_expr_schema() -> ToolSchema {
    ToolSchema::new(
        "evaluate_expr",
        "Evalúa una expresión matemática con variables opcionales; devuelve un número finito o un error de dominio.",
        json!({
            "type": "object",
            "properties": {
                "expression": {"type": "string", "description": "Expresión a evaluar, ej. 2+2, sin(0), 2*x+1"},
                "variables": {"type": "object", "description": "Mapa opcional de variables numéricas", "additionalProperties": {"type": "number"}}
            },
            "required": ["expression"]
        }),
    )
}

/// Schema de `grafito_docs`.
#[must_use]
pub fn grafito_docs_schema() -> ToolSchema {
    ToolSchema::new(
        "grafito_docs",
        "Devuelve el catálogo acotado de comandos verificados de Grafito que coinciden con una consulta.",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Consulta en lenguaje natural, ej. graficar función, recta, histograma"}
            },
            "required": ["query"]
        }),
    )
}

/// Schema de `ask_user` (requiere consentimiento).
#[must_use]
pub fn ask_user_schema() -> ToolSchema {
    ToolSchema::new(
        "ask_user",
        "Hace una única pregunta corta de aclaración matemática al usuario cuando falta un valor obligatorio.",
        json!({
            "type": "object",
            "properties": {
                "question": {"type": "string", "description": "Pregunta de aclaración para el usuario"}
            },
            "required": ["question"]
        }),
    )
    .with_consent(true)
}

/// Schema de `scaffold(concept, level)`.
#[must_use]
pub fn scaffold_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "scaffold",
        "Genera un andamiaje socrático puro para un concepto y nivel (pregunta, pista, explicación) sin mutar el documento.",
        json!({
            "type": "object",
            "properties": {
                "concept": {"type": "string", "description": "Concepto a andamiar, ej. derivada, integral, taylor"},
                "level": {"type": "string", "description": "Nivel pedagógico: primary, secondary, university, utn_am1, utn_am2, utn_algebra, utn_probabilidad"}
            },
            "required": ["concept"]
        }),
    )
}

/// Schema de `generate_exercise(lo_id, level, seed)`.
#[must_use]
pub fn generate_exercise_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "generate_exercise",
        "Genera un ejercicio determinista para un LearningObjective y nivel; devuelve prompt, solución y validador sin I/O.",
        json!({
            "type": "object",
            "properties": {
                "lo_id": {"type": "string", "description": "ID del objetivo de aprendizaje, ej. am1-der, sec-trig, am1-int"},
                "level": {"type": "string", "description": "Nivel pedagógico opcional"},
                "seed": {"type": "integer", "description": "Semilla opcional para variante determinista"}
            },
            "required": ["lo_id"]
        }),
    )
}

/// Schema de `assess_answer(exercise_id?, answer)`.
#[must_use]
pub fn assess_answer_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "assess_answer",
        "Evalúa una respuesta del estudiante contra un ejercicio (por lo_id/exercise_id) y devuelve feedback formativo con misconception y veredicto bien/parcial/mal.",
        json!({
            "type": "object",
            "properties": {
                "exercise_id": {"type": "string", "description": "ID del LO del ejercicio evaluado, ej. am1-der (alias lo_id)"},
                "lo_id": {"type": "string", "description": "Alias de exercise_id"},
                "answer": {"type": "string", "description": "Respuesta del estudiante"},
                "level": {"type": "string", "description": "Nivel opcional para regenerar el ejercicio"},
                "seed": {"type": "integer", "description": "Semilla opcional usada al generar el ejercicio"}
            },
            "required": ["answer"]
        }),
    )
}

/// Schema de `get_curriculum(query)`.
#[must_use]
pub fn get_curriculum_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "get_curriculum",
        "Busca objetivos de aprendizaje del currículum que matchean un concepto (título, descripción, tags); devuelve hasta 5.",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Texto a buscar, ej. derivada, taylor, integral"},
                "concept": {"type": "string", "description": "Alias de query"}
            },
            "required": ["query"]
        }),
    )
}

/// Schema de `suggest_next`.
#[must_use]
pub fn suggest_next_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "suggest_next",
        "Sugiere el siguiente objetivo de aprendizaje usando el perfil pedagógico (mock puro ordenado por mastery; enlaza a LOs reales del currículum).",
        json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    )
}

/// Schema de `generate_animation(template, concept, params)`.
#[must_use]
pub fn generate_animation_tool_schema() -> ToolSchema {
    ToolSchema::new(
        "generate_animation",
        "Valida y propone una solicitud de animación didáctica (template, concept, params) sin ejecutar el motor; usa protocolo AnimRequest.",
        json!({
            "type": "object",
            "properties": {
                "template": {"type": "string", "description": "Plantilla opcional: derivative-slope, integral-area, taylor-series, conformal-map, pitagoras, auto"},
                "concept": {"type": "string", "description": "Concepto en lenguaje natural, ej. derivada como pendiente"},
                "params": {"type": "object", "description": "Mapa opcional de parámetros numéricos finitos", "additionalProperties": {"type": "number"}},
                "canvas": {"type": "array", "description": "Resolución opcional [width, height] 64..4096", "items": {"type": "integer"}, "minItems": 2, "maxItems": 2},
                "width": {"type": "integer", "description": "Ancho opcional 64..4096 (fallback 640)"},
                "height": {"type": "integer", "description": "Alto opcional 64..4096 (fallback 480)"}
            },
            "required": []
        }),
    )
}

/// Las 6 tools pedagógicas para exponer al LLM.
#[must_use]
pub fn pedagogy_tool_schemas() -> Vec<ToolSchema> {
    vec![
        scaffold_tool_schema(),
        generate_exercise_tool_schema(),
        assess_answer_tool_schema(),
        get_curriculum_tool_schema(),
        suggest_next_tool_schema(),
        generate_animation_tool_schema(),
    ]
}

/// Conjunto completo seguro (3 base + 6 pedagógicas = 9).
#[must_use]
pub fn all_safe_tool_schemas() -> Vec<ToolSchema> {
    let mut schemas = vec![
        evaluate_expr_schema(),
        grafito_docs_schema(),
        ask_user_schema(),
    ];
    schemas.extend(pedagogy_tool_schemas());
    schemas
}

// ── Tests de integración por tool (válido + inválido) ───────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: args,
        }
    }

    fn safe_dispatch(name: &str, args: Value) -> ToolResult {
        let dispatcher = SafeGrafitoDispatcher;
        dispatcher.dispatch(&call("t1", name, args))
    }

    // — evaluate_expr —
    #[test]
    fn evaluate_expr_valido() {
        let r = safe_dispatch("evaluate_expr", json!({"expression": "2+2"}));
        assert!(r.ok, "esperaba ok: {}", r.content);
        assert!(r.content.contains('4'), "salida útil con 4: {}", r.content);
        let r2 = safe_dispatch("evaluate_expr", json!({"expression": "sin(0)"}));
        assert!(r2.ok, "{}", r2.content);
        assert!(r2.content.contains('0'));
        let r3 = safe_dispatch(
            "evaluate_expr",
            json!({"expression": "2*x+1", "variables": {"x": 3.0}}),
        );
        assert!(r3.ok, "{}", r3.content);
        assert!(r3.content.contains('7'));
    }

    #[test]
    fn evaluate_expr_invalido() {
        let r = safe_dispatch("evaluate_expr", json!({}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("expression"),
            "error tipado: {}",
            r.content
        );
        let r2 = safe_dispatch("evaluate_expr", json!({"expression": "2+*"}));
        assert!(!r2.ok);
        assert!(
            r2.content.contains("inválido")
                || r2.content.contains("E_CAMPO_INVALIDO")
                || r2.content.contains("E_EVALUACION"),
            "error tipado en español: {}",
            r2.content
        );
        let r3 = safe_dispatch("evaluate_expr", json!({"expression": "1/0"}));
        assert!(!r3.ok, "división por cero debe fallar");
    }

    // — grafito_docs —
    #[test]
    fn grafito_docs_valido() {
        let r = safe_dispatch("grafito_docs", json!({"query": "graficar función"}));
        assert!(r.ok, "{}", r.content);
        assert!(
            r.content.contains("Function"),
            "catálogo útil: {}",
            r.content
        );
    }

    #[test]
    fn grafito_docs_invalido() {
        let r = safe_dispatch("grafito_docs", json!({}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("query"),
            "{}",
            r.content
        );
        let r2 = safe_dispatch("grafito_docs", json!({"query": "zzzqqq-no-existe-123"}));
        assert!(!r2.ok);
        assert!(
            r2.content.contains("E_NO_ENCONTRADO") || r2.content.contains("sin comandos"),
            "{}",
            r2.content
        );
    }

    // — ask_user —
    #[test]
    fn ask_user_valido() {
        let r = safe_dispatch("ask_user", json!({"question": "¿cuánto vale x?"}));
        // ask_user nunca ejecuta en silencio: ok=false pero contenido útil y específico.
        assert!(!r.ok);
        assert!(
            r.content.contains("respuesta explícita") || r.content.contains("aclaración"),
            "salida útil no genérica: {}",
            r.content
        );
    }

    #[test]
    fn ask_user_invalido() {
        let r = safe_dispatch("ask_user", json!({}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("question"),
            "{}",
            r.content
        );
    }

    // — scaffold —
    #[test]
    fn scaffold_valido() {
        let r = safe_dispatch(
            "scaffold",
            json!({"concept": "derivada", "level": "secondary"}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json válido");
        assert_eq!(v["concept"], "derivada");
        assert!(v["question"].as_str().unwrap_or_default().len() > 5);
        assert!(v["explanation"].as_str().unwrap_or_default().len() > 5);
    }

    #[test]
    fn scaffold_invalido() {
        let r = safe_dispatch("scaffold", json!({"concept": ""}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("concept"),
            "{}",
            r.content
        );
    }

    // — generate_exercise —
    #[test]
    fn generate_exercise_valido() {
        let r = safe_dispatch(
            "generate_exercise",
            json!({"lo_id": "am1-der", "level": "university", "seed": 42}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["lo_id"], "am1-der");
        assert!(v["prompt"].as_str().unwrap_or_default().contains("Deriva"));
        assert!(!v["solution"].as_str().unwrap_or_default().is_empty());
        // Determinismo
        let r2 = safe_dispatch(
            "generate_exercise",
            json!({"lo_id": "am1-der", "level": "university", "seed": 42}),
        );
        assert_eq!(r.content, r2.content);
    }

    #[test]
    fn generate_exercise_invalido() {
        let r = safe_dispatch("generate_exercise", json!({}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("lo_id"),
            "{}",
            r.content
        );
        let r2 = safe_dispatch("generate_exercise", json!({"lo_id": "no-existe-xyz"}));
        assert!(!r2.ok);
        assert!(
            r2.content.contains("E_NO_ENCONTRADO") || r2.content.contains("no encontrado"),
            "{}",
            r2.content
        );
    }

    // — assess_answer: bien / mal / parcial —
    #[test]
    fn assess_answer_bien() {
        let ex = safe_dispatch("generate_exercise", json!({"lo_id": "am1-der", "seed": 0}));
        assert!(ex.ok, "{}", ex.content);
        let ev: Value = serde_json::from_str(&ex.content).expect("json");
        let expected = ev["solution"].as_str().expect("solution").to_string();
        let r = safe_dispatch(
            "assess_answer",
            json!({"exercise_id": "am1-der", "answer": expected, "seed": 0}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["correct"], true);
        assert_eq!(v["veredicto"], "bien");
    }

    #[test]
    fn assess_answer_mal() {
        let r = safe_dispatch(
            "assess_answer",
            json!({"exercise_id": "am1-der", "answer": "99999", "seed": 0}),
        );
        assert!(
            r.ok,
            "la evaluación debe tener éxito aunque la respuesta sea mala: {}",
            r.content
        );
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["correct"], false);
        assert_eq!(v["veredicto"], "mal");
        assert!(v["misconception"].as_str().unwrap_or_default().len() > 2);
        assert!(v["message"].as_str().unwrap_or_default().len() > 5);
    }

    #[test]
    fn assess_answer_parcial() {
        // am1-der seed 0: a,b deterministas; solución numérica S. Respuesta a +8 % => parcial.
        let ex = safe_dispatch("generate_exercise", json!({"lo_id": "am1-der", "seed": 0}));
        let ev: Value = serde_json::from_str(&ex.content).expect("json");
        let sol: f64 = ev["solution"].as_str().expect("sol").parse().expect("num");
        let parcial = format!("{}", sol * 1.08);
        let r = safe_dispatch(
            "assess_answer",
            json!({"exercise_id": "am1-der", "answer": parcial, "seed": 0}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["correct"], false);
        assert_eq!(
            v["veredicto"], "parcial",
            "8 % debe ser parcial: {}",
            r.content
        );
    }

    #[test]
    fn assess_answer_invalido() {
        let r = safe_dispatch("assess_answer", json!({"exercise_id": "am1-der"}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("answer"),
            "{}",
            r.content
        );
    }

    // — get_curriculum —
    #[test]
    fn get_curriculum_valido() {
        let r = safe_dispatch("get_curriculum", json!({"query": "derivada"}));
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert!(v["count"].as_u64().unwrap_or(0) > 0, "LOs reales no vacíos");
        let first_id = v["results"][0]["id"].as_str().unwrap_or("");
        assert!(!first_id.is_empty());
        assert!(
            curriculum_get(first_id).is_some(),
            "el LO devuelto debe existir en el currículum real: {first_id}"
        );
    }

    #[test]
    fn get_curriculum_invalido() {
        let r = safe_dispatch("get_curriculum", json!({"query": ""}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("query"),
            "{}",
            r.content
        );
    }

    // — suggest_next —
    #[test]
    fn suggest_next_valido() {
        let r = safe_dispatch("suggest_next", json!({}));
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["mock"], true);
        let next = v["next"].as_array().expect("next array");
        assert!(
            !next.is_empty(),
            "suggest_next no debe devolver lista vacía"
        );
        for item in next {
            let id = item["id"].as_str().unwrap_or("");
            assert!(curriculum_get(id).is_some(), "LO real enlazado: {id}");
        }
        // Ordenado por mastery ascendente (más débil primero: am1-der primero).
        let first = next[0]["id"].as_str().unwrap_or("");
        assert_eq!(first, "am1-der", "el más débil primero: {}", r.content);
    }

    #[test]
    fn suggest_next_invalido_oversized() {
        let big = "x".repeat(MAX_ARG_BYTES + 10);
        let r = safe_dispatch("suggest_next", json!({"branch_id": big}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_PRESUPUESTO") || r.content.contains("excede"),
            "{}",
            r.content
        );
    }

    // — generate_animation —
    #[test]
    fn generate_animation_valido() {
        let r = safe_dispatch(
            "generate_animation",
            json!({"concept": "derivada como pendiente", "template": "derivative-slope", "params": {"a": 1.0}}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        assert_eq!(v["template"], "derivative-slope");
        assert!(v["concept"].as_str().unwrap_or_default().len() > 3);
        assert_eq!(v["export"], "gif");
    }

    #[test]
    fn generate_animation_invalido() {
        let r = safe_dispatch("generate_animation", json!({"template": "", "concept": ""}));
        assert!(!r.ok);
        assert!(
            r.content.contains("E_FALTA_CAMPO") || r.content.contains("concept"),
            "{}",
            r.content
        );
    }

    #[test]
    fn generate_animation_parametros_nativos() {
        // Debe producir parámetros válidos para el motor nativo:
        // plantilla existente, Resolution 64..=4096 y Duration 100..=30000 ms.
        let r = safe_dispatch(
            "generate_animation",
            json!({"concept": "integral área bajo curva", "canvas": [640, 480]}),
        );
        assert!(r.ok, "{}", r.content);
        let v: Value = serde_json::from_str(&r.content).expect("json");
        let template = v["template"].as_str().unwrap_or("");
        assert!(
            is_known_template(template),
            "plantilla nativa válida: {template}"
        );
        let w = v["canvas"][0].as_u64().expect("w");
        let h = v["canvas"][1].as_u64().expect("h");
        assert!(
            is_valid_resolution(w as u32, h as u32),
            "Resolution válida {w}x{h}"
        );
        let dur = v["duration_ms"].as_u64().expect("duration_ms");
        assert!(
            (MIN_DURATION_MS..=MAX_DURATION_MS).contains(&dur),
            "Duration válida {dur} ms"
        );
        assert_eq!(v["protocol_version"], 1);
        // Auto-template: integral -> integral-area
        assert_eq!(template, "integral-area");
    }

    // — Schemas OpenAI-compat —
    #[test]
    fn schemas_openai_validos() {
        let all = all_safe_tool_schemas();
        assert_eq!(all.len(), 9, "3 base + 6 pedagógicas");
        for schema in &all {
            assert!(schema.validate().is_ok(), "schema {} inválido", schema.name);
            let openai = schema.openai_tool().expect("openai_tool");
            assert_eq!(openai["type"], "function");
            assert_eq!(openai["function"]["name"], schema.name.as_str());
            let params = &openai["function"]["parameters"];
            assert_eq!(params["type"], "object");
            assert!(params["properties"].is_object(), "{}", schema.name);
        }
        // required/properties correctos por tool
        let by_name = |n: &str| all.iter().find(|s| s.name == n).expect(n);
        let req = |n: &str| {
            by_name(n).parameters["required"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                .unwrap_or_default()
        };
        assert!(req("evaluate_expr").contains(&"expression"));
        assert!(req("grafito_docs").contains(&"query"));
        assert!(req("ask_user").contains(&"question"));
        assert!(req("scaffold").contains(&"concept"));
        assert!(req("generate_exercise").contains(&"lo_id"));
        assert!(req("assess_answer").contains(&"answer"));
        assert!(req("get_curriculum").contains(&"query"));
        assert!(req("suggest_next").is_empty());
        assert!(req("generate_animation").is_empty());
        // ask_user requiere consentimiento
        assert!(by_name("ask_user").needs_consent);
        assert_eq!(pedagogy_tool_schemas().len(), 6);
    }

    #[test]
    fn dispatcher_cubre_las_9_tools() {
        for (name, args) in [
            ("evaluate_expr", json!({"expression": "1+1"})),
            ("grafito_docs", json!({"query": "recta"})),
            ("ask_user", json!({"question": "¿x?"})),
            ("scaffold", json!({"concept": "derivada"})),
            ("generate_exercise", json!({"lo_id": "am1-der"})),
            ("assess_answer", json!({"answer": "2", "lo_id": "am1-der"})),
            ("get_curriculum", json!({"query": "derivada"})),
            ("suggest_next", json!({})),
            ("generate_animation", json!({"concept": "derivada"})),
        ] {
            let call = ToolCall {
                id: "cov".to_string(),
                name: name.to_string(),
                arguments: args,
            };
            let result = SafeGrafitoDispatcher.dispatch(&call);
            // ask_user devuelve ok=false por diseño (requiere usuario), el resto ok=true.
            if name == "ask_user" {
                assert!(!result.ok, "{name}");
                assert!(
                    result.content.contains("respuesta explícita"),
                    "{name}: {}",
                    result.content
                );
            } else {
                assert!(result.ok, "{name} debe tener éxito: {}", result.content);
            }
        }
    }

    #[test]
    fn pedagogy_dispatcher_aisla_pedagogia() {
        let pedagogy = PedagogyDispatcher;
        let ok = pedagogy.dispatch(&call("p1", "scaffold", json!({"concept": "integral"})));
        assert!(ok.ok);
        let denied = pedagogy.dispatch(&call("p2", "evaluate_expr", json!({"expression": "2+2"})));
        assert!(!denied.ok);
        assert!(denied.content.contains("no disponible"));
    }

    #[test]
    fn curriculum_real_tiene_43_los() {
        assert_eq!(all_los().len(), 43);
        for id in [
            "am1-der",
            "am1-int",
            "am1-func",
            "sec-trig",
            "sec-pitagoras",
            "am2-taylor",
            "alg-vectores",
            "prob-basica",
        ] {
            assert!(curriculum_get(id).is_some(), "LO real {id}");
        }
    }
}
