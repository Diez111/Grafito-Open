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

/// Motor de scaffold — puro, sin I/O.
#[derive(Debug, Clone, Default)]
pub struct ScaffoldEngine;

impl ScaffoldEngine {
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
}
