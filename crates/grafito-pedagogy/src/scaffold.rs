//! Scaffold socrático — pregunta, pista, explicación, adaptado al nivel.

use crate::level::PedagogicalLevel;

/// Pregunta fallback cuando no hay tema reconocido (sin interpolar texto crudo del usuario).
///
/// Evita la interpolación degenerada del tipo `¿Te imaginás hola...?`: si el
/// extractor no reconoce tema, se usa este mensaje fijo en vez del input crudo.
/// Puro y determinista.
pub const NO_CONCEPT_FALLBACK_QUESTION: &str = "¿Qué querés graficar primero: recta, parábola…?";

/// Stopwords normalizadas (minúsculas, sin tildes): saludos, verbos de pedido y
/// genéricos demo. Se ignoran en la extracción (puro, sin I/O).
const CONCEPT_STOPWORDS: &[&str] = &[
    "hola",
    "holis",
    "buenas",
    "hey",
    "chau",
    "gracias",
    "porfa",
    "porfavor",
    "favor",
    "haceme",
    "hace",
    "hacer",
    "haz",
    "dame",
    "danos",
    "mostrame",
    "mostra",
    "mostrar",
    "muestrame",
    "ensenarme",
    "quiero",
    "quiero",
    "necesito",
    "puedes",
    "podes",
    "podrias",
    "ejemplo",
    "ejemplos",
    "proba",
    "probar",
    "prueba",
    "test",
    "testear",
    "capacidad",
    "capacidades",
    "graficacion",
    "graficar",
    "grafica",
    "grafico",
    "para",
    "las",
    "los",
    "una",
    "unos",
    "unas",
    "con",
    "como",
    "que",
    "del",
    "de",
    "la",
    "el",
    "en",
    "y",
    "o",
    "un",
    "mi",
    "mis",
    "tu",
    "tus",
    "se",
    "es",
    "son",
    "esta",
    "este",
    "esto",
    "al",
    "lo",
];

/// Allowlist normalizada (sin tildes, minúsculas) → canónica para mostrar.
///
/// Solo estos temas se interpolan en plantillas. Todo lo demás (saludos,
/// verbos, genéricos) se descarta y cae al fallback sin interpolar crudo.
/// Puro y determinista.
const CONCEPT_ALLOWLIST: &[(&str, &str)] = &[
    ("recta", "recta"),
    ("rectas", "recta"),
    ("parabola", "parábola"),
    ("parabolas", "parábola"),
    ("derivada", "derivada"),
    ("derivadas", "derivada"),
    ("deriva", "derivada"),
    ("derivar", "derivada"),
    ("integral", "integral"),
    ("integrales", "integral"),
    ("integra", "integral"),
    ("integrar", "integral"),
    ("funcion", "función"),
    ("funciones", "función"),
    ("circulo", "círculo"),
    ("circunferencia", "circunferencia"),
    ("seno", "seno"),
    ("coseno", "coseno"),
    ("tangente", "tangente"),
    ("limite", "límite"),
    ("limites", "límite"),
    ("serie", "serie"),
    ("series", "serie"),
    ("taylor", "serie de Taylor"),
    ("matriz", "matriz"),
    ("matrices", "matriz"),
    ("vector", "vector"),
    ("vectores", "vector"),
    ("fraccion", "fracción"),
    ("fracciones", "fracción"),
    ("probabilidad", "probabilidad"),
    ("ecuacion", "ecuación"),
    ("ecuaciones", "ecuación"),
    ("hiperbola", "hipérbola"),
    ("elipse", "elipse"),
    ("pendiente", "pendiente"),
    ("area", "área"),
    ("volumen", "volumen"),
    ("esfera", "esfera"),
    ("cubo", "cubo"),
    ("cilindro", "cilindro"),
    ("cono", "cono"),
    ("toro", "toro"),
    ("tetra", "tetraedro"),
    ("piramide", "pirámide"),
    ("prisma", "prisma"),
    ("curva", "curva"),
    ("curvas", "curva"),
    ("polinomio", "polinomio"),
    ("logaritmo", "logaritmo"),
    ("exponencial", "exponencial"),
    ("trigonometria", "trigonometría"),
    ("geometria", "geometría"),
    ("algebra", "álgebra"),
    ("histograma", "histograma"),
    ("barras", "gráfico de barras"),
    ("torta", "gráfico de torta"),
    ("tesseract", "tesseract 4D"),
    ("hipercubo", "hipercubo 4D"),
    ("hipersfera", "hipersfera 4D"),
    ("pentachoron", "pentachoron 4D"),
    ("4d", "figura 4D"),
];

/// Marcadores de pedido demostrativo (normalizados, sin tildes): ejemplos /
/// probar / capacidades / graficar. Un pedido así es demo, no evaluación →
/// el guard telling debe bypassearse. Puro y determinista.
const EXPLORATORY_MARKERS: &[&str] = &[
    "ejemplo",
    "ejemplos",
    "proba",
    "probar",
    "prueba",
    "capacidad",
    "capacidades",
    "grafica",
    "graficar",
    "graficacion",
    "demo",
    "demostra",
    "mostrame",
];

/// Normaliza una palabra: minúsculas + sin tildes (para matching puro).
fn normalize_word(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Normaliza texto completo para `contains` (minúsculas, sin tildes).
fn normalize_text(text: &str) -> String {
    normalize_word(text)
}

/// Extractor de concepto: quita saludos/verbos/genéricos, exige min 3 letras
/// y allowlist de temas. Sin tema reconocido → `None` (el llamante usa el
/// fallback sin interpolar crudo).
///
/// Puro y determinista: mismo input → mismo `Option`. Sin `unwrap`, MSRV 1.92.
///
/// Ejemplo: `"hola haceme ejemplos para probar las capacidades de graficacion"`
/// → `None` (todo stopwords/genéricos, ningún tema de la allowlist).
/// `"graficá una parábola"` → `Some("parábola")`.
pub fn extract_concept(raw: &str) -> Option<String> {
    for token in raw.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let norm = normalize_word(token);
        if norm.chars().count() < 3 {
            continue;
        }
        if CONCEPT_STOPWORDS.contains(&norm.as_str()) {
            continue;
        }
        if let Some((_, canonical)) = CONCEPT_ALLOWLIST.iter().find(|(key, _)| *key == norm) {
            return Some((*canonical).to_owned());
        }
    }
    None
}

/// ¿Es pedido demostrativo/exploratorio? `true` si contiene marcadores de demo
/// (ejemplos/probá/capacidades/graficá) — es demo, no evaluación → bypassear
/// el telling. Puro y determinista, sin `unwrap`.
pub fn is_exploratory_request(raw: &str) -> bool {
    let norm = normalize_text(raw);
    EXPLORATORY_MARKERS
        .iter()
        .any(|marker| norm.contains(*marker))
}

/// Turno conversacional mínimo para contexto.
#[derive(Debug, Clone)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

/// Andamiaje socrático para un concepto.
#[derive(Debug, Clone)]
pub struct Scaffold {
    pub question: String,
    pub hint: Option<String>,
    pub explanation: String,
}

impl Scaffold {
    /// Segmento determinista y vinculante para el system prompt.
    ///
    /// Incluye pregunta BKT actual (scaffold.question), pista del misconception
    /// y explicación, más historial acotado. Todo truncado a presupuestos fijos
    /// para no romper `max_input_chars` (8192) ni `max_system_instructions` (4096).
    /// Puro, sin I/O, 100% testeable: misma entrada → mismo segmento.
    pub fn system_prompt_segment(&self, history: &[Turn]) -> String {
        const MAX_FIELD_CHARS: usize = 500;
        const MAX_HISTORY_TURNS: usize = 4;
        const MAX_HISTORY_CONTENT_CHARS: usize = 180;
        let mut out = String::new();
        out.push_str("[SOCRATIC SCAFFOLD — VINCULANTE]\n");
        let q: String = self.question.chars().take(MAX_FIELD_CHARS).collect();
        out.push_str("Pregunta BKT actual (current_question): ");
        out.push_str(&q);
        out.push('\n');
        if let Some(hint) = &self.hint {
            let h: String = hint.chars().take(MAX_FIELD_CHARS).collect();
            out.push_str("Pista scaffold (misconception): ");
            out.push_str(&h);
            out.push('\n');
        } else {
            out.push_str("Pista scaffold: (sin pista — pedí ejemplo concreto)\n");
        }
        let e: String = self.explanation.chars().take(MAX_FIELD_CHARS).collect();
        out.push_str("Explicación: ");
        out.push_str(&e);
        out.push('\n');
        out.push_str(&format!(
            "Historial ({} turnos, muestra {}): ",
            history.len(),
            MAX_HISTORY_TURNS.min(history.len())
        ));
        if history.is_empty() {
            out.push_str("(sin historial)");
        } else {
            for (idx, turn) in history.iter().take(MAX_HISTORY_TURNS).enumerate() {
                let role: String = turn.role.chars().take(16).collect();
                let content: String = turn
                    .content
                    .chars()
                    .take(MAX_HISTORY_CONTENT_CHARS)
                    .collect();
                // Normaliza saltos para no romper prompt
                let content = content.replace(['\n', '\r'], " ");
                out.push_str(&format!("[{idx}:{role}:{content}] "));
            }
        }
        out.push('\n');
        out.push_str("INSTRUCCIÓN VINCULANTE: Usá EXACTAMENTE esta pregunta y pista. No inventes otra. Adaptá sólo el nivel de detalle según el historial. Si el estudiante pide solución directa con attempts<2, no la des: re-preguntá con la pista.");
        out
    }

    /// Atajo sin historial.
    pub fn system_prompt_segment_no_history(&self) -> String {
        self.system_prompt_segment(&[])
    }
}

/// Motor de scaffold — puro, sin I/O.
#[derive(Debug, Clone, Default)]
pub struct ScaffoldEngine;

impl ScaffoldEngine {
    /// Genera el segmento determinista listo para inyectar al system prompt.
    ///
    /// Wrapper puro que encadena `scaffold(concept, level, history)` + `system_prompt_segment(history)`.
    /// Determinista y testeable: mismo concept/level/history → mismo segmento.
    pub fn scaffold_system_segment(
        &self,
        concept: &str,
        level: PedagogicalLevel,
        history: &[Turn],
    ) -> String {
        let sc = self.scaffold(concept, level, history);
        sc.system_prompt_segment(history)
    }

    pub fn scaffold(&self, concept: &str, level: PedagogicalLevel, history: &[Turn]) -> Scaffold {
        // Sanitiza: solo temas de la allowlist se interpolan. Sin tema reconocido
        // → fallback fijo, jamás el texto crudo (evita `¿Te imaginás hola...?`).
        // Puro y determinista, sin `unwrap`.
        let raw = concept.trim();
        let canonical: String = if raw.is_empty() {
            String::new()
        } else {
            extract_concept(raw).unwrap_or_default()
        };
        if canonical.is_empty() {
            let mut fallback = Scaffold {
                question: NO_CONCEPT_FALLBACK_QUESTION.into(),
                hint: Some("Probá con una recta o una parábola para empezar.".into()),
                explanation: "Contame qué forma querés ver y la graficamos juntos paso a paso."
                    .into(),
            };
            apply_history_adaptation(&mut fallback, history);
            return fallback;
        }
        let topic = canonical.as_str();
        let mut scaffold = match level {
            PedagogicalLevel::Primary => {
                // Para figuras 4D, dar una explicación más concreta y visual.
                // El canónico ya trae `4D` (ej. `tesseract 4D`); se chequea el
                // canónico, no el crudo, para no interpolar saludos/verbos.
                let is_4d = topic.contains("4D") || topic.contains("4d");
                if is_4d {
                    Scaffold {
                        question: format!("¿Te imaginás {topic} como algo que ves todos los días? ¿Qué forma tiene?"),
                        hint: Some("Pensá en un cubo dibujado en un papel: un cuadrado dentro de otro cuadrado unidos por líneas. Una figura 4D es un paso más allá.".into()),
                        explanation: format!("{topic} es un objeto de 4 dimensiones. No lo podemos tocar, pero Grafito lo proyecta a 3D y luego a tu pantalla para que lo veas girar, como si fuera su sombra."),
                    }
                } else {
                    Scaffold {
                        question: format!("¿Te imaginás {topic} como algo que ves todos los días? ¿Qué forma tiene?"),
                        hint: Some(format!("Pensá en {topic} con un dibujo. ¿Sube o baja?")),
                        explanation: format!("{topic} es como describir cómo cambia algo. En primaria lo vemos con ejemplos y gráficos simples."),
                    }
                }
            },
            PedagogicalLevel::Secondary => Scaffold {
                question: format!("¿Qué representa {topic} en el gráfico de y = x²?"),
                hint: Some("Mirá la pendiente de la tangente o el área bajo la curva".into()),
                explanation: format!("En secundaria, {topic} aparece como pendiente (derivada) o área (integral). Lo vemos en el canvas y con animación."),
            },
            PedagogicalLevel::University | PedagogicalLevel::UTN(_) => Scaffold {
                question: format!("¿Cómo definirías {topic} formalmente con límites?"),
                hint: Some("Recordá la definición por límite: f'(x)=lim_{h→0} [f(x+h)-f(x)]/h".into()),
                explanation: format!("Formalmente, {topic} se define vía límites y se demuestra con el teorema del valor medio. Grafito puede mostrar la derivada, la tangente y la serie de Taylor."),
            },
        };
        apply_history_adaptation(&mut scaffold, history);
        scaffold
    }
}

/// Aplica la adaptación por historial al scaffold (puro, sin I/O).
///
/// Extraído del cuerpo de `scaffold` para reutilizarlo en el fallback sin tema
/// (que jamás interpola texto crudo). Misma lógica que antes, sin `unwrap`.
fn apply_history_adaptation(scaffold: &mut Scaffold, history: &[Turn]) {
    // — Adaptación basada en historia (no se ignora) —
    // Si history no vacío, adapta pregunta/hint/explicación.
    if !history.is_empty() {
        if let Some(last) = history.last() {
            let lower = last.content.to_lowercase();
            let role_lower = last.role.to_lowercase();
            let is_incorrect_or_concept = lower.contains("incorrect")
                || lower.contains("concept")
                || lower.contains("concepto")
                || lower.contains("confund")
                || lower.contains("error")
                || lower.contains("no es")
                || lower.contains("definición")
                || lower.contains("definicion")
                || lower.contains("misconception")
                || lower.contains("mal")
                || role_lower.contains("incorrect")
                || role_lower.contains("concept");
            let has_no_se = lower.contains("no sé")
                || lower.contains("no se")
                || lower.contains("no entiendo")
                || lower.contains("ni idea")
                || lower.contains("no entiendo bien");

            // Si último Turn fue incorrecto/Concept, da hint más concreto
            if is_incorrect_or_concept {
                let concrete = " Pista concreta: probá con un ejemplo numérico simple (x=1, x=2) y compará resultados.";
                scaffold.hint = Some(match scaffold.hint.take() {
                    Some(h) => format!("{h}{concrete}"),
                    None => concrete.trim().to_string(),
                });
                // Adapta pregunta para guiar con ejemplo
                scaffold.question = format!(
                    "{} (¿podés probar con un ejemplo concreto para ver el patrón?)",
                    scaffold.question
                );
            }

            // Si mastery implicado, ajusta explicación
            let mastery_mentioned = history.iter().any(|t| {
                let c = t.content.to_lowercase();
                c.contains("dominio")
                    || c.contains("mastery")
                    || c.contains("básico")
                    || c.contains("basico")
                    || c.contains("principiante")
                    || c.contains("no domina")
                    || c.contains("nivel bajo")
                    || c.contains("me cuesta")
            });
            if mastery_mentioned {
                let low = history.iter().any(|t| {
                    let c = t.content.to_lowercase();
                    c.contains("bajo")
                        || c.contains("principiante")
                        || c.contains("no domina")
                        || c.contains("me cuesta")
                        || c.contains("básico")
                        || c.contains("basico")
                });
                if low {
                    scaffold.explanation = format!(
                            "{} Como estás empezando, lo vemos paso a paso con un dibujo simple antes de la fórmula.",
                            scaffold.explanation
                        );
                } else {
                    scaffold.explanation = format!(
                            "{} Ya tenés base, profundicemos con la definición formal y un ejemplo desafiante.",
                            scaffold.explanation
                        );
                }
            }

            // Lógica específica: si history.len()>2 y último contiene "no sé" → explicación más detallada
            if history.len() > 2 && has_no_se {
                scaffold.explanation = format!(
                        "{} Te lo explico con calma y más detalle, paso a paso: primero el ejemplo más simple (x=1), luego x=2, dibujando cada parte para que veas el patrón.",
                        scaffold.explanation
                    );
                let extra =
                    " Detalle extra: vamos a desglosarlo en pasos muy pequeños, sin saltos.";
                scaffold.hint = Some(match scaffold.hint.take() {
                    Some(h) => format!("{h}{extra}"),
                    None => "Vamos paso a paso con un ejemplo muy simple.".to_string(),
                });
            } else if has_no_se {
                scaffold.explanation = format!(
                    "{} Vamos a verlo con un ejemplo concreto y simple para que quede claro.",
                    scaffold.explanation
                );
                // Asegura que haya pista si no había
                if scaffold.hint.is_none() {
                    scaffold.hint = Some("Probá con x=1 y mirá qué pasa.".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scaffold_secondary_has_question() {
        let eng = ScaffoldEngine;
        let sc = eng.scaffold("derivada", PedagogicalLevel::Secondary, &[]);
        assert!(!sc.question.is_empty());
        assert!(sc.hint.is_some());
    }

    #[test]
    fn system_prompt_segment_is_deterministic_and_bounded() {
        let eng = ScaffoldEngine;
        let history = vec![
            Turn {
                role: "user".into(),
                content: "no entiendo bien".into(),
            },
            Turn {
                role: "assistant".into(),
                content: "probá con x=1".into(),
            },
        ];
        let seg1 = eng.scaffold_system_segment("derivada", PedagogicalLevel::Secondary, &history);
        let seg2 = eng.scaffold_system_segment("derivada", PedagogicalLevel::Secondary, &history);
        assert_eq!(seg1, seg2, "determinista");
        assert!(seg1.contains("Pregunta BKT actual"));
        assert!(seg1.contains("Pista scaffold"));
        assert!(seg1.contains("Historial (2 turnos"));
        assert!(seg1.contains("VINCULANTE"));
        // acotado
        assert!(seg1.chars().count() < 2000);
    }

    #[test]
    fn scaffold_segment_includes_history_adaptation() {
        let eng = ScaffoldEngine;
        let history = vec![Turn {
            role: "user".into(),
            content: "concepto incorrectoo me confundí".into(),
        }];
        let sc = eng.scaffold("integral", PedagogicalLevel::Secondary, &history);
        let seg = sc.system_prompt_segment(&history);
        // history adaptación debe haber agregado pista concreta
        assert!(
            sc.hint.as_deref().unwrap_or("").contains("Pista concreta")
                || seg.contains("Pista concreta")
                || seg.contains("concreto")
        );
        assert!(seg.contains("integral"));
    }

    #[test]
    fn scaffold_segment_no_history_is_stable() {
        let sc = Scaffold {
            question: "¿Qué es la derivada?".into(),
            hint: Some("Mirá la pendiente".into()),
            explanation: "Es la tasa de cambio".into(),
        };
        let seg = sc.system_prompt_segment_no_history();
        assert!(seg.contains("¿Qué es la derivada?"));
        assert!(seg.contains("Mirá la pendiente"));
        assert!(seg.contains("sin historial"));
    }

    #[test]
    fn extract_concept_strips_greetings_and_requires_allowlist() {
        // Input real del bug P0: todo saludos/verbos/genéricos → None.
        assert_eq!(
            extract_concept("hola haceme ejemplos para probar las capacidades de graficacion"),
            None
        );
        // Tema válido se extrae canónico (verbos mapean al sustantivo).
        assert_eq!(extract_concept("derivá x^2"), Some("derivada".into()));
        assert_eq!(
            extract_concept("graficá una parábola"),
            Some("parábola".into())
        );
        assert_eq!(extract_concept("hola"), None);
        assert_eq!(extract_concept("xy"), None);
        assert_eq!(extract_concept(""), None);
    }

    #[test]
    fn scaffold_never_interpolates_raw_greetings() {
        let eng = ScaffoldEngine;
        // Sin tema → fallback fijo, jamás `¿Te imaginás hola...?`.
        let sc = eng.scaffold(
            "hola haceme ejemplos para probar las capacidades de graficacion",
            PedagogicalLevel::Secondary,
            &[],
        );
        assert_eq!(sc.question, NO_CONCEPT_FALLBACK_QUESTION);
        assert!(!sc.question.contains("hola"), "{}", sc.question);
        // Vacío también cae al fallback.
        let empty = eng.scaffold("   ", PedagogicalLevel::Secondary, &[]);
        assert_eq!(empty.question, NO_CONCEPT_FALLBACK_QUESTION);
    }

    #[test]
    fn exploratory_bypass_marks_demo_requests() {
        assert!(is_exploratory_request(
            "hola haceme ejemplos para probar las capacidades de graficacion"
        ));
        assert!(is_exploratory_request("mostrame qué podés graficar"));
        assert!(!is_exploratory_request("¿qué es la derivada de x^2?"));
        assert!(!is_exploratory_request("derivada"));
    }
}
