//! Enrutamiento de modelos por tarea (perfil "Auto").

/// Clase de tarea que decide qué modelo usar dentro del perfil "Auto".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRoute {
    /// Tarea rutinaria que un modelo veloz resuelve sin costo extra.
    Fast,
    /// Tarea que se beneficia de un razonador más fuerte.
    Reasoner,
    /// La respuesta final se audita por separado (p. ej. Fusion/DeepSeek).
    AuditOnly,
}

impl ModelRoute {
    /// Etiqueta estable para mostrar en la UI y para pruebas.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Reasoner => "reasoner",
            Self::AuditOnly => "audit",
        }
    }
}

/// Pistas de razonamiento que inclinan la ruta hacia un razonador.
const REASONING_HINTS: &[&str] = &[
    "demostra",
    "demuestra",
    "demostrar",
    "derive",
    "deduci",
    "deducir",
    "justifica",
    "justificar",
    "porque",
    "razona",
    "razonar",
    "proof",
    "why",
    "raices",
    "raíces",
    "roots",
    "integral",
    "limite",
    "límite",
    "eigen",
    "autovalor",
    "differen",
    "serie",
    "fourier",
    "complejo",
    "complejos",
    "solve",
    "resuelve",
    "resolver",
];

/// Banda de complejidad de una tarea (gating J-Space fast/full/loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskBand {
    /// Un paso verificable, sin estructura extra.
    SingleStep,
    /// Algunas operaciones; se pueden usar herramientas puntuales.
    MultiStep,
    /// Tareas largas, multi-herramienta o con estado persistente (ledger).
    LongRunning,
}

impl TaskBand {
    /// Etiqueta estable del gating.
    pub const fn label(self) -> &'static str {
        match self {
            Self::SingleStep => "fast",
            Self::MultiStep => "full",
            Self::LongRunning => "loop",
        }
    }
}

/// Heurística local de complejidad para el gating de profundidad.
pub fn classify_band(problem: &str) -> TaskBand {
    let normalized = normalize_text(problem);
    let length = normalized.split_whitespace().count();
    let long_task = normalized.contains("paso a paso")
        || normalized.contains("pasos")
        || normalized.contains("completo")
        || normalized.contains("analiza")
        || normalized.contains("audit")
        || normalized.contains("resolveme")
        || normalized.contains("demostra")
        || normalized.contains("explica")
        || normalized.contains("deriva y")
        || normalized.contains("serie de fourier")
        || normalized.contains("politopo")
        || normalized.contains("informe");
    if length > 24 || long_task {
        TaskBand::LongRunning
    } else if length > 6 || normalized.contains("raices") || normalized.contains("integral") {
        TaskBand::MultiStep
    } else {
        TaskBand::SingleStep
    }
}

/// Clase matemática de una pregunta (F2: routing por tipo, no por `contains`).
///
/// Se obtiene con `classify_math_kind` vía tokenización + aridad (nº de `=`
/// a profundidad 0 y grado `^n`), sin subcadenas crudas sobre el texto: evita
/// falsos positivos tipo "anywhere" → "why". Hoja del DAG: no usa
/// `grafito-geometry` (el `parse_ast` real vive en el consumidor
/// `grafito-assistant`; acá hay una sonda estructural mínima y honesta).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathKind {
    /// Sin estructura detectable (vacío o texto libre).
    Empty,
    /// Sólo números y operadores (`2 + 3 * 4`).
    Arithmetic,
    /// Una igualdad con grado ≤ 2 en una variable.
    PolyEq,
    /// Sistema lineal (`a` matriz + `b` vector, o "sistema 2×2").
    LinearSystem,
    /// Derivada / integral / límite explícitos.
    Cas,
    /// Matrices, determinantes, autovalores, rango.
    Matrix,
    /// Pedido de gráfico o construcción geométrica.
    Graph,
    /// Series, Fourier/Taylor, EDOs (stub honesto: el solver local no las
    /// resuelve; derivan a remoto o a `Unsupported`).
    SerieEdo,
    /// Resto: texto con estructura no clasificada.
    General,
}

/// Sonda estructural: clasifica por tokens + aridad en vez de `contains`.
pub fn classify_math_kind(problem: &str) -> MathKind {
    let normalized = normalize_text(problem);
    if normalized.trim().is_empty() {
        return MathKind::Empty;
    }
    let tokens = tokenize_words(&normalized);
    let has_token = |words: &[&str]| {
        tokens
            .iter()
            .any(|token| words.iter().any(|word| *token == *word))
    };
    let has_token_or_phrase = |words: &[&str], phrases: &[&str]| {
        has_token(words) || phrases.iter().any(|phrase| normalized.contains(phrase))
    };

    // Verbos de construcción/gráfico (antes que `=` porque traen "y = ...").
    // "y = " solo cuenta con verbo explícito: un sistema "x + y = 3" no es
    // un gráfico.
    let graph_verb = has_token(&[
        "graficar",
        "grafica",
        "plot",
        "graph",
        "construir",
        "construye",
        "dibuja",
    ]);
    if graph_verb
        || ["semiplano", "segmento", "circunferencia"]
            .iter()
            .any(|phrase| normalized.contains(phrase))
    {
        return MathKind::Graph;
    }
    // Serie / Fourier / Taylor / EDO (stub honesto).
    if has_token_or_phrase(
        &[
            "serie",
            "series",
            "fourier",
            "taylor",
            "maclaurin",
            "edo",
            "edos",
            "diferencial",
            "laplace",
        ],
        &["ecuacion diferencial", "serie de potencias"],
    ) {
        return MathKind::SerieEdo;
    }
    // CAS explícito.
    if has_token_or_phrase(
        &[
            "derivar",
            "deriva",
            "derivada",
            "derivative",
            "integrar",
            "integra",
            "integral",
            "limite",
            "limit",
            "primitiva",
            "barrow",
        ],
        &["d/dx", "∫"],
    ) {
        return MathKind::Cas;
    }
    // Matrices / álgebra lineal.
    if has_token_or_phrase(
        &[
            "matriz",
            "matrices",
            "determinante",
            "autovalor",
            "autovalores",
            "autovector",
            "eigen",
            "rango",
            "gauss",
            "inversa",
        ],
        &["[[", "valor propio"],
    ) {
        if has_token(&["sistema"])
            || (normalized.contains("sistema")
                && (normalized.contains("lineal")
                    || normalized.contains("2x2")
                    || top_level_eq_count(&normalized) >= 1))
        {
            return MathKind::LinearSystem;
        }
        return MathKind::Matrix;
    }
    if has_token(&["sistema"]) && top_level_eq_count(&normalized) >= 1 {
        return MathKind::LinearSystem;
    }
    // Ecuaciones por aridad: un `=` a profundidad 0 + grado. Sin marcadores
    // de grado pero con variable se asume polinómica (el solver decide
    // fail-closed si no lo es); sin letras es aritmética/general.
    if top_level_eq_count(&normalized) == 1 {
        return match probe_poly_degree(&normalized) {
            Some(degree) if degree <= 2 => MathKind::PolyEq,
            Some(_) => MathKind::General,
            None if normalized.chars().any(|c| c.is_alphabetic()) => MathKind::PolyEq,
            None => MathKind::General,
        };
    }
    if is_plain_arithmetic(&normalized, &tokens) {
        return MathKind::Arithmetic;
    }
    MathKind::General
}

/// Tokeniza en corridas alfanuméricas (sin tildes, ya normalizado).
fn tokenize_words(normalized: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in normalized.chars() {
        if character.is_alphanumeric() || character == '_' {
            current.push(character);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Cuenta `=` a profundidad 0 de paréntesis (aridad de la ecuación).
fn top_level_eq_count(normalized: &str) -> usize {
    let mut depth = 0_usize;
    let mut count = 0_usize;
    for character in normalized.chars() {
        match character {
            '(' | '[' => depth = depth.saturating_add(1),
            ')' | ']' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => count += 1,
            _ => {}
        }
    }
    count
}

/// Grado aparente: mayor exponente `^n` / `**n`, o 2 ante "cuadr".
fn probe_poly_degree(normalized: &str) -> Option<u8> {
    let mut degree = 0_u8;
    let mut seen = false;
    let bytes = normalized.as_bytes();
    let mut index = 0_usize;
    while index < bytes.len() {
        let is_caret = bytes[index] == b'^'
            || (bytes[index] == b'*' && index + 1 < bytes.len() && bytes[index + 1] == b'*');
        if is_caret {
            index += if bytes[index] == b'^' { 1 } else { 2 };
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if start < index {
                if let Ok(exponent) = normalized[start..index].parse::<u8>() {
                    degree = degree.max(exponent);
                    seen = true;
                }
            }
            continue;
        }
        index += 1;
    }
    if normalized.contains("cuadr") || normalized.contains("²") {
        degree = degree.max(2);
        seen = true;
    }
    if normalized.contains("cub") || normalized.contains("³") {
        degree = degree.max(3);
        seen = true;
    }
    if normalized.contains("lineal") || normalized.contains("recta") {
        seen = true;
    }
    if seen {
        Some(degree.max(1))
    } else {
        None
    }
}

/// Aritmética plana: dígitos, operadores, paréntesis y como mucho la
/// variable `x` aislada (muestras tipo `f(2)` no cuentan).
fn is_plain_arithmetic(normalized: &str, tokens: &[String]) -> bool {
    if !normalized.chars().all(|character| {
        character.is_ascii_digit()
            || character.is_whitespace()
            || "+-*/^%().,".contains(character)
            || character == 'x'
    }) {
        return false;
    }
    // Al menos un dígito y ningún identificador distinto de `x`.
    normalized.chars().any(|c| c.is_ascii_digit())
        && tokens
            .iter()
            .all(|token| token == "x" || token.chars().all(|c| c.is_ascii_digit()))
}

/// Clasifica una pregunta de forma local y determinista para el enrutamiento.
///
/// F2: primero la sonda estructural (`classify_math_kind`); el histórico de
/// pistas de razonamiento queda como red para `General` (compatibilidad con
/// "demostrá/justificá/porqué" sin forma matemática explícita).
pub fn classify_route(problem: &str) -> ModelRoute {
    match classify_math_kind(problem) {
        MathKind::Empty | MathKind::Arithmetic | MathKind::Graph => ModelRoute::Fast,
        MathKind::PolyEq
        | MathKind::LinearSystem
        | MathKind::Cas
        | MathKind::Matrix
        | MathKind::SerieEdo => ModelRoute::Reasoner,
        MathKind::General => {
            // Red de compatibilidad: igualdad o prefijo por token (cubre
            // "demostrar"/"resuelve" sin el falso positivo "anywhere"→"why"
            // del `contains` histórico sobre la palabra completa).
            let normalized = normalize_text(problem);
            let tokens = tokenize_words(&normalized);
            let hit = tokens.iter().any(|word| {
                REASONING_HINTS.iter().any(|hint| {
                    *word == **hint || word.starts_with(hint) || hint.starts_with(word.as_str())
                })
            });
            if hit {
                ModelRoute::Reasoner
            } else {
                ModelRoute::Fast
            }
        }
    }
}

/// Normaliza minúsculas y diacríticos para comparar pistas de razonamiento.
fn normalize_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars().flat_map(char::to_lowercase) {
        normalized.push(match character {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' => 'u',
            'ü' => 'u',
            'ñ' => 'n',
            other => other,
        });
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{classify_math_kind, classify_route, MathKind, ModelRoute};

    #[test]
    fn routine_requests_prefer_the_fast_route() {
        assert_eq!(classify_route("2 + 2"), ModelRoute::Fast);
        assert_eq!(classify_route("graficá y = x^2 - 1"), ModelRoute::Fast);
        assert_eq!(classify_route(""), ModelRoute::Fast);
    }

    #[test]
    fn reasoning_keywords_prefer_the_reasoner_route() {
        for question in [
            "demostrá que la integral de x^2 es x^3/3",
            "derivá la función y explicá el porqué",
            "justificá por qué convergen las series de Fourier",
            "resolvé las raíces del polinomio",
            "why does the derivative of sin(x) equal cos(x)",
            "deducí los autovalores de la matriz",
        ] {
            assert_eq!(
                classify_route(question),
                ModelRoute::Reasoner,
                "question: {question}"
            );
        }
    }

    #[test]
    fn route_labels_are_stable() {
        assert_eq!(ModelRoute::Fast.label(), "fast");
        assert_eq!(ModelRoute::Reasoner.label(), "reasoner");
        assert_eq!(ModelRoute::AuditOnly.label(), "audit");
    }

    #[test]
    fn math_kind_routes_by_structure_not_substrings() {
        assert_eq!(classify_math_kind("2 + 3 * 4"), MathKind::Arithmetic);
        assert_eq!(classify_math_kind(""), MathKind::Empty);
        assert_eq!(classify_math_kind("graficá y = x^2 - 1"), MathKind::Graph);
        assert_eq!(classify_math_kind("x^2 - 5*x + 6 = 0"), MathKind::PolyEq);
        assert_eq!(classify_math_kind("2*x + 3 = 11"), MathKind::PolyEq);
        assert_eq!(classify_math_kind("derivada de x^2"), MathKind::Cas);
        assert_eq!(classify_math_kind("integral de x^2"), MathKind::Cas);
        assert_eq!(classify_math_kind("límite de sin(x)/x"), MathKind::Cas);
        assert_eq!(
            classify_math_kind("determinante de la matriz"),
            MathKind::Matrix
        );
        assert_eq!(
            classify_math_kind("sistema 2x2: x + y = 3, x - y = 1"),
            MathKind::LinearSystem
        );
        assert_eq!(
            classify_math_kind("serie de Fourier de la onda cuadrada"),
            MathKind::SerieEdo
        );
        // "anywhere" contiene "why" como subcadena pero no es pista.
        assert_eq!(classify_math_kind("anywhere"), MathKind::General);
        assert_eq!(classify_route("anywhere"), ModelRoute::Fast);
    }

    #[test]
    fn structural_kinds_map_to_expected_routes() {
        assert_eq!(classify_route("2 + 3 * 4"), ModelRoute::Fast);
        assert_eq!(classify_route("x^2 - 5*x + 6 = 0"), ModelRoute::Reasoner);
        assert_eq!(classify_route("derivada de x^2"), ModelRoute::Reasoner);
        assert_eq!(
            classify_route("serie de Fourier de la onda cuadrada"),
            ModelRoute::Reasoner
        );
        assert_eq!(
            classify_route("determinante de la matriz [[1,2],[3,4]]"),
            ModelRoute::Reasoner
        );
    }
}
