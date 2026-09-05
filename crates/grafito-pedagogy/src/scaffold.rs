//! Scaffold socrático — pregunta, pista, explicación, adaptado al nivel.

use crate::level::PedagogicalLevel;

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
        let concept = concept.trim();
        if concept.is_empty() {
            return Scaffold {
                question: "¿Qué concepto querés explorar?".into(),
                hint: None,
                explanation: "Escribí un tema como 'derivada' o 'integral'.".into(),
            };
        }
        let mut scaffold = match level {
            PedagogicalLevel::Primary => {
                // Para figuras 4D, dar una explicación más concreta y visual
                let is_4d = concept.to_lowercase().contains("4d")
                    || concept.to_lowercase().contains("tesseract")
                    || concept.to_lowercase().contains("hipercubo")
                    || concept.to_lowercase().contains("pentachoron")
                    || concept.to_lowercase().contains("hipersfera");
                if is_4d {
                    Scaffold {
                        question: format!("¿Te imaginás {} como algo que ves todos los días? ¿Qué forma tiene?", concept),
                        hint: Some("Pensá en un cubo dibujado en un papel: un cuadrado dentro de otro cuadrado unidos por líneas. Una figura 4D es un paso más allá.".into()),
                        explanation: format!("{} es un objeto de 4 dimensiones. No lo podemos tocar, pero Grafito lo proyecta a 3D y luego a tu pantalla para que lo veas girar, como si fuera su sombra.", concept),
                    }
                } else {
                    Scaffold {
                        question: format!("¿Te imaginás {} como algo que ves todos los días? ¿Qué forma tiene?", concept),
                        hint: Some(format!("Pensá en {} con un dibujo. ¿Sube o baja?", concept)),
                        explanation: format!("{} es como describir cómo cambia algo. En primaria lo vemos con ejemplos y gráficos simples.", concept),
                    }
                }
            },
            PedagogicalLevel::Secondary => Scaffold {
                question: format!("¿Qué representa {} en el gráfico de y = x²?", concept),
                hint: Some("Mirá la pendiente de la tangente o el área bajo la curva".into()),
                explanation: format!("En secundaria, {} aparece como pendiente (derivada) o área (integral). Lo vemos en el canvas y con animación.", concept),
            },
            PedagogicalLevel::University | PedagogicalLevel::UTN(_) => Scaffold {
                question: format!("¿Cómo definirías {} formalmente con límites?", concept),
                hint: Some("Recordá la definición por límite: f'(x)=lim_{h→0} [f(x+h)-f(x)]/h".into()),
                explanation: format!("Formalmente, {} se define vía límites y se demuestra con el teorema del valor medio. Grafito puede mostrar la derivada, la tangente y la serie de Taylor.", concept),
            },
        };

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

        scaffold
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
}
