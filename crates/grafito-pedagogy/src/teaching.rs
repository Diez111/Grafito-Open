//! Sistema de enseñanza paso a paso — burbujas que se transforman, pizarra y manim.
//!
//! Modelo puro sin egui: `TeachingSession` con 3-6 pasos, cada paso con
//! texto, expresión matemática, elementos de pizarra y especificación de
//! animación manim. La orquestación agéntica compleja usa `grafito-anim`
//! para generar animaciones 3b1b/manim.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::exercise::{Exercise, ExerciseDifficulty, ExerciseKind, ValidatorKind};
use crate::feedback::{Feedback, FeedbackEngine};

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
    Subespacio,
    Fractal,
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
        } else if lower.contains("subespacio")
            || lower.contains("span")
            || lower.contains("combinacion")
            || lower.contains("combinación")
        {
            Self::Subespacio
        } else if lower.contains("fractal")
            || lower.contains("koch")
            || lower.contains("mandelbrot")
            || lower.contains("julia")
            || lower.contains("autosimil")
        {
            Self::Fractal
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
            Self::Subespacio => "Subespacios".into(),
            Self::Fractal => "Fractales".into(),
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
            Self::Subespacio => Some("alg-subespacios".into()),
            Self::Fractal => Some("sec-fractales".into()),
            Self::General(_) => None,
        }
    }
}

/// Verificación final de un paso (remate): el estudiante calcula y se corrige
/// con `FeedbackEngine::assess` (voz Mili la pone la UI, acá solo datos).
///
/// API pura para el integrador (asistente): si `step.check` es `Some`, la UI
/// debe pedir respuesta y corregir con [`TeachingStep::assess_final`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCheck {
    /// Consigna (ej. "Si f(x)=x², ¿cuánto vale f'(1)?").
    pub probe: String,
    /// Respuesta esperada (ej. "2").
    pub expected: String,
    /// Validador (típico `NumericTol(0.02)`).
    pub validator: ValidatorKind,
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
    /// CAS-gate: `true` solo si `math_expr` parseó vía `verify_math_expr`
    /// (geometry `prepare_function_ast`) al construir la sesión.
    #[serde(default)]
    pub verified: bool,
    /// Binding temporal: la pizarra hidrata este paso cuando el elapsed de la
    /// sesión supera `cue_ms` (ver `TeachingSession::revealed_steps`). `0` =
    /// visible desde el inicio (comportamiento histórico).
    #[serde(default)]
    pub cue_ms: u64,
    /// Ventana de frames sugerida sobre el loop de animación (índices
    /// absolutos; debe vivir dentro del total real o `cue_frame_index`
    /// devuelve `None`). `None` = loop completo.
    #[serde(default)]
    pub frame_range: Option<(u32, u32)>,
    /// Remate verificable del paso (`None` = paso expositivo).
    #[serde(default)]
    pub check: Option<StepCheck>,
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
            verified: false,
            cue_ms: 0,
            frame_range: None,
            check: None,
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
    /// Binding temporal: pizarra de este paso visible con elapsed >= `cue_ms`.
    pub fn with_cue(mut self, cue_ms: u64) -> Self {
        self.cue_ms = cue_ms;
        self
    }
    /// Ventana de frames sugerida `[start, end)` sobre el loop de animación.
    /// Se normaliza: `end <= start` → `None` (loop completo, honesto).
    pub fn with_frames(mut self, start: u32, end: u32) -> Self {
        self.frame_range = if end > start {
            Some((start, end))
        } else {
            None
        };
        self
    }
    /// Remate verificable: consigna + respuesta esperada (corrige con
    /// `FeedbackEngine::assess`, tolerancia numérica 2 %).
    pub fn with_final_check(
        mut self,
        probe: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        self.check = Some(StepCheck {
            probe: probe.into(),
            expected: expected.into(),
            validator: ValidatorKind::NumericTol(0.02),
        });
        self
    }
    /// Corrige la respuesta del remate (`None` si el paso no tiene `check`).
    /// Pura: construye un `Exercise` mínimo y delega a `FeedbackEngine`.
    pub fn assess_final(&self, answer: &str) -> Option<Feedback> {
        let check = self.check.as_ref()?;
        let exercise = Exercise {
            prompt: check.probe.clone(),
            solution: check.expected.clone(),
            kind: ExerciseKind::Numeric,
            difficulty: ExerciseDifficulty::Easy,
            lo_id: self.id.clone(),
            params: BTreeMap::new(),
            seed: None,
            validator: check.validator,
        };
        Some(FeedbackEngine.assess(&exercise, answer))
    }
}

/// CAS-gate puro: ¿`expr` es una expresión computable (no prosa matemática)?
///
/// Valida vía geometry `prepare_function_ast` (mismo parser del canvas: la
/// pizarra jamás muestra como verificada una expresión que el canvas no puede
/// evaluar). Rechaza vacío, `=` (ecuaciones/derivadas tipo `f'(x)=2x`), prosa
/// con espacios múltiples/símbolos de integral/suma (`∫`, `Σ`, `→`) y texto
/// largo. Puro, sin I/O, sin `unwrap`.
///
/// Para la UI (MathTex overlay): lo verificado se dibuja con
/// `grafito_ui::assistant::draw_math` (fuente `$..$`); lo no verificado cae a
/// texto honesto y `TeachingSession::new` lo descarta (`math_expr = None`).
pub fn verify_math_expr(expr: &str) -> bool {
    let text = expr.trim();
    if text.is_empty() || text.len() > 200 {
        return false;
    }
    if text.contains(['=', '∫', 'Σ', '→', ';', '\n']) {
        return false;
    }
    let vars: BTreeMap<String, f64> = BTreeMap::new();
    grafito_geometry::expr::prepare_function_ast(text, &vars, &[]).is_ok()
}

/// ¿La prosa afirma un número-resultado que el CAS no cubre? (R6e)
///
/// Detecta afirmaciones explícitas de resultado en `explanation` y exige que
/// cada número afirmado aparezca en `math_expr` o en `check_expected`:
/// - keywords de resultado (`vale`, `valen`, `da`, `dan`, `dar`,
///   `resultado(s)`, `igual(es)`, `equivale(n)`) + número,
/// - `=` seguido de número (`f'(1)=2`, `x=-1.5`),
/// - `≈` seguido de número.
///
/// Cobertura textual, no semántica: basta que el token numérico aparezca en
/// la math o en el remate. Conservadora en el otro sentido: `h→0`, `paso 3`,
/// preguntas (`¿cuánto vale c?`), fórmulas (`f(x)=x²`, `c² = a² + b²`) y
/// `x=-b/a` NO son afirmaciones (el `=` no va seguido de dígito). Pura, sin
/// regex ni `unwrap`.
fn prose_claims_uncovered_numbers(
    explanation: &str,
    math_expr: Option<&str>,
    check_expected: Option<&str>,
) -> bool {
    fn is_num_start(s: &str) -> bool {
        let t = s.trim_start_matches([' ', '\t', ':', ',']);
        let t = t.strip_prefix('-').unwrap_or(t);
        t.starts_with(|c: char| c.is_ascii_digit())
    }
    fn take_number(s: &str) -> &str {
        let t = s.trim_start_matches([' ', '\t', ':', ',']);
        let t = t.strip_prefix('-').unwrap_or(t);
        let mut end = 0usize;
        for (i, c) in t.char_indices() {
            if c.is_ascii_digit() || c == '.' || c == ',' || c == '/' {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        &t[..end]
    }
    fn covered(number: &str, math_expr: Option<&str>, check_expected: Option<&str>) -> bool {
        if number.is_empty() {
            return true;
        }
        let norm = |s: &str| s.replace([' ', '\t'], "");
        math_expr.is_some_and(|m| norm(m).contains(number))
            || check_expected.is_some_and(|e| norm(e).contains(number))
    }
    // Ocurrencias de keyword con borde de palabra (sin substring: `da` en
    // `verificada` no cuenta). Los keywords son ASCII puros, así que los
    // offsets del `lower` valen para el original.
    const KEYWORDS: &[&str] = &[
        "vale",
        "valen",
        "da",
        "dan",
        "dar",
        "resultado",
        "resultados",
        "igual",
        "iguales",
        "equivale",
        "equivalen",
    ];
    let lower = explanation.to_lowercase();
    let mut claimed: Vec<&str> = Vec::new();
    let orig = explanation;
    let low_bytes = lower.as_bytes();
    for key in KEYWORDS {
        let mut from = 0usize;
        while from + key.len() <= low_bytes.len() {
            let Some(rel) = lower[from..].find(key) else {
                break;
            };
            let abs = from + rel;
            let before_ok = abs == 0
                || !lower[..abs]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphabetic());
            let after = abs + key.len();
            let after_ok = lower[after..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphabetic());
            if before_ok && after_ok {
                let tail = &orig[after.min(orig.len())..];
                if is_num_start(tail) {
                    claimed.push(take_number(tail));
                }
            }
            from = abs + key.len().max(1);
        }
    }
    // `=` / `≈` seguidos de número (fórmulas con rhs no numérico no cuentan).
    for (idx, ch) in explanation.char_indices() {
        if ch == '=' || ch == '≈' {
            let tail = &explanation[idx + ch.len_utf8()..];
            if is_num_start(tail) {
                claimed.push(take_number(tail));
            }
        }
    }
    claimed
        .iter()
        .any(|n| !covered(n, math_expr, check_expected))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingSession {
    pub topic: TeachingTopic,
    pub steps: Vec<TeachingStep>,
    pub current: usize,
    pub created_epoch: u64,
}

impl TeachingSession {
    /// Construye aplicando el CAS-gate + gate de prosa (R6e): cada `math_expr`
    /// que no parsea se descarta (`None`, `verified = false`); la que parsea
    /// se conserva pero solo queda `verified = true` si la prosa no afirma
    /// números-resultado fuera de la math y del remate (el CAS avaló la
    /// expresión, no la afirmación). El paso se conserva siempre.
    pub fn new(topic: TeachingTopic, steps: Vec<TeachingStep>) -> Self {
        let steps = steps
            .into_iter()
            .map(|mut step| {
                match step.math_expr.as_deref() {
                    Some(expr) if verify_math_expr(expr) => {
                        let check_expected = step.check.as_ref().map(|c| c.expected.as_str());
                        step.verified = !prose_claims_uncovered_numbers(
                            &step.explanation,
                            step.math_expr.as_deref(),
                            check_expected,
                        );
                    }
                    Some(_) => {
                        step.math_expr = None;
                        step.verified = false;
                    }
                    None => step.verified = false,
                }
                step
            })
            .collect();
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
                TeachingStep::new("d1", "¿Qué es la derivada?", "La derivada es la pendiente instantánea de una curva en un punto. Si tu función es el camino, la derivada te dice cuán inclinado está en cada instante. Abajo ves verificada f(x)=x²; su derivada f'(x)=2x la graficamos en el paso 3.")
                    .with_math("x^2").with_whiteboard("Dibuja la curva x² y una secante entre dos puntos").with_manim("derivative-slope").with_cue(0).with_frames(0, 12),
                TeachingStep::new("d2", "Visualicemos la pendiente", "Mirá cómo la secante entre dos puntos se acerca a la tangente cuando los puntos se juntan. Esa tangente es la derivada: el cociente diferencial de abajo tiende a f'(x) cuando h→0.")
                    .with_math("((x+h)^2-x^2)/h").with_whiteboard("Secante que colapsa a tangente en x=1").with_manim("derivative-slope").with_cue(5_000).with_frames(12, 24),
                TeachingStep::new("d3", "Grafiquemos f y f'", "Arriba: x² (parábola). Abajo: 2x (recta). Notá cómo la pendiente de la parábola crece linealmente.")
                    .with_math("2*x").with_whiteboard("Dos ejes: parábola y recta").with_manim("derivative-slope").with_cue(10_000).with_frames(24, 36),
                TeachingStep::new("d4", "Verificá en x=1", "Si f(x)=x², su derivada es f'(x)=2x. Calculá en x=1: debe dar 2. Escribí tu resultado y lo corregimos juntos.")
                    .with_whiteboard("Ejes con tangente en x=1 marcada").with_cue(15_000).with_frames(36, 48)
                    .with_final_check("Si f(x)=x², ¿cuánto vale f'(1)?", "2"),
            ],
            TeachingTopic::Integral => vec![
                TeachingStep::new("i1", "¿Qué es la integral?", "La integral es el área bajo la curva. Si la derivada es la pendiente, la integral es la acumulación. Abajo ves verificada la función; el área entre 0 y 2 vale 8/3.")
                    .with_math("x^2").with_whiteboard("Área bajo x² entre 0 y 2").with_manim("integral-area").with_cue(0),
                TeachingStep::new("i2", "Aproximación con rectángulos", "Suma de rectángulos de ancho pequeño. Cuando el ancho tiende a cero, la suma es el área exacta.")
                    .with_whiteboard("Rectángulos bajo la curva").with_manim("integral-area").with_cue(5_000),
                TeachingStep::new("i3", "Verificá el área", "La región sombreada es la integral. Si el área bajo x² entre 0 y 2 vale 8/3 ≈ 2.67, ¿cuánto vale redondeado a 2 decimales? Escribilo y lo corregimos.")
                    .with_whiteboard("Región sombreada con límites móviles").with_cue(10_000)
                    .with_final_check("Área bajo x² entre 0 y 2, a 2 decimales", "2.67"),
            ],
            TeachingTopic::Pitagoras => vec![
                TeachingStep::new("p1", "Teorema de Pitágoras", "En un triángulo rectángulo, c² = a² + b². La hipotenusa al cuadrado es la suma de los catetos al cuadrado. Abajo ves verificada la relación.")
                    .with_math("a^2+b^2").with_whiteboard("Triángulo rectángulo con cuadrados en cada lado").with_manim("pitagoras").with_cue(0),
                TeachingStep::new("p2", "Verificá con 3-4-5", "Los dos cuadrados de los catetos juntos tienen la misma área que el cuadrado de la hipotenusa. Si a=3 y b=4, ¿cuánto vale c? Escribilo y lo corregimos.")
                    .with_whiteboard("Animación de áreas que se reordenan").with_manim("pitagoras").with_cue(5_000)
                    .with_final_check("Si a=3 y b=4, ¿cuánto vale c?", "5"),
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
                TeachingStep::new("l1", "Idea de límite", "El límite describe hacia dónde tiende f(x) cuando x se acerca a un valor, aunque f no esté definida ahí. Abajo ves verificada la función clásica del hueco removible.")
                    .with_math("(x^2-1)/(x-1)").with_whiteboard("Recta con hueco en a").with_manim("derivative-slope").with_cue(0),
                TeachingStep::new("l2", "Acercamiento", "Acerquemos x a a por izquierda y derecha y miremos f(x).")
                    .with_whiteboard("Flechas hacia a").with_manim("derivative-slope").with_cue(5_000),
                TeachingStep::new("l3", "Calculémoslo", "Usá factorización o sustitución para resolverlo y verificá en la gráfica.")
                    .with_math(original).with_whiteboard("Pizarra para cálculo paso a paso").with_cue(10_000),
            ],
            TeachingTopic::Fraccion => vec![
                TeachingStep::new("frac1", "¿Qué es una fracción?", "Una fracción a/b representa partes de un todo. El denominador dice en cuántas partes dividimos, el numerador cuántas tomamos.")
                    .with_math("1/2+1/3").with_whiteboard("Rectángulo dividido en partes").with_manim("fraccion-visual").with_cue(0),
                TeachingStep::new("frac2", "Operaciones", "Para sumar, buscá común denominador; para multiplicar, numerador por numerador y denominador por denominador. Abajo ves verificada la suma (da 5/6).")
                    .with_math("1/2+1/3").with_whiteboard("Rectángulos con común denominador").with_manim("fraccion-visual").with_cue(5_000),
                TeachingStep::new("frac3", "Practiquemos", "Simplificá y compará fracciones dibujando en la pizarra.")
                    .with_whiteboard("Pizarra con fracciones equivalentes"),
            ],
            TeachingTopic::Vector => vec![
                TeachingStep::new("v1", "¿Qué es un vector?", "Un vector tiene dirección, sentido y módulo. En R² lo pensás como una flecha desde el origen. Abajo ves verificada la norma de (2,3).")
                    .with_math("sqrt(13)").with_whiteboard("Flecha en ejes R²").with_manim("vector-anim").with_cue(0),
                TeachingStep::new("v2", "Suma y producto", "Suma componente a componente. Producto escalar da un número, vectorial da otro vector perpendicular.")
                    .with_math("cos(x)").with_whiteboard("Dos flechas y su suma").with_manim("vector-anim").with_cue(5_000),
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
                TeachingStep::new("ec1", "Ecuación lineal", "Ecuación lineal: a·x+b=0 → x=-b/a. Representa recta que cruza el eje. Abajo ves verificada la función (el cero está en x=-1.5).")
                    .with_math("2*x+3").with_whiteboard("Recta y corte con eje").with_manim("ecuacion-anim").with_cue(0),
                TeachingStep::new("ec2", "Cuadrática", "Cuadrática: ax²+bx+c=0 → fórmula con discriminante Δ=b²-4ac.")
                    .with_math("x = (-b±√Δ)/2a").with_whiteboard("Parábola y raíces").with_manim("ecuacion-anim"),
                TeachingStep::new("ec3", "Sistemas", "Sistemas: dos ecuaciones, dos incógnitas. Resolvé por sustitución o Gauss.")
                    .with_whiteboard("Dos rectas que se cortan"),
            ],
            TeachingTopic::Trigonometria => vec![
                TeachingStep::new("trig1", "Seno y coseno", "En el círculo unitario, cos es x, sin es y. Hipotenusa 1, catetos cos y sin. Abajo ves verificada la identidad (vale 1 para todo x).")
                    .with_math("sin(x)^2+cos(x)^2").with_whiteboard("Círculo unitario con ángulo").with_manim("trig-anim").with_cue(0),
                TeachingStep::new("trig2", "Identidades", "Identidades relacionan ángulos: sin(a+b)=sin a cos b + cos a sin b.")
                    .with_math("sin(x)").with_whiteboard("Triángulo y círculo").with_manim("trig-anim").with_cue(5_000),
                TeachingStep::new("trig3", "Gráficas", "Ondas seno y coseno: periódicas, amplitud 1, período 2π.")
                    .with_whiteboard("Onda seno en ejes"),
            ],
            TeachingTopic::Conica => vec![
                TeachingStep::new("con1", "Cónicas", "Cónicas: cortás un cono con un plano y obtenés circunferencia, elipse, parábola o hipérbola. Abajo ves verificada la forma de la elipse (igualada a 1 en la gráfica).")
                    .with_math("x^2/a^2+y^2/b^2").with_whiteboard("Cono cortado").with_manim("conica-anim").with_cue(0),
                TeachingStep::new("con2", "Ecuaciones canónicas", "Cada cónica tiene ecuación canónica con centro y ejes. Cambiá parámetros y mirá el gráfico.")
                    .with_whiteboard("Elipse con focos"),
                TeachingStep::new("con3", "Practiquemos", "Dibujá la cónica en la pizarra y reconocé sus elementos (focos, vértices).")
                    .with_whiteboard("Pizarra cónica"),
            ],
            TeachingTopic::Subespacio => vec![
                TeachingStep::new("sub1", "¿Qué es un subespacio?", "Un subespacio es un conjunto de vectores cerrado bajo suma y producto por escalar: si combinás vectores del conjunto, nunca salís de él. Abajo ves verificada la combinación general.")
                    .with_math("a*u+b*v").with_whiteboard("Plano con dos vectores generadores y su paralelogramo").with_manim("subspace").with_cue(0),
                TeachingStep::new("sub2", "Combinación lineal y span", "El span es todo lo que podés alcanzar mezclando los generadores. Cada punto del plano es una mezcla con pesos distintos. Abajo ves verificada una mezcla concreta.")
                    .with_math("2*u+3*v").with_whiteboard("Mezclas con pesos que barren el plano").with_manim("subspace").with_cue(5_000),
                TeachingStep::new("sub3", "Base y dimensión", "Una base es un equipo mínimo e independiente que genera todo el subespacio; la dimensión cuenta cuántos vectores trae esa base. Independientes y justos, sin redundancia.")
                    .with_math("u+v").with_whiteboard("Base de dos flechas independientes y su grilla").with_manim("subspace").with_cue(10_000),
                TeachingStep::new("sub4", "Verificá la dimensión", "Si los generadores son independientes, el span llena el plano. Calculá la dimensión del span y escribila: la corregimos juntos.")
                    .with_whiteboard("Plano con ejes u y v marcados").with_manim("subspace").with_cue(15_000)
                    .with_final_check("Si u=(1,0) y v=(0,1), ¿cuál es la dimensión del span(u,v)?", "2"),
            ],
            TeachingTopic::Fractal => vec![
                TeachingStep::new("fr1", "Autosimilitud", "Un fractal se parece a sí mismo en cada escala: cada porción repite el todo. Hacé zoom mental y la forma vuelve a aparecer. Abajo ves verificada la regla de iteración compleja.")
                    .with_math("z^2+c").with_whiteboard("Zoom que repite la misma forma").with_manim("fractal").with_cue(0),
                TeachingStep::new("fr2", "Iteración", "Cada paso repite la misma regla sobre el resultado anterior. En el copo, cada segmento se quiebra y la cuenta crece como potencias de cuatro. Abajo ves verificada la cuenta de la segunda vuelta.")
                    .with_math("3*4^2").with_whiteboard("Segmento que se quiebra paso a paso").with_manim("fractal").with_cue(5_000),
                TeachingStep::new("fr3", "Dimensión fractal", "La dimensión fractal mide cómo llena el espacio: más que una línea, menos que un plano. El copo arruga tanto el borde que su dimensión vive entre ambas. Abajo ves verificada la razón de crecimiento.")
                    .with_math("4/3").with_whiteboard("Borde arrugado entre línea y plano").with_manim("fractal").with_cue(10_000),
                TeachingStep::new("fr4", "Verificá el conteo", "El copo parte de pocos segmentos y cada vuelta multiplica por cuatro. Contá los segmentos tras la primera vuelta y escribí el número: lo corregimos juntos.")
                    .with_whiteboard("Copo con segmentos numerados").with_manim("fractal").with_cue(15_000)
                    .with_final_check("El copo parte de 3 segmentos y cada iteración multiplica por 4. ¿Cuántos hay tras 1 iteración?", "12"),
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

    /// Binding temporal: pasos cuya pizarra ya puede hidratarse con
    /// `elapsed_ms` desde el inicio de la sesión (`cue_ms <= elapsed_ms`).
    ///
    /// La UI llama esto en `tick()` e hidrata la pizarra con la unión de los
    /// hints revelados, en vez de todo el hint de golpe. Puro, sin I/O.
    /// API para el integrador (asistente): el cableado a `TeachingUiState`
    /// vive en `grafito-app/src/teaching_ui.rs` (`poll_cues`).
    pub fn revealed_steps(&self, elapsed_ms: u64) -> Vec<&TeachingStep> {
        self.steps
            .iter()
            .filter(|step| step.cue_ms <= elapsed_ms)
            .collect()
    }

    /// Cantidad de pasos revelados (para detectar cambios de cue en `tick`).
    pub fn revealed_count(&self, elapsed_ms: u64) -> usize {
        self.steps
            .iter()
            .filter(|step| step.cue_ms <= elapsed_ms)
            .count()
    }

    /// Hints de pizarra revelados hasta `elapsed_ms`, unidos para hidratar.
    /// Vacío si nada reveló todavía.
    pub fn revealed_hints(&self, elapsed_ms: u64) -> String {
        self.revealed_steps(elapsed_ms)
            .iter()
            .map(|step| step.whiteboard_hint.trim())
            .filter(|hint| !hint.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Índice de frame dentro de `step.frame_range` para `elapsed_ms`.
    ///
    /// `fps` típico 12; `total_frames` es el largo real del loop. `None` en
    /// `frame_range` → loop completo (`elapsed*fps % total`). `total == 0`,
    /// `fps` no finito/`<= 0` o ventana fuera del loop (`start >= total` o
    /// `end > total`) → `None` honesto (R6e: jamás módulo silencioso sobre
    /// una ventana que no existe). Puro, sin I/O ni `unwrap`.
    pub fn cue_frame_index(
        step: &TeachingStep,
        elapsed_ms: u64,
        fps: f32,
        total_frames: usize,
    ) -> Option<usize> {
        if total_frames == 0 || !fps.is_finite() || fps <= 0.0 {
            return None;
        }
        let tick = (elapsed_ms as f32 / 1000.0 * fps) as u64;
        let total = total_frames as u64;
        match step.frame_range {
            None => Some((tick % total) as usize),
            Some((start, end)) => {
                if end <= start || start as u64 >= total || end as u64 > total {
                    return None;
                }
                let len = end.saturating_sub(start).max(1) as u64;
                Some((start as u64 + tick % len) as usize)
            }
        }
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
    fn cas_gate_rechaza_prosa_y_acepta_expresiones() {
        // Lo que hoy vive en strings sin validar debe rechazarse.
        for mala in [
            "f'(x)=2x",
            "∫₀²x²=8/3",
            "m_sec = (f(x+h)-f(x))/h → f'(x) cuando h→0",
            "c² = a² + b², c = √(a²+b²)",
            "Suma de Riemann: Σ f(xᵢ)Δx",
            "",
            "   ",
        ] {
            assert!(!verify_math_expr(mala), "debía rechazar: {mala}");
        }
        for buena in [
            "x^2",
            "2*x",
            "((x+h)^2-x^2)/h",
            "(x^2-1)/(x-1)",
            "1/2+1/3",
            "sqrt(13)",
            "sin(x)^2+cos(x)^2",
            "x^2/a^2+y^2/b^2",
            "2*x+3",
            "a^2+b^2",
        ] {
            assert!(verify_math_expr(buena), "debía aceptar: {buena}");
        }
    }
    #[test]
    fn sesion_marca_verified_y_descarta_lo_que_no_parsea() {
        let s = TeachingSession::for_topic("derivada");
        assert!(s
            .steps
            .iter()
            .any(|st| st.verified && st.math_expr.is_some()));
        // Ningún paso conserva math sin verificar.
        for st in &s.steps {
            assert!(
                st.math_expr.is_none() || st.verified,
                "math sin verificar en {}",
                st.id
            );
        }
        // Constructor directo también aplica el gate.
        let trucha = TeachingSession::new(
            TeachingTopic::Derivada,
            vec![TeachingStep::new("t", "T", "E").with_math("f'(x)=2x")],
        );
        assert!(trucha.steps[0].math_expr.is_none());
        assert!(!trucha.steps[0].verified);
    }
    #[test]
    fn remate_d4_verifica_con_feedback() {
        let s = TeachingSession::for_topic("derivada");
        let d4 = s.steps.iter().find(|st| st.id == "d4").expect("d4 existe");
        let check = d4.check.as_ref().expect("d4 tiene remate");
        assert!(check.probe.contains("f'(1)"));
        assert_eq!(check.expected, "2");
        let bien = d4.assess_final("2").expect("assess");
        assert!(bien.correct);
        let mal = d4.assess_final("5").expect("assess");
        assert!(!mal.correct);
        // Paso sin check → None honesto.
        assert!(s.steps[0].assess_final("2").is_none());
    }
    #[test]
    fn cues_revelan_progresivo_y_frames_en_rango() {
        let s = TeachingSession::for_topic("derivada");
        assert_eq!(s.revealed_count(0), 1);
        assert_eq!(s.revealed_count(4_999), 1);
        assert_eq!(s.revealed_count(5_000), 2);
        assert_eq!(s.revealed_count(u64::MAX), s.steps.len());
        assert!(!s.revealed_hints(0).is_empty());
        // Frames: d1 ventana [0,12) sobre loop de 48.
        let d1 = &s.steps[0];
        assert_eq!(d1.frame_range, Some((0, 12)));
        let idx = TeachingSession::cue_frame_index(d1, 0, 12.0, 48).expect("idx");
        assert!(idx < 12, "{idx}");
        // Sin rango → loop completo; total 0 → None.
        let sin_rango = TeachingStep::new("x", "X", "E");
        assert_eq!(
            TeachingSession::cue_frame_index(&sin_rango, 1000, 12.0, 48),
            Some(12)
        );
        assert_eq!(
            TeachingSession::cue_frame_index(&sin_rango, 0, 12.0, 0),
            None
        );
        // Rango inválido se normaliza a None.
        assert_eq!(
            TeachingStep::new("x", "X", "E")
                .with_frames(5, 5)
                .frame_range,
            None
        );
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
        assert_eq!(
            TeachingTopic::Subespacio.lo_id().as_deref(),
            Some("alg-subespacios")
        );
        assert_eq!(
            TeachingTopic::Fractal.lo_id().as_deref(),
            Some("sec-fractales")
        );
        assert!(TeachingTopic::General("x".into()).lo_id().is_none());
    }
    #[test]
    fn r6e_prosa_con_numeros_sin_gate_no_verifica_conservando_math() {
        // Math válida pero la prosa afirma 8/3 que ni la math ni el remate
        // cubren: se conserva la math y verified=false (el CAS avaló la
        // expresión, no la afirmación).
        let s = TeachingSession::new(
            TeachingTopic::Integral,
            vec![TeachingStep::new("t", "T", "el área bajo la curva vale 8/3").with_math("x^2")],
        );
        assert_eq!(s.steps[0].math_expr.as_deref(), Some("x^2"));
        assert!(!s.steps[0].verified);
        // Control: misma math sin afirmaciones en prosa → verificado.
        let s2 = TeachingSession::new(
            TeachingTopic::Integral,
            vec![TeachingStep::new("t", "T", "mirá la curva en el canvas").with_math("x^2")],
        );
        assert!(s2.steps[0].verified);
        // Remate que cubre el número afirmado → verificado.
        let s3 = TeachingSession::new(
            TeachingTopic::Integral,
            vec![TeachingStep::new("t", "T", "el área vale 8/3")
                .with_math("x^2")
                .with_final_check("área bajo x² entre 0 y 2", "8/3")],
        );
        assert!(s3.steps[0].verified);
        // Preguntas, fórmulas y notación de límite no son afirmaciones.
        let s4 = TeachingSession::new(
            TeachingTopic::Derivada,
            vec![TeachingStep::new(
                "t",
                "T",
                "¿cuánto vale c? mirá f(x)=x² en el paso 3 cuando h→0",
            )
            .with_math("x^2")],
        );
        assert!(s4.steps[0].verified, "pregunta/fórmula no es afirmación");
        // `=` con rhs numérico no cubierto tampoco verifica.
        let s5 = TeachingSession::new(
            TeachingTopic::Ecuacion,
            vec![TeachingStep::new("t", "T", "el cero está en x=-1.5").with_math("2*x+3")],
        );
        assert!(!s5.steps[0].verified);
    }
    #[test]
    fn r6e_frame_range_fuera_de_ventana_da_none() {
        // Ventana que excede el loop real: None honesto, sin módulo.
        let fuera = TeachingStep::new("x", "X", "E").with_frames(40, 60);
        assert_eq!(fuera.frame_range, Some((40, 60)));
        assert_eq!(TeachingSession::cue_frame_index(&fuera, 0, 12.0, 48), None);
        let inicio_fuera = TeachingStep::new("x", "X", "E").with_frames(50, 60);
        assert_eq!(
            TeachingSession::cue_frame_index(&inicio_fuera, 5000, 12.0, 48),
            None
        );
        // En ventana sigue mapeando dentro.
        let dentro = TeachingStep::new("x", "X", "E").with_frames(16, 32);
        let idx = TeachingSession::cue_frame_index(&dentro, 2400, 12.0, 48).expect("idx");
        assert!((16..32).contains(&idx), "idx={idx}");
    }
    #[test]
    fn r6e_mark_success_exige_remate_correcto() {
        // End-to-end: el éxito del FSM exige assess_final().correct.
        let paso = TeachingStep::new("d4", "Verificá en x=1", "calculá y escribí")
            .with_final_check("Si f(x)=x², ¿cuánto vale f'(1)?", "2");
        let bien = paso.assess_final("2").expect("assess");
        assert!(bien.correct);
        let mal = paso.assess_final("5").expect("assess");
        assert!(!mal.correct);
        let mut fsm = crate::socratic::SocraticFsm::new("derivada");
        fsm.record_attempt(None);
        assert_eq!(
            fsm.mark_success(mal.correct).unwrap_err(),
            crate::socratic::GuardError::CheckNotCorrect
        );
        assert!(fsm.mark_success(bien.correct).is_ok());
        assert!(matches!(
            fsm.state,
            crate::socratic::SocraticState::Summarize
        ));
    }
    #[test]
    fn subespacio_y_fractal_detectan_sesion_y_remate() {
        assert_eq!(
            TeachingTopic::from_text("subespacio generado por u y v"),
            TeachingTopic::Subespacio
        );
        assert_eq!(
            TeachingTopic::from_text("span de dos vectores"),
            TeachingTopic::Subespacio
        );
        assert_eq!(
            TeachingTopic::from_text("combinación lineal y base"),
            TeachingTopic::Subespacio
        );
        assert_eq!(
            TeachingTopic::from_text("fractal copo de Koch"),
            TeachingTopic::Fractal
        );
        assert_eq!(
            TeachingTopic::from_text("conjunto de Mandelbrot"),
            TeachingTopic::Fractal
        );
        assert_eq!(
            TeachingTopic::from_text("autosimilitud de Julia"),
            TeachingTopic::Fractal
        );
        assert_eq!(TeachingTopic::Subespacio.label(), "Subespacios");
        assert_eq!(TeachingTopic::Fractal.label(), "Fractales");
        let sub = TeachingSession::for_topic("subespacios y span");
        assert_eq!(sub.topic, TeachingTopic::Subespacio);
        assert_eq!(sub.steps.len(), 4);
        assert!(sub
            .steps
            .iter()
            .any(|st| st.manim_template.as_deref() == Some("subspace")));
        for st in sub.steps.iter().take(3) {
            assert!(
                st.math_expr.is_none() || st.verified,
                "math sin verificar en {}",
                st.id
            );
        }
        assert!(sub.steps.iter().take(3).any(|st| st.verified));
        let sub4 = sub
            .steps
            .iter()
            .find(|st| st.id == "sub4")
            .expect("sub4 existe");
        assert_eq!(
            sub4.check.as_ref().expect("sub4 tiene remate").expected,
            "2"
        );
        assert!(sub4.assess_final("2").expect("assess").correct);
        assert!(!sub4.assess_final("3").expect("assess").correct);
        let fra = TeachingSession::for_topic("fractales de Koch");
        assert_eq!(fra.topic, TeachingTopic::Fractal);
        assert_eq!(fra.steps.len(), 4);
        assert!(fra
            .steps
            .iter()
            .any(|st| st.manim_template.as_deref() == Some("fractal")));
        for st in fra.steps.iter().take(3) {
            assert!(
                st.math_expr.is_none() || st.verified,
                "math sin verificar en {}",
                st.id
            );
        }
        assert!(fra.steps.iter().take(3).any(|st| st.verified));
        let fr4 = fra
            .steps
            .iter()
            .find(|st| st.id == "fr4")
            .expect("fr4 existe");
        assert_eq!(fr4.check.as_ref().expect("fr4 tiene remate").expected, "12");
        assert!(fr4.assess_final("12").expect("assess").correct);
        assert!(!fr4.assess_final("16").expect("assess").correct);
    }
}
