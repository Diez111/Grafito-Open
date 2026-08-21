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
    pub fn scaffold(&self, concept: &str, level: PedagogicalLevel, _history: &[Turn]) -> Scaffold {
        let concept = concept.trim();
        if concept.is_empty() {
            return Scaffold {
                question: "¿Qué concepto querés explorar?".into(),
                hint: None,
                explanation: "Escribí un tema como 'derivada' o 'integral'.".into(),
            };
        }
        match level {
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
}
