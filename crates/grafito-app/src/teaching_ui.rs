//! UI de enseñanza — integra TeachingSession + Whiteboard + ManimOrchestrator.
//!
//! Muestra burbujas morph desde el avatar, pizarra para cada paso y
//! controles para avanzar. Usa `grafito-whiteboard` para dibujo y
//! `manim_orchestrator` para animaciones 3b1b (con fallback nativo).

use crate::manim_orchestrator::{ManimOrchestrator, OrchestratorState};
use crate::whiteboard_ui::WhiteboardSession;
use egui::{Color32, Stroke};
use grafito_pedagogy::{
    Curriculum, Exercise, ExerciseGenerator, LearningObjective, PedagogicalLevel, TeachingSession,
    TeachingTopic,
};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_whiteboard::WhiteboardDoc;
use std::time::{Duration, Instant};

/// Ancho ideal de la ventana de enseñanza (SPACE_XXL * 16 = 640, tokenizado).
const VENTANA_ANCHO_IDEAL: f32 = grafito_ui::tokens::SPACE_XXL * 16.0;
/// Ancho mínimo de la ventana (SPACE_XXL * 12 = 480).
const VENTANA_ANCHO_MIN: f32 = grafito_ui::tokens::SPACE_XXL * 12.0;
/// Ancho máximo de la ventana (SPACE_XXL * 20 = 800).
const VENTANA_ANCHO_MAX: f32 = grafito_ui::tokens::SPACE_XXL * 20.0;
/// Alto ideal de la ventana (SPACE_XXL * 13 = 520).
const VENTANA_ALTO_IDEAL: f32 = grafito_ui::tokens::SPACE_XXL * 13.0;

/// Estado de la UI de enseñanza.
pub struct TeachingUiState {
    pub session: Option<TeachingSession>,
    pub whiteboard: WhiteboardSession,
    pub orchestrator: ManimOrchestrator,
    pub show_manim: bool,
    /// Instant en que se abrió la overlay — para morph avatar→burbuja (ANIM_MICRO 180ms ease-out).
    pub opened_at: Option<Instant>,
    /// Frames nativos generados al completar el orchestrator (fallback sin Manim).
    pub anim_frames: Option<Vec<egui::ColorImage>>,
    /// Texturas cacheadas de `anim_frames` (creadas lazily con `ctx.load_texture`).
    pub anim_textures: Vec<egui::TextureHandle>,
    /// Panel de ejercicio B7 (pedir→generar→responder→próximo). Vacío por defecto.
    pub ejercicio: PanelEjercicio,
    cached_hash: u64,
    cached_len: usize,
}

#[allow(clippy::derivable_impls)]
impl Default for TeachingUiState {
    fn default() -> Self {
        Self {
            session: None,
            whiteboard: WhiteboardSession::default(),
            orchestrator: ManimOrchestrator::default(),
            show_manim: false,
            opened_at: None,
            anim_frames: None,
            anim_textures: Vec::new(),
            ejercicio: PanelEjercicio::default(),
            cached_hash: 0,
            cached_len: 0,
        }
    }
}

/// Clasificación pura del hint de pizarra en 14 temas más casos borde.
///
/// Es `pub` y totalmente testeable (sin egui, sin I/O): dado el texto del
/// paso pedagógico decide qué dibujar. Los chequeos de [`hint_for_topic`]
/// están ordenados por especificidad —los más específicos primero— para que
/// un hint como "triángulo rectángulo" caiga en Pitágoras y no en el
/// genérico Área (que también matchea "rectángulo").
///
/// NOTA DEUDA: extraer a `grafito-pedagogy` (p. ej. `TeachingTopic::hint_kind`)
/// cuando el DAG lo permita; la Piel sólo debería consumir el enum, no el
/// string-matching. Ver [`whiteboard_elements_for_hint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteboardHint {
    /// Hint vacío o sólo espacios: no dibuja nada.
    Vacio,
    Secante,
    Fraccion,
    Vector,
    Matriz,
    Probabilidad,
    Serie,
    Trigonometria,
    Conica,
    Ecuacion,
    Limite,
    Funcion,
    Area,
    Pitagoras,
    /// «pizarra libre»: dibuja el usuario, no se hidrata nada.
    Libre,
    /// Hint desconocido: texto centrado como fallback.
    General,
}

/// Clasifica el hint de un paso de enseñanza (puro, sin I/O).
///
/// Ordenado por especificidad: Pitágoras precede a Área porque
/// "triángulo rectángulo" contiene "rectángulo".
pub fn hint_for_topic(topic: &str) -> WhiteboardHint {
    let lower = topic.to_lowercase();
    if topic.trim().is_empty() {
        return WhiteboardHint::Vacio;
    }
    if lower.contains("pitágoras")
        || lower.contains("pitagoras")
        || lower.contains("triángulo rectángulo")
        || lower.contains("triangulo rectangulo")
        || lower.contains("cuadrados en cada lado")
    {
        return WhiteboardHint::Pitagoras;
    }
    if lower.contains("secante") || lower.contains("tangente") || lower.contains("pendiente") {
        return WhiteboardHint::Secante;
    }
    if lower.contains("fracc")
        || lower.contains("dividido en partes")
        || lower.contains("común denominador")
        || lower.contains("comun denominador")
        || lower.contains("fracciones equivalentes")
        || lower.contains("rectángulo dividido")
        || lower.contains("rectangulo dividido")
    {
        return WhiteboardHint::Fraccion;
    }
    if lower.contains("vector")
        || lower.contains("flecha en ejes")
        || lower.contains("dos flechas")
        || lower.contains("pizarra vectorial")
        || lower.contains("r²")
        || lower.contains("r2")
    {
        return WhiteboardHint::Vector;
    }
    if lower.contains("matriz")
        || lower.contains("matrices")
        || lower.contains("grilla")
        || lower.contains("2x2")
        || lower.contains("determinante")
        || lower.contains("gauss")
        || lower.contains("pivote")
        || lower.contains("aumentada")
    {
        return WhiteboardHint::Matriz;
    }
    if lower.contains("probab")
        || lower.contains("histograma")
        || lower.contains("árbol")
        || lower.contains("arbol")
        || lower.contains("contingencia")
        || lower.contains("normal")
        || lower.contains("bayes")
        || lower.contains("muestreo")
        || lower.contains("binom")
        || lower.contains("distrib")
        || lower.contains("curva normal")
    {
        return WhiteboardHint::Probabilidad;
    }
    if lower.contains("serie")
        || lower.contains("taylor")
        || lower.contains("fourier")
        || lower.contains("sucesi")
        || lower.contains("geométrica")
        || lower.contains("geometrica")
        || lower.contains("suma parcial")
        || lower.contains("aproxima")
    {
        return WhiteboardHint::Serie;
    }
    if lower.contains("trigon")
        || lower.contains("seno")
        || lower.contains("coseno")
        || lower.contains("círculo unitario")
        || lower.contains("circulo unitario")
        || lower.contains("onda seno")
        || lower.contains("sin(")
        || lower.contains("cos(")
    {
        return WhiteboardHint::Trigonometria;
    }
    if lower.contains("cónica")
        || lower.contains("conica")
        || lower.contains("cono cortado")
        || lower.contains("elipse con focos")
        || lower.contains("hipérbola")
        || lower.contains("hiperbola")
        || (lower.contains("elipse") && lower.contains("focos"))
    {
        return WhiteboardHint::Conica;
    }
    if lower.contains("ecuac")
        || lower.contains("cuadrática")
        || lower.contains("cuadratica")
        || lower.contains("parábola y raíces")
        || lower.contains("parabola y raices")
        || lower.contains("discriminante")
        || lower.contains("recta y corte")
        || lower.contains("dos rectas")
        || lower.contains("sistema")
    {
        return WhiteboardHint::Ecuacion;
    }
    if lower.contains("límite")
        || lower.contains("limite")
        || lower.contains("hueco en a")
        || lower.contains("flechas hacia a")
        || lower.contains("cálculo paso a paso")
        || lower.contains("calculo paso a paso")
    {
        return WhiteboardHint::Limite;
    }
    if lower.contains("función")
        || lower.contains("funcion")
        || lower.contains("ejes con puntos")
        || lower.contains("tabla de valores")
        || lower.contains("curva con ejes")
        || lower.contains("ejemplo con gráfica")
        || lower.contains("gráfica interactiva")
    {
        return WhiteboardHint::Funcion;
    }
    if lower.contains("rectángulo")
        || lower.contains("rectangulo")
        || lower.contains("riemann")
        || lower.contains("área")
        || lower.contains("area")
    {
        return WhiteboardHint::Area;
    }
    if lower.contains("pizarra libre") || lower.contains("libre") {
        return WhiteboardHint::Libre;
    }
    WhiteboardHint::General
}

/// Constructor puro de elementos para secante (ver [`hint_for_topic`]).
fn push_secante_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Curva x² como trazo suave + secante + tangente
    elems.push(WhiteboardElement::Stroke {
        points: vec![
            (-3.0, 9.0),
            (-2.0, 4.0),
            (-1.0, 1.0),
            (0.0, 0.0),
            (1.0, 1.0),
            (2.0, 4.0),
        ],
        color: (55, 55, 55),
        width: 2.0,
    });
    elems.push(WhiteboardElement::Arrow {
        from: (-1.0, 1.0),
        to: (1.0, 1.0),
    });
    elems.push(WhiteboardElement::Arrow {
        from: (0.5, 0.25),
        to: (1.5, 2.25),
    });
}

/// Constructor puro de elementos para fraccion (ver [`hint_for_topic`]).
fn push_fraccion_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Fracción — rectángulo dividido en 3 + sombreado 2/3 + texto
    elems.push(WhiteboardElement::Rectangle {
        min: (0.0, 0.0),
        max: (2.4, 1.0),
        fill: None,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.8, 0.0), (0.8, 1.0)],
        color: (55, 55, 55),
        width: 1.4,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(1.6, 0.0), (1.6, 1.0)],
        color: (55, 55, 55),
        width: 1.4,
    });
    // Sombrear dos tercios
    elems.push(WhiteboardElement::Rectangle {
        min: (0.05, 0.05),
        max: (0.75, 0.95),
        fill: Some((135, 180, 255)),
    });
    elems.push(WhiteboardElement::Rectangle {
        min: (0.85, 0.05),
        max: (1.55, 0.95),
        fill: Some((135, 180, 255)),
    });
    elems.push(WhiteboardElement::Text {
        at: (0.15, -0.35),
        text: "2/3 del rectángulo".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.2, 1.2),
        text: "1/2 + 1/3 = 5/6".into(),
        size: 9.0,
    });
}

/// Constructor puro de elementos para vector (ver [`hint_for_topic`]).
fn push_vector_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Vector — ejes + dos flechas (u, v) y suma tip-to-tail
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.5, 0.0), (1.8, 0.0)],
        color: (55, 55, 55),
        width: 1.2,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, -1.2), (0.0, 1.6)],
        color: (55, 55, 55),
        width: 1.2,
    });
    elems.push(WhiteboardElement::Arrow {
        from: (0.0, 0.0),
        to: (1.2, 0.8),
    });
    elems.push(WhiteboardElement::Arrow {
        from: (0.0, 0.0),
        to: (0.6, 1.3),
    });
    // Suma visual tip-to-tail
    elems.push(WhiteboardElement::Arrow {
        from: (1.2, 0.8),
        to: (1.8, 2.1),
    });
    elems.push(WhiteboardElement::Text {
        at: (0.35, 0.55),
        text: "v=(2,3)".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.15, 1.35),
        text: "u".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (1.25, 1.95),
        text: "u+v".into(),
        size: 9.0,
    });
}

/// Constructor puro de elementos para matriz (ver [`hint_for_topic`]).
fn push_matriz_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Matriz — grilla 2×2 + etiquetas + determinante
    for row in 0..2 {
        for col in 0..2 {
            let x0 = col as f64 * 0.95;
            let y0 = row as f64 * 0.70;
            elems.push(WhiteboardElement::Rectangle {
                min: (x0, y0),
                max: (x0 + 0.85, y0 + 0.60),
                fill: None,
            });
        }
    }
    // diagonales para pivotes
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.05, 0.05), (0.80, 0.55)],
        color: (66, 133, 244),
        width: 1.5,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(1.00, 0.75), (1.75, 1.25)],
        color: (66, 133, 244),
        width: 1.5,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.22, 0.22),
        text: "1".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (1.17, 0.22),
        text: "2".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.22, 0.92),
        text: "3".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (1.17, 0.92),
        text: "4".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.15, 1.55),
        text: "det≠0 → invertible".into(),
        size: 8.0,
    });
}

/// Constructor puro de elementos para probabilidad (ver [`hint_for_topic`]).
fn push_probabilidad_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Probabilidad — histograma de 4 barras + eje + fórmula
    let heights = [0.45, 0.95, 0.65, 0.35];
    for (i, h) in heights.iter().enumerate() {
        let x = i as f64 * 0.60;
        elems.push(WhiteboardElement::Rectangle {
            min: (x, 0.0),
            max: (x + 0.45, *h),
            fill: Some((126, 214, 160)),
        });
    }
    // eje base
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, 0.0), (2.45, 0.0)],
        color: (55, 55, 55),
        width: 1.2,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.15, 1.15),
        text: "P(A)=|A|/|Ω|".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.15, -0.35),
        text: "Ω  →  evento A".into(),
        size: 9.0,
    });
}

/// Constructor puro de elementos para serie (ver [`hint_for_topic`]).
fn push_serie_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Serie — seno + polinomios de Taylor que se acercan
    elems.push(WhiteboardElement::Stroke {
        points: vec![
            (-1.8, 0.0),
            (-1.0, 0.85),
            (0.0, 0.0),
            (1.0, -0.85),
            (1.8, 0.0),
        ],
        color: (55, 55, 55),
        width: 1.6,
    });
    // polinomio aproximante (más grueso, punteado visual con color)
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.2, -0.25), (-0.6, 0.45), (0.6, -0.45), (1.2, 0.25)],
        color: (66, 133, 244),
        width: 1.3,
    });
    elems.push(WhiteboardElement::Text {
        at: (-1.6, 1.15),
        text: "f(x)≈ Σ f⁽ⁿ⁾/n!·(x-a)ⁿ".into(),
        size: 8.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (-1.5, -0.55),
        text: "Sₙ → L".into(),
        size: 10.0,
    });
}

/// Constructor puro de elementos para trigonometria (ver [`hint_for_topic`]).
fn push_trigonometria_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Trigonometría — círculo unitario + ángulo + triángulo
    elems.push(WhiteboardElement::Ellipse {
        center: (0.0, 0.0),
        rx: 1.0,
        ry: 1.0,
    });
    // radio en 45°
    elems.push(WhiteboardElement::Arrow {
        from: (0.0, 0.0),
        to: (0.71, 0.71),
    });
    // proyecciones
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.71, 0.71), (0.71, 0.0)],
        color: (66, 133, 244),
        width: 1.2,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, 0.0), (0.71, 0.0)],
        color: (235, 120, 80),
        width: 1.2,
    });
    elems.push(WhiteboardElement::Text {
        at: (-0.95, 1.25),
        text: "sin²+cos²=1".into(),
        size: 9.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.35, -0.25),
        text: "cos".into(),
        size: 8.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.78, 0.35),
        text: "sin".into(),
        size: 8.0,
    });
}

/// Constructor puro de elementos para conica (ver [`hint_for_topic`]).
fn push_conica_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Cónica — elipse con focos + ejes
    elems.push(WhiteboardElement::Ellipse {
        center: (0.0, 0.2),
        rx: 1.35,
        ry: 0.85,
    });
    // focos
    elems.push(WhiteboardElement::Ellipse {
        center: (-0.75, 0.2),
        rx: 0.07,
        ry: 0.07,
    });
    elems.push(WhiteboardElement::Ellipse {
        center: (0.75, 0.2),
        rx: 0.07,
        ry: 0.07,
    });
    // ejes punteados sutiles
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.35, 0.2), (1.35, 0.2)],
        color: (55, 55, 55),
        width: 1.0,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, -0.65), (0.0, 1.05)],
        color: (55, 55, 55),
        width: 1.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (-1.2, 1.20),
        text: "x²/a²+y²/b²=1".into(),
        size: 8.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (-0.90, -0.45),
        text: "foco".into(),
        size: 7.0,
    });
}

/// Constructor puro de elementos para ecuacion (ver [`hint_for_topic`]).
fn push_ecuacion_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>, lower: &str) {
    use grafito_whiteboard::WhiteboardElement;
    // Ecuación — parábola con raíces + discriminante + sistema de rectas
    elems.push(WhiteboardElement::Stroke {
        points: vec![
            (-1.4, 1.2),
            (-0.7, 0.15),
            (0.0, -0.25),
            (0.7, 0.15),
            (1.4, 1.2),
        ],
        color: (55, 55, 55),
        width: 1.6,
    });
    // eje x
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.6, 0.0), (1.6, 0.0)],
        color: (55, 55, 55),
        width: 1.0,
    });
    // raíces marcadas
    elems.push(WhiteboardElement::Ellipse {
        center: (-0.85, 0.0),
        rx: 0.07,
        ry: 0.07,
    });
    elems.push(WhiteboardElement::Ellipse {
        center: (0.85, 0.0),
        rx: 0.07,
        ry: 0.07,
    });
    // sistema alternativa: dos rectas que se cortan (si hint contiene sistema/dos rectas)
    if lower.contains("sistema") || lower.contains("dos rectas") {
        elems.push(WhiteboardElement::Stroke {
            points: vec![(-1.3, -0.9), (1.3, 0.9)],
            color: (66, 133, 244),
            width: 1.2,
        });
        elems.push(WhiteboardElement::Stroke {
            points: vec![(-1.3, 0.9), (1.3, -0.9)],
            color: (235, 120, 80),
            width: 1.2,
        });
    }
    elems.push(WhiteboardElement::Text {
        at: (-1.45, 1.40),
        text: "Δ=b²-4ac".into(),
        size: 9.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (0.95, -0.35),
        text: "raíz".into(),
        size: 8.0,
    });
}

/// Constructor puro de elementos para limite (ver [`hint_for_topic`]).
fn push_limite_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Límite — recta con hueco + flechas de acercamiento
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.5, -0.9), (1.5, 0.9)],
        color: (55, 55, 55),
        width: 1.5,
    });
    // hueco abierto en a=0
    elems.push(WhiteboardElement::Ellipse {
        center: (0.0, 0.0),
        rx: 0.12,
        ry: 0.12,
    });
    elems.push(WhiteboardElement::Arrow {
        from: (-0.85, -0.45),
        to: (-0.12, -0.06),
    });
    elems.push(WhiteboardElement::Arrow {
        from: (0.85, 0.45),
        to: (0.12, 0.06),
    });
    elems.push(WhiteboardElement::Text {
        at: (-0.25, 0.35),
        text: "a".into(),
        size: 10.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (-1.35, 1.1),
        text: "limₓ→ₐ f(x)=L".into(),
        size: 9.0,
    });
}

/// Constructor puro de elementos para funcion (ver [`hint_for_topic`]).
fn push_funcion_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // Función — ejes + curva + puntos (x, f(x))
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-1.6, 0.0), (1.6, 0.0)],
        color: (55, 55, 55),
        width: 1.1,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, -1.2), (0.0, 1.4)],
        color: (55, 55, 55),
        width: 1.1,
    });
    // curva f(x)=x² leve
    elems.push(WhiteboardElement::Stroke {
        points: vec![
            (-1.2, 1.1),
            (-0.6, 0.25),
            (0.0, 0.05),
            (0.6, 0.25),
            (1.2, 1.1),
        ],
        color: (66, 133, 244),
        width: 1.6,
    });
    // puntos marcados
    elems.push(WhiteboardElement::Ellipse {
        center: (-0.6, 0.25),
        rx: 0.07,
        ry: 0.07,
    });
    elems.push(WhiteboardElement::Ellipse {
        center: (0.6, 0.25),
        rx: 0.07,
        ry: 0.07,
    });
    elems.push(WhiteboardElement::Text {
        at: (-0.55, 0.45),
        text: "(x, f(x))".into(),
        size: 8.0,
    });
    elems.push(WhiteboardElement::Text {
        at: (-1.45, -0.95),
        text: "f: x → y".into(),
        size: 9.0,
    });
}

/// Constructor puro de elementos para area (ver [`hint_for_topic`]).
fn push_area_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    // 4 rectángulos de Riemann bajo x² entre 0 y 2
    for i in 0..4 {
        let x = i as f64 * 0.5;
        let y = x * x * 0.5 + 0.2;
        elems.push(WhiteboardElement::Rectangle {
            min: (x, 0.0),
            max: (x + 0.45, y),
            fill: None,
        });
    }
    elems.push(WhiteboardElement::Stroke {
        points: vec![(0.0, 0.0), (2.0, 2.0)],
        color: (55, 55, 55),
        width: 1.5,
    });
}

/// Constructor puro de elementos para pitagoras (ver [`hint_for_topic`]).
fn push_pitagoras_hint(elems: &mut Vec<grafito_whiteboard::WhiteboardElement>) {
    use grafito_whiteboard::WhiteboardElement;
    elems.push(WhiteboardElement::Rectangle {
        min: (-2.0, -1.0),
        max: (0.0, 1.0),
        fill: None,
    });
    elems.push(WhiteboardElement::Rectangle {
        min: (0.0, -1.0),
        max: (2.0, 1.0),
        fill: None,
    });
    elems.push(WhiteboardElement::Stroke {
        points: vec![(-2.0, -1.0), (2.0, -1.0), (0.0, 1.0), (-2.0, -1.0)],
        color: (55, 55, 55),
        width: 2.0,
    });
}

/// Hidrata la pizarra del paso actual según su hint (dispatch sobre [`hint_for_topic`]).
pub(crate) fn whiteboard_elements_for_hint(
    hint: &str,
) -> Vec<grafito_whiteboard::WhiteboardElement> {
    if hint.trim().is_empty() {
        return Vec::new();
    }
    let mut elems = Vec::new();
    match hint_for_topic(hint) {
        WhiteboardHint::Vacio | WhiteboardHint::Libre => {}
        WhiteboardHint::Secante => push_secante_hint(&mut elems),
        WhiteboardHint::Fraccion => push_fraccion_hint(&mut elems),
        WhiteboardHint::Vector => push_vector_hint(&mut elems),
        WhiteboardHint::Matriz => push_matriz_hint(&mut elems),
        WhiteboardHint::Probabilidad => push_probabilidad_hint(&mut elems),
        WhiteboardHint::Serie => push_serie_hint(&mut elems),
        WhiteboardHint::Trigonometria => push_trigonometria_hint(&mut elems),
        WhiteboardHint::Conica => push_conica_hint(&mut elems),
        WhiteboardHint::Ecuacion => {
            let lower = hint.to_lowercase();
            push_ecuacion_hint(&mut elems, &lower);
        }
        WhiteboardHint::Limite => push_limite_hint(&mut elems),
        WhiteboardHint::Funcion => push_funcion_hint(&mut elems),
        WhiteboardHint::Area => push_area_hint(&mut elems),
        WhiteboardHint::Pitagoras => push_pitagoras_hint(&mut elems),
        WhiteboardHint::General => {
            use grafito_whiteboard::WhiteboardElement;
            // Fallback: texto centrado

            elems.push(WhiteboardElement::Text {
                at: (-1.5, 0.0),

                text: hint.chars().take(40).collect(),

                size: 14.0,
            });
        }
    }
    elems
}

/// Mapeo puro tópico→pista sin strings (reemplazo del string-matching frágil).
///
/// No toca otros crates: vive en la Piel y sólo consume `TeachingTopic`.
/// El dispatch por enum hace imposibles los errores de tipeo del `hint`
/// (`hint_for_topic` queda como compat legacy para hints libres).
#[allow(dead_code)] // TODO otro agente: cablear la tarjeta de ejercicio
pub fn pista_para_topico(topico: &TeachingTopic) -> WhiteboardHint {
    match topico {
        TeachingTopic::Derivada => WhiteboardHint::Secante,
        TeachingTopic::Integral => WhiteboardHint::Area,
        TeachingTopic::Limite => WhiteboardHint::Limite,
        TeachingTopic::Funcion => WhiteboardHint::Funcion,
        TeachingTopic::Pitagoras => WhiteboardHint::Pitagoras,
        TeachingTopic::Fraccion => WhiteboardHint::Fraccion,
        TeachingTopic::Vector => WhiteboardHint::Vector,
        TeachingTopic::Matriz => WhiteboardHint::Matriz,
        TeachingTopic::Probabilidad => WhiteboardHint::Probabilidad,
        TeachingTopic::Serie => WhiteboardHint::Serie,
        TeachingTopic::Ecuacion => WhiteboardHint::Ecuacion,
        TeachingTopic::Trigonometria => WhiteboardHint::Trigonometria,
        TeachingTopic::Conica => WhiteboardHint::Conica,
        TeachingTopic::General(_) => WhiteboardHint::General,
    }
}

/// Elementos de pizarra para un tópico (puro, sin I/O, sin strings).
///
/// Despacha por [`pista_para_topico`]; para `Ecuacion` usa la parábola base
/// (la variante «dos rectas» sólo existe en el path legacy con hint textual).
/// `General` devuelve un único texto acotado con el `label` del tópico.
#[allow(dead_code)] // TODO otro agente: cablear la tarjeta de ejercicio
pub fn elementos_para_topico(topico: &TeachingTopic) -> Vec<grafito_whiteboard::WhiteboardElement> {
    let mut elems = Vec::new();
    match pista_para_topico(topico) {
        WhiteboardHint::Vacio | WhiteboardHint::Libre => {}
        WhiteboardHint::Secante => push_secante_hint(&mut elems),
        WhiteboardHint::Fraccion => push_fraccion_hint(&mut elems),
        WhiteboardHint::Vector => push_vector_hint(&mut elems),
        WhiteboardHint::Matriz => push_matriz_hint(&mut elems),
        WhiteboardHint::Probabilidad => push_probabilidad_hint(&mut elems),
        WhiteboardHint::Serie => push_serie_hint(&mut elems),
        WhiteboardHint::Trigonometria => push_trigonometria_hint(&mut elems),
        WhiteboardHint::Conica => push_conica_hint(&mut elems),
        WhiteboardHint::Ecuacion => {
            push_ecuacion_hint(&mut elems, "");
        }
        WhiteboardHint::Limite => push_limite_hint(&mut elems),
        WhiteboardHint::Funcion => push_funcion_hint(&mut elems),
        WhiteboardHint::Area => push_area_hint(&mut elems),
        WhiteboardHint::Pitagoras => push_pitagoras_hint(&mut elems),
        WhiteboardHint::General => {
            use grafito_whiteboard::WhiteboardElement;
            let etiqueta: String = topico.label().chars().take(40).collect();
            elems.push(WhiteboardElement::Text {
                at: (-1.5, 0.0),
                text: etiqueta,
                size: 14.0,
            });
        }
    }
    elems
}

// ── Tarjeta de ejercicio inline (pura + dibujo acotado) ──

/// Tope de caracteres del enunciado para que la tarjeta no desborde.
pub const MAX_ENUNCIADO_CHARS: usize = 280;
/// Tope por opción múltiple.
pub const MAX_OPCION_CHARS: usize = 120;

/// Recorta por borde de carácter (nunca parte UTF-8). Puro, sin I/O.
pub fn acotar_texto(texto: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if texto.chars().count() <= max {
        return texto.to_string();
    }
    let recortado: String = texto.chars().take(max.saturating_sub(1)).collect();
    format!("{recortado}…")
}

fn numero_simple(texto: &str) -> Option<f64> {
    let t = texto.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    if let Some((a, b)) = t.split_once('/') {
        let a: f64 = a.trim().parse().ok()?;
        let b: f64 = b.trim().parse().ok()?;
        if b.abs() > 1e-12 {
            return Some(a / b);
        }
        return None;
    }
    t.parse::<f64>().ok()
}

fn formatear_numero(valor: f64) -> String {
    if (valor - valor.round()).abs() < 1e-9 && valor.abs() < 1e9 {
        format!("{}", valor.round() as i64)
    } else {
        format!("{valor:.2}")
    }
}

/// Opciones múltiples deterministas (4, con la solución incluida). Puro, sin I/O.
pub fn opciones_para_ejercicio(ejercicio: &Exercise) -> Vec<String> {
    let solucion = ejercicio.solution.trim().to_string();
    let mut candidatas: Vec<String> = Vec::with_capacity(4);
    candidatas.push(solucion.clone());
    if let Some(n) = numero_simple(&solucion) {
        for d in [1.0, -1.0, 2.0] {
            let v = if d == 2.0 { n * 2.0 } else { n + d };
            let s = formatear_numero(v);
            if !candidatas.iter().any(|c| c == &s) {
                candidatas.push(s);
            }
            if candidatas.len() >= 4 {
                break;
            }
        }
        for extra in ["1", "2", "-1", "10"] {
            if candidatas.len() >= 4 {
                break;
            }
            if !candidatas.iter().any(|c| c == extra) {
                candidatas.push(extra.to_string());
            }
        }
    } else {
        for suf in [" + 1", " − 1", " (otra forma)"] {
            if candidatas.len() >= 4 {
                break;
            }
            let s = format!("{solucion}{suf}");
            if !candidatas.iter().any(|c| c == &s) {
                candidatas.push(s);
            }
        }
        if !candidatas.iter().any(|c| c == "Ninguna de estas") {
            candidatas.push("Ninguna de estas".to_string());
        }
    }
    candidatas.truncate(4);
    while candidatas.len() < 4 {
        let relleno = format!("Variante {}", candidatas.len() + 1);
        candidatas.push(relleno);
    }
    let rot = (ejercicio.seed.unwrap_or(0) % 4) as usize;
    candidatas.rotate_left(rot);
    candidatas
}

/// Índice de la solución dentro de `opciones`. Puro, sin I/O.
pub fn indice_respuesta_correcta(ejercicio: &Exercise, opciones: &[String]) -> Option<usize> {
    let sol = ejercicio.solution.trim();
    opciones.iter().position(|o| o.trim() == sol)
}

/// Estado local de la tarjeta inline (vive en la Piel, no persiste).
#[derive(Debug, Clone, Default)]
pub struct EstadoTarjetaEjercicio {
    pub respuesta: String,
    pub opcion_elegida: Option<usize>,
    pub devolucion: Option<grafito_pedagogy::Feedback>,
}

/// Tarjeta inline: enunciado + opciones + devolución tras responder.
///
/// Layout acotado (ancho del panel, `wrap` + recorte). Sin I/O ni spawn:
/// sólo evalúa con `FeedbackEngine` (CPU en memoria). Textos en rioplatense.
pub fn draw_tarjeta_ejercicio(
    ui: &mut egui::Ui,
    ejercicio: &Exercise,
    opciones: &[String],
    estado: &mut EstadoTarjetaEjercicio,
) {
    let tema = grafito_ui::theme::current_theme(ui.ctx());
    egui::Frame::none()
        .fill(tema.panel_bg)
        .stroke(Stroke::new(1.0, tema.separator.gamma_multiply(0.10)))
        .rounding(grafito_ui::tokens::RADIUS_MD)
        .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(
                egui::Label::new(
                    egui::RichText::new(acotar_texto(&ejercicio.prompt, MAX_ENUNCIADO_CHARS))
                        .strong()
                        .size(grafito_ui::tokens::TYPE_BASE)
                        .color(tema.text_primary),
                )
                .wrap(),
            );
            ui.add_space(grafito_ui::tokens::SPACE_XS);
            for (idx, op) in opciones.iter().enumerate() {
                let texto = acotar_texto(op, MAX_OPCION_CHARS);
                if ui
                    .radio_value(&mut estado.opcion_elegida, Some(idx), texto)
                    .clicked()
                {
                    estado.respuesta = op.clone();
                    estado.devolucion = None;
                }
            }
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("Tu respuesta:")
                        .size(grafito_ui::tokens::TYPE_SM)
                        .color(tema.text_secondary),
                );
                ui.text_edit_singleline(&mut estado.respuesta);
                let boton = egui::Button::new(
                    egui::RichText::new("Responder")
                        .size(grafito_ui::tokens::TYPE_SM)
                        .strong(),
                )
                .fill(tema.accent)
                .stroke(Stroke::NONE)
                .rounding(grafito_ui::tokens::RADIUS_MD);
                if ui.add(boton).clicked() {
                    let fb = grafito_pedagogy::FeedbackEngine.assess(ejercicio, &estado.respuesta);
                    estado.devolucion = Some(fb);
                }
            });
            if let Some(fb) = estado.devolucion.as_ref() {
                ui.add_space(grafito_ui::tokens::SPACE_SM);
                let (borde, fondo) = if fb.correct {
                    (
                        egui::Color32::from_rgb(40, 180, 70),
                        egui::Color32::from_rgb(232, 245, 233),
                    )
                } else {
                    (
                        egui::Color32::from_rgb(200, 60, 60),
                        egui::Color32::from_rgb(253, 237, 237),
                    )
                };
                egui::Frame::none()
                    .fill(fondo)
                    .stroke(Stroke::new(1.0, borde))
                    .rounding(grafito_ui::tokens::RADIUS_MD)
                    .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let titulo = if fb.correct {
                            "¡Bien ahí, che! Respuesta correcta."
                        } else {
                            "Casi… fijate de nuevo."
                        };
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(titulo)
                                    .strong()
                                    .size(grafito_ui::tokens::TYPE_SM)
                                    .color(borde),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(acotar_texto(&fb.message, MAX_ENUNCIADO_CHARS))
                                    .size(grafito_ui::tokens::TYPE_SM)
                                    .color(egui::Color32::from_rgb(40, 40, 40)),
                            )
                            .wrap(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(acotar_texto(
                                    &fb.next_step,
                                    MAX_ENUNCIADO_CHARS,
                                ))
                                .size(grafito_ui::tokens::TYPE_XS)
                                .color(egui::Color32::from_rgb(90, 90, 90)),
                            )
                            .wrap(),
                        );
                    });
            }
        });
}

// ── Panel de ejercicio con estados (B7: cablea la tarjeta) ──

/// Tope del tema pedido (mensajes y resolución de LO acotados).
pub const MAX_TEMA_CHARS: usize = 60;

/// Estado del panel de ejercicio: vacío / generando / lista / error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EstadoPanelEjercicio {
    #[default]
    Vacio,
    Generando,
    Lista,
    Error,
}

/// Panel de ejercicio: dueña del ciclo pedir→generar→responder→próximo.
///
/// Vive en `TeachingUiState` (sin tocar `app.rs`): el hilo de generación
/// escribe por `mpsc` y `sondear` lo cosecha sin bloquear. La corrección
/// (`Responder`) sigue inline en `draw_tarjeta_ejercicio` (CPU en memoria,
/// precedente aceptado); la app asienta el resultado en el perfil aparte.
#[derive(Default)]
pub struct PanelEjercicio {
    pub estado: EstadoPanelEjercicio,
    pub tema: String,
    pub ejercicio: Option<Exercise>,
    pub opciones: Vec<String>,
    pub tarjeta: EstadoTarjetaEjercicio,
    /// Próximo paso BKT (`recommend_next`) o texto honesto si no hay ramas.
    pub proximo: Option<String>,
    pub error: Option<String>,
    /// Semilla: crece en cada pedido para que «Otro ejercicio» varíe.
    pub semilla: u64,
    /// Respuesta ya asentada en el perfil (evita doble conteo BKT).
    pub registrada: Option<String>,
    receptor: Option<std::sync::mpsc::Receiver<Result<Exercise, String>>>,
}

/// Quita tildes para el reintento fuzzy (`función`→`funcion`). Puro.
fn sin_tildes(texto: &str) -> String {
    texto
        .chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' | 'ü' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            otro => otro,
        })
        .collect()
}

/// Resuelve texto libre → `LearningObjective` (exacto por id, si no fuzzy
/// por concepto con reintento sin tildes). Puro, sin I/O. Mismo criterio
/// que `generate_exercise` del dispatcher, sin round-trip JSON.
pub fn resolver_lo(tema: &str) -> Result<LearningObjective, String> {
    let recortado: String = tema.trim().chars().take(MAX_TEMA_CHARS).collect();
    if recortado.is_empty() {
        return Err("Decime un tema primero, che (ej. «derivada»).".to_string());
    }
    if let Some(lo) = Curriculum::get(recortado.trim()) {
        return Ok(lo);
    }
    let mut candidatos = Curriculum::find_for_concept(&recortado);
    if candidatos.is_empty() {
        candidatos = Curriculum::find_for_concept(&sin_tildes(&recortado));
    }
    candidatos.into_iter().next().ok_or_else(|| {
        format!("No encontré ejercicios de «{recortado}», probá con otro tema, che.")
    })
}

/// Arma un ejercicio validado para el tema (núcleo sync del job). Puro.
pub fn armar_ejercicio(
    tema: &str,
    nivel: PedagogicalLevel,
    semilla: u64,
) -> Result<Exercise, String> {
    let lo = resolver_lo(tema)?;
    let ejercicio = ExerciseGenerator.generate_with_seed(&lo, nivel, semilla);
    ejercicio
        .validate()
        .map_err(|motivo| format!("El ejercicio salió fallado ({motivo}), probá de nuevo, che."))?;
    Ok(ejercicio)
}

impl PanelEjercicio {
    /// Pide un ejercicio: pasa a `Generando` y lo arma en background.
    /// La UI sólo cosecha con `sondear`; cero spawn en el dibujado.
    pub fn pedir(&mut self, tema: &str, nivel: PedagogicalLevel) {
        let recortado: String = tema.trim().chars().take(MAX_TEMA_CHARS).collect();
        if recortado.is_empty() {
            self.estado = EstadoPanelEjercicio::Error;
            self.error = Some("Decime un tema primero, che (ej. «derivada»).".to_string());
            self.receptor = None;
            return;
        }
        let semilla = self.semilla.wrapping_add(1);
        let para_hilo = recortado.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        match std::thread::Builder::new()
            .name("grafito-ejercicio".to_string())
            .spawn(move || {
                let _ = tx.send(armar_ejercicio(&para_hilo, nivel, semilla));
            }) {
            Ok(_) => {
                self.semilla = semilla;
                self.tema = recortado;
                self.receptor = Some(rx);
                self.estado = EstadoPanelEjercicio::Generando;
                self.error = None;
                self.proximo = None;
                self.registrada = None;
            }
            Err(_) => {
                self.receptor = None;
                self.estado = EstadoPanelEjercicio::Error;
                self.error =
                    Some("No pude arrancar la generación, probá de nuevo, che.".to_string());
            }
        }
    }

    /// Cosecha el job sin bloquear (agota la cola: vale el último pedido).
    /// Retorna `true` si hubo cambio (el dibujado repinta).
    pub fn sondear(&mut self) -> bool {
        if self.receptor.is_none() {
            return false;
        }
        let mut cambio = false;
        loop {
            let recibido = match self.receptor.as_ref() {
                None => break,
                Some(rx) => rx.try_recv(),
            };
            match recibido {
                Ok(Ok(ejercicio)) => {
                    cambio = true;
                    self.opciones = opciones_para_ejercicio(&ejercicio);
                    // Invariante de la tarjeta: la solución está entre las opciones.
                    debug_assert!(indice_respuesta_correcta(&ejercicio, &self.opciones).is_some());
                    self.ejercicio = Some(ejercicio);
                    self.tarjeta = EstadoTarjetaEjercicio::default();
                    self.estado = EstadoPanelEjercicio::Lista;
                    self.error = None;
                }
                Ok(Err(mensaje)) => {
                    cambio = true;
                    self.estado = EstadoPanelEjercicio::Error;
                    self.error = Some(mensaje);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self.estado == EstadoPanelEjercicio::Generando {
                        self.estado = EstadoPanelEjercicio::Error;
                        self.error =
                            Some("Se cortó la generación, probá de nuevo, che.".to_string());
                        cambio = true;
                    }
                    self.receptor = None;
                    break;
                }
            }
        }
        if cambio && !matches!(self.estado, EstadoPanelEjercicio::Generando) {
            self.receptor = None;
        }
        cambio
    }

    /// Cierra el panel (suelta el job en curso: su `send` falla en silencio).
    pub fn cerrar(&mut self) {
        *self = Self::default();
    }
}

/// Intención del panel (la app ejecuta; el dibujado no genera ni persiste).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccionEjercicio {
    Otro,
    Cerrar,
}

/// Dibuja el panel según estado. `Vacio` no dibuja nada (los entry points
/// son el botón «Andamiar» en cada respuesta y el CTA del estado vacío).
/// Todos los botones hacen algo: sin botones mudos.
pub fn draw_panel_ejercicio(
    ui: &mut egui::Ui,
    panel: &mut PanelEjercicio,
) -> Option<AccionEjercicio> {
    if panel.sondear() {
        ui.ctx().request_repaint();
    }
    let tema = grafito_ui::theme::current_theme(ui.ctx());
    match panel.estado {
        EstadoPanelEjercicio::Vacio => None,
        EstadoPanelEjercicio::Generando => {
            // El job es instantáneo (CPU pura): este frame cosecha al siguiente.
            ui.ctx().request_repaint();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    egui::RichText::new("Armando tu ejercicio…")
                        .size(grafito_ui::tokens::TYPE_SM)
                        .color(tema.text_secondary),
                );
            });
            None
        }
        EstadoPanelEjercicio::Error => {
            let mensaje = panel
                .error
                .clone()
                .unwrap_or_else(|| "Algo falló, probá de nuevo, che.".to_string());
            egui::Frame::none()
                .fill(tema.panel_bg)
                .stroke(Stroke::new(1.0, tema.separator.gamma_multiply(0.10)))
                .rounding(grafito_ui::tokens::RADIUS_MD)
                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(acotar_texto(&mensaje, MAX_ENUNCIADO_CHARS))
                            .size(grafito_ui::tokens::TYPE_SM)
                            .color(tema.text_primary),
                    );
                });
            ui.horizontal(|ui| {
                let mut accion = None;
                if ui.button("Reintentar").clicked() {
                    accion = Some(AccionEjercicio::Otro);
                }
                if ui.button("Cerrar").clicked() {
                    accion = Some(AccionEjercicio::Cerrar);
                }
                accion
            })
            .inner
        }
        EstadoPanelEjercicio::Lista => {
            let Some(ejercicio) = panel.ejercicio.as_ref() else {
                panel.estado = EstadoPanelEjercicio::Error;
                panel.error = Some("Se perdió el ejercicio, pedí otro, che.".to_string());
                return None;
            };
            ui.label(
                egui::RichText::new(format!("Ejercicio · {}", panel.tema))
                    .size(grafito_ui::tokens::TYPE_XS)
                    .color(tema.text_secondary),
            );
            draw_tarjeta_ejercicio(ui, ejercicio, &panel.opciones, &mut panel.tarjeta);
            if let Some(proximo) = panel.proximo.as_ref() {
                ui.add_space(grafito_ui::tokens::SPACE_XS);
                ui.label(
                    egui::RichText::new(acotar_texto(proximo, MAX_ENUNCIADO_CHARS))
                        .size(grafito_ui::tokens::TYPE_XS)
                        .color(tema.text_secondary),
                );
            }
            ui.add_space(grafito_ui::tokens::SPACE_XS);
            ui.horizontal(|ui| {
                let mut accion = None;
                if ui
                    .button("Otro ejercicio")
                    .on_hover_text("Genero uno nuevo del mismo tema")
                    .clicked()
                {
                    accion = Some(AccionEjercicio::Otro);
                }
                if ui.button("Cerrar").clicked() {
                    accion = Some(AccionEjercicio::Cerrar);
                }
                accion
            })
            .inner
        }
    }
}

// ── Burbuja morph avatar→burbuja (pura + clamp a viewport) ──

/// Ease-out cúbico del morph (ANIM_MICRO 180ms). Puro, sin I/O.
pub fn progreso_morph_suave(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    1.0 - (1.0 - x) * (1.0 - x) * (1.0 - x)
}

/// Interpola origen→destino y clampeea al `limite` (normalmente `available_rect`).
///
/// Garantiza que la burbuja nunca se pase del viewport: el resultado siempre
/// está contenido en `limite`. Puro, sin I/O.
pub fn rect_burbuja_morph(
    origen: egui::Rect,
    destino: egui::Rect,
    progreso: f32,
    limite: egui::Rect,
) -> egui::Rect {
    let t = progreso.clamp(0.0, 1.0);
    let min = egui::pos2(
        origen.min.x + (destino.min.x - origen.min.x) * t,
        origen.min.y + (destino.min.y - origen.min.y) * t,
    );
    let max = egui::pos2(
        origen.max.x + (destino.max.x - origen.max.x) * t,
        origen.max.y + (destino.max.y - origen.max.y) * t,
    );
    let mut r = egui::Rect::from_min_max(min, max);
    // Clamp duro al límite: intersección + resguardo si no hay solape.
    if r.width() > limite.width() {
        r.set_width(limite.width());
    }
    if r.height() > limite.height() {
        r.set_height(limite.height());
    }
    r = r.intersect(limite);
    if r.is_negative() {
        r = egui::Rect::from_min_size(limite.min, egui::Vec2::ZERO);
    }
    r
}

impl TeachingUiState {
    fn anim_frames_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let Some(frames) = &self.anim_frames else {
            return 0;
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        frames.len().hash(&mut hasher);
        for frame in frames.iter().take(6) {
            frame.size.hash(&mut hasher);
            for pixel in &frame.pixels {
                pixel.r().hash(&mut hasher);
                pixel.g().hash(&mut hasher);
                pixel.b().hash(&mut hasher);
                pixel.a().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        let Some(frames) = &self.anim_frames else {
            if !self.anim_textures.is_empty() {
                for idx in 0..self.anim_textures.len() {
                    ctx.forget_image(&format!("teaching_anim_{idx}"));
                }
                self.anim_textures.clear();
                self.cached_hash = 0;
                self.cached_len = 0;
            }
            return;
        };
        if frames.is_empty() {
            if !self.anim_textures.is_empty() {
                for idx in 0..self.anim_textures.len() {
                    ctx.forget_image(&format!("teaching_anim_{idx}"));
                }
                self.anim_textures.clear();
                self.cached_hash = 0;
                self.cached_len = 0;
            }
            return;
        }
        let hash = self.anim_frames_hash();
        let len = frames.len();
        if self.anim_textures.len() == len && self.cached_hash == hash && self.cached_len == len {
            return;
        }
        for idx in 0..self.anim_textures.len() {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        self.anim_textures.clear();
        self.anim_textures = frames
            .iter()
            .enumerate()
            .map(|(idx, frame)| {
                ctx.load_texture(
                    format!("teaching_anim_{idx}"),
                    frame.clone(),
                    egui::TextureOptions::LINEAR,
                )
            })
            .collect();
        self.cached_hash = hash;
        self.cached_len = len;
    }

    pub fn clear(&mut self) {
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
        self.anim_frames = None;
    }

    pub fn clear_with_ctx(&mut self, ctx: &egui::Context) {
        for idx in 0..self.anim_textures.len() {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
        self.anim_frames = None;
    }

    fn clear_anim_textures_only(&mut self, ctx: Option<&egui::Context>) {
        if let Some(ctx) = ctx {
            for idx in 0..self.anim_textures.len() {
                ctx.forget_image(&format!("teaching_anim_{idx}"));
            }
        }
        self.anim_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
    }

    pub fn start(&mut self, topic: &str) {
        let session = TeachingSession::for_topic(topic);
        // Inicializar pizarra con elementos vectoriales reales según hint
        if let Some(step) = session.current() {
            let mut doc = WhiteboardDoc::default();
            for elem in whiteboard_elements_for_hint(&step.whiteboard_hint) {
                doc.add(elem);
            }
            self.whiteboard.doc = doc;
            // Iniciar orquestación manim para el primer paso — cancela cualquier job previo
            if let Some(tmpl) = &step.manim_template {
                self.orchestrator.cancel();
                let _ = self.orchestrator.start(topic, tmpl.clone());
            }
        }
        self.session = Some(session);
        self.opened_at = Some(Instant::now());
        self.anim_frames = None;
        self.clear_anim_textures_only(None);
    }
    pub fn advance(&mut self) -> bool {
        if let Some(session) = &mut self.session {
            let ok = session.advance();
            if let Some(step) = session.current() {
                // Hidratar pizarra del nuevo paso
                let mut doc = WhiteboardDoc::default();
                for elem in whiteboard_elements_for_hint(&step.whiteboard_hint) {
                    doc.add(elem);
                }
                self.whiteboard.doc = doc;
                if let Some(tmpl) = &step.manim_template {
                    // Avanzar implica nuevo concepto → cancelar previo y relanzar
                    self.orchestrator.cancel();
                    let _ = self.orchestrator.start(&step.title, tmpl.clone());
                    self.anim_frames = None;
                    self.clear_anim_textures_only(None);
                }
            }
            // Reinicia morph para el nuevo paso (burbuja entra de nuevo)
            self.opened_at = Some(Instant::now());
            ok
        } else {
            false
        }
    }
    pub fn close(&mut self) {
        self.session = None;
        self.orchestrator.cancel();
        self.opened_at = None;
        self.anim_frames = None;
        self.clear_anim_textures_only(None);
    }

    pub fn close_with_ctx(&mut self, ctx: &egui::Context) {
        self.session = None;
        self.orchestrator.cancel();
        self.opened_at = None;
        self.anim_frames = None;
        self.clear_anim_textures_only(Some(ctx));
    }
    pub fn tick(&mut self, now: Instant) {
        if let Some(state) = self.orchestrator.tick(now) {
            // Al completar, generar fallback nativo via anim_native si no hay artefacto real
            if matches!(state, OrchestratorState::Completed { .. }) && self.anim_frames.is_none() {
                let concept = self.orchestrator.concept.clone();
                let template = self.orchestrator.template.clone();
                let frames =
                    crate::anim_native::render_anim_for_concept(&template, &concept, 320, 180);
                self.anim_frames = Some(frames);
                self.clear_anim_textures_only(None);
            }
            let _ = state;
        }
    }
    /// Progreso 0..=1 del morph burbuja (ANIM_MICRO 180ms ease-out).
    pub fn morph_progress(&self) -> f32 {
        let Some(opened) = self.opened_at else {
            return 1.0;
        };
        let elapsed_ms = Instant::now().duration_since(opened).as_secs_f32() * 1000.0;
        (elapsed_ms / grafito_ui::tokens::ANIM_MICRO).clamp(0.0, 1.0)
    }
}

/// Dibuja la enseñanza si hay sesión activa. Retorna true si se cerró.
pub fn draw_teaching_overlay(
    state: &mut TeachingUiState,
    ctx: &egui::Context,
    budget: &mut crate::app::RepaintBudget,
) -> bool {
    if state.session.is_none() {
        return false;
    }
    // Unifica patrón cached_hash + ensure_textures como en anim_ui.rs:61 — evita clone masivo y leak.
    state.ensure_textures(ctx);
    // Snapshot inmutable para el closure (evita borrow cruzado &mut state.session + &state.orchestrator)
    let opened_at = state.opened_at;
    let Some(session_snapshot) = state.session.clone() else {
        return false;
    };
    let ledger = state.orchestrator.ledger.clone();
    let template = state.orchestrator.template.clone();
    let is_busy = state.orchestrator.is_busy();
    // Evita clone en draw: sólo se clonan handles si hash cambió (ensure_textures). Aquí se snapshot sin clonar frames.
    let anim_textures: Vec<egui::TextureHandle> = state.anim_textures.clone();
    let progress = session_snapshot.progress();
    let is_last = session_snapshot.is_last();
    let topic_label = session_snapshot.topic.label();
    let step_count = session_snapshot.steps.len();
    let current_idx = session_snapshot.current;
    let current_step = session_snapshot.current().cloned();

    let mut should_close = false;
    let mut should_advance = false;
    let theme = grafito_ui::theme::current_theme(ctx);
    let _ = opened_at;
    egui::Window::new("Enseñanza — Paso a paso")
        .id(egui::Id::new("teaching_overlay"))
        .collapsible(false)
        .resizable(true)
        .default_width(VENTANA_ANCHO_IDEAL)
        .min_width(VENTANA_ANCHO_MIN)
        .max_width(VENTANA_ANCHO_MAX)
        .default_height(VENTANA_ALTO_IDEAL)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel_bg)
                .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                .rounding(grafito_ui::tokens::RADIUS_LG)
                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
                .shadow(egui::Shadow {
                    offset: egui::vec2(0.0, grafito_ui::tokens::SHADOW_WINDOW_OFFSET_Y),
                    blur: grafito_ui::tokens::SHADOW_WINDOW_BLUR,
                    spread: 0.0,
                    color: Color32::from_black_alpha(grafito_ui::tokens::SHADOW_ALPHA),
                }),
        )
        .show(ctx, |ui| {
            let time = ui.input(|i| i.time);
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            // Header Scandinavian — left-aligned, avatar pequeño + título, cierre con icono
            ui.horizontal(|ui| {
                let cfg = grafito_profile::AvatarConfig::default();
                let (avatar_rect, _) = ui.allocate_exact_size(
                    egui::vec2(grafito_ui::tokens::TYPE_XXL, grafito_ui::tokens::TYPE_XXL),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(avatar_rect) {
                    let painter = ui.painter_at(avatar_rect);
                    grafito_ui::avatar::draw_avatar(&painter, avatar_rect, &cfg, time, hover_pos);
                }
                ui.add_space(grafito_ui::tokens::SPACE_SM);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(topic_label.clone())
                            .strong()
                            .size(grafito_ui::tokens::TYPE_BASE)
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(format!("Paso {} de {}", current_idx + 1, step_count))
                            .size(grafito_ui::tokens::TYPE_XS)
                            .color(theme.text_secondary.gamma_multiply(0.60)),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if action_icon_button(ui, Icon::Close, theme.text_secondary, "Cerrar").clicked()
                    {
                        should_close = true;
                    }
                });
            });
            // Burbuja morph avatar→burbuja, siempre dentro del viewport.
            // Clamp duro a `available_rect`: nunca se pasa del borde visible.
            {
                let progreso = state.morph_progress();
                let suave = progreso_morph_suave(progreso);
                let limite = ui.available_rect_before_wrap();
                let ancho = limite.width().max(grafito_ui::tokens::SPACE_XXL * 2.0);
                let (espacio, _) = ui.allocate_exact_size(
                    egui::vec2(ancho, grafito_ui::tokens::SPACE_XXL),
                    egui::Sense::hover(),
                );
                let origen = egui::Rect::from_min_size(
                    espacio.min,
                    egui::vec2(
                        (espacio.width() * 0.35).max(grafito_ui::tokens::SPACE_XXL * 2.0),
                        grafito_ui::tokens::SPACE_LG,
                    ),
                );
                let destino = egui::Rect::from_min_size(
                    espacio.min,
                    egui::vec2(
                        espacio.width(),
                        grafito_ui::tokens::SPACE_LG + grafito_ui::tokens::SPACE_SM,
                    ),
                );
                let burbuja = rect_burbuja_morph(origen, destino, suave, limite);
                if ui.is_rect_visible(burbuja) {
                    let alpha = (80.0 + 175.0 * suave) as u8;
                    ui.painter().rect_filled(
                        burbuja,
                        grafito_ui::tokens::RADIUS_MD,
                        theme.accent.gamma_multiply(suave * 0.16),
                    );
                    ui.painter().rect_stroke(
                        burbuja,
                        grafito_ui::tokens::RADIUS_MD,
                        Stroke::new(1.0, theme.separator.gamma_multiply(0.10)),
                    );
                    let texto = format!(
                        "Mirá el paso {} de {} — vamos de a poco, che.",
                        current_idx + 1,
                        step_count
                    );
                    ui.painter().text(
                        burbuja.center(),
                        egui::Align2::CENTER_CENTER,
                        texto.chars().take(72).collect::<String>(),
                        egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
                        theme.text_primary.gamma_multiply(alpha as f32 / 255.0),
                    );
                }
                // Mientras el morph (<180ms) pide cuadros cortos; al asentar, nada.
                if progreso < 1.0 {
                    budget.request(std::time::Duration::from_millis(16));
                }
            }
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Barra progreso — hairline 4px, sin animación extra
            let (r, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), grafito_ui::tokens::SPACE_XS),
                egui::Sense::hover(),
            );
            ui.painter()
                .rect_filled(r, 2.0, theme.separator.gamma_multiply(0.10));
            ui.painter().rect_filled(
                egui::Rect::from_min_size(r.min, egui::vec2(r.width() * progress, r.height())),
                2.0,
                theme.accent,
            );
            ui.add_space(grafito_ui::tokens::SPACE_SM);
            // Contenido principal — dentro de ScrollArea con max_height para no crear "altura enorme al pedo"
            // cuando la explicación es larga o la animación es alta. El footer (controles) queda fijo.
            let max_scroll_h = (ctx.screen_rect().height() * 0.55).clamp(220.0, 420.0);
            egui::ScrollArea::vertical()
                .id_salt("teaching_scroll")
                .max_height(max_scroll_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(step) = current_step.clone() {
                        egui::Frame::none()
                            .fill(theme.panel_bg)
                            .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                            .rounding(grafito_ui::tokens::RADIUS_LG)
                            .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_MD))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.label(
                                    egui::RichText::new(&step.title)
                                        .strong()
                                        .size(grafito_ui::tokens::TYPE_MD)
                                        .color(theme.accent),
                                );
                                ui.add_space(grafito_ui::tokens::SPACE_XS);
                                ui.label(
                                    egui::RichText::new(&step.explanation)
                                        .size(grafito_ui::tokens::TYPE_BASE)
                                        .color(theme.text_primary),
                                );
                                if let Some(expr) = &step.math_expr {
                                    ui.add_space(grafito_ui::tokens::SPACE_SM);
                                    egui::Frame::none()
                                        .fill(theme.input_bg)
                                        .stroke(Stroke::new(
                                            1.0,
                                            theme.separator.gamma_multiply(0.10),
                                        ))
                                        .rounding(grafito_ui::tokens::RADIUS_MD)
                                        .inner_margin(egui::Margin::same(
                                            grafito_ui::tokens::SPACE_SM,
                                        ))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(expr)
                                                    .monospace()
                                                    .size(grafito_ui::tokens::TYPE_SM)
                                                    .color(theme.text_primary),
                                            );
                                        });
                                }
                                if !step.whiteboard_hint.is_empty() {
                                    ui.add_space(grafito_ui::tokens::SPACE_XS);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Pizarra: {}",
                                            step.whiteboard_hint
                                        ))
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_tertiary)
                                        .weak(),
                                    );
                                }
                            });
                        // Pizarra vectorial real — altura responsive clamp 96..160, no fija 120 enorme
                        if !step.whiteboard_hint.is_empty() || !state.whiteboard.doc.is_empty() {
                            ui.add_space(grafito_ui::tokens::SPACE_SM);
                            egui::Frame::none()
                                .fill(theme.input_bg)
                                .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                                .rounding(grafito_ui::tokens::RADIUS_MD)
                                .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                                .show(ui, |ui| {
                                    ui.set_min_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("Pizarra")
                                                .size(grafito_ui::tokens::TYPE_XS)
                                                .color(theme.text_tertiary)
                                                .strong(),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "· {}",
                                                step.whiteboard_hint
                                            ))
                                            .size(grafito_ui::tokens::TYPE_XS)
                                            .color(theme.text_secondary),
                                        );
                                    });
                                    ui.add_space(grafito_ui::tokens::SPACE_XS);
                                    // Altura responsive: 120 ideal pero clamp a 96..160 y a 30% del alto disponible
                                    let wb_h = (grafito_ui::tokens::SPACE_XXL * 3.0)
                                        .clamp(96.0, 160.0)
                                        .min((ui.available_height() * 0.35).max(96.0));
                                    let (wb_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(ui.available_width(), wb_h),
                                        egui::Sense::click_and_drag(),
                                    );
                                    // Dibujar pizarra vectorial real (trazo, rectángulos, flechas)
                                    state.whiteboard.draw(ui, wb_rect);
                                    // Permitir dibujar encima (pencil) dentro del overlay
                                    state.whiteboard.handle_canvas_input(wb_rect, ui, budget);
                                    if ui.is_rect_visible(wb_rect) {
                                        // Borde sutil por encima del draw para definición
                                        ui.painter().rect_stroke(
                                            wb_rect,
                                            grafito_ui::tokens::RADIUS_MD,
                                            Stroke::new(1.0, theme.separator.gamma_multiply(0.08)),
                                        );
                                        // Hint centrado sobre grilla
                                        ui.painter().text(
                                            wb_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            &step.whiteboard_hint,
                                            egui::FontId::proportional(grafito_ui::tokens::TYPE_XS),
                                            theme.text_tertiary.gamma_multiply(0.85),
                                        );
                                    }
                                });
                        }
                        // Animación nativa fallback (si completó)
                        if !anim_textures.is_empty() {
                            ui.add_space(grafito_ui::tokens::SPACE_SM);
                            let time = ui.input(|i| i.time);
                            let idx = ((time * 12.0) as usize) % anim_textures.len();
                            let tex = &anim_textures[idx];
                            let max_w = ui
                                .available_width()
                                .max(grafito_ui::tokens::SPACE_XXL * 2.0);
                            // Clampear altura para no generar "altura enorme al pedo" con texturas retrato
                            let max_h = (grafito_ui::tokens::SPACE_XXL * 5.0).min(
                                ui.available_height()
                                    .max(grafito_ui::tokens::SPACE_XXL * 2.0)
                                    * 0.6,
                            );
                            let size = tex.size_vec2();
                            let scale_w = (max_w / size.x.max(1.0)).clamp(0.25, 1.0);
                            let scale_h = (max_h / size.y.max(1.0)).clamp(0.25, 1.0);
                            let scale = scale_w.min(scale_h);
                            let display = egui::vec2(size.x * scale, size.y * scale).ceil();
                            let (rect, _) = ui.allocate_exact_size(display, egui::Sense::hover());
                            ui.painter().image(
                                tex.id(),
                                rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                Color32::WHITE,
                            );
                            ui.painter().rect_stroke(
                                rect,
                                grafito_ui::tokens::RADIUS_MD,
                                Stroke::new(1.0, theme.separator.gamma_multiply(0.10)),
                            );
                            // F17: playback 12fps vía presupuesto coalescido.
                            budget.request(Duration::from_millis(80));
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Animación: {} — {} cuadros (reserva nativa)",
                                    template,
                                    anim_textures.len()
                                ))
                                .size(grafito_ui::tokens::TYPE_XS)
                                .color(theme.text_tertiary)
                                .weak(),
                            );
                        } else if is_busy {
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
                            ui.horizontal(|ui| {
                                let t = ui.input(|i| i.time);
                                let pulse = ((t * 3.0).sin() + 1.0) * 0.5;
                                let col = theme.accent.gamma_multiply(0.45 + 0.55 * pulse as f32);
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(
                                        grafito_ui::tokens::SPACE_SM
                                            + grafito_ui::tokens::SHADOW_WINDOW_OFFSET_Y,
                                        grafito_ui::tokens::SPACE_SM
                                            + grafito_ui::tokens::SHADOW_WINDOW_OFFSET_Y,
                                    ),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(
                                    rect.center(),
                                    grafito_ui::tokens::SPACE_XS,
                                    col,
                                );
                                ui.label(
                                    egui::RichText::new("Generando animación con Manim…")
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_secondary),
                                );
                            });
                            // F17: pulso "Generando animación" vía presupuesto coalescido.
                            budget.request(Duration::from_millis(48));
                        }
                        // Ledger colapsable — no ocupa altura si no se necesita
                        if let Some(ledger) = &ledger {
                            ui.add_space(grafito_ui::tokens::SPACE_XS);
                            egui::CollapsingHeader::new(
                                egui::RichText::new("Detalle de generación")
                                    .size(grafito_ui::tokens::TYPE_XS)
                                    .color(theme.text_tertiary),
                            )
                            .id_salt("teaching_ledger")
                            .show(ui, |ui| {
                                egui::Frame::none()
                                    .fill(theme.input_bg)
                                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.08)))
                                    .rounding(grafito_ui::tokens::RADIUS_MD)
                                    .inner_margin(egui::Margin::same(grafito_ui::tokens::SPACE_SM))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(ledger)
                                                .monospace()
                                                .size(grafito_ui::tokens::TYPE_2XS)
                                                .color(theme.text_secondary),
                                        );
                                    });
                            });
                        }
                    }
                });
            ui.add_space(grafito_ui::tokens::SPACE_MD);
            // Controles profesionales — primaria llena ancho, secundaria ghost, iconografía limpia
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = grafito_ui::tokens::SPACE_SM;
                let primary_label = if is_last { "Finalizar" } else { "Siguiente" };
                let primary_icon = if is_last {
                    Icon::Check
                } else {
                    Icon::ChevronRight
                };
                // Botón primario: fill accent, 36h, RADIUS_MD, left-aligned label + right icon
                let btn = egui::Button::new(
                    egui::RichText::new(primary_label)
                        .strong()
                        .size(grafito_ui::tokens::TYPE_SM)
                        .color(Color32::WHITE),
                )
                .fill(theme.accent)
                .stroke(Stroke::NONE)
                .rounding(grafito_ui::tokens::RADIUS_MD);
                // Tamaño igual para primaria, ocupa espacio proporcional
                if ui
                    .add_sized(
                        egui::vec2(
                            grafito_ui::tokens::SPACE_XXL * 3.5,
                            grafito_ui::tokens::SPACE_LG * 2.25,
                        ),
                        btn,
                    )
                    .on_hover_text(if is_last {
                        "Cierra la enseñanza"
                    } else {
                        "Avanza al siguiente paso"
                    })
                    .clicked()
                {
                    if is_last {
                        should_close = true;
                    } else {
                        should_advance = true;
                    }
                }
                // Icono decorativo pequeño al lado (no duplica label, solo indica dirección)
                let icon_color = theme.accent;
                let (icon_rect, _) = ui.allocate_exact_size(
                    egui::vec2(grafito_ui::tokens::ICON_SM, grafito_ui::tokens::ICON_SM),
                    egui::Sense::hover(),
                );
                if ui.is_rect_visible(icon_rect) {
                    grafito_ui::icons::draw_icon(ui.painter(), icon_rect, primary_icon, icon_color);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ghost = egui::Button::new(
                        egui::RichText::new("Cerrar").size(grafito_ui::tokens::TYPE_SM),
                    )
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme.separator.gamma_multiply(0.12)))
                    .rounding(grafito_ui::tokens::RADIUS_MD);
                    if ui
                        .add_sized(
                            egui::vec2(
                                grafito_ui::tokens::SPACE_XL * 4.0,
                                grafito_ui::tokens::SPACE_LG * 2.25,
                            ),
                            ghost,
                        )
                        .clicked()
                    {
                        should_close = true;
                    }
                });
            });
        });
    if should_advance {
        // Avance con ctx para forget_image correcto.
        let cached_before = state.cached_len;
        let ok = state.advance();
        // advance limpió sin ctx; ahora olvidar los URIs previos que quedaron huérfanos.
        for idx in 0..cached_before {
            ctx.forget_image(&format!("teaching_anim_{idx}"));
        }
        let _ = ok;
    }
    if should_close {
        state.close_with_ctx(ctx);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pista_de_sesion(texto: &str) -> String {
        let sesion = TeachingSession::for_topic(texto);
        sesion
            .steps
            .first()
            .map(|s| s.whiteboard_hint.clone())
            .unwrap_or_default()
    }

    #[test]
    fn pista_para_topico_cubre_13_variantes_cerradas() {
        assert_eq!(
            pista_para_topico(&TeachingTopic::Derivada),
            WhiteboardHint::Secante
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Integral),
            WhiteboardHint::Area
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Limite),
            WhiteboardHint::Limite
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Funcion),
            WhiteboardHint::Funcion
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Pitagoras),
            WhiteboardHint::Pitagoras
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Fraccion),
            WhiteboardHint::Fraccion
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Vector),
            WhiteboardHint::Vector
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Matriz),
            WhiteboardHint::Matriz
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Probabilidad),
            WhiteboardHint::Probabilidad
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Serie),
            WhiteboardHint::Serie
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Ecuacion),
            WhiteboardHint::Ecuacion
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Trigonometria),
            WhiteboardHint::Trigonometria
        );
        assert_eq!(
            pista_para_topico(&TeachingTopic::Conica),
            WhiteboardHint::Conica
        );
    }

    #[test]
    fn pista_para_topico_general_cae_en_general() {
        assert_eq!(
            pista_para_topico(&TeachingTopic::General("estrellas".into())),
            WhiteboardHint::General
        );
    }

    #[test]
    fn elementos_para_topico_no_vacios_para_las_14_variantes() {
        let topicos = [
            TeachingTopic::Derivada,
            TeachingTopic::Integral,
            TeachingTopic::Limite,
            TeachingTopic::Funcion,
            TeachingTopic::Pitagoras,
            TeachingTopic::Fraccion,
            TeachingTopic::Vector,
            TeachingTopic::Matriz,
            TeachingTopic::Probabilidad,
            TeachingTopic::Serie,
            TeachingTopic::Ecuacion,
            TeachingTopic::Trigonometria,
            TeachingTopic::Conica,
            TeachingTopic::General("origami".into()),
        ];
        for t in topicos {
            let elems = elementos_para_topico(&t);
            assert!(!elems.is_empty(), "tópico sin elementos: {t:?}");
        }
    }

    #[test]
    fn hint_legacy_derivada_da_secante_e_integral_da_area() {
        assert_eq!(
            hint_for_topic(&pista_de_sesion("derivada de x^2")),
            WhiteboardHint::Secante
        );
        assert_eq!(
            hint_for_topic(&pista_de_sesion("integral de x^2")),
            WhiteboardHint::Area
        );
    }

    #[test]
    fn hint_legacy_limite_funcion_pitagoras() {
        assert_eq!(
            hint_for_topic("Recta con hueco en a"),
            WhiteboardHint::Limite
        );
        assert_eq!(
            hint_for_topic("Ejes con puntos (x, f(x))"),
            WhiteboardHint::Funcion
        );
        assert_eq!(
            hint_for_topic("Triángulo rectángulo con cuadrados en cada lado"),
            WhiteboardHint::Pitagoras
        );
    }

    #[test]
    fn hint_legacy_fraccion_vector_matriz() {
        assert_eq!(
            hint_for_topic("Rectángulo dividido en partes"),
            WhiteboardHint::Fraccion
        );
        assert_eq!(hint_for_topic("Flecha en ejes R²"), WhiteboardHint::Vector);
        assert_eq!(hint_for_topic("Grilla 2x2"), WhiteboardHint::Matriz);
    }

    #[test]
    fn hint_legacy_probabilidad_serie_trigonometria() {
        assert_eq!(
            hint_for_topic("Diagrama de árbol"),
            WhiteboardHint::Probabilidad
        );
        assert_eq!(
            hint_for_topic("Suma parcial que se aproxima"),
            WhiteboardHint::Serie
        );
        assert_eq!(
            hint_for_topic("Círculo unitario con ángulo"),
            WhiteboardHint::Trigonometria
        );
    }

    #[test]
    fn hint_legacy_conica_ecuacion_libre_general() {
        assert_eq!(hint_for_topic("Elipse con focos"), WhiteboardHint::Conica);
        assert_eq!(
            hint_for_topic("Parábola y raíces"),
            WhiteboardHint::Ecuacion
        );
        assert_eq!(
            hint_for_topic("Pizarra libre para trazar"),
            WhiteboardHint::Libre
        );
        assert!(hint_for_topic("").is_vacio_o_libre());
        assert_eq!(hint_for_topic("origami de papel"), WhiteboardHint::General);
    }

    #[test]
    fn hint_pitagoras_tiene_prioridad_sobre_area() {
        // "triángulo rectángulo" contiene "rectángulo": debe ganar Pitágoras.
        assert_eq!(
            hint_for_topic("triángulo rectángulo con cuadrados"),
            WhiteboardHint::Pitagoras
        );
    }

    #[test]
    fn elementos_para_hint_vacio_y_libre_no_dibujan() {
        assert!(whiteboard_elements_for_hint("").is_empty());
        assert!(whiteboard_elements_for_hint("   ").is_empty());
        assert!(whiteboard_elements_for_hint("Pizarra libre").is_empty());
    }

    #[test]
    fn morph_suave_arranca_lento_y_termina_en_uno() {
        assert_eq!(progreso_morph_suave(0.0), 0.0);
        assert_eq!(progreso_morph_suave(1.0), 1.0);
        let medio = progreso_morph_suave(0.5);
        assert!(medio > 0.5 && medio < 1.0);
    }

    #[test]
    fn burbuja_nunca_se_pasa_del_limite() {
        let limite = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(300.0, 200.0));
        let origen = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(60.0, 30.0));
        let destino = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(900.0, 800.0));
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let r = rect_burbuja_morph(origen, destino, t, limite);
            assert!(limite.contains_rect(r), "se pasó en t={t}: {r:?}");
        }
    }

    #[test]
    fn acotar_no_rompe_utf8_y_opciones_son_cuatro() {
        assert!(acotar_texto("áéíóú", 3).chars().count() <= 3);
        let ejercicio = Exercise {
            prompt: "Si f(x)=2*x+1, evalúa en x=3".into(),
            solution: "7".into(),
            kind: grafito_pedagogy::ExerciseKind::Numeric,
            difficulty: grafito_pedagogy::ExerciseDifficulty::Medium,
            lo_id: "x".into(),
            params: std::collections::HashMap::new(),
            seed: Some(1),
            validator: grafito_pedagogy::ValidatorKind::NumericTol(0.02),
        };
        let ops = opciones_para_ejercicio(&ejercicio);
        assert_eq!(ops.len(), 4);
        assert!(ops.iter().any(|o| o.trim() == "7"));
        assert!(indice_respuesta_correcta(&ejercicio, &ops).is_some());
    }

    // ── B7: panel de ejercicio con estados ──

    #[test]
    fn b7_resolver_lo_exactoy_fuzzy() {
        let por_id = resolver_lo("am1-der").expect("am1-der existe");
        assert_eq!(por_id.id, "am1-der");
        let fuzzy = resolver_lo("derivada").expect("derivada resuelve");
        assert_eq!(fuzzy.id, "am1-der");
        // Con tilde también (reintento sin tildes).
        let tilde = resolver_lo("función").expect("función resuelve");
        assert!(!tilde.id.is_empty());
    }

    #[test]
    fn b7_resolver_lo_vacio_y_desconocido_dan_error_honesto() {
        assert!(resolver_lo("").is_err());
        assert!(resolver_lo("   ").is_err());
        let err = resolver_lo("origami de papel xyz").expect_err("sin LO");
        assert!(err.contains("origami de papel xyz"));
    }

    #[test]
    fn b7_armar_ejercicio_valido_y_acotado() {
        let ejercicio =
            armar_ejercicio("derivada", grafito_pedagogy::PedagogicalLevel::Secondary, 7)
                .expect("ejercicio válido");
        assert!(ejercicio.validate().is_ok());
        assert!(!ejercicio.prompt.trim().is_empty());
        assert!(!ejercicio.solution.trim().is_empty());
        let ops = opciones_para_ejercicio(&ejercicio);
        assert_eq!(ops.len(), 4);
        assert!(indice_respuesta_correcta(&ejercicio, &ops).is_some());
    }

    #[test]
    fn b7_pedir_genera_en_background_y_lista() {
        let mut panel = PanelEjercicio::default();
        assert_eq!(panel.estado, EstadoPanelEjercicio::Vacio);
        panel.pedir("derivada", grafito_pedagogy::PedagogicalLevel::Secondary);
        assert_eq!(panel.estado, EstadoPanelEjercicio::Generando);
        // El job es CPU pura: converge en pocos ms (cota 2s anti-flake).
        let mut lista = false;
        for _ in 0..400 {
            if panel.sondear() && panel.estado == EstadoPanelEjercicio::Lista {
                lista = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(lista, "el job debió listar, estado={:?}", panel.estado);
        assert_eq!(panel.opciones.len(), 4);
        assert!(panel.ejercicio.is_some());
    }

    #[test]
    fn b7_pedir_tema_vacio_da_error_y_cerrar_resetea() {
        let mut panel = PanelEjercicio::default();
        panel.pedir("   ", grafito_pedagogy::PedagogicalLevel::Secondary);
        assert_eq!(panel.estado, EstadoPanelEjercicio::Error);
        assert!(panel.error.is_some());
        panel.cerrar();
        assert_eq!(panel.estado, EstadoPanelEjercicio::Vacio);
        assert!(panel.error.is_none());
        assert!(panel.ejercicio.is_none());
    }

    #[test]
    fn b7_otro_ejercicio_cambia_semilla() {
        let mut panel = PanelEjercicio::default();
        panel.pedir("derivada", grafito_pedagogy::PedagogicalLevel::Secondary);
        let primera = panel.semilla;
        panel.pedir("derivada", grafito_pedagogy::PedagogicalLevel::Secondary);
        assert_ne!(panel.semilla, primera);
        panel.cerrar();
    }

    trait VacioOlibre {
        fn is_vacio_o_libre(&self) -> bool;
    }
    impl VacioOlibre for WhiteboardHint {
        fn is_vacio_o_libre(&self) -> bool {
            matches!(self, WhiteboardHint::Vacio | WhiteboardHint::Libre)
        }
    }
}
