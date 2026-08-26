//! Pizarra nativa en overlay (estilo macOS): lienzo a pantalla completa con
//! toolbar flotante redondeada (pill) y herramientas de dibujo libres sobre
//! `grafito_whiteboard`. No toca `Document`/`GeoObject`.

use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::current_theme;
use grafito_ui::tokens::{
    RADIUS_MD, RADIUS_PILL, SHADOW_ALPHA, SHADOW_POPUP_BLUR, SHADOW_POPUP_OFFSET_Y, SPACE_MD,
    SPACE_SM, SPACE_XS, TYPE_XS, ZOOM_ICON_HIT, ZOOM_PCT_MIN_W, ZOOM_PILL_GAP, ZOOM_PILL_PAD_X,
    ZOOM_PILL_PAD_Y, ZOOM_PILL_RADIUS, ZOOM_WB_DEFAULT, ZOOM_WB_MAX, ZOOM_WB_MIN,
};
use grafito_whiteboard::{
    arrow_tip, smooth_stroke, WhiteboardDoc, WhiteboardElement, WhiteboardInteraction,
    WhiteboardTool,
};

/// Página individual de la pizarra (una “hoja” tipo Notepad).
#[derive(Clone)]
pub struct WhiteboardPage {
    pub title: String,
    pub doc: WhiteboardDoc,
    pub pan: (f64, f64),
    pub zoom: f64,
}

impl WhiteboardPage {
    fn new(id: usize) -> Self {
        Self {
            title: format!("Hoja {}", id),
            doc: WhiteboardDoc::new(),
            pan: (0.0, 0.0),
            zoom: ZOOM_WB_DEFAULT,
        }
    }
}

/// Libro de pizarras — colección de hojas con índice actual.
#[derive(Clone)]
pub struct WhiteboardBook {
    pub pages: Vec<WhiteboardPage>,
    pub current: usize,
    next_id: usize,
}

impl Default for WhiteboardBook {
    fn default() -> Self {
        Self {
            pages: vec![WhiteboardPage::new(1)],
            current: 0,
            next_id: 2,
        }
    }
}

impl WhiteboardBook {
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn current(&self) -> Option<&WhiteboardPage> {
        self.pages.get(self.current)
    }

    pub fn current_mut(&mut self) -> Option<&mut WhiteboardPage> {
        self.pages.get_mut(self.current)
    }

    pub fn create_page(&mut self) -> usize {
        let page = WhiteboardPage::new(self.next_id);
        self.next_id += 1;
        self.pages.push(page);
        self.pages.len() - 1
    }

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.pages.len() {
            self.current = index;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if self.pages.len() <= 1 || index >= self.pages.len() {
            return false;
        }
        self.pages.remove(index);
        if self.current >= self.pages.len() {
            self.current = self.pages.len() - 1;
        } else if index < self.current {
            self.current -= 1;
        }
        true
    }

    pub fn save_current_from_session(&mut self, session: &WhiteboardSession) {
        if let Some(page) = self.current_mut() {
            page.doc = session.doc.clone();
            page.pan = session.pan;
            page.zoom = session.zoom;
        }
    }

    pub fn load_to_session(&self, session: &mut WhiteboardSession) {
        if let Some(page) = self.current() {
            session.doc = page.doc.clone();
            session.pan = page.pan;
            session.zoom = page.zoom;
            session.active_text = None;
            session.interaction = WhiteboardInteraction::Idle;
            session.pencil_points.clear();
            session.marquee = None;
        }
    }
}

/// Sesión de pizarra activa (overlay) — con color y grosor del lápiz.
#[derive(Clone)]
pub struct WhiteboardSession {
    pub doc: WhiteboardDoc,
    pub tool: WhiteboardTool,
    interaction: WhiteboardInteraction,
    pencil_points: Vec<(f64, f64)>,
    pub pan: (f64, f64),
    pub zoom: f64,
    active_text: Option<usize>,
    marquee: Option<((f64, f64), (f64, f64))>,
    pub pen_color: Color32,
    pub pen_width: f32,
    pub show_palette: bool,
}

impl Default for WhiteboardSession {
    fn default() -> Self {
        Self {
            doc: WhiteboardDoc::new(),
            tool: WhiteboardTool::Pencil,
            interaction: WhiteboardInteraction::Idle,
            pencil_points: Vec::new(),
            pan: (0.0, 0.0),
            zoom: ZOOM_WB_DEFAULT,
            active_text: None,
            marquee: None,
            // Color por defecto con contraste en ambos temas; paleta permite cambiar.
            // Se ajusta automáticamente en draw() si el contraste con canvas_bg es bajo.
            pen_color: Color32::from_rgb(26, 26, 26),
            pen_width: 2.0,
            show_palette: false,
        }
    }
}

impl WhiteboardSession {
    pub fn world_from_screen(&self, screen: Pos2, rect: Rect) -> (f64, f64) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 || !self.zoom.is_finite() || self.zoom == 0.0
        {
            return (0.0, 0.0);
        }
        if !screen.x.is_finite() || !screen.y.is_finite() {
            return (0.0, 0.0);
        }
        let x = (screen.x - rect.center().x - self.pan.0 as f32) / self.zoom as f32;
        let y = (screen.y - rect.center().y - self.pan.1 as f32) / self.zoom as f32;
        if !x.is_finite() || !y.is_finite() {
            return (0.0, 0.0);
        }
        (x as f64, y as f64)
    }

    fn screen_from_world(&self, world: (f64, f64), rect: Rect) -> Pos2 {
        if !world.0.is_finite() || !world.1.is_finite() || !rect.center().x.is_finite() {
            return rect.center();
        }
        pos2(
            rect.center().x + self.pan.0 as f32 + world.0 as f32 * self.zoom as f32,
            rect.center().y + self.pan.1 as f32 + world.1 as f32 * self.zoom as f32,
        )
    }

    pub fn set_tool(&mut self, tool: WhiteboardTool) {
        self.tool = tool;
        self.interaction = WhiteboardInteraction::Idle;
        self.pencil_points.clear();
        self.marquee = None;
        self.active_text = None;
        // Palette solo para herramientas con color; la mantenemos si ya estaba abierta
        if !matches!(
            tool,
            WhiteboardTool::Pencil
                | WhiteboardTool::Rectangle
                | WhiteboardTool::Ellipse
                | WhiteboardTool::Arrow
                | WhiteboardTool::Text
        ) {
            self.show_palette = false;
        }
    }

    pub fn clear(&mut self) {
        self.doc.clear();
        self.active_text = None;
    }

    pub fn zoom_at(&mut self, factor: f64, screen: Pos2, rect: Rect) {
        if !factor.is_finite() || factor <= 0.0 || !screen.x.is_finite() {
            return;
        }
        let before = self.world_from_screen(screen, rect);
        self.zoom = (self.zoom * factor).clamp(ZOOM_WB_MIN, ZOOM_WB_MAX);
        if !self.zoom.is_finite() {
            self.zoom = ZOOM_WB_DEFAULT;
        }
        let after = self.world_from_screen(screen, rect);
        if !before.0.is_finite() || !after.0.is_finite() {
            return;
        }
        // Corregido drift: pan está en píxeles, delta mundo debe escalarse por zoom
        let dx = (before.0 - after.0) * self.zoom;
        let dy = (before.1 - after.1) * self.zoom;
        if dx.is_finite() && dy.is_finite() {
            self.pan = (self.pan.0 - dx, self.pan.1 - dy);
        }
    }

    pub fn handle_pointer(&mut self, world: (f64, f64), pressed: bool, released: bool) {
        if !world.0.is_finite() || !world.1.is_finite() {
            return;
        }
        if pressed {
            // Validar herramienta antes de iniciar interacción
            self.interaction = WhiteboardInteraction::begin(world, self.tool);
            self.marquee = None;
            if self.tool == WhiteboardTool::Pencil {
                self.pencil_points = vec![world];
                // Capar puntos para evitar DoS
                if self.pencil_points.len() > 4096 {
                    self.pencil_points.truncate(4096);
                }
            }
            if self.tool == WhiteboardTool::Select {
                self.marquee = Some((world, world));
            }
            return;
        }
        if matches!(self.interaction, WhiteboardInteraction::Idle) {
            return;
        }
        self.interaction.update(world);
        if self.tool == WhiteboardTool::Pencil
            && matches!(self.interaction, WhiteboardInteraction::Creating { .. })
            && self.pencil_points.len() < 4096
        {
            self.pencil_points.push(world);
        }
        if self.tool == WhiteboardTool::Select {
            if let Some((a, _)) = self.marquee {
                self.marquee = Some((a, world));
            }
        }
        if released {
            self.finalize();
        }
    }

    fn finalize(&mut self) {
        match self.tool {
            WhiteboardTool::Pencil => {
                let points = smooth_stroke(&self.pencil_points, 3);
                if points.len() >= 2 {
                    // Validar puntos finitos para evitar crashes por NaN
                    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
                        self.pencil_points.clear();
                        return;
                    }
                    let c = self.pen_color;
                    self.doc.add(WhiteboardElement::Stroke {
                        points,
                        color: (c.r(), c.g(), c.b()),
                        width: self.pen_width.clamp(1.0, 8.0) as f64,
                    });
                }
                self.pencil_points.clear();
            }
            WhiteboardTool::Select => {
                if let Some((a, b)) = self.marquee {
                    let center = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                    let _ = self.doc.select_at(center, SPACE_SM as f64 / self.zoom);
                }
                self.marquee = None;
            }
            WhiteboardTool::Eraser => {
                if let Some(path) = self.interaction.take_erase_path() {
                    self.erase_trace(path);
                }
            }
            other => {
                if let Some(element) = self.interaction.end() {
                    self.doc.add(element);
                    if other == WhiteboardTool::Text {
                        self.active_text = Some(self.doc.len().saturating_sub(1));
                    }
                }
            }
        }
        self.interaction = WhiteboardInteraction::Idle;
    }

    fn erase_trace(&mut self, path: Vec<(f64, f64)>) {
        // Tolerancia ligada a tokens: SPACE_SM/2 ≈ 4 + ajuste para grosor trazo
        let tolerance = (SPACE_SM as f64 - 2.0) / self.zoom;
        for point in path {
            let mut before = self.doc.len();
            while let Some(erased) = self.doc.erase_at(point, tolerance) {
                // Si borramos el texto activo, invalidar índice (evita crash al seguir escribiendo)
                if let Some(active) = self.active_text {
                    if erased == active {
                        self.active_text = None;
                    } else if erased < active {
                        self.active_text = Some(active.saturating_sub(1));
                    }
                    if self.active_text.is_some_and(|i| i >= self.doc.len()) {
                        self.active_text = None;
                    }
                }
                if self.doc.len() == before {
                    break;
                }
                before = self.doc.len();
            }
        }
        self.validate_active_text();
        if self.doc.is_empty() {
            self.active_text = None;
        }
    }

    pub fn type_char(&mut self, character: char) {
        let Some(index) = self.active_text else {
            return;
        };
        if index >= self.doc.len() {
            self.active_text = None;
            return;
        }
        if let Some(WhiteboardElement::Text { text, .. }) = self.doc.element_mut(index) {
            // Guard contra control chars que crashean layout (excepto \n)
            if character.is_control() && character != '\n' {
                return;
            }
            text.push(character);
        } else {
            self.active_text = None;
        }
    }

    pub fn backspace_text(&mut self) {
        let Some(index) = self.active_text else {
            return;
        };
        if index >= self.doc.len() {
            self.active_text = None;
            return;
        }
        if let Some(WhiteboardElement::Text { text, .. }) = self.doc.element_mut(index) {
            text.pop();
        } else {
            self.active_text = None;
        }
    }

    /// Invalida active_text si apunta fuera de rango (tras borrado/clear).
    #[allow(dead_code)]
    fn validate_active_text(&mut self) {
        if self.active_text.is_some_and(|i| i >= self.doc.len()) {
            self.active_text = None;
        }
    }

    fn effective_pen_color(&self, theme: &grafito_ui::theme::Theme) -> Color32 {
        // Si el usuario eligió un color claro sobre canvas claro (o oscuro sobre oscuro),
        // usa text_primary que siempre contrasta. Solo para el default histórico claro.
        let is_light_canvas = theme.canvas_bg.r() > 200;
        let pen_is_light =
            self.pen_color.r() > 200 && self.pen_color.g() > 200 && self.pen_color.b() > 200;
        let pen_is_dark =
            self.pen_color.r() < 60 && self.pen_color.g() < 60 && self.pen_color.b() < 60;
        if is_light_canvas && pen_is_light {
            return theme.text_primary;
        }
        if !is_light_canvas && pen_is_dark {
            return Color32::from_rgb(240, 241, 244);
        }
        // Compatibilidad: el default histórico era claro (240,241,244) que es invisible en modo claro
        if is_light_canvas && self.pen_color == Color32::from_rgb(240, 241, 244) {
            return Color32::from_rgb(26, 26, 26);
        }
        self.pen_color
    }

    pub fn draw(&self, ui: &mut egui::Ui, rect: Rect) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 || !rect.min.x.is_finite() {
            return;
        }
        let theme = current_theme(ui.ctx());
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, theme.canvas_bg);
        // Grid con contraste adaptativo: más opaco en modo claro sobre #FAFAF9
        let is_light = theme.canvas_bg.r() > 200;
        let grid_col = if is_light {
            theme.separator.gamma_multiply(0.35)
        } else {
            theme.separator.gamma_multiply(0.10)
        };
        if grid_col.a() > 0 {
            draw_grid(painter, rect, self.pan, self.zoom, grid_col);
        }
        for element in self.doc.elements() {
            // Proteger contra elementos con NaN
            if let Some((min, max)) = element.bounds() {
                if !min.0.is_finite() || !max.0.is_finite() {
                    continue;
                }
            }
            draw_element(painter, element, rect, self, theme);
        }
        // Trazo en vivo del lápiz: vivo, con color y grosor actuales (con contraste adaptativo)
        if self.tool == WhiteboardTool::Pencil && !self.pencil_points.is_empty() {
            let stroke = Stroke::new(
                self.pen_width.clamp(1.0, 8.0) * self.zoom as f32,
                self.effective_pen_color(theme),
            );
            for pair in self.pencil_points.windows(2) {
                if !pair[0].0.is_finite() || !pair[1].0.is_finite() {
                    continue;
                }
                let a = self.screen_from_world(pair[0], rect);
                let b = self.screen_from_world(pair[1], rect);
                if a.x.is_finite() && b.x.is_finite() {
                    painter.line_segment([a, b], stroke);
                }
            }
            if self.pencil_points.len() == 1 {
                let p = self.screen_from_world(self.pencil_points[0], rect);
                if p.x.is_finite() {
                    painter.circle_filled(
                        p,
                        (self.pen_width * 0.75).clamp(1.0, 6.0) * self.zoom as f32,
                        self.effective_pen_color(theme),
                    );
                }
            }
        }
        if let Some(preview) = self.interaction.preview() {
            draw_element(painter, &preview, rect, self, theme);
        }
        if let Some((a, b)) = self.marquee {
            if a.0.is_finite() && b.0.is_finite() {
                let (pa, pb) = (
                    self.screen_from_world(a, rect),
                    self.screen_from_world(b, rect),
                );
                if pa.x.is_finite() && pb.x.is_finite() {
                    painter.rect_stroke(
                        Rect::from_two_pos(pa, pb),
                        2.0,
                        Stroke::new(1.5, theme.accent.gamma_multiply(0.6)),
                    );
                }
            }
        }
    }

    pub fn handle_canvas_input(&mut self, rect: Rect, ui: &egui::Ui) {
        // Repintar a 60Hz si hay cualquier pointer/touch activo (tableta, mouse, touch)
        if ui
            .ctx()
            .input(|i| i.pointer.any_down() || i.multi_touch().is_some())
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        let response = ui.interact(
            rect,
            egui::Id::new("whiteboard_canvas"),
            Sense::click_and_drag(),
        );
        let zoom_delta = ui.input(|i| i.zoom_delta());
        let pointer = ui.input(|i| i.pointer.clone());
        let current_pos = response
            .interact_pointer_pos()
            .or(response.hover_pos())
            .or(pointer.latest_pos());
        if zoom_delta != 1.0 {
            if let Some(pos) = current_pos {
                self.zoom_at(zoom_delta as f64, pos, rect);
            }
        }
        // Rueda del mouse: zoom centrado en el cursor
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if scroll.abs() > f32::EPSILON {
            if let Some(pos) = current_pos {
                if rect.contains(pos) {
                    let factor = (1.0 + scroll * 0.008).clamp(0.85, 1.15) as f64;
                    self.zoom_at(factor, pos, rect);
                    ui.ctx().request_repaint();
                }
            }
        }
        // Atajos de teclado: Ctrl/Cmd + rueda ya cubiertos; +/- y 0 para reset
        if ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
            if ui.input(|i| i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus)) {
                if let Some(pos) = current_pos.or(Some(rect.center())) {
                    self.zoom_at(1.2, pos, rect);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Minus)) {
                if let Some(pos) = current_pos.or(Some(rect.center())) {
                    self.zoom_at(0.8333, pos, rect);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Num0)) {
                self.zoom = ZOOM_WB_DEFAULT;
                self.pan = (0.0, 0.0);
            }
        } else {
            // Sin Ctrl: +/- también funcionan si el canvas tiene foco
            if ui.input(|i| i.key_pressed(egui::Key::Equals) || i.key_pressed(egui::Key::Plus)) {
                if let Some(pos) = current_pos.or(Some(rect.center())) {
                    self.zoom_at(1.15, pos, rect);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Minus)) {
                if let Some(pos) = current_pos.or(Some(rect.center())) {
                    self.zoom_at(0.87, pos, rect);
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            return; // pan medio reservado para el canvas principal
        }
        // Compatibilidad tableta: cualquier botón de dibujo + fallback Touch
        let any_draw_button = pointer.button_down(egui::PointerButton::Primary)
            || pointer.button_down(egui::PointerButton::Secondary)
            || pointer.button_down(egui::PointerButton::Middle)
            || pointer.any_down();
        if let Some(world) = current_pos.map(|pos| self.world_from_screen(pos, rect)) {
            let is_idle = matches!(self.interaction, WhiteboardInteraction::Idle);
            let stylus_pressed =
                any_draw_button && is_idle && current_pos.is_some_and(|p| rect.contains(p));
            let drag_any = response.dragged();
            if stylus_pressed || response.drag_started() || response.clicked() {
                self.handle_pointer(world, true, false);
            } else if response.drag_stopped() || (!any_draw_button && !is_idle) {
                self.handle_pointer(world, false, true);
            } else if drag_any || any_draw_button {
                self.handle_pointer(world, false, false);
            }
        }
    }
}

fn draw_grid(painter: &egui::Painter, rect: Rect, pan: (f64, f64), zoom: f64, color: Color32) {
    let step = 40.0_f32 * zoom as f32;
    if step < 6.0 {
        return;
    }
    let mut x = rect.min.x + (((pan.0 * zoom) as f32) % step + step) % step;
    while x < rect.max.x {
        painter.line_segment(
            [pos2(x, rect.min.y), pos2(x, rect.max.y)],
            Stroke::new(1.0, color),
        );
        x += step;
    }
    let mut y = rect.min.y + (((pan.1 * zoom) as f32) % step + step) % step;
    while y < rect.max.y {
        painter.line_segment(
            [pos2(rect.min.x, y), pos2(rect.max.x, y)],
            Stroke::new(1.0, color),
        );
        y += step;
    }
}

fn draw_element(
    painter: &egui::Painter,
    element: &WhiteboardElement,
    rect: Rect,
    session: &WhiteboardSession,
    theme: &grafito_ui::theme::Theme,
) {
    // Stroke para formas: usar text_primary para visibilidad en ambos temas (claro sobre oscuro, oscuro sobre claro)
    let shape_stroke = Stroke::new(
        1.8 * session.zoom as f32,
        theme.text_primary.gamma_multiply(0.92),
    );
    let text_color = theme.text_primary;
    match element {
        WhiteboardElement::Stroke {
            points,
            width,
            color,
        } => {
            let raw = Color32::from_rgb(color.0, color.1, color.2);
            // Adaptación para contraste: si el trazo almacenado es claro sobre canvas claro (o oscuro sobre oscuro), usa text_primary
            let is_light_canvas = theme.canvas_bg.r() > 200;
            let col_is_light = raw.r() > 200 && raw.g() > 200 && raw.b() > 200;
            let col_is_dark = raw.r() < 60 && raw.g() < 60 && raw.b() < 60;
            let col = if is_light_canvas && col_is_light {
                theme.text_primary
            } else if !is_light_canvas && col_is_dark {
                Color32::from_rgb(240, 241, 244)
            } else {
                raw
            };
            for pair in points.windows(2) {
                if !pair[0].0.is_finite() || !pair[1].0.is_finite() {
                    continue;
                }
                let a = session.screen_from_world(pair[0], rect);
                let b = session.screen_from_world(pair[1], rect);
                if !a.x.is_finite() || !b.x.is_finite() {
                    continue;
                }
                painter.line_segment(
                    [a, b],
                    Stroke::new((*width as f32) * session.zoom as f32, col),
                );
            }
        }
        WhiteboardElement::Rectangle { min, max, .. } => {
            if !min.0.is_finite() || !max.0.is_finite() {
                return;
            }
            let a = session.screen_from_world(*min, rect);
            let b = session.screen_from_world(*max, rect);
            if a.x.is_finite() && b.x.is_finite() {
                painter.rect_stroke(Rect::from_two_pos(a, b), RADIUS_MD, shape_stroke);
            }
        }
        WhiteboardElement::Ellipse { center, rx, .. } => {
            if !center.0.is_finite() || !rx.is_finite() {
                return;
            }
            let radius = (*rx as f32 * session.zoom as f32).max(2.0);
            let c = session.screen_from_world(*center, rect);
            if c.x.is_finite() {
                painter.circle_stroke(c, radius, shape_stroke);
            }
        }
        WhiteboardElement::Arrow { from, to } => {
            if !from.0.is_finite() || !to.0.is_finite() {
                return;
            }
            let a = session.screen_from_world(*from, rect);
            let b = session.screen_from_world(*to, rect);
            if a.x.is_finite() && b.x.is_finite() {
                painter.line_segment([a, b], shape_stroke);
                let wing = arrow_tip(*from, *to, 0.55);
                let wing_a = session.screen_from_world(wing.0, rect);
                let wing_b = session.screen_from_world(wing.1, rect);
                if wing_a.x.is_finite() {
                    painter.line_segment([wing_a, b], shape_stroke);
                    painter.line_segment([wing_b, b], shape_stroke);
                }
            }
        }
        WhiteboardElement::Text { at, text, size } => {
            if !text.is_empty() && at.0.is_finite() && size.is_finite() {
                let font = egui::FontId::monospace((*size as f32) * session.zoom as f32);
                let pos = session.screen_from_world(*at, rect);
                if pos.x.is_finite() {
                    painter.text(pos, egui::Align2::LEFT_TOP, text, font, text_color);
                }
            }
        }
    }
}

/// Contenido interno de la toolbar mega simple (mover, escribir, linea, cerrar).
/// Sin desbordes: solo 4 botones, sin separadores extra ni scroll.
#[allow(clippy::too_many_arguments)]
fn draw_toolbar_contents(
    ui: &mut egui::Ui,
    app: &mut crate::GrafitoApp,
    selected_tool: &mut Option<WhiteboardTool>,
    _clear: &mut bool,
    close: &mut bool,
    _ask_ai: &mut bool,
    _toggle_assistant: &mut bool,
    _toggle_palette: &mut bool,
) {
    let theme = current_theme(ui.ctx());
    let tool = app.whiteboard.tool;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_XS;
        // 4 herramientas + cerrar — burbuja compacta centrada
        for (icon, t, tip) in [
            (Icon::Move, WhiteboardTool::Select, "Mover"),
            (Icon::Pencil, WhiteboardTool::Pencil, "Escribir"),
            (Icon::ArrowRight, WhiteboardTool::Arrow, "Línea"),
            (Icon::Eraser, WhiteboardTool::Eraser, "Borrar"),
        ] {
            let selected = tool == t;
            if action_icon_button(
                ui,
                icon,
                if selected {
                    theme.accent
                } else {
                    theme.text_secondary
                },
                tip,
            )
            .clicked()
            {
                *selected_tool = Some(t);
            }
        }
        ui.separator();
        if action_icon_button(ui, Icon::Close, theme.text_secondary, "Cerrar (Esc)").clicked() {
            *close = true;
        }
    });
}

#[allow(dead_code)]
fn draw_toolbar(ui: &mut egui::Ui, app: &mut crate::GrafitoApp) {
    let theme = current_theme(ui.ctx());
    let mut selected_tool: Option<WhiteboardTool> = None;
    let mut clear = false;
    let mut close = false;
    let mut ask_ai = false;
    let mut toggle_assistant = false;
    let mut toggle_palette = false;

    // Barra pill flotante centrada — no ocupa todo el ancho (mac style)
    ui.vertical_centered(|ui| {
        egui::Frame::none()
            .fill(theme.panel_bg.gamma_multiply(0.99))
            .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.12)))
            .rounding(egui::Rounding::same(RADIUS_PILL))
            .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
            .shadow(egui::epaint::Shadow {
                offset: vec2(0.0, 4.0),
                blur: 20.0,
                spread: 0.0,
                color: Color32::from_black_alpha(28),
            })
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .auto_shrink([true, false])
                    .show(ui, |ui| {
                        draw_toolbar_contents(
                            ui,
                            app,
                            &mut selected_tool,
                            &mut clear,
                            &mut close,
                            &mut ask_ai,
                            &mut toggle_assistant,
                            &mut toggle_palette,
                        );
                    });
            });
    });
    if let Some(t) = selected_tool {
        app.whiteboard.set_tool(t);
    }
    if toggle_palette {
        app.whiteboard.show_palette = !app.whiteboard.show_palette;
    }
    if clear {
        app.whiteboard.clear();
    }
    if close {
        app.whiteboard.show_palette = false;
        app.whiteboard_open = false;
    }
    if toggle_assistant {
        app.show_whiteboard_assistant = !app.show_whiteboard_assistant;
    }
    if ask_ai {
        let description = app.whiteboard.doc.describe();
        app.assistant.problem =
            format!("Entendé este dibujo de la pizarra de Grafito y explicámelo: {description}");
        app.start_local_assistant_request(ui.ctx());
        app.notify(
            "Pidiendo análisis de la pizarra…",
            grafito_ui::toast::ToastKind::Info,
        );
    }
}

pub fn draw_whiteboard_overlay(app: &mut crate::GrafitoApp, ctx: &egui::Context) {
    let theme = current_theme(ctx);
    app.sync_assistant_for_frame(ctx);
    {
        let cur = app.whiteboard.clone();
        app.whiteboard_book.save_current_from_session(&cur);
    }

    // ── Toolbar burbuja centrada — Area flotante pill, compacta ──
    // Centrada como burbuja aparte, no barra larga. Solo 4 herramientas + borrar.
    {
        let mut selected_tool: Option<WhiteboardTool> = None;
        let mut close = false;
        let avail = ctx.available_rect();
        let screen = ctx.screen_rect();
        let center_offset_x = avail.center().x - screen.center().x;
        egui::Area::new(egui::Id::new("whiteboard_toolbar"))
            .anchor(egui::Align2::CENTER_TOP, vec2(center_offset_x, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(theme.panel_bg.gamma_multiply(0.96))
                    .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.08)))
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(SPACE_XS, 4.0))
                    .shadow(egui::Shadow {
                        offset: vec2(0.0, 1.0),
                        blur: 4.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(4),
                    })
                    .show(ui, |ui| {
                        let mut dummy_clear = false;
                        let mut dummy_ask = false;
                        let mut dummy_toggle_a = false;
                        let mut dummy_toggle_p = false;
                        draw_toolbar_contents(
                            ui,
                            app,
                            &mut selected_tool,
                            &mut dummy_clear,
                            &mut close,
                            &mut dummy_ask,
                            &mut dummy_toggle_a,
                            &mut dummy_toggle_p,
                        );
                    });
            });
        if let Some(t) = selected_tool {
            app.whiteboard.set_tool(t);
        }
        if close {
            app.whiteboard_open = false;
        }
    }
    // ── Palette flotante (Area) — mega simple: solo si herramienta es Escribir (Pencil) ──
    let is_pencil = app.whiteboard.tool == WhiteboardTool::Pencil;
    // Palette compacta y centrada — solo se achica al contenido, no ocupa 560 por defecto
    let palette_anim = ctx.animate_bool(egui::Id::new("whiteboard_palette"), is_pencil);
    if palette_anim > 0.01 {
        let avail = ctx.available_rect();
        let screen = ctx.screen_rect();
        let center_offset_x = avail.center().x - screen.center().x;
        egui::Area::new(egui::Id::new("whiteboard_palette_area"))
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(center_offset_x, -12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(theme.panel_bg.gamma_multiply(0.96 * palette_anim + 0.04))
                    .stroke(egui::Stroke::new(
                        1.0,
                        theme.separator.gamma_multiply(0.10 * palette_anim),
                    ))
                    .rounding(RADIUS_MD)
                    .inner_margin(egui::Margin::symmetric(SPACE_SM, SPACE_SM))
                    .shadow(egui::Shadow {
                        offset: vec2(0.0, 2.0),
                        blur: 8.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha((8.0 * palette_anim) as u8),
                    })
                    .show(ui, |ui| {
                        ui.scope(|ui| {
                            ui.style_mut().visuals.override_text_color =
                                Some(theme.text_primary.gamma_multiply(palette_anim));
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = SPACE_SM;
                                egui::ScrollArea::horizontal()
                                    .auto_shrink([true, false])
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            const PALETTE: &[[u8; 3]] = &[
                                                [250, 250, 249],
                                                [212, 212, 216],
                                                [107, 122, 111],
                                                [100, 116, 139],
                                                [168, 123, 110],
                                                [91, 125, 177],
                                                [196, 91, 91],
                                                [209, 183, 91],
                                            ];
                                            let mut picked: Option<Color32> = None;
                                            for rgb in PALETTE.iter() {
                                                let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                                let is_sel = app.whiteboard.pen_color == col;
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    vec2(24.0, 24.0),
                                                    Sense::click(),
                                                );
                                                if ui.is_rect_visible(rect) {
                                                    ui.painter().circle_filled(
                                                        rect.center(),
                                                        11.0,
                                                        col,
                                                    );
                                                    ui.painter().circle_stroke(
                                                        rect.center(),
                                                        11.0,
                                                        Stroke::new(
                                                            if is_sel { 2.0 } else { 1.0 },
                                                            if is_sel {
                                                                theme
                                                                    .accent
                                                                    .gamma_multiply(palette_anim)
                                                            } else {
                                                                theme.separator.gamma_multiply(
                                                                    0.20 * palette_anim,
                                                                )
                                                            },
                                                        ),
                                                    );
                                                    if is_sel {
                                                        ui.painter().circle_stroke(
                                                            rect.center(),
                                                            (app.whiteboard.pen_width * 1.1)
                                                                .clamp(2.0, 7.0),
                                                            Stroke::new(
                                                                1.0,
                                                                Color32::from_black_alpha(25),
                                                            ),
                                                        );
                                                    }
                                                }
                                                if resp
                                                    .on_hover_text(format!(
                                                        "#{:02X}{:02X}{:02X}",
                                                        rgb[0], rgb[1], rgb[2]
                                                    ))
                                                    .clicked()
                                                {
                                                    picked = Some(col);
                                                }
                                            }
                                            if let Some(col) = picked {
                                                app.whiteboard.pen_color = col;
                                            }
                                        });
                                    });
                                ui.separator();
                                ui.label(
                                    egui::RichText::new("Grosor")
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_secondary.gamma_multiply(palette_anim))
                                        .strong(),
                                );
                                let mut w = app.whiteboard.pen_width;
                                let resp = ui.add(
                                    egui::Slider::new(&mut w, 1.0..=8.0)
                                        .show_value(false)
                                        .trailing_fill(true),
                                );
                                if resp.changed() {
                                    app.whiteboard.pen_width = w.clamp(1.0, 8.0);
                                }
                                let (pr, _) =
                                    ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                                ui.painter().circle_stroke(
                                    pr.center(),
                                    (app.whiteboard.pen_width * 1.1).clamp(2.0, 7.0),
                                    Stroke::new(1.2, app.whiteboard.pen_color),
                                );
                                ui.painter().circle_filled(
                                    pr.center(),
                                    1.5,
                                    theme.separator.gamma_multiply(0.25),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{:.1}", app.whiteboard.pen_width))
                                        .size(grafito_ui::tokens::TYPE_XS)
                                        .color(theme.text_primary.gamma_multiply(palette_anim)),
                                );
                            });
                        });
                    });
            });
    }
    // ── Canvas + controles de zoom (Scandinavian: pill sutil abajo-derecha) ──
    let mut zoom_in = false;
    let mut zoom_out = false;
    let mut zoom_reset = false;
    let current_zoom = app.whiteboard.zoom;
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme.canvas_bg))
        .show(ctx, |ui| {
            let available = ui.available_rect_before_wrap();
            if available.width() < 10.0 || available.height() < 10.0 {
                return;
            }
            let canvas_rect = available.shrink(8.0);
            ctx.memory_mut(|mem| {
                mem.data
                    .insert_temp(egui::Id::new("whiteboard_canvas_rect"), canvas_rect)
            });
            ui.painter().rect_stroke(
                canvas_rect,
                RADIUS_MD,
                Stroke::new(1.0, theme.separator.gamma_multiply(0.08)),
            );
            app.whiteboard.handle_canvas_input(canvas_rect, ui);
            app.whiteboard.draw(ui, canvas_rect);

            // Pill de zoom abajo-derecha — Scandinavian: infinito Geogebra 1e-6..1e6
            let zoom_percent_f = current_zoom * 100.0;
            let at_min = current_zoom <= ZOOM_WB_MIN * 1.001;
            let at_max = current_zoom >= ZOOM_WB_MAX * 0.999;
            let at_limits_exact = at_min || at_max;
            // Infinito: botones siempre habilitados salvo en los extremos absolutos
            let minus_enabled = !at_min;
            let plus_enabled = !at_max;
            egui::Area::new(egui::Id::new("whiteboard_zoom_controls"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -56.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(theme.panel_bg)
                        .stroke(egui::Stroke::new(1.0, theme.separator))
                        .rounding(ZOOM_PILL_RADIUS)
                        .inner_margin(egui::Margin::symmetric(ZOOM_PILL_PAD_X, ZOOM_PILL_PAD_Y))
                        .shadow(egui::Shadow {
                            offset: egui::vec2(0.0, SHADOW_POPUP_OFFSET_Y),
                            blur: SHADOW_POPUP_BLUR,
                            spread: 0.0,
                            color: Color32::from_black_alpha(SHADOW_ALPHA),
                        })
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = ZOOM_PILL_GAP;
                                ui.add_enabled_ui(minus_enabled, |ui| {
                                    let col = if minus_enabled {
                                        theme.text_primary
                                    } else {
                                        theme.text_tertiary
                                    };
                                    if action_icon_button(ui, Icon::Minus, col, "Alejar (-)")
                                        .clicked()
                                    {
                                        zoom_out = true;
                                    }
                                });
                                let pct_text = if zoom_percent_f >= 1e7 {
                                    format!("{:.0e}%", zoom_percent_f)
                                } else if zoom_percent_f >= 1000.0 {
                                    format!("{:.0}%", zoom_percent_f)
                                } else if zoom_percent_f < 0.1 {
                                    format!("{:.3}%", zoom_percent_f)
                                } else if zoom_percent_f < 10.0 {
                                    format!("{:.1}%", zoom_percent_f)
                                } else {
                                    format!("{:.0}%", zoom_percent_f)
                                };
                                let pct_color = if at_limits_exact {
                                    theme.warning
                                } else {
                                    theme.text_secondary
                                };
                                let pct_btn = egui::Button::new(
                                    egui::RichText::new(pct_text)
                                        .size(TYPE_XS)
                                        .color(pct_color)
                                        .monospace(),
                                )
                                .min_size(egui::vec2(ZOOM_PCT_MIN_W, ZOOM_ICON_HIT))
                                .frame(false);
                                if ui
                                    .add(pct_btn)
                                    .on_hover_text("Clic para 100% — Ctrl+0 / doble clic canvas")
                                    .clicked()
                                {
                                    zoom_reset = true;
                                }
                                ui.add_enabled_ui(plus_enabled, |ui| {
                                    let col = if plus_enabled {
                                        theme.text_primary
                                    } else {
                                        theme.text_tertiary
                                    };
                                    if action_icon_button(ui, Icon::Plus, col, "Acercar (+)")
                                        .clicked()
                                    {
                                        zoom_in = true;
                                    }
                                });
                                ui.add_space(SPACE_SM);
                                ui.separator();
                                ui.add_space(SPACE_SM);
                                if action_icon_button(
                                    ui,
                                    Icon::Grid,
                                    theme.text_secondary,
                                    "Ajustar vista (Ctrl+0)",
                                )
                                .clicked()
                                {
                                    zoom_reset = true;
                                }
                            });
                        });
                });
        });
    // Aplicar zoom anclado al centro del canvas real (Memory), no screen dummy
    let canvas_center_and_rect = ctx.memory(|mem| {
        mem.data
            .get_temp::<egui::Rect>(egui::Id::new("whiteboard_canvas_rect"))
            .map(|r| (r.center(), r))
    });
    let (zoom_center, zoom_rect) = canvas_center_and_rect.unwrap_or_else(|| {
        let c = ctx.screen_rect().center();
        (c, egui::Rect::from_center_size(c, egui::vec2(400.0, 300.0)))
    });
    if zoom_in {
        app.whiteboard.zoom_at(1.2, zoom_center, zoom_rect);
    }
    if zoom_out {
        app.whiteboard.zoom_at(0.8333, zoom_center, zoom_rect);
    }
    if zoom_reset {
        app.whiteboard.zoom = ZOOM_WB_DEFAULT;
        app.whiteboard.pan = (0.0, 0.0);
    }
    // Doble clic en canvas resetea zoom (gesto rápido) — solo si fue sobre el lienzo
    if ctx.input(|i| {
        i.pointer
            .button_double_clicked(egui::PointerButton::Primary)
    }) {
        if let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) {
            if let Some(canvas) = ctx.memory(|mem| {
                mem.data
                    .get_temp::<egui::Rect>(egui::Id::new("whiteboard_canvas_rect"))
            }) {
                if canvas.contains(pos) {
                    app.whiteboard.zoom = ZOOM_WB_DEFAULT;
                    app.whiteboard.pan = (0.0, 0.0);
                }
            }
        }
    }

    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        if app.whiteboard.show_palette {
            app.whiteboard.show_palette = false;
        } else if app.whiteboard_open {
            app.whiteboard_open = false;
        }
    }
    let mut typed = Vec::new();
    let mut backspace = false;
    ctx.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Text(text) => typed.extend(text.chars()),
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    ..
                } => backspace = true,
                _ => {}
            }
        }
    });
    if app.whiteboard.tool == WhiteboardTool::Text {
        for character in typed {
            app.whiteboard.type_char(character);
        }
        if backspace {
            app.whiteboard.backspace_text();
        }
    }
    let busy = ctx.input(|input| input.pointer.any_down());
    ctx.request_repaint_after(std::time::Duration::from_millis(if busy {
        16
    } else {
        100
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whiteboard_creates_a_normalized_rectangle_from_a_drag() {
        let mut session = WhiteboardSession::default();
        session.set_tool(WhiteboardTool::Rectangle);
        session.handle_pointer((6.0, 8.0), true, false);
        session.handle_pointer((2.0, 3.0), false, false);
        session.handle_pointer((2.0, 3.0), false, true);
        assert_eq!(session.doc.len(), 1);
        let (min, max) = session.doc.elements()[0].bounds().unwrap();
        assert_eq!(min, (2.0, 3.0));
        assert_eq!(max, (6.0, 8.0));
    }

    #[test]
    fn whiteboard_pencil_creates_a_smoothed_stroke() {
        let mut session = WhiteboardSession::default();
        session.set_tool(WhiteboardTool::Pencil);
        session.handle_pointer((0.0, 0.0), true, false);
        session.handle_pointer((0.5, 0.5), false, false);
        session.handle_pointer((1.0, 1.0), false, true);
        assert_eq!(session.doc.len(), 1);
        match &session.doc.elements()[0] {
            WhiteboardElement::Stroke { points, .. } => assert!(points.len() >= 2),
            other => panic!("expected stroke, got {other:?}"),
        }
    }

    #[test]
    fn whiteboard_erases_along_the_trace() {
        let mut session = WhiteboardSession::default();
        session.set_tool(WhiteboardTool::Rectangle);
        session.handle_pointer((0.0, 0.0), true, false);
        session.handle_pointer((5.0, 5.0), false, true);
        assert_eq!(session.doc.len(), 1);
        session.set_tool(WhiteboardTool::Eraser);
        session.handle_pointer((2.0, 2.0), true, false);
        session.handle_pointer((2.2, 2.2), false, true);
        assert!(session.doc.is_empty());
    }

    #[test]
    fn world_screen_roundtrip_is_stable_without_pan() {
        let session = WhiteboardSession::default();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let world = session.world_from_screen(pos2(200.0, 150.0), rect);
        assert!(world.0.abs() < 1e-6 && world.1.abs() < 1e-6);
    }
}
