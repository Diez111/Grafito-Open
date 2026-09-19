//! Contorno complejo dibujado a mano: estado del trazo, guías (imán,
//! estabilizador, cierre automático, suavizado) y overlay en vivo.
//!
//! El trazo vive fuera del documento hasta soltar: se dibuja con el painter
//! del canvas (como el integrador interactivo clásico), el valor `∮ f(z) dz`
//! se acumula incrementalmente con [`ContourAccumulator`] y recién al
//! terminar se insertan `PencilObj` + `ComplexIntegralObj` en un solo paso de
//! undo. Así no hay churn del documento ni del índice espacial por frame.

use std::collections::HashMap;

use grafito_complex::math::complex_calculus::{
    circle_contour_integral, format_complex_rounded, ContourAccumulator, CIRCLE_QUADRATURE_ARCS,
    MAX_QUADRATURE_SEGMENTS,
};
use grafito_complex::math::complex_expr::{parse as parse_complex, ComplexExpr};
use grafito_core::PencilObj;
use grafito_geometry::Point2;
use num_complex::Complex64;

/// Radio (px) para considerar que el trazo vuelve al inicio y cerrar el lazo.
pub const CLOSURE_RADIUS_PX: f64 = 12.0;
/// Distancia mínima (px) entre muestras capturadas del trazo.
pub const MIN_SAMPLE_STEP_PX: f64 = 1.5;
/// Tope de puntos vivos antes de decimar (mantiene vivo == persistido).
const MAX_LIVE_POINTS: usize = MAX_QUADRATURE_SEGMENTS + 1;
/// Subdivisiones de Catmull-Rom al suavizar el trazo para el cálculo.
const SMOOTH_SUBDIVISIONS: usize = 6;

/// Modo de captura del contorno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComplexContourMode {
    /// Trazo libre con el puntero (o stylus).
    #[default]
    Freehand,
    /// Clic en el centro + arrastre del radio (circunferencia exacta).
    Circle,
}

/// Ajustes del contorno elegidos en el panel de Números Complejos.
#[derive(Debug, Clone)]
pub struct ComplexContourSettings {
    /// Expresión `f(z)` a integrar.
    pub expr: String,
    /// `true` = crear `Gauss` (ΣResiduos) en vez de la integral cruda.
    pub residues: bool,
    /// Imán: adhiere cada muestra a puntos/ejes/cuadrícula (Shift libera).
    pub snap: bool,
    /// Cierra el lazo si el trazo termina cerca del inicio.
    pub auto_close: bool,
    /// 0..=0.8: cuánto frena el pulso (alpha = 1 − estabilizador).
    pub stabilizer: f32,
    /// Suaviza (Catmull-Rom) el trazo persistido para el cálculo.
    pub smooth: bool,
    /// Modo activo de captura.
    pub mode: ComplexContourMode,
}

impl Default for ComplexContourSettings {
    fn default() -> Self {
        Self {
            expr: "1/z".to_string(),
            residues: false,
            snap: true,
            auto_close: true,
            stabilizer: 0.25,
            smooth: true,
            mode: ComplexContourMode::Freehand,
        }
    }
}

/// Estado completo del contorno en la app (ajustes + trazo vivo + último valor).
#[derive(Debug, Default)]
pub struct ComplexContourState {
    pub settings: ComplexContourSettings,
    pub live: Option<LiveContour>,
    /// Último valor publicado (chip persistente de la barra de estado).
    pub last_value: Option<String>,
    /// Último resultado del imán (marca + etiqueta en el overlay).
    pub snap_marker: Option<crate::snap::SnapResult>,
}

/// Trazo en curso. Los puntos ya vienen filtrados por imán/estabilizador.
pub struct LiveContour {
    points: Vec<Point2>,
    accumulator: ContourAccumulator,
    expression: ComplexExpr,
    symbol: String,
    vars: HashMap<String, Complex64>,
    stabilized: Option<Point2>,
    /// Centro del modo círculo (si aplica).
    pub center: Option<Point2>,
    /// Último radio del modo círculo.
    pub radius: f64,
    pub error: Option<String>,
}

impl std::fmt::Debug for LiveContour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveContour")
            .field("points", &self.points.len())
            .field("segments", &self.accumulator.segments())
            .field("value", &self.accumulator.value())
            .finish()
    }
}

impl LiveContour {
    /// Arranca el trazo libre con el primer punto (ya en mundo).
    fn freehand(
        expression: ComplexExpr,
        symbol: &str,
        vars: HashMap<String, Complex64>,
        first: Point2,
    ) -> Self {
        let mut accumulator = ContourAccumulator::new(expression.clone(), symbol, vars.clone());
        let _ = accumulator.push(Complex64::new(first.x, first.y));
        Self {
            points: vec![first],
            accumulator,
            expression,
            symbol: symbol.to_string(),
            vars,
            stabilized: Some(first),
            center: None,
            radius: 0.0,
            error: None,
        }
    }

    /// Arranca el modo círculo con el centro elegido (ya en mundo).
    fn circle(
        expression: ComplexExpr,
        symbol: &str,
        vars: HashMap<String, Complex64>,
        center: Point2,
    ) -> Self {
        let accumulator = ContourAccumulator::new(expression.clone(), symbol, vars.clone());
        Self {
            points: Vec::new(),
            accumulator,
            expression,
            symbol: symbol.to_string(),
            vars,
            stabilized: None,
            center: Some(center),
            radius: 0.0,
            error: None,
        }
    }

    pub fn points(&self) -> &[Point2] {
        &self.points
    }

    /// Valor en vivo: acumulado del trazo libre o integral analítica del
    /// círculo actual. `Err` si el contorno cruzó un polo (honesto, sin valor).
    pub fn value(&self) -> Result<Complex64, String> {
        if let Some(center) = self.center {
            if self.radius <= 0.0 {
                return Ok(Complex64::new(0.0, 0.0));
            }
            return circle_contour_integral(
                &self.expression,
                Complex64::new(center.x, center.y),
                self.radius,
                CIRCLE_QUADRATURE_ARCS,
                &self.vars,
                &self.symbol,
            );
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(self.accumulator.value())
    }

    /// Suma un punto al trazo libre. Aplica el estabilizador (lerp) antes de
    /// integrar el segmento. Ignora muestras demasiado cercanas.
    fn push_stabilized(&mut self, raw: Point2, stabilizer: f32, view_scale: f64) {
        let smoothed = match self.stabilized {
            Some(previous) => stabilize_point(previous, raw, stabilizer),
            None => raw,
        };
        self.stabilized = Some(smoothed);
        let min_step = MIN_SAMPLE_STEP_PX / view_scale.max(1e-6);
        let should_push = self
            .points
            .last()
            .map(|last| last.distance(&smoothed) >= min_step)
            .unwrap_or(true);
        if !should_push {
            return;
        }
        if self.error.is_some() {
            return;
        }
        self.points.push(smoothed);
        if self.points.len() > MAX_LIVE_POINTS {
            self.decimate();
        }
        if let Err(error) = self
            .accumulator
            .push(Complex64::new(smoothed.x, smoothed.y))
        {
            self.error = Some(error);
        }
    }

    /// Decima el trazo por mitades y reconstruye el acumulador (el valor vivo
    /// vuelve a coincidir con el que se persistirá).
    fn decimate(&mut self) {
        let mut decimated: Vec<Point2> = self.points.iter().step_by(2).copied().collect();
        if let Some(last) = self.points.last().copied() {
            if decimated.last() != Some(&last) {
                decimated.push(last);
            }
        }
        self.points = decimated;
        self.rebuild_accumulator();
    }

    fn rebuild_accumulator(&mut self) {
        let mut accumulator =
            ContourAccumulator::new(self.expression.clone(), &self.symbol, self.vars.clone());
        self.error = None;
        for point in &self.points {
            if let Err(error) = accumulator.push(Complex64::new(point.x, point.y)) {
                self.error = Some(error);
                break;
            }
        }
        self.accumulator = accumulator;
    }
}

/// Estabilizador puro: interpola `previous → raw` con `alpha = 1 − stabilizer`.
/// Testeable sin estado.
pub fn stabilize_point(previous: Point2, raw: Point2, stabilizer: f32) -> Point2 {
    let alpha = (1.0 - stabilizer.clamp(0.0, 0.8)).max(0.2);
    Point2::new(
        previous.x + (raw.x - previous.x) * f64::from(alpha),
        previous.y + (raw.y - previous.y) * f64::from(alpha),
    )
}

/// ¿El trazo debe cerrarse? Distancia del último punto al primero dentro del
/// radio de cierre (en mundo, derivado de píxeles).
pub fn should_close(first: Point2, last: Point2, view_scale: f64) -> bool {
    first.distance(&last) <= CLOSURE_RADIUS_PX / view_scale.max(1e-6)
}

/// Suaviza el trazo con Catmull-Rom (whiteboard) para el cálculo y la
/// persistencia. Devuelve los puntos originales si el suavizado no aplica.
pub fn smooth_points(points: &[Point2]) -> Vec<Point2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let as_f64: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
    let smoothed = grafito_whiteboard::smooth_stroke(&as_f64, SMOOTH_SUBDIVISIONS);
    if smoothed.len() < 2 {
        return points.to_vec();
    }
    smoothed
        .into_iter()
        .map(|(x, y)| Point2::new(x, y))
        .collect()
}

/// Convierte un `Document` en el entorno complejo de `f(z)`: todas las
/// variables del documento como reales.
pub fn document_complex_vars(document: &grafito_core::Document) -> HashMap<String, Complex64> {
    document
        .variables
        .iter()
        .map(|(name, value)| (name.clone(), Complex64::new(*value, 0.0)))
        .collect()
}

/// Arranca un trazo libre. `Err` con la expresión parseada si `f(z)` es inválida.
pub fn start_freehand(
    document: &grafito_core::Document,
    first: Point2,
    settings: &ComplexContourSettings,
) -> Result<LiveContour, String> {
    let expression =
        parse_complex(settings.expr.trim()).map_err(|error| format!("f(z) inválida: {error}"))?;
    let symbol = document.complex_base_symbol.clone();
    Ok(LiveContour::freehand(
        expression,
        &symbol,
        document_complex_vars(document),
        first,
    ))
}

/// Arranca un trazo de círculo (centro ya elegido).
pub fn start_circle(
    document: &grafito_core::Document,
    center: Point2,
    settings: &ComplexContourSettings,
) -> Result<LiveContour, String> {
    let expression =
        parse_complex(settings.expr.trim()).map_err(|error| format!("f(z) inválida: {error}"))?;
    let symbol = document.complex_base_symbol.clone();
    Ok(LiveContour::circle(
        expression,
        &symbol,
        document_complex_vars(document),
        center,
    ))
}

impl ComplexContourState {
    /// Captura un punto del trazo libre (estabiliza, imán/cierre ya aplicados
    /// por el caller). No hace nada sin trazo activo.
    pub fn capture_freehand(&mut self, raw: Point2, view_scale: f64) {
        if let Some(live) = self.live.as_mut() {
            if live.center.is_none() {
                live.push_stabilized(raw, self.settings.stabilizer, view_scale);
            }
        }
    }

    /// Actualiza el radio del modo círculo.
    pub fn capture_circle(&mut self, raw: Point2) {
        if let Some(live) = self.live.as_mut() {
            if let Some(center) = live.center {
                live.radius = center.distance(&raw);
            }
        }
    }

    /// Cierra el lazo si corresponde y devuelve el texto del valor final.
    /// `None` si no hay trazo (o si no junta 2 puntos: se descarta).
    pub fn finish(self, view_scale: f64) -> FinishedContour {
        let Some(mut live) = self.live else {
            return FinishedContour::Empty;
        };
        if let Some(center) = live.center {
            if live.radius <= 0.0 {
                return FinishedContour::Empty;
            }
            return FinishedContour::Circle {
                center,
                radius: live.radius,
            };
        }
        if live.points.len() < 2 {
            return FinishedContour::Empty;
        }
        if self.settings.auto_close {
            if let (Some(first), Some(last)) =
                (live.points.first().copied(), live.points.last().copied())
            {
                if last.distance(&first) > 1e-9 && should_close(first, last, view_scale) {
                    // Cierre exacto: el punto inicial y el acumulador integran
                    // el segmento de vuelta (lazo ∮). Si el trazo ya terminó
                    // justo en el inicio no se duplica el punto.
                    live.points.push(first);
                    if live.error.is_none() {
                        let _ = live.accumulator.push(Complex64::new(first.x, first.y));
                    }
                }
            }
        }
        let points = if self.settings.smooth {
            smooth_points(&live.points)
        } else {
            live.points.clone()
        };
        let value = live
            .value()
            .map(|raw| format_complex_rounded(displayed_value(raw, self.settings.residues)))
            .ok();
        FinishedContour::Freehand { points, value }
    }
}

/// Resultado de terminar un contorno.
pub enum FinishedContour {
    Empty,
    Freehand {
        points: Vec<Point2>,
        value: Option<String>,
    },
    Circle {
        center: Point2,
        radius: f64,
    },
}

/// Valor mostrado según el modo: crudo o ΣResiduos (factor de Cauchy).
pub fn displayed_value(raw: Complex64, residues: bool) -> Complex64 {
    if residues {
        grafito_complex::math::complex_calculus::residues_from_contour_integral(raw)
    } else {
        raw
    }
}

/// Construye los objetos persistidos del contorno terminado: el trazo
/// (`Pencil` o `Circle`) y su `ComplexIntegral` apuntando a él. Lista vacía
/// si el contorno no dejó nada. El `id` del trazo se genera acá, así el
/// integral referencia exactamente el objeto que se inserta.
pub fn build_contour_objects(
    finished: FinishedContour,
    settings: &ComplexContourSettings,
    color: grafito_geometry::Color,
) -> Vec<grafito_core::GeoObject> {
    match finished {
        FinishedContour::Empty => Vec::new(),
        FinishedContour::Freehand { points, .. } => {
            let pencil = pencil_from_points(points, color);
            let pencil_id = pencil.id;
            let integral = grafito_core::ComplexIntegralObj::new(
                settings.expr.trim(),
                pencil_id,
                settings.residues,
            );
            vec![
                grafito_core::GeoObject::Pencil(pencil),
                grafito_core::GeoObject::ComplexIntegral(integral),
            ]
        }
        FinishedContour::Circle { center, radius } => {
            let circle = grafito_core::CircleObj::new(Point2::new(center.x, center.y), radius);
            let circle_id = circle.id;
            let integral = grafito_core::ComplexIntegralObj::new(
                settings.expr.trim(),
                circle_id,
                settings.residues,
            );
            vec![
                grafito_core::GeoObject::Circle(circle),
                grafito_core::GeoObject::ComplexIntegral(integral),
            ]
        }
    }
}

/// Construye el trazo persistente a partir de los puntos capturados.
pub fn pencil_from_points(points: Vec<Point2>, color: grafito_geometry::Color) -> PencilObj {
    let mut pencil = PencilObj::new(points);
    pencil.color = color;
    pencil.width = 2.0;
    pencil
}

impl LiveContour {
    /// Último punto estabilizado (o centro del círculo): ancla del chip.
    pub fn cursor(&self) -> Option<Point2> {
        self.stabilized.or(self.center)
    }
}

/// Dibuja el trazo vivo, el imán y el chip con el valor `∮ f dz`.
///
/// Puro sobre `egui::Painter`: el caller ya recortó al canvas. El chip usa
/// tokens del tema (panel + hairline + radio), nunca colores inventados.
pub fn draw_overlay(
    painter: &egui::Painter,
    canvas_rect: egui::Rect,
    view: &grafito_geometry::ViewTransform,
    live: &LiveContour,
    settings: &ComplexContourSettings,
    snap: Option<&crate::snap::SnapResult>,
    theme: &grafito_ui::theme::Theme,
) {
    use grafito_ui::tokens::{RADIUS_SM, SPACE_SM, SPACE_XS, TYPE_SM, TYPE_XS};

    let to_screen = |point: Point2| {
        let screen = view.world_to_screen(point);
        egui::pos2(canvas_rect.min.x + screen.x, canvas_rect.min.y + screen.y)
    };

    // ── Trazo / circunferencia ──
    let stroke = egui::Stroke::new(2.0, theme.accent);
    if let Some(center) = live.center {
        // Círculo: 72 segmentos bastan visualmente y no dependen del zoom.
        let segments = 72;
        let mut previous = None;
        for i in 0..=segments {
            let angle = i as f64 / segments as f64 * std::f64::consts::TAU;
            let point = Point2::new(
                center.x + live.radius * angle.cos(),
                center.y + live.radius * angle.sin(),
            );
            let screen = to_screen(point);
            if let Some(previous) = previous {
                painter.line_segment([previous, screen], stroke);
            }
            previous = Some(screen);
        }
        painter.circle_filled(to_screen(center), 3.0, theme.accent);
    } else {
        let points = live.points();
        for window in points.windows(2) {
            painter.line_segment([to_screen(window[0]), to_screen(window[1])], stroke);
        }
        // Punto de inicio: anillo que invita a cerrar el lazo.
        if let Some(first) = points.first() {
            painter.circle_stroke(to_screen(*first), 6.0, egui::Stroke::new(1.5, theme.accent));
        }
    }

    // ── Imán: marca del punto adherido (si no está libre) ──
    if let Some(snap) = snap {
        if snap.kind != crate::snap::SnapKind::Free {
            let at = to_screen(snap.point);
            painter.circle_stroke(at, 7.0, egui::Stroke::new(2.0, theme.text_primary));
            painter.circle_filled(at, 2.5, theme.text_primary);
        }
    }

    // ── Chip con el valor en vivo ──
    let Some(anchor) = live.cursor() else {
        return;
    };
    let anchor_screen = to_screen(anchor);
    let prefix = if settings.residues { "ΣRes" } else { "∮" };
    let (text, color) = match live.value() {
        Ok(raw) => (
            format!(
                "{prefix} = {}",
                format_complex_rounded(displayed_value(raw, settings.residues))
            ),
            theme.text_primary,
        ),
        Err(error) => (format!("∮ sin valor: {error}"), theme.warning),
    };
    let galley = painter.layout_no_wrap(text, egui::FontId::proportional(TYPE_SM), color);
    let mut chip = egui::Rect::from_min_size(
        anchor_screen + egui::vec2(14.0, 14.0),
        galley.size() + egui::vec2(SPACE_SM, SPACE_XS),
    );
    // El chip nunca se sale del canvas (se voltea al lado opuesto).
    if chip.right() > canvas_rect.right() - SPACE_XS {
        chip = chip.translate(egui::vec2(-(chip.width() + 28.0), 0.0));
    }
    if chip.bottom() > canvas_rect.bottom() - SPACE_XS {
        chip = chip.translate(egui::vec2(0.0, -(chip.height() + 28.0)));
    }
    painter.rect_filled(chip, RADIUS_SM, theme.panel_bg);
    painter.rect_stroke(chip, RADIUS_SM, theme.hairline_stroke());
    painter.galley(
        chip.min + egui::vec2(SPACE_SM / 2.0, SPACE_XS / 2.0),
        galley,
        color,
    );

    // ── Etiqueta del imán debajo del chip ──
    if let Some(snap) = snap {
        if snap.kind != crate::snap::SnapKind::Free {
            painter.text(
                chip.left_bottom() + egui::vec2(0.0, 2.0),
                egui::Align2::LEFT_TOP,
                format!("imán: {}", snap.label),
                egui::FontId::proportional(TYPE_XS),
                theme.text_tertiary,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::{CircleObj, Document, GeoObject, PointObj};

    #[test]
    fn estabilizador_interpola_y_libera_con_cero() {
        let previous = Point2::new(0.0, 0.0);
        let raw = Point2::new(10.0, 0.0);
        // Sin estabilizador: el punto pasa tal cual.
        let free = stabilize_point(previous, raw, 0.0);
        assert!((free.x - 10.0).abs() < 1e-9);
        // Con estabilizador 0.5: media distancia.
        let half = stabilize_point(previous, raw, 0.5);
        assert!((half.x - 5.0).abs() < 1e-9);
        // El tope 0.8 deja el 20% (nunca congela el cursor). El cálculo es
        // f32 hasta el lerp: tolerancia acorde.
        let capped = stabilize_point(previous, raw, 2.0);
        assert!((capped.x - 2.0).abs() < 1e-6, "capped {}", capped.x);
    }

    #[test]
    fn cierre_por_radio_en_pixeles() {
        let scale = 50.0;
        // 10 px del inicio: cierra.
        assert!(should_close(
            Point2::new(0.0, 0.0),
            Point2::new(10.0 / scale, 0.0),
            scale
        ));
        // 20 px: no cierra.
        assert!(!should_close(
            Point2::new(0.0, 0.0),
            Point2::new(20.0 / scale, 0.0),
            scale
        ));
    }

    #[test]
    fn suavizado_conserva_extremos_y_sube_resolucion() {
        let points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 1.0),
        ];
        let smooth = smooth_points(&points);
        assert!(smooth.len() > points.len(), "más muestras");
        assert_eq!(smooth[0], points[0]);
        assert_eq!(
            *smooth.last().expect("último"),
            *points.last().expect("original")
        );
        // Trazos muy cortos no se tocan.
        assert_eq!(smooth_points(&points[..2]), points[..2].to_vec());
    }

    #[test]
    fn trazo_libre_acumula_y_cierra_el_lazo() {
        let mut document = Document::new();
        document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("punto");
        let settings = ComplexContourSettings {
            expr: "1/z".to_string(),
            smooth: false,
            stabilizer: 0.0,
            ..Default::default()
        };
        // Cuadrado alrededor del origen: ∮ 1/z dz = 2πi.
        let mut state = ComplexContourState {
            settings,
            ..Default::default()
        };
        state.live = Some(
            start_freehand(&document, Point2::new(-1.0, -1.0), &state.settings).expect("arranca"),
        );
        for point in [
            Point2::new(1.0, -1.0),
            Point2::new(1.0, 1.0),
            Point2::new(-1.0, 1.0),
            Point2::new(-1.0, -1.0),
        ] {
            state.capture_freehand(point, 50.0);
        }
        let finished = state.finish(50.0);
        match finished {
            FinishedContour::Freehand { points, value } => {
                assert_eq!(points.len(), 5, "lazo cerrado con el punto inicial");
                let value = value.expect("valor finito");
                assert!(value.contains("6.283"), "2πi: {value}");
            }
            _ => panic!("esperaba trazo libre terminado"),
        }
    }

    #[test]
    fn polos_dan_error_honesto_en_el_chip() {
        let mut document = Document::new();
        document
            .try_add_object(GeoObject::Point(PointObj::new(Point2::new(0.0, 0.0))))
            .expect("punto");
        let settings = ComplexContourSettings {
            expr: "1/z".to_string(),
            smooth: false,
            ..Default::default()
        };
        let mut state = ComplexContourState {
            settings,
            ..Default::default()
        };
        state.live = Some(
            start_freehand(&document, Point2::new(0.0, 0.0), &state.settings).expect("arranca"),
        );
        state.capture_freehand(Point2::new(1.0, 0.0), 50.0);
        let live = state.live.as_ref().expect("vivo");
        assert!(live.value().is_err(), "polo en el vértice: sin valor");
    }

    #[test]
    fn modo_circulo_radio_y_valor() {
        let document = Document::new();
        let settings = ComplexContourSettings {
            expr: "1/z".to_string(),
            residues: true,
            ..Default::default()
        };
        let mut state = ComplexContourState {
            settings,
            ..Default::default()
        };
        state.live =
            Some(start_circle(&document, Point2::new(0.0, 0.0), &state.settings).expect("arranca"));
        state.capture_circle(Point2::new(2.0, 0.0));
        let live = state.live.as_ref().expect("vivo");
        assert!((live.radius - 2.0).abs() < 1e-9);
        let value = live.value().expect("valor");
        assert!(
            (value.im - std::f64::consts::TAU).abs() < 1e-9,
            "2πi: {value}"
        );
        match state.finish(50.0) {
            FinishedContour::Circle { center, radius } => {
                assert_eq!(center, Point2::new(0.0, 0.0));
                assert!((radius - 2.0).abs() < 1e-9);
            }
            _ => panic!("esperaba círculo"),
        }
    }

    #[test]
    fn objetos_persistidos_referencian_el_trazo() {
        let settings = ComplexContourSettings {
            expr: "1/z".to_string(),
            residues: true,
            ..Default::default()
        };
        let finished = FinishedContour::Freehand {
            points: vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)],
            value: None,
        };
        let objects = build_contour_objects(finished, &settings, grafito_geometry::Color::BLUE);
        assert_eq!(objects.len(), 2, "trazo + integral");
        let (
            grafito_core::GeoObject::Pencil(pencil),
            grafito_core::GeoObject::ComplexIntegral(integral),
        ) = (&objects[0], &objects[1])
        else {
            panic!("orden esperado Pencil, ComplexIntegral");
        };
        assert_eq!(integral.target, pencil.id, "apunta al trazo insertado");
        assert!(integral.compute_residue, "modo Gauss");
        assert_eq!(integral.expr, "1/z");
        // Vacío no deja basura.
        assert!(build_contour_objects(
            FinishedContour::Empty,
            &settings,
            grafito_geometry::Color::BLUE
        )
        .is_empty());
    }

    #[test]
    fn insercion_del_par_valida_en_el_documento() {
        let mut document = Document::new();
        let settings = ComplexContourSettings::default();
        let objects = build_contour_objects(
            FinishedContour::Freehand {
                points: vec![Point2::new(0.0, 0.0), Point2::new(1.0, 1.0)],
                value: None,
            },
            &settings,
            grafito_geometry::Color::BLUE,
        );
        for object in objects {
            document.try_add_object(object).expect("validación del par");
        }
        assert!(document
            .objects_iter()
            .any(|(_, object)| matches!(object, GeoObject::ComplexIntegral(_))));
    }

    /// Textos visibles de una corrida headless (evidencia de render del chip).
    fn headless_texts(output: &egui::FullOutput) -> Vec<String> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    Some(text.galley.text().to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    #[test]
    fn overlay_headless_dibuja_el_chip_con_el_valor() {
        let document = Document::new();
        let settings = ComplexContourSettings {
            expr: "1/z".to_string(),
            stabilizer: 0.0,
            smooth: false,
            ..Default::default()
        };
        let mut state = ComplexContourState {
            settings,
            ..Default::default()
        };
        state.live = Some(
            start_freehand(&document, Point2::new(-1.0, -1.0), &state.settings).expect("arranca"),
        );
        for point in [
            Point2::new(1.0, -1.0),
            Point2::new(1.0, 1.0),
            Point2::new(-1.0, 1.0),
            Point2::new(-1.0, -1.0),
        ] {
            state.capture_freehand(point, 50.0);
        }
        let live = state.live.as_ref().expect("trazo vivo");
        let ctx = egui::Context::default();
        let view = grafito_geometry::ViewTransform::new(800.0, 600.0);
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let raw = egui::RawInput {
            screen_rect: Some(canvas),
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("contour_overlay_test"),
            ));
            draw_overlay(
                &painter,
                canvas,
                &view,
                live,
                &state.settings,
                None,
                &grafito_ui::theme::DARK,
            );
        });
        let texts = headless_texts(&output);
        assert!(
            texts.iter().any(|text| text.contains("∮ =")),
            "el chip del overlay muestra el valor: {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("6.283")),
            "lazo cerrado 2πi en el chip: {texts:?}"
        );
    }

    #[test]
    fn circulo_sobre_objeto_existente_no_rompe() {
        // Regresión de sanidad: el estado arranca círculo aunque haya objetos.
        let mut document = Document::new();
        document.add_object(GeoObject::Circle(CircleObj::new(
            Point2::new(1.0, 1.0),
            1.0,
        )));
        let settings = ComplexContourSettings::default();
        let live = start_circle(&document, Point2::new(0.0, 0.0), &settings).expect("arranca");
        assert!(live.center.is_some());
    }
}
