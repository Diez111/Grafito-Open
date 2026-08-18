//! Pizarra nativa en overlay (estilo macOS): lienzo a pantalla completa con
//! toolbar flotante redondeada y herramientas de dibujo libres sobre
//! `grafito_whiteboard`. No toca `Document`/`GeoObject`.

use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke};
use grafito_ui::icons::{action_icon_button, Icon};
use grafito_ui::theme::current_theme;
use grafito_ui::tokens::RADIUS_MD;
use grafito_whiteboard::{
    arrow_tip, smooth_stroke, WhiteboardDoc, WhiteboardElement, WhiteboardInteraction,
    WhiteboardTool,
};

/// Sesión de pizarra activa (overlay).
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
        }
    }
}

impl WhiteboardSession {
    pub fn world_from_screen(&self, screen: Pos2, rect: Rect) -> (f64, f64) {
        let x = (screen.x - rect.center().x - self.pan.0 as f32) / self.zoom as f32;
        let y = (screen.y - rect.center().y - self.pan.1 as f32) / self.zoom as f32;
        (x as f64, y as f64)
    }

    fn screen_from_world(&self, world: (f64, f64), rect: Rect) -> Pos2 {
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
    }

    pub fn clear(&mut self) {
        self.doc.clear();
        self.active_text = None;
    }

    pub fn zoom_at(&mut self, factor: f64, screen: Pos2, rect: Rect) {
        let before = self.world_from_screen(screen, rect);
        self.zoom = (self.zoom * factor).clamp(0.2, 6.0);
        let after = self.world_from_screen(screen, rect);
        self.pan = (
            self.pan.0 - (before.0 - after.0),
            self.pan.1 - (before.1 - after.1),
        );
    }

    pub fn handle_pointer(&mut self, world: (f64, f64), pressed: bool, released: bool) {
        if pressed {
            self.interaction = WhiteboardInteraction::begin(world, self.tool);
            self.marquee = None;
            if self.tool == WhiteboardTool::Pencil {
                self.pencil_points = vec![world];
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
                    self.doc.add(WhiteboardElement::Stroke {
                        points,
                        color: (55, 55, 55),
                        width: 2.0,
                    });
                }
                self.pencil_points.clear();
            }
            WhiteboardTool::Select => {
                if let Some((a, b)) = self.marquee {
                    let center = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                    let _ = self.doc.select_at(center, 8.0 / self.zoom);
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
        let tolerance = 6.0 / self.zoom;
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
        let theme = current_theme(ui.ctx());
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, theme.canvas_bg);
        draw_grid(painter, rect, self.pan, self.zoom, theme.separator);
        for element in self.doc.elements() {
            draw_element(painter, element, rect, self);
        }
        // Trazo en vivo del lápiz: se dibuja mientras se arrastra (natural).
        if self.tool == WhiteboardTool::Pencil && !self.pencil_points.is_empty() {
            let accent = theme.accent;
            let stroke = Stroke::new(2.0 * self.zoom as f32, Color32::from_rgb(55, 55, 55));
            for pair in self.pencil_points.windows(2) {
                let a = self.screen_from_world(pair[0], rect);
                let b = self.screen_from_world(pair[1], rect);
                painter.line_segment([a, b], stroke);
            }
            if self.pencil_points.len() == 1 {
                let p = self.screen_from_world(self.pencil_points[0], rect);
                painter.circle_filled(p, 1.5 * self.zoom as f32, accent);
            }
        }
        if let Some(preview) = self.interaction.preview() {
            draw_element(painter, &preview, rect, self);
        }
        if let Some((a, b)) = self.marquee {
            let (pa, pb) = (
                self.screen_from_world(a, rect),
                self.screen_from_world(b, rect),
            );
            painter.rect_stroke(
                Rect::from_two_pos(pa, pb),
                0.0,
                Stroke::new(1.0, theme.accent.gamma_multiply(0.6)),
            );
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
) {
    let stroke = Stroke::new(1.8, Color32::from_rgb(70, 70, 70));
    match element {
        WhiteboardElement::Stroke { points, width, .. } => {
            for pair in points.windows(2) {
                let a = session.screen_from_world(pair[0], rect);
                let b = session.screen_from_world(pair[1], rect);
                painter.line_segment(
                    [a, b],
                    Stroke::new(
                        (*width as f32) * session.zoom as f32,
                        Color32::from_rgb(55, 55, 55),
                    ),
                );
            }
        }
        WhiteboardElement::Rectangle { min, max, .. } => {
            let a = session.screen_from_world(*min, rect);
            let b = session.screen_from_world(*max, rect);
            painter.rect_stroke(Rect::from_two_pos(a, b), RADIUS_MD, stroke);
        }
        WhiteboardElement::Ellipse { center, rx, .. } => {
            let radius = (*rx as f32 * session.zoom as f32).max(2.0);
            painter.circle_stroke(session.screen_from_world(*center, rect), radius, stroke);
        }
        WhiteboardElement::Arrow { from, to } => {
            let a = session.screen_from_world(*from, rect);
            let b = session.screen_from_world(*to, rect);
            painter.line_segment([a, b], stroke);
            let wing = arrow_tip(*from, *to, 0.55);
            let wing_a = session.screen_from_world(wing.0, rect);
            let wing_b = session.screen_from_world(wing.1, rect);
            painter.line_segment([wing_a, b], stroke);
            painter.line_segment([wing_b, b], stroke);
        }
        WhiteboardElement::Text { at, text, size } => {
            if !text.is_empty() {
                let font = egui::FontId::monospace((*size as f32) * session.zoom as f32);
                painter.text(
                    session.screen_from_world(*at, rect),
                    egui::Align2::LEFT_TOP,
                    text,
                    font,
                    Color32::from_rgb(50, 50, 50),
                );
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
    egui::Frame::none()
        .fill(theme.panel_bg)
        .stroke(egui::Stroke::new(1.0, theme.separator))
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
        .shadow(egui::epaint::Shadow {
            offset: vec2(0.0, 4.0),
            blur: 14.0,
            spread: 0.0,
            color: Color32::from_black_alpha(40),
        })
        .show(ui, |ui| {
            let session = &app.whiteboard;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    egui::RichText::new("Pizarra")
                        .color(theme.accent)
                        .size(13.0)
                        .strong(),
                );
                ui.separator();
                for (icon, tool, tip) in [
                    (Icon::Move, WhiteboardTool::Select, "Seleccionar"),
                    (Icon::Pencil, WhiteboardTool::Pencil, "Lápiz libre"),
                    (Icon::Shapes, WhiteboardTool::Rectangle, "Rectángulo"),
                    (Icon::Ellipse, WhiteboardTool::Ellipse, "Elipse"),
                    (Icon::ArrowRight, WhiteboardTool::Arrow, "Flecha"),
                    (Icon::Notebook, WhiteboardTool::Text, "Texto"),
                    (Icon::Eraser, WhiteboardTool::Eraser, "Borrador"),
                ] {
                    let selected = session.tool == tool;
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
                        selected_tool = Some(tool);
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
                if action_icon_button(ui, Icon::Delete, theme.text_secondary, "Limpiar").clicked() {
                    clear = true;
                }
                if action_icon_button(ui, Icon::Close, theme.text_secondary, "Cerrar (Esc)")
                    .clicked()
                {
                    close = true;
                }
            });
        });
    if let Some(tool) = selected_tool {
        app.whiteboard.set_tool(tool);
    }
    if clear {
        app.whiteboard.clear();
    }
    if close {
        app.whiteboard_open = false;
    }
    if toggle_assistant {
        app.show_whiteboard_assistant = !app.show_whiteboard_assistant;
    }
    if ask_ai {
        // El asistente «ve» el dibujo por su descripción estructurada y lo
        // explica con DeepSeek V4 Flash (seam listo para un modelo de visión
        // barato como MiniMax/MiMo 2.5-VL sin tocar el flujo del usuario).
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
    // El asistente sigue avanzando (y el análisis de la pizarra se resuelve)
    // aunque el overlay esté a pantalla completa.
    app.sync_assistant_for_frame(ctx);

    // Asistente visible dentro de la pizarra (y ocultable desde la toolbar).
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
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme.canvas_bg))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            egui::Area::new(egui::Id::new("whiteboard_toolbar"))
                .anchor(egui::Align2::CENTER_TOP, vec2(0.0, 14.0))
                .order(egui::Order::Foreground)
                .interactable(true)
                .show(ctx, |ui| {
                    draw_toolbar(ui, app);
                });
            app.whiteboard.handle_canvas_input(rect, ui);
            app.whiteboard.draw(ui, rect);
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        app.whiteboard_open = false;
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
    ctx.request_repaint();
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
