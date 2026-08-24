//! Sistema de enseñanza paso a paso — burbujas que se transforman, pizarra y manim.
//!
//! Modelo puro sin egui: `TeachingSession` con 3-6 pasos, cada paso con
//! texto, expresión matemática, elementos de pizarra y especificación de
//! animación manim. La orquestación agéntica compleja usa `grafito-anim`
//! para generar animaciones 3b1b/manim.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TeachingTopic {
    Derivada,
    Integral,
    Limite,
    Funcion,
    Pitagoras,
    General(String),
}

impl TeachingTopic {
    pub fn from_text(text: &str) -> Self {
        let lower = text.to_lowercase();
        if lower.contains("deriv") {
            Self::Derivada
        } else if lower.contains("integral") {
            Self::Integral
        } else if lower.contains("límite") || lower.contains("limite") {
            Self::Limite
        } else if lower.contains("func") {
            Self::Funcion
        } else if lower.contains("pitag") {
            Self::Pitagoras
        } else {
            Self::General(text.chars().take(80).collect())
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Derivada => "Derivada".into(),
            Self::Integral => "Integral".into(),
            Self::Limite => "Límite".into(),
            Self::Funcion => "Función".into(),
            Self::Pitagoras => "Teorema de Pitágoras".into(),
            Self::General(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingStep {
    pub id: String,
    pub title: String,
    pub explanation: String,
    pub math_expr: Option<String>,
    /// Elementos de pizarra para este paso (serializados como JSON simple)
    pub whiteboard_hint: String,
    /// Template manim sugerido
    pub manim_template: Option<String>,
    pub completed: bool,
}

impl TeachingStep {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            explanation: explanation.into(),
            math_expr: None,
            whiteboard_hint: String::new(),
            manim_template: None,
            completed: false,
        }
    }
    pub fn with_math(mut self, expr: impl Into<String>) -> Self {
        self.math_expr = Some(expr.into());
        self
    }
    pub fn with_whiteboard(mut self, hint: impl Into<String>) -> Self {
        self.whiteboard_hint = hint.into();
        self
    }
    pub fn with_manim(mut self, template: impl Into<String>) -> Self {
        self.manim_template = Some(template.into());
        self
    }
}

/// Sesión de enseñanza con pasos y pizarra asociada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingSession {
    pub topic: TeachingTopic,
    pub steps: Vec<TeachingStep>,
    pub current: usize,
    pub created_epoch: u64,
}

impl TeachingSession {
    pub fn new(topic: TeachingTopic, steps: Vec<TeachingStep>) -> Self {
        Self {
            topic,
            steps,
            current: 0,
            created_epoch: 0,
        }
    }
    pub fn for_topic(topic_text: &str) -> Self {
        let topic = TeachingTopic::from_text(topic_text);
        let steps = Self::steps_for_topic(&topic, topic_text);
        Self::new(topic, steps)
    }
    fn steps_for_topic(topic: &TeachingTopic, original: &str) -> Vec<TeachingStep> {
        match topic {
            TeachingTopic::Derivada => vec![
                TeachingStep::new("d1", "¿Qué es la derivada?", "La derivada es la pendiente instantánea de una curva en un punto. Si tu función es el camino, la derivada te dice cuán inclinado está en cada instante.")
                    .with_math("f(x)=x², f'(x)=2x").with_whiteboard("Dibuja la curva x² y una secante entre dos puntos").with_manim("derivative-slope"),
                TeachingStep::new("d2", "Visualicemos la pendiente", "Mirá cómo la secante entre dos puntos se acerca a la tangente cuando los puntos se juntan. Esa tangente es la derivada.")
                    .with_math("m_sec = (f(x+h)-f(x))/h → f'(x) cuando h→0").with_whiteboard("Secante que colapsa a tangente en x=1").with_manim("derivative-slope"),
                TeachingStep::new("d3", "Grafiquemos f y f'", "Arriba: x² (parábola). Abajo: 2x (recta). Notá cómo la pendiente de la parábola crece linealmente.")
                    .with_math("Gráfica: f(x)=x² y f'(x)=2x").with_whiteboard("Dos ejes: parábola y recta").with_manim("derivative-slope"),
                TeachingStep::new("d4", "Probemos en la pizarra", "Dibujá tu propia función en la pizarra y calculá la derivada en un punto. Usá la herramienta de tangente.")
                    .with_whiteboard("Pizarra libre para trazar y medir pendiente"),
            ],
            TeachingTopic::Integral => vec![
                TeachingStep::new("i1", "¿Qué es la integral?", "La integral es el área bajo la curva. Si la derivada es la pendiente, la integral es la acumulación.")
                    .with_math("∫₀² x² dx = 8/3").with_whiteboard("Área bajo x² entre 0 y 2").with_manim("integral-area"),
                TeachingStep::new("i2", "Aproximación con rectángulos", "Suma de rectángulos de ancho pequeño. Cuando el ancho tiende a cero, la suma es el área exacta.")
                    .with_math("Suma de Riemann: Σ f(xᵢ)Δx").with_whiteboard("Rectángulos bajo la curva").with_manim("integral-area"),
                TeachingStep::new("i3", "Visualicemos el área", "La región sombreada es la integral. Cambiá los límites y mirá cómo cambia el área.")
                    .with_whiteboard("Región sombreada con límites móviles"),
            ],
            TeachingTopic::Pitagoras => vec![
                TeachingStep::new("p1", "Teorema de Pitágoras", "En un triángulo rectángulo, c² = a² + b². La hipotenusa al cuadrado es la suma de los catetos al cuadrado.")
                    .with_math("c² = a² + b², c = √(a²+b²)").with_whiteboard("Triángulo rectángulo con cuadrados en cada lado").with_manim("pitagoras"),
                TeachingStep::new("p2", "Demostración visual", "Los dos cuadrados de los catetos juntos tienen la misma área que el cuadrado de la hipotenusa.")
                    .with_whiteboard("Animación de áreas que se reordenan").with_manim("pitagoras"),
            ],
            TeachingTopic::Funcion => vec![
                TeachingStep::new("f1", "¿Qué es una función?", "Una función asigna a cada x un único y. Pensala como una máquina: entra x, sale f(x).")
                    .with_math(original).with_whiteboard("Ejes con puntos (x, f(x))").with_manim("universal"),
                TeachingStep::new("f2", "Grafiquemos", "Mirá la curva en el canvas y explorá dominio, cortes y extremos.")
                    .with_whiteboard("Curva con ejes y marcas").with_manim("derivative-slope"),
                TeachingStep::new("f3", "Probemos valores", "Cambiá x y observá cómo responde f(x). Probá en la pizarra.")
                    .with_whiteboard("Tabla de valores x→f(x)").with_manim("universal"),
            ],
            TeachingTopic::Limite => vec![
                TeachingStep::new("l1", "Idea de límite", "El límite describe hacia dónde tiende f(x) cuando x se acerca a un valor, aunque f no esté definida ahí.")
                    .with_math("lim_{x→a} f(x) = L").with_whiteboard("Recta con hueco en a").with_manim("derivative-slope"),
                TeachingStep::new("l2", "Acercamiento", "Acerquemos x a a por izquierda y derecha y miremos f(x).")
                    .with_whiteboard("Flechas hacia a").with_manim("derivative-slope"),
                TeachingStep::new("l3", "Calculémoslo", "Usá factorización o sustitución para resolverlo y verificá en la gráfica.")
                    .with_math(original).with_whiteboard("Pizarra para cálculo paso a paso"),
            ],
            _ => {
                // General — selección por complejidad, no todo a la vez
                let is_short = original.trim().chars().count() < 24;
                let has_math_chars = original.contains(['x', 'y', '=', '+', '-', '/', '∫', '√']);
                if is_short && !has_math_chars {
                    vec![
                        TeachingStep::new("g1", "Concepto", format!("Vamos a desglosar: {}", original))
                            .with_whiteboard("Pizarra para explorar"),
                        TeachingStep::new("g2", "Profundicemos", "Hagamos un ejemplo concreto y verifiquémoslo gráficamente.")
                            .with_whiteboard("Ejemplo con gráfica"),
                    ]
                } else {
                    vec![
                        TeachingStep::new("g1", "Exploremos el concepto", format!("Vamos a desglosar: {}", original))
                            .with_math(original).with_whiteboard("Pizarra para explorar"),
                        TeachingStep::new("g2", "Grafiquemos", "Visualizá la función y sus propiedades en el canvas.")
                            .with_whiteboard("Gráfica interactiva").with_manim("universal"),
                        TeachingStep::new("g3", "Practiquemos", "Usá la pizarra para dibujar y la consola para probar valores.")
                            .with_whiteboard("Pizarra libre"),
                    ]
                }
            }
        }
    }
    pub fn current(&self) -> Option<&TeachingStep> {
        self.steps.get(self.current)
    }
    pub fn current_mut(&mut self) -> Option<&mut TeachingStep> {
        self.steps.get_mut(self.current)
    }
    pub fn advance(&mut self) -> bool {
        if self.current + 1 < self.steps.len() {
            if let Some(s) = self.steps.get_mut(self.current) {
                s.completed = true;
            }
            self.current += 1;
            true
        } else {
            if let Some(s) = self.steps.get_mut(self.current) {
                s.completed = true;
            }
            false
        }
    }
    pub fn is_last(&self) -> bool {
        self.current + 1 >= self.steps.len()
    }
    pub fn progress(&self) -> f32 {
        (self.current as f32 + 1.0) / self.steps.len().max(1) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topic_detection() {
        assert_eq!(
            TeachingTopic::from_text("derivada de x³"),
            TeachingTopic::Derivada
        );
        assert_eq!(
            TeachingTopic::from_text("integral de x²"),
            TeachingTopic::Integral
        );
    }
    #[test]
    fn session_advances() {
        let mut s = TeachingSession::for_topic("derivada");
        let n = s.steps.len();
        assert!(n >= 3);
        assert_eq!(s.current, 0);
        assert!(s.advance());
        assert_eq!(s.current, 1);
    }
    #[test]
    fn steps_have_manim() {
        let s = TeachingSession::for_topic("derivada");
        assert!(s.steps.iter().any(|st| st.manim_template.is_some()));
    }
}
