//! Aula loopback F0 sin red — ClassroomPanel + ShareCode + QR real.
//!
//! Fase C1: el panel existe testeado pero no estaba cableado. Este módulo es
//! Piel pura: `ui()` no hace I/O ni spawn, solo renderiza `&Estado`.
//! El QR es real y escaneable vía `qrcode` 0.14 (`generate_qr_modules`):
//! codifica `grafito://aula/{código}`; si el código es vacío/inválido o el
//! encoder falla, el panel muestra fallback de texto honesto (sin QR falso).

use egui::{Color32, Rect, Vec2};
use grafito_ui::tokens::{
    panel_left_max_width, PANEL_LEFT_DEFAULT, PANEL_LEFT_MIN, SPACE_LG, SPACE_SM, SPACE_XL,
    SPACE_XS, SPACE_XXL, TYPE_BASE, TYPE_SM, TYPE_XS,
};

/// Lado del QR del ShareCode — derivado de tokens (sin hardcodes):
/// `PANEL_LEFT_DEFAULT − SPACE_XXL − SPACE_XL − SPACE_LG` = 180.
pub const CLASSROOM_QR_SIDE: f32 = PANEL_LEFT_DEFAULT - SPACE_XXL - SPACE_XL - SPACE_LG;

/// Código para compartir en lobby/live. Validación newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareCode(pub String);

impl ShareCode {
    /// Crea un ShareCode validando caracteres permitidos y longitud 1..=64.
    /// Solo alfanumérico, '-' y '_' (compatible con URL/query).
    pub fn new(code: impl Into<String>) -> Option<Self> {
        let s = code.into();
        if s.is_empty() || s.len() > 64 {
            return None;
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        Some(Self(s))
    }

    /// Genera un código loopback para F0 sin red. Determinista por proceso+tiempo,
    /// sin I/O. No usa `rand` para evitar dep extra.
    pub fn generate() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id() as u128;
        let mut seed = nanos ^ (pid.rotate_left(7).wrapping_add(0x9e3779b97f4a7c15));
        if seed == 0 {
            seed = 0x9e3779b97f4a7c15;
        }
        const ALPH: &[u8] = b"ABCDEFGHJKMNPQRSTVWXYZ23456789";
        let mut out = String::with_capacity(10);
        out.push_str("AULA-");
        let mut v = seed;
        for _ in 0..6 {
            let idx = (v % ALPH.len() as u128) as usize;
            out.push(ALPH[idx] as char);
            v = v.wrapping_div(ALPH.len() as u128);
            if v == 0 {
                v = seed.wrapping_add(0x9e3779b97f4a7c15);
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            }
        }
        // SAFETY: construcción garantiza alfanumérico + '-' y len 11
        Self(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Estado del panel Aula (F0 loopback sin red).
#[derive(Debug, Clone)]
pub struct ClassroomPanel {
    opt_in: bool,
    share_code: Option<ShareCode>,
    is_host: bool,
}

impl Default for ClassroomPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassroomPanel {
    pub fn new() -> Self {
        Self {
            opt_in: false,
            share_code: None,
            is_host: false,
        }
    }

    /// Sincroniza el opt-in desde `GrafitoApp.advanced_red_opt_in` (llamado por frame).
    pub fn set_opt_in(&mut self, opt: bool) {
        self.opt_in = opt;
        if opt && self.share_code.is_none() {
            self.share_code = Some(ShareCode::generate());
        }
    }

    pub fn is_opt_in(&self) -> bool {
        self.opt_in
    }

    /// ¿Debe mostrarse el tab "Aula" en el sidebar? Solo si opt-in.
    pub fn should_show_aula_tab(&self) -> bool {
        self.opt_in
    }

    pub fn share_code(&self) -> Option<&ShareCode> {
        self.share_code.as_ref()
    }

    pub fn set_share_code(&mut self, code: ShareCode) {
        self.share_code = Some(code);
    }

    pub fn set_host(&mut self, host: bool) {
        self.is_host = host;
    }

    pub fn is_host(&self) -> bool {
        self.is_host
    }

    /// Render puro del contenido del panel (sin I/O, sin spawn). Llamado desde
    /// `draw_classroom_panel` que crea el SidePanel izquierdo.
    pub fn ui(&self, ui: &mut egui::Ui) {
        ui.add_space(SPACE_SM);
        ui.label(
            egui::RichText::new("Aula — loopback sin red (F0)")
                .size(TYPE_BASE)
                .strong(),
        );
        ui.add_space(SPACE_SM);
        ui.separator();
        ui.add_space(SPACE_SM);
        if !self.opt_in {
            ui.label(
                egui::RichText::new(
                    "Aula deshabilitada. Activá el opt-in para ver el tab \"Aula\" y el QR de conexión.",
                )
                .size(TYPE_XS)
                .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new("El opt-in se activa en Configuración → Aula (avanzado).")
                    .size(TYPE_XS)
                    .weak(),
            );
            return;
        }
        if let Some(code) = &self.share_code {
            ui.label(
                egui::RichText::new(format!("Código de sala: {}", code.as_str()))
                    .size(TYPE_SM)
                    .strong(),
            );
            ui.add_space(SPACE_XS);
            ui.label(
                egui::RichText::new("Escaneá el QR con otro dispositivo en la misma red local (loopback F0: sin conexión real).")
                    .size(TYPE_XS)
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(SPACE_SM);
            draw_share_qr(ui, code.as_str(), CLASSROOM_QR_SIDE);
            ui.add_space(SPACE_SM);
            ui.label(
                egui::RichText::new(format!("grafito://aula/{}", code.as_str()))
                    .size(TYPE_XS)
                    .monospace()
                    .color(ui.visuals().weak_text_color()),
            );
            ui.add_space(SPACE_SM);
            if self.is_host {
                ui.label(
                    egui::RichText::new("Modo anfitrión (lobby activo)")
                        .size(TYPE_XS)
                        .color(Color32::from_rgb(0x1a, 0x7f, 0x37)),
                );
            } else {
                ui.label(
                    egui::RichText::new("Modo invitado — el anfitrión comparte este código")
                        .size(TYPE_XS)
                        .weak(),
                );
            }
        } else {
            ui.label("Generando código de sala…");
        }
        ui.add_space(SPACE_SM);
        ui.separator();
        ui.add_space(SPACE_XS);
        ui.label(
            egui::RichText::new("F0 sin red: no hay P2P ni servidor. El QR es local y el share es loopback para validar flujo UI.")
                .size(TYPE_XS)
                .weak(),
        );
    }
}

/// Payload escaneable del QR para un código de sala. `None` honesto si el
/// código es vacío o inválido (`ShareCode::new` lo rechaza: longitud 1..=64,
/// sólo ASCII alfanumérico + `-`/`_`).
pub fn qr_payload_for_code(code: &str) -> Option<String> {
    ShareCode::new(code)?;
    Some(format!("grafito://aula/{code}"))
}

/// Genera la grid booleana de un QR REAL con `qrcode` 0.14 (core, sin
/// features de imagen/svg: sólo `QrCode::new` + `into_colors`).
///
/// Codifica `grafito://aula/{código}`. Retorna `None` honesto con código
/// vacío/inválido o si el encoder falla (p. ej. payload excede la capacidad
/// QR). Grid cuadrada de lado `width()` (≥21 según versión), `true` = módulo
/// oscuro. Pura, sin I/O, testeable.
pub fn generate_qr_modules(code: &str) -> Option<Vec<Vec<bool>>> {
    let payload = qr_payload_for_code(code)?;
    let qr = qrcode::QrCode::new(payload.as_bytes()).ok()?;
    let width = qr.width();
    if width == 0 {
        return None;
    }
    let colors = qr.into_colors();
    if colors.len() != width.saturating_mul(width) {
        return None;
    }
    let mut grid = Vec::with_capacity(width);
    for row in colors.chunks(width) {
        grid.push(
            row.iter()
                .map(|color| *color == qrcode::Color::Dark)
                .collect(),
        );
    }
    Some(grid)
}

/// Dibuja el QR REAL del `code` con rects egui. Si `generate_qr_modules`
/// falla (código vacío/inválido o encoder sin capacidad), fallback honesto
/// de texto en vez de un QR falso que no escanea.
pub fn draw_share_qr(ui: &mut egui::Ui, code: &str, size: f32) {
    let Some(grid) = generate_qr_modules(code) else {
        ui.label(
            egui::RichText::new(
                "QR no disponible para este código — compartí el código de sala como texto.",
            )
            .size(TYPE_XS)
            .color(ui.visuals().weak_text_color()),
        );
        return;
    };
    let n = grid.len() as f32;
    let cell = size / n.max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, SPACE_XS, Color32::WHITE);
    painter.rect_stroke(
        rect,
        SPACE_XS,
        egui::Stroke::new(1.0, Color32::from_gray(200)),
    );
    for (y, row) in grid.iter().enumerate() {
        for (x, &filled) in row.iter().enumerate() {
            if !filled {
                continue;
            }
            let r = Rect::from_min_size(
                egui::pos2(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            );
            painter.rect_filled(r, 0.0, Color32::BLACK);
        }
    }
    // Borde fino alrededor del QR para contraste
    painter.rect_stroke(rect, SPACE_XS, egui::Stroke::new(1.0, Color32::BLACK));
}

/// Entry point para `GrafitoApp` (left drawer). Crea SidePanel y delega a `ClassroomPanel::ui`.
///
/// Piel pura: dentro del `show` (Ui::) solo se renderiza y se REGISTRAN
/// acciones; la mutación + persistencia (`save_app_config`, I/O) se aplica
/// DESPUÉS del cierre. El sync `set_opt_in` vive por frame en `update`
/// (`app.rs`), nunca en Ui::.
pub fn draw_classroom_panel(app: &mut crate::app::GrafitoApp, ctx: &egui::Context) {
    let mut opt_in_toggled: Option<bool> = None;
    let mut regenerate_code = false;
    let mut toggle_host = false;
    let opt_in_snapshot = app.advanced_red_opt_in;
    let panel_was_opt_in = app.classroom.is_opt_in();
    let mut opt_in = opt_in_snapshot;
    egui::SidePanel::left("aula_panel")
        .default_width(PANEL_LEFT_DEFAULT)
        .min_width(PANEL_LEFT_MIN)
        .max_width(panel_left_max_width(ctx.screen_rect().width()))
        .resizable(true)
        .show(ctx, |ui| {
            // Toggle opt-in: solo registra la intención (Piel pura, sin I/O).
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Aula").size(TYPE_SM).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.checkbox(&mut opt_in, "Activar").changed() {
                        opt_in_toggled = Some(opt_in);
                    }
                });
            });
            ui.separator();
            app.classroom.ui(ui);
            ui.add_space(SPACE_SM);
            if panel_was_opt_in {
                ui.horizontal(|ui| {
                    if ui.button("Regenerar código").clicked() {
                        regenerate_code = true;
                    }
                    if ui.button("Alternar host/invitado").clicked() {
                        toggle_host = true;
                    }
                });
            }
        });
    // Aplicación FUERA de Ui:: (contexto `update`, sin layout en curso).
    if let Some(opt_in) = opt_in_toggled {
        app.advanced_red_opt_in = opt_in;
        app.classroom.set_opt_in(opt_in);
        app.save_app_config();
    }
    if regenerate_code {
        app.classroom.set_share_code(ShareCode::generate());
    }
    if toggle_host {
        let cur = app.classroom.is_host();
        app.classroom.set_host(!cur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_code_validation_accepts_alphanumeric_dash_underscore() {
        assert!(ShareCode::new("AULA-123").is_some());
        assert!(ShareCode::new("abc_123").is_some());
        assert!(ShareCode::new("").is_none());
        assert!(ShareCode::new("a".repeat(65)).is_none());
        assert!(ShareCode::new("bad code!").is_none());
        assert!(ShareCode::new("ok-CODE_99").is_some());
    }

    #[test]
    fn share_code_generate_is_valid_and_nonempty() {
        let c = ShareCode::generate();
        assert!(!c.as_str().is_empty());
        assert!(ShareCode::new(c.as_str()).is_some());
        assert!(c.as_str().starts_with("AULA-"));
        assert_eq!(c.as_str().len(), 11);
    }

    #[test]
    fn classroom_panel_opt_in_controls_aula_tab_visibility() {
        let mut panel = ClassroomPanel::new();
        assert!(
            !panel.should_show_aula_tab(),
            "sin opt-in no se muestra tab Aula"
        );
        assert!(!panel.is_opt_in());
        panel.set_opt_in(true);
        assert!(panel.should_show_aula_tab(), "con opt-in tab visible");
        assert!(panel.is_opt_in());
        assert!(panel.share_code().is_some(), "al activar genera ShareCode");
        panel.set_opt_in(false);
        assert!(!panel.should_show_aula_tab());
        // ShareCode persiste tras desactivar (no se borra, solo se oculta tab)
        assert!(panel.share_code().is_some());
    }

    #[test]
    fn classroom_panel_set_opt_in_generates_share_code_once() {
        let mut panel = ClassroomPanel::new();
        panel.set_opt_in(true);
        let first = panel.share_code().unwrap().as_str().to_owned();
        panel.set_opt_in(true);
        let second = panel.share_code().unwrap().as_str().to_owned();
        assert_eq!(first, second, "no regenerar si ya existe");
        let custom = ShareCode::new("CUSTOM-01").unwrap();
        panel.set_share_code(custom.clone());
        assert_eq!(panel.share_code().unwrap(), &custom);
    }

    #[test]
    fn qr_grid_is_deterministic_and_has_finder_patterns() {
        let g1 = generate_qr_modules("AULA-123").expect("QR real genera");
        let g2 = generate_qr_modules("AULA-123").expect("QR real genera");
        assert_eq!(g1, g2, "mismo código -> misma grid");
        let g3 = generate_qr_modules("AULA-999").expect("QR real genera");
        assert_ne!(g1, g3, "código distinto -> grid distinta");
        let n = g1.len();
        assert!(n >= 21, "versión QR mínima 21×21, fue {n}");
        assert!(g1.iter().all(|row| row.len() == n), "grid cuadrada");
        // Finder top-left del QR real: esquinas borde negro, centro negro
        assert!(g1[0][0]);
        assert!(g1[0][6]);
        assert!(g1[6][0]);
        assert!(g1[3][3], "centro finder");
        // Dark module del estándar (fila N-8, col 8)
        assert!(g1[n - 8][8]);
    }

    #[test]
    fn qr_grid_different_codes_produce_different_data_area() {
        let a = generate_qr_modules("code-a").expect("QR real genera");
        let b = generate_qr_modules("code-b").expect("QR real genera");
        assert_eq!(a.len(), b.len());
        // Comparar zona central (10,10) debería diferir con alta prob
        let mut diff = 0;
        for y in 8..13 {
            for x in 8..13 {
                if a[y][x] != b[y][x] {
                    diff += 1;
                }
            }
        }
        assert!(
            diff > 0,
            "zona de datos debe diferir entre códigos distintos"
        );
    }

    #[test]
    fn classroom_ui_is_pure_and_does_not_panic() {
        // No test de egui render real (requiere ctx), solo verificar que construir panel
        // y generar QR no paniquea en modo headless.
        let mut panel = ClassroomPanel::new();
        panel.set_opt_in(true);
        let code = panel.share_code().unwrap().as_str().to_owned();
        let grid = generate_qr_modules(&code).expect("QR real genera");
        assert!(grid.len() >= 21);
    }

    #[test]
    fn aula_tab_visible_with_opt_in() {
        let mut panel = ClassroomPanel::new();
        panel.set_opt_in(true);
        assert!(panel.is_opt_in());
        assert!(
            panel.should_show_aula_tab(),
            "con opt-in el tab Aula debe ser visible"
        );
    }

    #[test]
    fn aula_tab_hidden_without_opt_in() {
        let panel = ClassroomPanel::new();
        assert!(!panel.is_opt_in());
        assert!(
            !panel.should_show_aula_tab(),
            "sin opt-in el tab Aula debe estar oculto"
        );
        let mut panel = ClassroomPanel::new();
        panel.set_opt_in(true);
        panel.set_opt_in(false);
        assert!(
            !panel.should_show_aula_tab(),
            "al revocar el opt-in el tab se oculta"
        );
    }

    #[test]
    fn qr_side_derives_from_tokens() {
        assert_eq!(
            CLASSROOM_QR_SIDE,
            grafito_ui::tokens::PANEL_LEFT_DEFAULT
                - grafito_ui::tokens::SPACE_XXL
                - grafito_ui::tokens::SPACE_XL
                - grafito_ui::tokens::SPACE_LG
        );
        assert_eq!(CLASSROOM_QR_SIDE % 4.0, 0.0);
    }

    #[test]
    fn qr_payload_encodes_join_uri_for_valid_code() {
        assert_eq!(
            qr_payload_for_code("AULA-ABC123").as_deref(),
            Some("grafito://aula/AULA-ABC123")
        );
        assert_eq!(qr_payload_for_code(""), None);
        assert_eq!(qr_payload_for_code("bad code!"), None);
        assert_eq!(qr_payload_for_code(&"a".repeat(65)), None);
    }

    #[test]
    fn qr_real_generates_deterministic_scannable_grid() {
        let first = generate_qr_modules("AULA-ABC123").expect("QR real genera");
        let second = generate_qr_modules("AULA-ABC123").expect("QR real genera");
        assert_eq!(first, second, "mismo código -> misma grid");
        let n = first.len();
        assert!(n >= 21, "versión QR mínima 21×21, fue {n}");
        assert!(
            first.iter().all(|row| row.len() == n),
            "grid cuadrada {n}×{n}"
        );
        // Finder top-left del QR real: esquina y centro oscuros.
        assert!(first[0][0]);
        assert!(first[3][3]);
        // Hay módulos oscuros y claros (no vacía ni sólida).
        let dark = first.iter().flatten().filter(|cell| **cell).count();
        assert!(dark > 0 && dark < n * n);
        // Código distinto -> grid distinta.
        let other = generate_qr_modules("AULA-XYZ999").expect("QR real genera");
        assert_ne!(first, other);
    }

    #[test]
    fn qr_real_fails_honestly_with_empty_or_invalid_code() {
        assert_eq!(generate_qr_modules(""), None);
        assert_eq!(generate_qr_modules("bad code!"), None);
        assert_eq!(generate_qr_modules(&"a".repeat(65)), None);
    }
}
