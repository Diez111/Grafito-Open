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

/// Clasifica una pregunta de forma local y determinista para el enrutamiento.
pub fn classify_route(problem: &str) -> ModelRoute {
    let normalized = normalize_text(problem);
    if normalized
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .any(|word| {
            REASONING_HINTS
                .iter()
                .any(|hint| *hint == word || word.contains(hint))
        })
    {
        ModelRoute::Reasoner
    } else {
        ModelRoute::Fast
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
    use super::{classify_route, ModelRoute};

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
}
