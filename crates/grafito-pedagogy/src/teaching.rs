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
    Fraccion,
    Vector,
    Matriz,
    Probabilidad,
    Serie,
    Ecuacion,
    Trigonometria,
    Conica,
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
        } else if lower.contains("fracc") {
            Self::Fraccion
        } else if lower.contains("vector") {
            Self::Vector
        } else if lower.contains("matriz")
            || lower.contains("matrices")
            || lower.contains("determin")
        {
            Self::Matriz
        } else if lower.contains("probab")
            || lower.contains("estad")
            || lower.contains("bayes")
            || lower.contains("muestreo")
            || lower.contains("regres")
        {
            Self::Probabilidad
        } else if lower.contains("serie")
            || lower.contains("taylor")
            || lower.contains("fourier")
            || lower.contains("sucesi")
        {
            Self::Serie
        } else if lower.contains("ecuac") || lower.contains("sistema") {
            Self::Ecuacion
        } else if lower.contains("trigon")
            || lower.contains("seno")
            || lower.contains("coseno")
            || lower.contains("trig")
            || lower.contains("sen(")
            || lower.contains("cos(")
        {
            Self::Trigonometria
        } else if lower.contains("conica")
            || lower.contains("elipse")
            || lower.contains("parabola")
            || lower.contains("parábola")
            || lower.contains("hiperbola")
            || lower.contains("hipérbola")
        {
            Self::Conica
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
            Self::Fraccion => "Fracciones".into(),
            Self::Vector => "Vectores".into(),
            Self::Matriz => "Matrices".into(),
            Self::Probabilidad => "Probabilidad".into(),
            Self::Serie => "Series".into(),
            Self::Ecuacion => "Ecuaciones".into(),
            Self::Trigonometria => "Trigonometría".into(),
            Self::Conica => "Cónicas".into(),
            Self::General(s) => s.clone(),
        }
    }

    /// ID de LO del currículum más cercano para este tópico.
    /// `Pitagoras` mapea a `sec-pitagoras` (no a `sec-fracc`; ver curriculum.rs).
    pub fn lo_id(&self) -> Option<String> {
        match self {
            Self::Derivada => Some("am1-der".into()),
            Self::Integral => Some("am1-int".into()),
            Self::Limite => Some("am1-lim".into()),
            Self::Funcion => Some("am1-func".into()),
            Self::Pitagoras => Some("sec-pitagoras".into()),
            Self::Fraccion => Some("sec-fracc".into()),
            Self::Vector => Some("sec-vect".into()),
            Self::Matriz => Some("alg-matrices".into()),
            Self::Probabilidad => Some("prob-basica".into()),
            Self::Serie => Some("am2-series".into()),
            Self::Ecuacion => Some("sec-ec".into()),
            Self::Trigonometria => Some("sec-trig".into()),
            Self::Conica => Some("alg-conicas".into()),
            Self::General(_) => None,
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
            TeachingTopic::Fraccion => vec![
                TeachingStep::new("frac1", "¿Qué es una fracción?", "Una fracción a/b representa partes de un todo. El denominador dice en cuántas partes dividimos, el numerador cuántas tomamos.")
                    .with_math("1/2, 3/4, 2/3").with_whiteboard("Rectángulo dividido en partes").with_manim("fraccion-visual"),
                TeachingStep::new("frac2", "Operaciones", "Para sumar, buscá común denominador; para multiplicar, numerador por numerador y denominador por denominador.")
                    .with_math("1/2 + 1/3 = 5/6").with_whiteboard("Rectángulos con común denominador").with_manim("fraccion-visual"),
                TeachingStep::new("frac3", "Practiquemos", "Simplificá y compará fracciones dibujando en la pizarra.")
                    .with_whiteboard("Pizarra con fracciones equivalentes"),
            ],
            TeachingTopic::Vector => vec![
                TeachingStep::new("v1", "¿Qué es un vector?", "Un vector tiene dirección, sentido y módulo. En R² lo pensás como una flecha desde el origen.")
                    .with_math("v = (2,3), |v| = √(13)").with_whiteboard("Flecha en ejes R²").with_manim("vector-anim"),
                TeachingStep::new("v2", "Suma y producto", "Suma componente a componente. Producto escalar da un número, vectorial da otro vector perpendicular.")
                    .with_math("u·v = |u||v|cosθ").with_whiteboard("Dos flechas y su suma").with_manim("vector-anim"),
                TeachingStep::new("v3", "Practiquemos", "Dibujá vectores en la pizarra y calculá su norma y ángulo.")
                    .with_whiteboard("Pizarra vectorial libre"),
            ],
            TeachingTopic::Matriz => vec![
                TeachingStep::new("m1", "¿Qué es una matriz?", "Una matriz es una tabla de números. Representa sistemas lineales y transformaciones.")
                    .with_math("A = [[1,2],[3,4]]").with_whiteboard("Grilla 2x2").with_manim("matriz-anim"),
                TeachingStep::new("m2", "Operaciones y Gauss", "Suma, multiplicación y eliminación de Gauss para resolver sistemas.")
                    .with_math("Ax=b → Gauss-Jordan").with_whiteboard("Matriz aumentada y pivotes").with_manim("matriz-anim"),
                TeachingStep::new("m3", "Determinante e inversa", "El determinante dice si la matriz es invertible. Si det≠0, existe A⁻¹.")
                    .with_math("det A, A⁻¹ = (1/det) adj(A)").with_whiteboard("Cálculo de determinante 2x2"),
            ],
            TeachingTopic::Probabilidad => vec![
                TeachingStep::new("pr1", "Espacio muestral", "Probabilidad mide chance de un evento: casos favorables sobre totales. Empezá listando todos los resultados posibles.")
                    .with_math("P(A)=|A|/|Ω|").with_whiteboard("Diagrama de árbol").with_manim("prob-anim"),
                TeachingStep::new("pr2", "Condicional y Bayes", "Probabilidad condicional: P(A|B)=P(A∩B)/P(B). Bayes invierte la condición.")
                    .with_math("P(A|B)=P(B|A)P(A)/P(B)").with_whiteboard("Tabla de contingencia").with_manim("prob-anim"),
                TeachingStep::new("pr3", "Distribuciones", "Binomial, Poisson, Normal: cada una modela un tipo de fenómeno aleatorio.")
                    .with_math("X~N(μ,σ²)").with_whiteboard("Curva normal sombreada"),
            ],
            TeachingTopic::Serie => vec![
                TeachingStep::new("ser1", "Sucesiones y series", "Una serie suma infinitos términos. Converge si sus sumas parciales se acercan a un límite.")
                    .with_math("Σ aₙ, Sₙ = a₁+...+aₙ").with_whiteboard("Suma parcial que se aproxima").with_manim("serie-anim"),
                TeachingStep::new("ser2", "Criterios", "Criterios de convergencia: D'Alembert, Cauchy, integral. Probá con la geométrica.")
                    .with_math("Σ rⁿ converge si |r|<1").with_whiteboard("Serie geométrica en pizarra").with_manim("serie-anim"),
                TeachingStep::new("ser3", "Taylor", "Taylor aproxima funciones con polinomios. Más términos, mejor aproximación local.")
                    .with_math("f(x)≈ Σ f⁽ⁿ⁾(a)/n! (x-a)ⁿ").with_whiteboard("Polinomios que se acercan a la curva"),
            ],
            TeachingTopic::Ecuacion => vec![
                TeachingStep::new("ec1", "Ecuación lineal", "Ecuación lineal: a·x+b=0 → x=-b/a. Representa recta que cruza el eje.")
                    .with_math("2x+3=7 → x=2").with_whiteboard("Recta y corte con eje").with_manim("ecuacion-anim"),
                TeachingStep::new("ec2", "Cuadrática", "Cuadrática: ax²+bx+c=0 → fórmula con discriminante Δ=b²-4ac.")
                    .with_math("x = (-b±√Δ)/2a").with_whiteboard("Parábola y raíces").with_manim("ecuacion-anim"),
                TeachingStep::new("ec3", "Sistemas", "Sistemas: dos ecuaciones, dos incógnitas. Resolvé por sustitución o Gauss.")
                    .with_whiteboard("Dos rectas que se cortan"),
            ],
            TeachingTopic::Trigonometria => vec![
                TeachingStep::new("trig1", "Seno y coseno", "En el círculo unitario, cos es x, sin es y. Hipotenusa 1, catetos cos y sin.")
                    .with_math("sin²+cos²=1").with_whiteboard("Círculo unitario con ángulo").with_manim("trig-anim"),
                TeachingStep::new("trig2", "Identidades", "Identidades relacionan ángulos: sin(a+b)=sin a cos b + cos a sin b.")
                    .with_math("sin(π/2)=1, cos(π)= -1").with_whiteboard("Triángulo y círculo").with_manim("trig-anim"),
                TeachingStep::new("trig3", "Gráficas", "Ondas seno y coseno: periódicas, amplitud 1, período 2π.")
                    .with_whiteboard("Onda seno en ejes"),
            ],
            TeachingTopic::Conica => vec![
                TeachingStep::new("con1", "Cónicas", "Cónicas: cortás un cono con un plano y obtenés circunferencia, elipse, parábola o hipérbola.")
                    .with_math("x²/a² + y²/b² =1 (elipse)").with_whiteboard("Cono cortado").with_manim("conica-anim"),
                TeachingStep::new("con2", "Ecuaciones canónicas", "Cada cónica tiene ecuación canónica con centro y ejes. Cambiá parámetros y mirá el gráfico.")
                    .with_whiteboard("Elipse con focos"),
                TeachingStep::new("con3", "Practiquemos", "Dibujá la cónica en la pizarra y reconocé sus elementos (focos, vértices).")
                    .with_whiteboard("Pizarra cónica"),
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

    /// Crea un FSM socrático inicializado con el tópico de la sesión.
    pub fn socratic_fsm(&self) -> crate::socratic::SocraticFsm {
        crate::socratic::SocraticFsm::new(self.topic.label())
    }

    /// Crea FSM con epoch para `AwaitStudent` inicial (útil para tests).
    pub fn socratic_fsm_awaiting(&self, deadline_epoch: u64) -> crate::socratic::SocraticFsm {
        let mut fsm = crate::socratic::SocraticFsm::new(self.topic.label());
        fsm.await_student(deadline_epoch);
        fsm
    }

    /// Helper: sesión desde texto + FSM listo para usar.
    pub fn for_topic_with_fsm(topic_text: &str) -> (Self, crate::socratic::SocraticFsm) {
        let session = Self::for_topic(topic_text);
        let fsm = session.socratic_fsm();
        (session, fsm)
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
        assert_eq!(
            TeachingTopic::from_text("fracciones 1/2"),
            TeachingTopic::Fraccion
        );
        assert_eq!(
            TeachingTopic::from_text("vector en R3"),
            TeachingTopic::Vector
        );
        assert_eq!(
            TeachingTopic::from_text("matrices 2x2"),
            TeachingTopic::Matriz
        );
        assert_eq!(
            TeachingTopic::from_text("probabilidad condicional"),
            TeachingTopic::Probabilidad
        );
        assert_eq!(
            TeachingTopic::from_text("serie de Taylor"),
            TeachingTopic::Serie
        );
        assert_eq!(
            TeachingTopic::from_text("ecuación cuadrática"),
            TeachingTopic::Ecuacion
        );
        assert_eq!(
            TeachingTopic::from_text("trigonometría seno"),
            TeachingTopic::Trigonometria
        );
        assert_eq!(
            TeachingTopic::from_text("cónica elipse"),
            TeachingTopic::Conica
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
    #[test]
    fn fraccion_vector_steps() {
        let s = TeachingSession::for_topic("fracciones equivalentes");
        assert!(s.steps.len() >= 2);
        assert_eq!(s.topic, TeachingTopic::Fraccion);
        let v = TeachingSession::for_topic("vectores en R3");
        assert_eq!(v.topic, TeachingTopic::Vector);
        assert!(v.steps.len() >= 2);
    }
    #[test]
    fn socratic_fsm_helpers() {
        let session = TeachingSession::for_topic("derivada");
        let fsm = session.socratic_fsm();
        assert_eq!(fsm.topic, "Derivada");
        let (s2, fsm2) = TeachingSession::for_topic_with_fsm("vectores");
        assert_eq!(s2.topic, TeachingTopic::Vector);
        assert_eq!(fsm2.topic, "Vectores");
        let awaiting = session.socratic_fsm_awaiting(12345);
        assert!(matches!(
            awaiting.state,
            crate::socratic::SocraticState::AwaitStudent {
                deadline_epoch: 12345
            }
        ));
    }
    #[test]
    fn lo_id_mapping() {
        assert_eq!(
            TeachingTopic::Fraccion.lo_id().as_deref(),
            Some("sec-fracc")
        );
        assert_eq!(TeachingTopic::Vector.lo_id().as_deref(), Some("sec-vect"));
        assert_eq!(
            TeachingTopic::Matriz.lo_id().as_deref(),
            Some("alg-matrices")
        );
        assert!(TeachingTopic::General("x".into()).lo_id().is_none());
    }
}
