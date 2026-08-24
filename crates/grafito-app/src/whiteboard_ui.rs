//! Pizarra nativa en overlay (estilo macOS): lienzo a pantalla completa con
//! toolbar flotante redondeada y herramientas de dibujo libres sobre
//! `grafito_whiteboard`. No toca `Document`/`GeoObject`.

use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::current_theme;
use grafito_ui::tokens::{
    RADIUS_MD, SHADOW_WINDOW_BLUR, SHADOW_WINDOW_OFFSET_Y, SPACE_MD, SPACE_SM, SPACE_XS,
};
use grafito_whiteboard::{
    arrow_tip, smooth_stroke, WhiteboardDoc, WhiteboardElement, WhiteboardInteraction,
    WhiteboardTool,
};

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
            zoom: 1.0,
            active_text: None,
            marquee: None,
            // Color claro para canvas oscuro (Scandinavian: ink sobre near-black)
            pen_color: Color32::from_rgb(240, 241, 244),
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
        self.zoom = (self.zoom * factor).clamp(0.2, 6.0);
        if !self.zoom.is_finite() {
            self.zoom = 1.0;
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
            while self.doc.erase_at(point, tolerance).is_some() {
                if self.doc.len() == before {
                    break;
                }
                before = self.doc.len();
            }
        }
    }

    pub fn type_char(&mut self, character: char) {
        if let Some(index) = self.active_text {
            if let Some(WhiteboardElement::Text { text, .. }) = self.doc.element_mut(index) {
                text.push(character);
            }
        }
    }

    pub fn backspace_text(&mut self) {
        if let Some(index) = self.active_text {
            if let Some(WhiteboardElement::Text { text, .. }) = self.doc.element_mut(index) {
                text.pop();
            }
        }
    }

    pub fn draw(&self, ui: &mut egui::Ui, rect: Rect) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 || !rect.min.x.is_finite() {
            return;
        }
        let theme = current_theme(ui.ctx());
        let painter = ui.painter();
        // Fondo con ligera diferencia para profesionalismo (canvas_bg ya es near-black)
        painter.rect_filled(rect, 0.0, theme.canvas_bg);
        let grid_col = theme.separator.gamma_multiply(0.10);
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
        // Trazo en vivo del lápiz: vivo, con color y grosor actuales
        if self.tool == WhiteboardTool::Pencil && !self.pencil_points.is_empty() {
            let stroke = Stroke::new(
                self.pen_width.clamp(1.0, 8.0) * self.zoom as f32,
                self.pen_color,
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
                        self.pen_color,
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
        if ui.ctx().input(|input| input.pointer.any_down()) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        let response = ui.interact(
            rect,
            egui::Id::new("whiteboard_canvas"),
            Sense::click_and_drag(),
        );
        let zoom_delta = ui.input(|input| input.zoom_delta());
        let pointer = response.interact_pointer_pos();
        if zoom_delta != 1.0 {
            if let Some(pos) = pointer {
                self.zoom_at(zoom_delta as f64, pos, rect);
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            return; // el pan medio se deja al manejador egui del canvas principal
        }
        if let Some(world) = pointer.map(|pos| self.world_from_screen(pos, rect)) {
            let primary = response.dragged_by(egui::PointerButton::Primary);
            if response.drag_started_by(egui::PointerButton::Primary) || response.clicked() {
                self.handle_pointer(world, true, false);
            } else if response.drag_stopped() {
                self.handle_pointer(world, false, true);
            } else if primary {
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
            let col = Color32::from_rgb(color.0, color.1, color.2);
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

fn draw_toolbar(ui: &mut egui::Ui, app: &mut crate::GrafitoApp) {
    let theme = current_theme(ui.ctx());
    let mut selected_tool: Option<WhiteboardTool> = None;
    let mut clear = false;
    let mut close = false;
    let mut ask_ai = false;
    let mut toggle_assistant = false;
    let mut toggle_palette = false;
    // Capturar estado necesario sin prestar &app.whiteboard largo
    let tool = app.whiteboard.tool;
    let pen_color = app.whiteboard.pen_color;
    let pen_width = app.whiteboard.pen_width;
    let palette_open = app.whiteboard.show_palette;
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
        .rounding(egui::Rounding::same(grafito_ui::tokens::RADIUS_LG))
        .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
        .shadow(egui::epaint::Shadow {
            offset: vec2(0.0, SHADOW_WINDOW_OFFSET_Y),
            blur: SHADOW_WINDOW_BLUR,
            spread: 0.0,
            color: Color32::from_black_alpha(grafito_ui::tokens::SHADOW_ALPHA),
        })
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = SPACE_XS;
                        ui.label(
                            egui::RichText::new("Pizarra")
                                .color(theme.accent)
                                .size(grafito_ui::tokens::TYPE_SM)
                                .strong(),
                        );
                        ui.separator();
                        for (icon, t, tip) in [
                            (Icon::Move, WhiteboardTool::Select, "Seleccionar"),
                            (Icon::Pencil, WhiteboardTool::Pencil, "Lápiz libre"),
                            (Icon::Shapes, WhiteboardTool::Rectangle, "Rectángulo"),
                            (Icon::Ellipse, WhiteboardTool::Ellipse, "Elipse"),
                            (Icon::ArrowRight, WhiteboardTool::Arrow, "Flecha"),
                            (Icon::Notebook, WhiteboardTool::Text, "Texto"),
                            (Icon::Eraser, WhiteboardTool::Eraser, "Borrador"),
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
                                selected_tool = Some(t);
                            }
                        }
                        // ——— Color y grosor (solo para herramientas con trazo) ———
                        let supports_color = matches!(
                            tool,
                            WhiteboardTool::Pencil
                                | WhiteboardTool::Rectangle
                                | WhiteboardTool::Ellipse
                                | WhiteboardTool::Arrow
                                | WhiteboardTool::Text
                        );
                        if supports_color {
                            ui.separator();
                            // Swatch circular con borde que indica apertura
                            let (swatch_rect, swatch_resp) =
                                ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                            if ui.is_rect_visible(swatch_rect) {
                                let painter = ui.painter();
                                painter.circle_filled(swatch_rect.center(), 9.0, pen_color);
                                painter.circle_stroke(
                                    swatch_rect.center(),
                                    9.0,
                                    Stroke::new(
                                        1.5,
                                        if palette_open {
                                            theme.accent
                                        } else {
                                            theme.separator.gamma_multiply(0.30)
                                        },
                                    ),
                                );
                                // Anillo interior que sugiere grosor
                                painter.circle_stroke(
                                    swatch_rect.center(),
                                    (pen_width * 1.1).clamp(2.0, 7.0),
                                    Stroke::new(1.0, Color32::from_black_alpha(25)),
                                );
                            }
                            if swatch_resp
                                .on_hover_text("Color y grosor del trazo")
                                .clicked()
                            {
                                toggle_palette = true;
                            }
                        }
                        ui.separator();
                        if action_icon_button(
                            ui,
                            Icon::Settings,
                            if app.show_whiteboard_assistant {
                                theme.accent
                            } else {
                                theme.text_secondary
                            },
                            "Asistente (mostrar/ocultar)",
                        )
                        .clicked()
                        {
                            toggle_assistant = true;
                        }
                        if action_icon_button(
                            ui,
                            Icon::Search,
                            theme.accent,
                            "Entender este dibujo con IA",
                        )
                        .clicked()
                        {
                            ask_ai = true;
                        }
                        if action_icon_button(ui, Icon::Delete, theme.text_secondary, "Limpiar")
                            .clicked()
                        {
                            clear = true;
                        }
                        if action_icon_button(ui, Icon::Close, theme.text_secondary, "Cerrar (Esc)")
                            .clicked()
                        {
                            close = true;
                        }
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
    if app.show_whiteboard_assistant {
        let visuals = grafito_ui::assistant::AssistantVisuals {
            mora_texture: app.mora_texture.as_ref().map(egui::TextureHandle::id),
        };
        let action = grafito_ui::assistant::draw_assistant_panel(
            ctx,
            &mut app.assistant,
            0.0,
            visuals,
            &mut app.assistant_blocks_cache,
        );
        if let Some(action) = action {
            app.handle_assistant_action(ctx, action);
        }
    }
    let mut keep_open = app.whiteboard_open;
    egui::Window::new("Pizarra")
        .id(egui::Id::new("whiteboard_window"))
        .collapsible(false)
        .resizable(true)
        .min_size(vec2(420.0, 320.0))
        .max_size(vec2(720.0, 560.0))
        .default_size(vec2(560.0, 380.0))
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel_bg)
                .stroke(egui::Stroke::new(1.0, theme.separator.gamma_multiply(0.10)))
                .rounding(grafito_ui::tokens::RADIUS_LG)
                .shadow(egui::Shadow {
                    offset: vec2(0.0, 4.0),
                    blur: 16.0,
                    spread: 0.0,
                    color: Color32::from_black_alpha(20),
                }),
        )
        .open(&mut keep_open)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                draw_toolbar(ui, app);
                let anim = ui.ctx().animate_bool(
                    egui::Id::new("whiteboard_palette"),
                    app.whiteboard.show_palette,
                );
                if anim > 0.01 {
                    let bg = theme.panel_bg.gamma_multiply(anim * 0.98 + 0.02);
                    let stroke_a = 0.10 * anim;
                    egui::Frame::none()
                        .fill(bg)
                        .stroke(egui::Stroke::new(
                            1.0,
                            theme.separator.gamma_multiply(stroke_a),
                        ))
                        .rounding(grafito_ui::tokens::RADIUS_MD)
                        .inner_margin(egui::Margin::symmetric(SPACE_MD, SPACE_SM))
                        .show(ui, |ui| {
                            ui.scope(|ui| {
                                ui.style_mut().visuals.override_text_color =
                                    Some(theme.text_primary.gamma_multiply(anim));
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Color")
                                            .size(grafito_ui::tokens::TYPE_XS)
                                            .color(theme.text_secondary.gamma_multiply(anim))
                                            .strong(),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let (r, _) = ui.allocate_exact_size(
                                                vec2(20.0, 20.0),
                                                Sense::hover(),
                                            );
                                            ui.painter().circle_filled(
                                                r.center(),
                                                10.0,
                                                app.whiteboard.pen_color.gamma_multiply(anim),
                                            );
                                            ui.painter().circle_stroke(
                                                r.center(),
                                                10.0,
                                                Stroke::new(
                                                    1.0,
                                                    theme.separator.gamma_multiply(0.25 * anim),
                                                ),
                                            );
                                        },
                                    );
                                });
                                ui.add_space(SPACE_XS);
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
                                egui::Grid::new("whiteboard_palette_grid")
                                    .num_columns(4)
                                    .spacing(vec2(8.0, 8.0))
                                    .show(ui, |ui| {
                                        for (idx, rgb) in PALETTE.iter().enumerate() {
                                            let col = Color32::from_rgb(rgb[0], rgb[1], rgb[2])
                                                .gamma_multiply(anim);
                                            let is_sel = app.whiteboard.pen_color == col;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                vec2(28.0, 28.0),
                                                Sense::click(),
                                            );
                                            if ui.is_rect_visible(rect) {
                                                let painter = ui.painter();
                                                painter.circle_filled(rect.center(), 13.0, col);
                                                painter.circle_stroke(
                                                    rect.center(),
                                                    13.0,
                                                    Stroke::new(
                                                        if is_sel { 2.0 } else { 1.0 },
                                                        if is_sel {
                                                            theme.accent.gamma_multiply(anim)
                                                        } else {
                                                            theme
                                                                .separator
                                                                .gamma_multiply(0.25 * anim)
                                                        },
                                                    ),
                                                );
                                                if is_sel {
                                                    painter.circle_filled(
                                                        rect.center(),
                                                        3.0,
                                                        theme.accent.gamma_multiply(anim),
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
                                                picked =
                                                    Some(Color32::from_rgb(rgb[0], rgb[1], rgb[2]));
                                            }
                                            if idx % 4 == 3 {
                                                ui.end_row();
                                            }
                                        }
                                    });
                                if let Some(col) = picked {
                                    app.whiteboard.pen_color = col;
                                }
                                ui.add_space(SPACE_SM);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Grosor")
                                            .size(grafito_ui::tokens::TYPE_XS)
                                            .color(theme.text_secondary.gamma_multiply(anim)),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let txt = format!("{:.1}", app.whiteboard.pen_width);
                                            egui::Frame::none()
                                                .fill(theme.input_bg.gamma_multiply(anim))
                                                .rounding(grafito_ui::tokens::RADIUS_PILL)
                                                .inner_margin(egui::Margin::symmetric(8.0, 2.0))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        egui::RichText::new(txt)
                                                            .size(grafito_ui::tokens::TYPE_XS)
                                                            .strong()
                                                            .color(
                                                                theme
                                                                    .text_primary
                                                                    .gamma_multiply(anim),
                                                            ),
                                                    );
                                                });
                                            let mut w = app.whiteboard.pen_width;
                                            let resp = ui.add(
                                                egui::Slider::new(&mut w, 1.0..=6.0)
                                                    .show_value(false)
                                                    .trailing_fill(true),
                                            );
                                            if resp.changed() {
                                                app.whiteboard.pen_width = w.clamp(1.0, 6.0);
                                            }
                                        },
                                    );
                                });
                                ui.add_space(SPACE_XS);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new(
                                            "Toca un color para aplicar — se guarda al instante",
                                        )
                                        .size(10.0)
                                        .color(theme.text_tertiary.gamma_multiply(0.85 * anim))
                                        .weak()
                                        .italics(),
                                    );
                                });
                            });
                        });
                    ui.add_space(SPACE_SM);
                }
                // Canvas compacto — altura fija 260 para elegancia, sin desborde
                let canvas_h = 260.0;
                let (canvas_rect, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), canvas_h),
                    Sense::click_and_drag(),
                );
                // Fondo sutil para diferenciar del panel
                ui.painter().rect_filled(
                    canvas_rect,
                    grafito_ui::tokens::RADIUS_MD,
                    theme.canvas_bg.gamma_multiply(0.96),
                );
                ui.painter().rect_stroke(
                    canvas_rect,
                    grafito_ui::tokens::RADIUS_MD,
                    Stroke::new(1.0, theme.separator.gamma_multiply(0.08)),
                );
                app.whiteboard.handle_canvas_input(canvas_rect, ui);
                app.whiteboard.draw(ui, canvas_rect);
            });
        });
    if !keep_open {
        app.whiteboard.show_palette = false;
        app.whiteboard_open = false;
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
