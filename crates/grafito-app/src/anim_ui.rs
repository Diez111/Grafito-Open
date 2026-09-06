//! Panel de animaciones — vista previa, progreso y exportación sin bloquear la interfaz.
//!
//! Estado puro en `AnimPreviewState`; la generación corre en un hilo aparte y el
//! progreso llega por `ctx.request_repaint()`. Separación Cerebro/Piel: este módulo
//! solo dibuja a partir de `&AnimPreviewState` (sin E/S ni lanzamientos en `Ui::`).
//!
//! Cotas contra desborde (todas derivadas de `grafito_ui::tokens` más fracciones
//! del ancho disponible): barra de progreso, vista previa con proporción 16:9 y
//! encajonado, deslizador acotado y tira de miniaturas paginada. La única
//! `ScrollArea` del panel es la tira horizontal de miniaturas, con altura máxima
//! acotada; la lista nunca crece sin cota (ventana de `MINIATURAS_POR_PAGINA`).

use egui::{Color32, ProgressBar, ScrollArea, Stroke};
use grafito_anim::protocol::{
    is_known_easing, Keyframe, Timeline, EASING_NAMES, MAX_TIMELINE_DURATION_MS,
};
use grafito_anim::{AnimDuration, AnimParams, ExportFormat, Resolution};
use grafito_ui::animation::easing as ui_easing;
use grafito_ui::tokens;
use std::collections::BTreeMap;

/// Miniaturas visibles por página (paginación: la tira nunca crece sin cota).
const MINIATURAS_POR_PAGINA: usize = 8;
/// Proporción de la vista previa (ancho 16, alto 9).
const VISTA_REL_ANCHO: f32 = 16.0;
/// Proporción de la vista previa (alto 9).
const VISTA_REL_ALTO: f32 = 9.0;
/// Proporción de cada miniatura (ancho 4, alto 3).
const MINI_REL_ANCHO: f32 = 4.0;
/// Proporción de cada miniatura (alto 3).
const MINI_REL_ALTO: f32 = 3.0;
/// Tope de fotogramas inspeccionados por `frames_hash` (igual que antes).
const FRAME_HASH_MAX_FRAMES: usize = 6;
/// Tope de píxeles muestreados por fotograma en `frames_hash`.
///
/// Presupuesto: antes se hasheaba CADA píxel de 6 frames por cada frame de UI
/// (640×480×6 ≈ 1.8M píxeles × 4 hashes ≈ 7M ops/frame a 12fps). Ahora se
/// muestrean ≤512 píxeles por frame (stride uniforme, siempre incluye el
/// píxel 0): 6×512×4 ≈ 12K hashes/frame, ~600× menos, suficiente para
/// invalidar el caché de texturas ante regeneración o cambio de página.
const FRAME_HASH_SAMPLE_PIXELS: usize = 512;
/// Fracción del ancho disponible que ocupa cada miniatura.
const MINI_FRACCION_ANCHO: f32 = 0.28;
/// Repintado de reproducción (~12 fotogramas por segundo).
/// TODO(v3-fps): selector de fps en la UI (6/12/24). Hoy es fijo 12: cambiar
/// exige solo exponer `PLAYBACK_FPS` en un ComboBox y derivar este intervalo.
pub const PLAYBACK_FPS: u32 = 12;
const REPRODUCCION_MS: u64 = 1000 / PLAYBACK_FPS as u64; // 83 ms ≈ 12 fps
/// Duración por defecto del timeline de scrub (igual que `AnimDuration` 2 s).
const TIMELINE_DEFAULT_MS: u64 = 2000;

pub struct AnimPreviewState {
    pub template: String,
    pub concept: String,
    pub progress: f32,
    pub status: String,
    pub media_path: Option<String>,
    pub frames: Vec<egui::ColorImage>,
    pub source_turn: Option<usize>,
    // Reproducción para deslizador + Reproducir/Pausar (12fps repaint)
    pub frame_idx: usize,
    pub playing: bool,
    // ── Params vivos v3 ──────────────────────────────────────────────
    // Los sliders editan `live_params`; `build_anim_params` los propaga al
    // motor y el scrub nativo re-renderiza con ellos
    // (`render_anim_for_concept_with_params`). `params_dirty` avisa que
    // cambiaron desde la última generación (el re-render real lo dispara el
    // hilo del asistente, fuera de este módulo — ver TODO en el badge).
    pub live_params: BTreeMap<String, f64>,
    pub params_dirty: bool,
    // ── Timeline v3 ──────────────────────────────────────────────────
    // Scrub con keyframes lineales puros del protocolo + easing por nombre
    // (fns reusadas de `grafito-ui`, sin enum nuevo). Vinculado al
    // deslizador de fotogramas en ambas direcciones (ver `timeline_frame`
    // y `sync_phase_from_frame`).
    pub timeline: Option<Timeline>,
    pub timeline_easing: String,
    pub timeline_phase: f32,
    // Caché de texturas para evitar clone+load cada frame (27KB×8 leak).
    cached_textures: Vec<egui::TextureHandle>,
    cached_hash: u64,
    cached_len: usize,
    // Inicio de la página cacheada (la ventana siempre contiene `frame_idx`).
    cached_inicio: usize,
}

impl Default for AnimPreviewState {
    fn default() -> Self {
        Self {
            template: String::new(),
            concept: String::new(),
            progress: 0.0,
            status: String::new(),
            media_path: None,
            frames: Vec::new(),
            source_turn: None,
            frame_idx: 0,
            playing: false,
            live_params: BTreeMap::new(),
            params_dirty: false,
            timeline: None,
            timeline_easing: "cubic_in_out".to_string(),
            timeline_phase: 0.0,
            cached_textures: Vec::new(),
            cached_hash: 0,
            cached_len: 0,
            cached_inicio: 0,
        }
    }
}

/// Estado visible del panel (cada uno con dibujo acotado propio).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EstadoVista {
    /// Sin fotogramas, sin archivo y sin error: mensaje de bienvenida.
    Vacia,
    /// Generación en curso (`0.0 < progress < 1.0`): barra acotada.
    Generando,
    /// Con fotogramas o archivo listo (incluye reproducción y pausa).
    Lista,
    /// Sin contenido y con mensaje de fallo: aviso acotado con reintento.
    Fallo,
}

impl AnimPreviewState {
    pub fn is_active(&self) -> bool {
        self.progress > 0.0 && self.progress < 1.0
    }
    pub fn clear(&mut self) {
        self.cached_textures.clear();
        *self = Self::default();
    }
    /// Avanza un fotograma si está reproduciendo (llamado antes de repintar).
    pub fn tick_playback(&mut self) {
        if self.playing && !self.frames.is_empty() {
            self.frame_idx = (self.frame_idx + 1) % self.frames.len();
        }
    }

    // ── Params vivos v3 ──────────────────────────────────────────────
    /// Fija un param vivo (solo finitos; NaN/inf se ignoran) y marca dirty.
    pub fn set_live_param(&mut self, key: &str, value: f64) {
        if !value.is_finite() {
            return;
        }
        if self.live_params.get(key).copied() != Some(value) {
            self.live_params.insert(key.to_string(), value);
            self.params_dirty = true;
        }
    }

    /// Confirma el aviso de params: la próxima generación los usará.
    pub fn mark_params_applied(&mut self) {
        self.params_dirty = false;
    }

    // ── Timeline v3 ──────────────────────────────────────────────────
    /// Fn de easing por nombre, reusando `grafito_ui::animation::easing::*`
    /// (sin enum nuevo). Desconocido → `cubic_in_out`.
    pub fn easing_fn(&self) -> fn(f32) -> f32 {
        match self.timeline_easing.trim() {
            "linear" => ui_easing::linear,
            "quadratic_in" => ui_easing::quadratic_in,
            "quadratic_out" => ui_easing::quadratic_out,
            "cubic_in" => ui_easing::cubic_in,
            "cubic_out" => ui_easing::cubic_out,
            "cubic_in_out" => ui_easing::cubic_in_out,
            "sin_in_out" => ui_easing::sin_in_out,
            "ease_out_back" => ui_easing::ease_out_back,
            _ => ui_easing::cubic_in_out,
        }
    }

    /// Fija el easing solo si el nombre existe en `EASING_NAMES`.
    pub fn set_timeline_easing(&mut self, name: &str) {
        let name = name.trim();
        if is_known_easing(name) {
            self.timeline_easing = name.to_string();
        }
    }

    /// Crea el timeline por defecto (0→0, duración→1) si no existe.
    /// Idempotente: si ya hay uno, lo conserva.
    pub fn ensure_default_timeline(&mut self, duration_ms: u64) {
        if self.timeline.is_none() {
            let duration_ms = duration_ms.clamp(1, MAX_TIMELINE_DURATION_MS);
            self.timeline = Some(Timeline {
                duration_ms,
                keyframes: vec![
                    Keyframe {
                        t_ms: 0,
                        value: 0.0,
                    },
                    Keyframe {
                        t_ms: duration_ms,
                        value: 1.0,
                    },
                ],
            });
            self.timeline_phase = 0.0;
        }
    }

    /// Olvida el timeline y vuelve la fase a 0.
    pub fn clear_timeline(&mut self) {
        self.timeline = None;
        self.timeline_phase = 0.0;
    }

    /// Fotograma para la fase actual con easing aplicado: el easing deforma
    /// el tiempo (`ms = ease(phase) * duración`) y el timeline lineal mapea
    /// ese tiempo a 0..1 y luego a `0..total`. `None` sin timeline/frames.
    pub fn timeline_frame(&self, total: usize) -> Option<usize> {
        let tl = self.timeline.as_ref()?;
        if total == 0 {
            return None;
        }
        if total == 1 {
            return Some(0);
        }
        let phase = self.timeline_phase.clamp(0.0, 1.0);
        let eased = (self.easing_fn())(phase).clamp(0.0, 1.0);
        let ms = (eased * tl.duration_ms as f32) as u64;
        let value = tl.sample(ms).clamp(0.0, 1.0);
        Some(((value * (total as f32 - 1.0)).round() as usize).min(total - 1))
    }

    /// Sincroniza la fase desde el fotograma actual (dirección inversa del
    /// vínculo: arrastrar el deslizador de fotogramas mueve el scrub).
    fn sync_phase_from_frame(&mut self) {
        if self.timeline.is_some() && self.frames.len() > 1 {
            self.timeline_phase =
                self.frame_idx.min(self.frames.len() - 1) as f32 / (self.frames.len() - 1) as f32;
        }
    }

    /// Limpia texturas explicitamente con contexto (libera GPU). Llamar desde UI con ctx.
    pub fn clear_with_ctx(&mut self, ctx: &egui::Context) {
        self.olvida_texturas(ctx);
        *self = Self::default();
    }

    /// Página actual según `frame_idx` (ventana de `MINIATURAS_POR_PAGINA`).
    fn pagina_actual(&self) -> usize {
        self.frame_idx / MINIATURAS_POR_PAGINA
    }

    /// Primer índice de la ventana visible (siempre contiene a `frame_idx`).
    fn inicio_ventana(&self) -> usize {
        self.pagina_actual() * MINIATURAS_POR_PAGINA
    }

    /// Fin exclusivo de la ventana visible, acotado a `frames.len()`.
    fn fin_ventana(&self) -> usize {
        self.inicio_ventana()
            .saturating_add(MINIATURAS_POR_PAGINA)
            .min(self.frames.len())
    }

    /// Cantidad de páginas (mínimo 1 cuando hay fotogramas, 0 si vacío).
    fn total_paginas(&self) -> usize {
        if self.frames.is_empty() {
            0
        } else {
            self.frames.len().div_ceil(MINIATURAS_POR_PAGINA)
        }
    }

    fn olvida_texturas(&mut self, ctx: &egui::Context) {
        for (idx, _) in self.cached_textures.iter().enumerate() {
            // TextureHandle Drop libera, pero forget_image asegura limpieza si el id fue reutilizado.
            ctx.forget_image(&format!("anim_preview_{}_{}", self.cached_inicio, idx));
        }
        self.cached_textures.clear();
        self.cached_hash = 0;
        self.cached_len = 0;
    }

    fn frames_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.frames.len().hash(&mut hasher);
        for f in self.frames.iter().take(FRAME_HASH_MAX_FRAMES) {
            f.size.hash(&mut hasher);
            // Muestreo uniforme acotado (ver `FRAME_HASH_SAMPLE_PIXELS`): el
            // stride siempre incluye el píxel 0 y cubre todo el frame.
            let total = f.pixels.len();
            if total == 0 {
                continue;
            }
            let stride = (total / FRAME_HASH_SAMPLE_PIXELS).max(1);
            for px in f.pixels.iter().step_by(stride) {
                px.r().hash(&mut hasher);
                px.g().hash(&mut hasher);
                px.b().hash(&mut hasher);
                px.a().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Asegura texturas solo para la ventana visible (paginada, sin crecer sin cota).
    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.frames.is_empty() {
            if !self.cached_textures.is_empty() {
                self.olvida_texturas(ctx);
            }
            return;
        }
        if self.frame_idx >= self.frames.len() {
            self.frame_idx = 0;
        }
        let inicio = self.inicio_ventana();
        let fin = self.fin_ventana();
        let hash = self.frames_hash();
        let largo = fin.saturating_sub(inicio);
        if self.cached_textures.len() == largo
            && self.cached_hash == hash
            && self.cached_len == largo
            && self.cached_inicio == inicio
        {
            return;
        }
        // Libera la página anterior antes de cargar la nueva.
        self.olvida_texturas(ctx);
        // Solo clona cuando cambia; evita clone 27KB×8 cada fotograma.
        self.cached_textures = self.frames[inicio..fin]
            .iter()
            .enumerate()
            .map(|(pos, fotograma)| {
                let tex_id = format!("anim_preview_{inicio}_{pos}");
                // fotograma.clone() solo aquí, cuando cambió la página o su hash.
                ctx.load_texture(tex_id, fotograma.clone(), egui::TextureOptions::LINEAR)
            })
            .collect();
        self.cached_hash = hash;
        self.cached_len = largo;
        self.cached_inicio = inicio;
    }
}

/// ¿El mensaje describe un fallo? (minúsculas, todo en español).
fn es_estado_error(mensaje: &str) -> bool {
    let min = mensaje.to_lowercase();
    min.contains("error")
        || min.contains("fall")
        || min.contains("vacío")
        || min.contains("largo")
        || min.contains("inválid")
}

/// Estado visible a partir del contenido (sin pánicos con listas vacías).
fn estado_vista(state: &AnimPreviewState) -> EstadoVista {
    if state.is_active() {
        EstadoVista::Generando
    } else if !state.frames.is_empty() || state.media_path.is_some() {
        EstadoVista::Lista
    } else if es_estado_error(&state.status) {
        EstadoVista::Fallo
    } else {
        EstadoVista::Vacia
    }
}

/// Nombre en español para cada identificador interno de plantilla.
fn nombre_plantilla(id: &str) -> &str {
    match id {
        "derivative-slope" => "Derivada (pendiente)",
        "integral-area" => "Integral (área)",
        "taylor-series" => "Serie de Taylor",
        "conformal-map" => "Mapa conforme",
        "pitagoras" | "pythagoras" => "Pitágoras (triángulo)",
        "euler" => "Euler (e^x)",
        "fourier" => "Fourier (ondas)",
        "logistic-bifurcation" => "Bifurcación logística",
        "gradient-field" => "Campo de gradiente",
        "mobius-transform" => "Möbius (conforme)",
        "universal" => "Universal (auto)",
        _ => "Derivada (pendiente)",
    }
}

/// Plantillas ofrecidas en el ComboBox (las 11 canónicas nativas).
const PLANTILLAS_COMBO: &[&str] = &[
    "derivative-slope",
    "integral-area",
    "taylor-series",
    "conformal-map",
    "pitagoras",
    "euler",
    "fourier",
    "logistic-bifurcation",
    "gradient-field",
    "mobius-transform",
    "universal",
];

/// Ancho de la barra de progreso: todo el ancho disponible, con techo de token.
/// Nunca supera el panel (si el panel es angosto, usa lo que hay).
pub fn ancho_barra_para(ancho_disponible: f32) -> f32 {
    let disponible = ancho_disponible.max(0.0);
    let piso = tokens::SPACE_XXL * 2.0;
    let techo = tokens::DRAWER_RIGHT_MAX;
    if disponible < piso {
        disponible
    } else {
        disponible.min(techo)
    }
}

/// Alto de la barra de progreso (tokens, sin hardcode).
pub fn alto_barra() -> f32 {
    tokens::SPACE_LG + tokens::SPACE_XS
}

/// Tamaño de la vista previa 16:9 con encajonado: nunca excede el panel.
pub fn tamano_vista_para(ancho_disponible: f32) -> (f32, f32) {
    let disponible = ancho_disponible.max(0.0);
    let piso = tokens::SPACE_XXL * 2.0;
    let techo = tokens::DRAWER_RIGHT_MAX;
    let ancho = if disponible < piso {
        disponible
    } else {
        disponible.min(techo)
    };
    let alto_sin_cota = ancho * VISTA_REL_ALTO / VISTA_REL_ANCHO;
    let alto_min = tokens::SPACE_XL * 2.0;
    let alto_max = tokens::SPACE_XXL * 6.0;
    let alto = if alto_sin_cota < alto_min {
        alto_sin_cota
    } else {
        alto_sin_cota.min(alto_max)
    };
    (ancho, alto)
}

/// Tamaño de cada miniatura 4:3: fracción del ancho, con piso y techo de tokens.
pub fn tamano_miniatura_para(ancho_disponible: f32) -> (f32, f32) {
    let disponible = ancho_disponible.max(0.0);
    let piso = tokens::SPACE_XXL + tokens::SPACE_LG;
    let techo = tokens::SPACE_XXL * 2.0 + tokens::SPACE_LG;
    let crudo = disponible * MINI_FRACCION_ANCHO;
    let mut ancho = crudo.min(techo).min(disponible);
    if disponible >= piso {
        ancho = ancho.max(piso);
    }
    let alto = ancho * MINI_REL_ALTO / MINI_REL_ANCHO;
    (ancho, alto)
}

/// Altura máxima de la tira de miniaturas (tokens, sin hardcode).
pub fn alto_tira() -> f32 {
    tokens::SPACE_XXL * 2.0 + tokens::SPACE_XS
}

/// Cáscara egui del panel (AS4 cableado): dibuja desde `&Estado` y devuelve los
/// eventos con efecto para que `app.rs` los ejecute en hilos (`Generate` /
/// `Regenerate` → `assistant.rs` con token cancelable, `Export` → hilo
/// `spawn_gif_export`). Los locales (play, deslizador, plantilla, vaciar) ya
/// quedaron aplicados por el reductor puro antes de devolverse. Sin E/S ni
/// `spawn` aquí: solo `ctx` para texturas y repaints acotados.
pub fn draw_anim_panel(ui: &mut egui::Ui, state: &mut AnimPreviewState) -> Vec<AnimPanelEvent> {
    let mut eventos: Vec<AnimPanelEvent> = Vec::new();
    let disponible = ui.available_width().max(0.0);
    let barra_ancho = ancho_barra_para(disponible);
    let barra_alto = alto_barra();
    let (vista_ancho, vista_alto) = tamano_vista_para(disponible);
    let (mini_ancho, mini_alto) = tamano_miniatura_para(disponible);
    let tira_max = alto_tira();

    ui.heading("Animaciones didácticas");
    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Plantilla:");
        // Vía reductor puro: el ComboBox escribe un temporal y solo el cambio
        // real viaja como `SelectTemplate` (mismo resultado que antes).
        let mut elegido = if state.template.is_empty() {
            "derivative-slope".to_string()
        } else {
            state.template.clone()
        };
        egui::ComboBox::from_id_salt("anim_template")
            .selected_text(nombre_plantilla(&elegido))
            .show_ui(ui, |ui| {
                for id in PLANTILLAS_COMBO {
                    ui.selectable_value(&mut elegido, (*id).to_string(), nombre_plantilla(id));
                }
            });
        if elegido != state.template {
            eventos.extend(apply_anim_panel_action(
                state,
                AnimPanelAction::SelectTemplate(elegido),
            ));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Concepto:");
        ui.text_edit_singleline(&mut state.concept);
    });
    ui.add_space(tokens::SPACE_SM);

    // ── Params vivos v3 ────────────────────────────────────────────────
    // Sliders que alimentan `live_params` → `build_anim_params().params` →
    // re-render nativo (`render_anim_for_concept_with_params`).
    // TODO(v3-auto-rerender): el re-render automático al mover el scrub exige
    // cablear `assistant.rs` (hilo de generación, fuera del scope ANIM); hoy
    // el badge avisa y la próxima generación ya usa los params.
    {
        let t = state.template.clone();
        let mut x0 = state.live_params.get("x0").copied().unwrap_or(1.0);
        let mut a = state.live_params.get("a").copied().unwrap_or(0.0);
        let mut b = state.live_params.get("b").copied().unwrap_or(2.0);
        let terms_def = if t == "fourier" { 6.0 } else { 7.0 };
        let mut terms = state.live_params.get("terms").copied().unwrap_or(terms_def);
        egui::CollapsingHeader::new("Parámetros vivos (x0 · a · b · terms)")
            .default_open(false)
            .show(ui, |ui| {
                if ui
                    .add(egui::Slider::new(&mut x0, -3.0..=3.0).text("x0 (centro)"))
                    .changed()
                {
                    state.set_live_param("x0", x0);
                }
                if ui
                    .add(egui::Slider::new(&mut a, -3.0..=3.0).text("a (cota inf)"))
                    .changed()
                {
                    state.set_live_param("a", a);
                }
                if ui
                    .add(egui::Slider::new(&mut b, -3.0..=3.0).text("b (cota sup)"))
                    .changed()
                {
                    state.set_live_param("b", b);
                }
                if ui
                    .add(egui::Slider::new(&mut terms, 1.0..=7.0).text("terms (euler/fourier)"))
                    .changed()
                {
                    state.set_live_param("terms", terms);
                }
                if state.params_dirty {
                    ui.label(
                        egui::RichText::new("Parámetros modificados: se aplicarán al regenerar.")
                            .size(tokens::TYPE_SM)
                            .color(Color32::GRAY),
                    );
                    if ui.button("Entendido").clicked() {
                        state.mark_params_applied();
                    }
                }
            });
    }
    ui.add_space(tokens::SPACE_SM);

    // ── Estado (cada uno con dibujo acotado) ─────────────────────────────
    match estado_vista(state) {
        EstadoVista::Generando => {
            ui.label(
                egui::RichText::new("Generando animación…")
                    .size(tokens::TYPE_SM)
                    .color(Color32::GRAY),
            );
            ui.add_sized(
                [barra_ancho, barra_alto],
                ProgressBar::new(state.progress.clamp(0.0, 1.0)).text(&state.status),
            );
            ui.label(
                egui::RichText::new(&state.status)
                    .color(Color32::GRAY)
                    .size(tokens::TYPE_SM),
            );
        }
        EstadoVista::Vacia => {
            ui.label(
                egui::RichText::new(
                    "Todavía no hay fotogramas. Pedí al asistente que explique un concepto con animación.",
                )
                .size(tokens::TYPE_SM)
                .color(Color32::GRAY),
            );
            // Tras Generar/Regenerar el progreso real aún no llegó (lo mueve el
            // hilo de `assistant.rs`): se muestra el estado solicitado, sin inventar %.
            if !state.status.is_empty() {
                ui.label(
                    egui::RichText::new(&state.status)
                        .size(tokens::TYPE_SM)
                        .color(Color32::GRAY),
                );
            }
        }
        EstadoVista::Fallo => {
            ui.label(
                egui::RichText::new("No se pudo generar la animación.")
                    .size(tokens::TYPE_SM)
                    .color(Color32::DARK_RED)
                    .strong(),
            );
            ui.label(
                egui::RichText::new(&state.status)
                    .size(tokens::TYPE_SM)
                    .color(Color32::DARK_RED),
            );
            ui.small("Revisá el concepto e intentá de nuevo.");
        }
        EstadoVista::Lista => {
            if let Some(ruta) = state.media_path.as_ref() {
                ui.label(egui::RichText::new(format!("Listo: {ruta}")).size(tokens::TYPE_SM));
            } else if state.playing {
                ui.label(egui::RichText::new("Reproduciendo…").size(tokens::TYPE_SM));
            } else {
                ui.label(egui::RichText::new("En pausa.").size(tokens::TYPE_SM));
            }
        }
    }
    ui.add_space(tokens::SPACE_SM);

    // ── Controles: siempre visibles, con motivo cuando no aplican ─────────
    // Toda la lógica vive en el reductor puro (`apply_anim_panel_action`); la
    // cáscara reenvía clicks y devuelve los eventos con efecto para `app.rs`
    // (`Generate`/`Regenerate` → hilo `assistant.rs` con token,
    // `Cancel { was_generating: true }` → señala ese token, `Export` → hilo
    // `spawn_gif_export`).
    let puede_reproducir = !state.frames.is_empty();
    let puede_cancelar = can_cancel(state);
    let puede_exportar = can_export(state);
    let puede_generar = can_generate(state);
    let puede_vaciar =
        !state.frames.is_empty() || state.media_path.is_some() || !state.status.is_empty();
    ui.horizontal_wrapped(|ui| {
        let resp_gen = ui.add_enabled(puede_generar, egui::Button::new("▶ Generar"));
        if resp_gen.clicked() {
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::Generate));
        }
        if !puede_generar {
            resp_gen.on_disabled_hover_text("Escribí un concepto (1..200 chars) para generar.");
        }
        let resp_regen = ui.add_enabled(puede_generar, egui::Button::new("↻ Regenerar"));
        if resp_regen.clicked() {
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::Regenerate));
        }
        if !puede_generar {
            resp_regen.on_disabled_hover_text("Escribí un concepto (1..200 chars) para regenerar.");
        }
        let etiqueta = if state.playing {
            "⏸ Pausar"
        } else {
            "▶ Reproducir"
        };
        let resp_repro = ui.add_enabled(puede_reproducir, egui::Button::new(etiqueta));
        if resp_repro.clicked() {
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::TogglePlay));
        }
        if !puede_reproducir {
            resp_repro.on_disabled_hover_text("Sin fotogramas: primero generá una animación.");
        }
        let resp_cancela = ui.add_enabled(puede_cancelar, egui::Button::new("✕ Cancelar"));
        if resp_cancela.clicked() {
            // `CancelRequested { was_generating: true }` lo propaga `app.rs`
            // al token que observa `run_job` (ver `engine.rs`) y el closure
            // de progreso nativo.
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::Cancel));
        }
        if !puede_cancelar {
            resp_cancela.on_disabled_hover_text("No hay generación ni reproducción en curso.");
        }
        let resp_exporta = ui.add_enabled(puede_exportar, egui::Button::new("⤓ Exportar"));
        if resp_exporta.clicked() {
            // `ExportRequested` lo ejecuta `app.rs` en el hilo de E/S
            // (`spawn_gif_export`); `media_path` vacío = codificar los frames
            // actuales (nativo sin archivo aún).
            // NOTA(v3-webm): formato webm + selector de fps solo si ffmpeg ya
            // está cableado (hoy el worker solo escribe gif/png/mp4; ver
            // protocolo `ExportFormat`). Sin ffmpeg, no prometer webm en la UI.
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::Export));
        }
        if !puede_exportar {
            resp_exporta
                .on_disabled_hover_text("Todavía no hay fotogramas ni archivo para exportar.");
        }
        let resp_vacia = ui.add_enabled(puede_vaciar, egui::Button::new("🗑 Vaciar"));
        if resp_vacia.clicked() {
            // El reductor puro no tiene ctx: se olvidan texturas GPU primero.
            state.olvida_texturas(ui.ctx());
            eventos.extend(apply_anim_panel_action(state, AnimPanelAction::Clear));
        }
        if !puede_vaciar {
            resp_vacia.on_disabled_hover_text("No hay nada para vaciar.");
        }
    });
    if !puede_reproducir {
        ui.small("Reproducción desactivada: aún no hay fotogramas.");
    }
    ui.add_space(tokens::SPACE_SM);

    // ── Deslizador: siempre visible, sin pánicos con lista vacía ──────────
    let total = state.frames.len();
    if total == 0 {
        let mut fijo = 0usize;
        let resp = ui.add_enabled(
            false,
            egui::Slider::new(&mut fijo, 0..=0).text("Sin fotogramas"),
        );
        resp.on_disabled_hover_text("El deslizador se activa al llegar el primer fotograma.");
    } else {
        if state.frame_idx >= total {
            state.frame_idx = 0;
        }
        state.ensure_textures(ui.ctx());
        let max_idx = total.saturating_sub(1);
        let mut idx = state.frame_idx.min(max_idx);
        let etiqueta = format!("Fotograma {} de {total}", idx + 1);
        ui.horizontal(|ui| {
            ui.label("Fotograma:");
            if ui
                .add(egui::Slider::new(&mut idx, 0..=max_idx).text(etiqueta))
                .changed()
            {
                // Vía reductor puro: clampea, pausa y sincroniza el scrub.
                // `FrameSelected` lo usa `app.rs` solo para repintar el preview.
                eventos.extend(apply_anim_panel_action(
                    state,
                    AnimPanelAction::SelectFrame(idx),
                ));
            } else {
                state.frame_idx = idx;
                if state.playing {
                    state.sync_phase_from_frame();
                }
            }
        });
        // ── Timeline v3: scrub con easing, vinculado al deslizador ──────
        ui.horizontal_wrapped(|ui| {
            ui.label("Timeline:");
            if state.timeline.is_none() {
                let resp = ui.button("＋ Crear scrub");
                if resp.clicked() {
                    state.ensure_default_timeline(TIMELINE_DEFAULT_MS);
                }
                resp.on_hover_text("Crea un scrub 0→1 con easing sobre los fotogramas.");
            } else if ui.button("✕ Quitar scrub").clicked() {
                state.clear_timeline();
            }
        });
        if state.timeline.is_some() {
            let mut actual = state.timeline_easing.clone();
            ui.horizontal(|ui| {
                ui.label("Easing:");
                egui::ComboBox::from_id_salt("anim_easing")
                    .selected_text(actual.clone())
                    .show_ui(ui, |ui| {
                        for name in EASING_NAMES {
                            ui.selectable_value(&mut actual, (*name).to_string(), *name);
                        }
                    });
            });
            if actual != state.timeline_easing {
                state.set_timeline_easing(&actual);
            }
            let mut phase = state.timeline_phase.clamp(0.0, 1.0);
            if ui
                .add(egui::Slider::new(&mut phase, 0.0..=1.0).text("Scrub"))
                .changed()
            {
                state.timeline_phase = phase;
                if let Some(idx) = state.timeline_frame(total) {
                    state.frame_idx = idx;
                    state.playing = false; // el scrub pausa
                }
            } else {
                state.timeline_phase = phase;
            }
            if let Some(idx) = state.timeline_frame(total) {
                ui.small(format!(
                    "Scrub → fotograma {} de {total} (easing {}).",
                    idx + 1,
                    state.timeline_easing
                ));
            }
        }
    }

    // ── Vista previa 16:9 con encajonado (nunca excede el panel) ──────────
    if !state.frames.is_empty() && vista_ancho > 0.0 && vista_alto > 0.0 {
        ui.add_space(tokens::SPACE_XS);
        let marco = egui::vec2(vista_ancho, vista_alto);
        let (rect, _) = ui.allocate_exact_size(marco, egui::Sense::hover());
        let inicio = state.inicio_ventana();
        let pos = state.frame_idx.saturating_sub(inicio);
        if let Some(tex) = state.cached_textures.get(pos) {
            let (fw, fh) = state
                .frames
                .get(state.frame_idx)
                .map(|f| (f.size[0] as f32, f.size[1] as f32))
                .unwrap_or((VISTA_REL_ANCHO, VISTA_REL_ALTO));
            if fw > 0.0 && fh > 0.0 {
                let escala = (rect.width() / fw).min(rect.height() / fh);
                let tam = egui::vec2(fw * escala, fh * escala);
                let dentro = egui::Rect::from_center_size(rect.center(), tam);
                ui.painter().image(
                    tex.id(),
                    dentro,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            ui.painter().rect_stroke(
                rect,
                tokens::RADIUS_SM,
                Stroke::new(tokens::SPACE_XS * 0.25, Color32::GRAY),
            );
        } else {
            ui.painter().rect_stroke(
                rect,
                tokens::RADIUS_SM,
                Stroke::new(tokens::SPACE_XS * 0.25, Color32::GRAY),
            );
        }
        let actual = state.frame_idx.saturating_add(1).min(total.max(1));
        ui.small(format!("Vista previa del fotograma {actual} de {total}."));
        ui.add_space(tokens::SPACE_XS);

        // ── Tira paginada (única ScrollArea; ventana acotada) ─────────────
        let inicio = state.inicio_ventana();
        let fin = state.fin_ventana();
        let total_pags = state.total_paginas().max(1);
        let pag = state.pagina_actual().saturating_add(1).min(total_pags);
        ui.horizontal_wrapped(|ui| {
            ui.small(format!(
                "Mostrando {}–{} de {total} · página {pag} de {total_pags}",
                inicio.saturating_add(1),
                fin,
            ));
            let resp_ant = ui.add_enabled(inicio > 0, egui::Button::new("← Anterior"));
            if resp_ant.clicked() {
                state.frame_idx = inicio.saturating_sub(MINIATURAS_POR_PAGINA);
                state.playing = false;
            }
            if inicio == 0 {
                resp_ant.on_disabled_hover_text("Ya estás en la primera página.");
            }
            let resp_sig = ui.add_enabled(fin < total, egui::Button::new("Siguiente →"));
            if resp_sig.clicked() {
                state.frame_idx = fin.min(total.saturating_sub(1));
                state.playing = false;
            }
            if fin >= total {
                resp_sig.on_disabled_hover_text("Ya estás en la última página.");
            }
        });
        ScrollArea::horizontal()
            .max_height(tira_max)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (pos, tex) in state.cached_textures.iter().enumerate() {
                        let global = inicio.saturating_add(pos);
                        let tinta = if global == state.frame_idx {
                            Color32::WHITE
                        } else {
                            Color32::from_white_alpha(180)
                        };
                        ui.add(
                            egui::Image::new((tex.id(), egui::vec2(mini_ancho, mini_alto)))
                                .tint(tinta),
                        );
                    }
                });
            });
        ui.small("Usá el deslizador para elegir el fotograma.");
        // 12fps repaint (~83ms) si está reproduciendo.
        if state.playing {
            state.tick_playback();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(REPRODUCCION_MS));
        }
    } else if !state.cached_textures.is_empty() {
        // Asegura limpieza si los fotogramas se vaciaron desde afuera.
        let ctx = ui.ctx().clone();
        state.olvida_texturas(&ctx);
    }
    // Repintado suave 12fps si hay generación activa (además de reproducir).
    if state.is_active() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(REPRODUCCION_MS));
    }
    ui.separator();
    ui.small(
        "Consejo: pedí al asistente «explica la derivada con animación» y controlá aquí el avance.",
    );
    eventos
}

pub fn build_anim_params(state: &AnimPreviewState) -> Result<AnimParams, String> {
    let template = if state.template.is_empty() {
        "derivative-slope".to_string()
    } else {
        state.template.clone()
    };
    let concept = state.concept.trim().to_string();
    if concept.is_empty() {
        return Err("concepto vacío".into());
    }
    if concept.len() > 200 {
        return Err("concepto demasiado largo (max 200)".into());
    }
    let mut params = BTreeMap::new();
    // Params vivos v3: lo que movieron los sliders (solo finitos). Sin ellos,
    // x0=1.0 histórico para no cambiar el comportamiento previo.
    for (k, v) in &state.live_params {
        if v.is_finite() {
            params.insert(k.clone(), *v);
        }
    }
    if !params.contains_key("x0") {
        params.insert("x0".to_string(), 1.0);
    }
    let anim_params = AnimParams {
        template,
        concept,
        params,
        duration: AnimDuration::try_new(2.0).unwrap_or_default(),
        resolution: Resolution::try_new(640, 480).unwrap_or_default(),
        export: ExportFormat::Gif,
        spec: None,
    };
    anim_params.validate().map_err(|e| e.to_string())?;
    Ok(anim_params)
}

// ── Panel puro headless v4 (ANIM-REVIVE + AS4 cableado) ────────────────────
// `draw_anim_panel` lo llama `GrafitoApp::draw_anim_preview_panel` (`app.rs`,
// ventana compañera del asistente). TODA la lógica vive aquí como reductor
// puro testeable headless:
//
//   `apply_anim_panel_action(&mut state, action) -> Vec<AnimPanelEvent>`
//
// Sin `egui::Context`, sin E/S, sin `spawn`: dada una acción
// (Generar/Regenerar/Cancelar/Exportar/…) muta el `state` y devuelve eventos.
// `draw_anim_panel` es la cáscara egui que reenvía cada click al reductor y
// devuelve los eventos con efecto para `app.rs` (ver `AnimPanelEvent` y
// `can_*`). Generar limpia (arranque fresco); Regenerar conserva los frames
// visibles hasta que lleguen los nuevos. Ningún evento inventa progreso: el
// hilo de generación (`assistant.rs`) es el único que mueve `progress` con
// datos reales del worker.

/// Acción del usuario sobre el panel (pura, sin egui ni E/S).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimPanelAction {
    /// Arranque fresco: valida params, limpia frames/media y pide generar.
    Generate,
    /// Nueva generación con los params vivos: conserva frames hasta el relevo.
    Regenerate,
    /// Cancela generación en curso o detiene reproducción.
    Cancel,
    /// Pide exportar el archivo ya listo (sin efecto aquí).
    Export,
    /// Alterna reproducción/pausa (no-op sin frames).
    TogglePlay,
    /// Vacía el estado (la cáscara egui olvida texturas GPU antes).
    Clear,
    /// Elige fotograma (clampea; pausa y sincroniza el scrub).
    SelectFrame(usize),
    /// Elige plantilla (ignora vacío o sin cambio).
    SelectTemplate(String),
}

/// Evento emitido por el reductor (puro, sin efectos).
///
/// Los que requieren efecto (`GenerateRequested`, `RegenerateRequested`,
/// `ExportRequested`, `CancelRequested` con generación en curso) los ejecuta
/// `app.rs::handle_anim_panel_events`: hilo de generación en `assistant.rs`
/// (paramétrico `render_parametric_frames_with_progress` o clásico
/// `render_anim_with_progress`, con token cancelable) y `spawn_gif_export`
/// para el worker externo (`run_job` con closure `cancel`).
/// `ExportRequested` con `media_path` vacío = codificar los frames actuales
/// (nativo sin archivo aún); con ruta = archivo ya listo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimPanelEvent {
    GenerateRequested { template: String, concept: String },
    RegenerateRequested { template: String, concept: String },
    CancelRequested { was_generating: bool },
    ExportRequested { media_path: String },
    PlaybackStarted,
    PlaybackPaused,
    Cleared,
    FrameSelected(usize),
    TemplateSelected(String),
    ValidationFailed(String),
}

/// ¿Hay params válidos para generar? (concepto no vacío, ≤200 chars).
pub fn can_generate(state: &AnimPreviewState) -> bool {
    build_anim_params(state).is_ok()
}

/// ¿Hay algo que cancelar? (generación activa o reproduciendo).
pub fn can_cancel(state: &AnimPreviewState) -> bool {
    state.is_active() || state.playing
}

/// ¿Hay algo exportable? Archivo listo o frames actuales para codificar
/// (AS4: el nativo genera frames sin archivo; `Export` los codifica a GIF).
pub fn can_export(state: &AnimPreviewState) -> bool {
    state.media_path.is_some() || !state.frames.is_empty()
}

/// Reductor puro del panel: aplica `action` sobre `state` y devuelve eventos.
///
/// Headless y total: cubre Generar/Regenerar/Cancelar/Exportar más los
/// controles locales (play, vaciar, deslizador, plantilla). Los eventos con
/// efecto se devuelven para que `app.rs` los ejecute; aquí nunca hay E/S.
pub fn apply_anim_panel_action(
    state: &mut AnimPreviewState,
    action: AnimPanelAction,
) -> Vec<AnimPanelEvent> {
    match action {
        AnimPanelAction::Generate => match build_anim_params(state) {
            Ok(params) => {
                state.frames.clear();
                state.media_path = None;
                state.frame_idx = 0;
                state.playing = false;
                state.progress = 0.0;
                state.status = "Generación solicitada: esperando al motor…".to_string();
                state.mark_params_applied();
                vec![AnimPanelEvent::GenerateRequested {
                    template: params.template,
                    concept: params.concept,
                }]
            }
            Err(reason) => {
                state.status = format!("Error: {reason}");
                vec![AnimPanelEvent::ValidationFailed(reason)]
            }
        },
        AnimPanelAction::Regenerate => match build_anim_params(state) {
            Ok(params) => {
                // A diferencia de Generate, conserva frames/media visibles.
                state.frame_idx = 0;
                state.playing = false;
                state.progress = 0.0;
                state.status = "Regenerando con los parámetros actuales…".to_string();
                state.mark_params_applied();
                vec![AnimPanelEvent::RegenerateRequested {
                    template: params.template,
                    concept: params.concept,
                }]
            }
            Err(reason) => {
                state.status = format!("Error: {reason}");
                vec![AnimPanelEvent::ValidationFailed(reason)]
            }
        },
        AnimPanelAction::Cancel => {
            if !can_cancel(state) {
                return Vec::new();
            }
            let was_generating = state.is_active();
            state.playing = false;
            if was_generating {
                state.progress = 0.0;
                state.status = "Generación cancelada por el usuario.".to_string();
            } else {
                state.status = "Reproducción detenida.".to_string();
            }
            vec![AnimPanelEvent::CancelRequested { was_generating }]
        }
        AnimPanelAction::Export => match state.media_path.clone() {
            Some(media_path) => vec![AnimPanelEvent::ExportRequested { media_path }],
            // Sin archivo pero con frames (nativo AS4): pide codificar el set
            // actual; `app.rs` elige destino y corre `spawn_gif_export`.
            // `media_path` vacío distingue este caso en el manejador.
            // Sin nada: no-op (la cáscara deshabilita vía `can_export`).
            None if !state.frames.is_empty() => {
                vec![AnimPanelEvent::ExportRequested {
                    media_path: String::new(),
                }]
            }
            None => Vec::new(),
        },
        AnimPanelAction::TogglePlay => {
            if state.frames.is_empty() {
                return Vec::new();
            }
            state.playing = !state.playing;
            vec![if state.playing {
                AnimPanelEvent::PlaybackStarted
            } else {
                AnimPanelEvent::PlaybackPaused
            }]
        }
        AnimPanelAction::Clear => {
            // Headless: `clear()` suelta los `TextureHandle` (su Drop libera).
            // La cáscara egui olvida además por id antes de llamar aquí.
            state.clear();
            vec![AnimPanelEvent::Cleared]
        }
        AnimPanelAction::SelectFrame(idx) => {
            if state.frames.is_empty() {
                return Vec::new();
            }
            let clamped = idx.min(state.frames.len() - 1);
            state.frame_idx = clamped;
            state.playing = false;
            state.sync_phase_from_frame();
            vec![AnimPanelEvent::FrameSelected(clamped)]
        }
        AnimPanelAction::SelectTemplate(name) => {
            let name = name.trim().to_string();
            if name.is_empty() || name == state.template {
                return Vec::new();
            }
            state.template = name.clone();
            vec![AnimPanelEvent::TemplateSelected(name)]
        }
    }
}

// ── AS4: vista previa paramétrica con transporte real y OOM acotado ─────
// El transporte YA es real (`TogglePlay`/`SelectFrame`/`tick_playback` + la
// ventana de 8 texturas `MINIATURAS_POR_PAGINA`); aquí se suma la entrada
// paramétrica: `set_parametric_frames` instala el set con guardas honestas
// (OOM vía `estimate_frames_bytes`, conteo y tamaño) y deja estado visible
// (`parametric_transport_status`) con prosa humanizada — nombres del mapa
// `humanize_control_name` (deslizador, reproducir, pausar), jamás
// identificadores literales. Sin E/S, sin spawn: el render corre en el hilo
// de generación de `assistant.rs` con `render_parametric_frames_with_progress`
// (ver `run_assistant_animation_with` + `install_anim_preview_frames`).
use crate::anim_native::estimate_frames_bytes;
use grafito_anim::parametric::{parametric_hint, ParametricAnim, PARAMETRIC_MAX_BYTES};

/// Resumen de una vista previa paramétrica instalada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametricPreviewInfo {
    pub frames: usize,
    pub bytes: usize,
}

/// Instala el set paramétrico en el estado con guardas honestas.
///
/// - Vacío → `Err` ("sin fotogramas…").
/// - OOM: `estimate_frames_bytes(w, h, n)` (`None` o > tope) → `Err` honesto.
/// - Conteo distinto de `anim.frame_count()` o tamaño distinto del viewport
///   → `Err` honesto (no se instala a medias).
/// - Ok: resetea `frame_idx`, pausa, marca progreso completo, pone la pista
///   humanizada en `status` y conserva la plantilla/concepto para el combo.
pub fn set_parametric_frames(
    state: &mut AnimPreviewState,
    anim: &ParametricAnim,
    frames: Vec<egui::ColorImage>,
) -> Result<ParametricPreviewInfo, String> {
    if frames.is_empty() {
        return Err("sin fotogramas para la vista previa".to_string());
    }
    let (w, h) = (anim.viewport.width as usize, anim.viewport.height as usize);
    let n = frames.len();
    match estimate_frames_bytes(w, h, n) {
        Some(got) if got <= PARAMETRIC_MAX_BYTES => {}
        Some(got) => {
            return Err(format!(
                "el set estimado ({got} bytes) excede el tope de {PARAMETRIC_MAX_BYTES} bytes: bajá la resolución o los fotogramas"
            ));
        }
        None => {
            return Err(format!(
                "el set estimado desborda el contador: bajá la resolución o los fotogramas (tope {PARAMETRIC_MAX_BYTES} bytes)"
            ));
        }
    }
    if n != anim.frame_count() {
        return Err(format!(
            "el set trae {n} fotogramas pero la animación pide {}",
            anim.frame_count()
        ));
    }
    for (i, f) in frames.iter().enumerate() {
        if f.size != [w, h] {
            return Err(format!(
                "el fotograma {i} mide {:?}, esperaba [{w}, {h}]",
                f.size
            ));
        }
    }
    let bytes = estimate_frames_bytes(w, h, n).unwrap_or(0);
    state.frames = frames;
    state.frame_idx = 0;
    state.playing = false;
    state.progress = 1.0;
    state.media_path = None;
    state.template = anim.kind.as_str().to_string();
    state.concept = anim.expr_a.clone();
    state.status = parametric_hint(anim);
    state.mark_params_applied();
    Ok(ParametricPreviewInfo { frames: n, bytes })
}

/// Estado visible del transporte para el chat (prosa humanizada).
///
/// Ej: "fotograma 3 de 24 · en pausa — mové el deslizador para elegir
/// fotograma". Sin fotogramas indica cómo generar. Nunca contiene
/// identificadores literales de control.
pub fn parametric_transport_status(state: &AnimPreviewState) -> String {
    let total = state.frames.len();
    if total == 0 {
        return "sin fotogramas: generá una animación para ver la vista previa".to_string();
    }
    let idx = state.frame_idx.min(total - 1) + 1;
    let modo = if state.playing {
        "reproduciendo"
    } else {
        "en pausa"
    };
    format!("fotograma {idx} de {total} · {modo} — mové el deslizador para elegir fotograma")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_state_lifecycle() {
        let mut s = AnimPreviewState::default();
        assert!(!s.is_active());
        s.progress = 0.5;
        assert!(s.is_active());
        s.clear();
        assert_eq!(s.progress, 0.0);
    }
    #[test]
    fn build_params_valid() {
        let state = AnimPreviewState {
            template: "derivative-slope".into(),
            concept: "derivada".into(),
            ..Default::default()
        };
        assert!(build_anim_params(&state).is_ok());
    }
    #[test]
    fn build_params_empty_concept_fails() {
        let state = AnimPreviewState::default();
        assert!(build_anim_params(&state).is_err());
    }
    #[test]
    fn estados_se_deducen_sin_panico_vacio() {
        let vacio = AnimPreviewState::default();
        assert_eq!(estado_vista(&vacio), EstadoVista::Vacia);
        // Deslizador con lista vacía: rango 0..=0 nunca en panico.
        let max_idx = vacio.frames.len().saturating_sub(1);
        assert_eq!(max_idx, 0);
    }
    #[test]
    fn estado_fallo_solo_con_mensaje_en_espanol() {
        let s = AnimPreviewState {
            status: "Error: concepto vacío".to_string(),
            ..Default::default()
        };
        assert_eq!(estado_vista(&s), EstadoVista::Fallo);
        assert!(es_estado_error("falló la generación"));
        assert!(!es_estado_error("Generación cancelada por el usuario."));
    }
    #[test]
    fn estado_generando_y_lista() {
        let mut s = AnimPreviewState {
            progress: 0.4,
            ..Default::default()
        };
        assert_eq!(estado_vista(&s), EstadoVista::Generando);
        s.progress = 0.0;
        s.media_path = Some("/tmp/a.gif".to_string());
        assert_eq!(estado_vista(&s), EstadoVista::Lista);
    }
    #[test]
    fn paginacion_acota_la_ventana() {
        let mut s = AnimPreviewState {
            frames: vec![egui::ColorImage::new([2, 2], Color32::BLACK); 20],
            ..Default::default()
        };
        assert_eq!(s.inicio_ventana(), 0);
        assert_eq!(s.fin_ventana(), MINIATURAS_POR_PAGINA);
        assert_eq!(s.total_paginas(), 3);
        s.frame_idx = 19;
        assert_eq!(s.inicio_ventana(), 16);
        assert_eq!(s.fin_ventana(), 20);
        // La ventana siempre contiene al índice actual.
        assert!(s.frame_idx >= s.inicio_ventana() && s.frame_idx < s.fin_ventana());
    }
    #[test]
    fn medidas_nunca_exceden_el_panel() {
        // Panel angosto: todo se encoge, nada desborda.
        assert_eq!(ancho_barra_para(0.0), 0.0);
        assert_eq!(ancho_barra_para(50.0), 50.0);
        // Panel típico: usa todo el ancho disponible.
        assert_eq!(ancho_barra_para(400.0), 400.0);
        // Panel enorme: techo de token (DRAWER_RIGHT_MAX 440).
        assert_eq!(ancho_barra_para(2000.0), tokens::DRAWER_RIGHT_MAX);
        let (vw, vh) = tamano_vista_para(400.0);
        assert_eq!(vw, 400.0);
        assert!((vh - 225.0).abs() < 0.01);
        assert!(vw <= tokens::DRAWER_RIGHT_MAX);
        assert!(vh <= tokens::SPACE_XXL * 6.0);
        let (mw, mh) = tamano_miniatura_para(400.0);
        assert_eq!(mw, tokens::SPACE_XXL * 2.0 + tokens::SPACE_LG);
        assert!((mh - mw * 3.0 / 4.0).abs() < 0.01);
        assert_eq!(alto_tira(), tokens::SPACE_XXL * 2.0 + tokens::SPACE_XS);
        assert_eq!(alto_barra(), tokens::SPACE_LG + tokens::SPACE_XS);
    }
    #[test]
    fn etiquetas_de_plantilla_en_espanol() {
        assert_eq!(nombre_plantilla("derivative-slope"), "Derivada (pendiente)");
        assert_eq!(nombre_plantilla("integral-area"), "Integral (área)");
        assert_eq!(nombre_plantilla("taylor-series"), "Serie de Taylor");
        assert_eq!(nombre_plantilla("conformal-map"), "Mapa conforme");
        // Sin inglés a la vista: nada de "slope", "area", "series" ni "map".
        for id in [
            "derivative-slope",
            "integral-area",
            "taylor-series",
            "conformal-map",
        ] {
            let nombre = nombre_plantilla(id);
            assert!(!nombre.contains("slope"));
            assert!(!nombre.contains("series"));
        }
    }

    #[test]
    fn combo_cubre_las_once_canonicas() {
        assert_eq!(PLANTILLAS_COMBO.len(), 11, "sync v3 con NATIVE_TEMPLATES");
        // La que faltaba: pitagoras ya no cae al fallback inglés.
        assert!(PLANTILLAS_COMBO.contains(&"pitagoras"));
        for id in PLANTILLAS_COMBO {
            assert_ne!(nombre_plantilla(id), "", "{id} con nombre");
            assert!(!nombre_plantilla(id).contains("slope"), "{id}");
        }
        assert_eq!(
            nombre_plantilla("logistic-bifurcation"),
            "Bifurcación logística"
        );
        assert_eq!(nombre_plantilla("gradient-field"), "Campo de gradiente");
        assert_eq!(nombre_plantilla("mobius-transform"), "Möbius (conforme)");
        assert_eq!(nombre_plantilla("universal"), "Universal (auto)");
    }

    // ── v3: params vivos ───────────────────────────────────────────────
    #[test]
    fn live_params_marcan_dirty_y_filtran_no_finitos() {
        let mut s = AnimPreviewState::default();
        assert!(!s.params_dirty);
        s.set_live_param("x0", 2.0);
        assert!(s.params_dirty);
        assert_eq!(s.live_params.get("x0"), Some(&2.0));
        // Mismo valor: no re-marca (pero sigue dirty de antes).
        s.mark_params_applied();
        assert!(!s.params_dirty);
        s.set_live_param("x0", 2.0);
        assert!(!s.params_dirty, "mismo valor no ensucia");
        // NaN/inf se ignoran.
        s.set_live_param("x0", f64::NAN);
        s.set_live_param("a", f64::INFINITY);
        assert_eq!(s.live_params.get("x0"), Some(&2.0));
        assert!(!s.live_params.contains_key("a"));
        assert!(!s.params_dirty);
    }

    #[test]
    fn build_params_usa_los_vivos_con_x0_historico() {
        let mut s = AnimPreviewState {
            template: "derivative-slope".into(),
            concept: "derivada".into(),
            ..Default::default()
        };
        // Sin tocar sliders: x0=1.0 histórico.
        let p = build_anim_params(&s).unwrap();
        assert_eq!(p.params.get("x0"), Some(&1.0));
        // Sliders vivos viajan al request.
        s.set_live_param("x0", 2.5);
        s.set_live_param("terms", 3.0);
        let p2 = build_anim_params(&s).unwrap();
        assert_eq!(p2.params.get("x0"), Some(&2.5));
        assert_eq!(p2.params.get("terms"), Some(&3.0));
    }

    // ── v3: easing reusado + timeline vinculado ────────────────────────
    #[test]
    fn easing_fn_reusa_grafito_ui_sin_enum_nuevo() {
        let mut s = AnimPreviewState::default();
        // Las 8 fns existen y fijan extremos 0→0, 1→1.
        for name in EASING_NAMES {
            s.set_timeline_easing(name);
            assert_eq!(s.timeline_easing, *name);
            let f = s.easing_fn();
            assert!((f(0.0) - 0.0).abs() < 1e-6, "{name}(0)");
            assert!((f(1.0) - 1.0).abs() < 1e-6, "{name}(1)");
        }
        // Nombre desconocido: se ignora el set y la fn cae a cubic_in_out.
        s.set_timeline_easing("cubic_in_out");
        s.set_timeline_easing("bounce");
        assert_eq!(s.timeline_easing, "cubic_in_out");
        s.timeline_easing = "invento".to_string();
        let f = s.easing_fn();
        assert!((f(0.3) - ui_easing::cubic_in_out(0.3)).abs() < 1e-6);
        assert!((ui_easing::linear(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn timeline_scrub_mapea_fase_a_fotograma_con_easing() {
        let mut s = AnimPreviewState {
            frames: vec![egui::ColorImage::new([2, 2], Color32::BLACK); 48],
            ..Default::default()
        };
        assert_eq!(s.timeline_frame(48), None, "sin timeline → None");
        s.ensure_default_timeline(TIMELINE_DEFAULT_MS);
        assert!(s.timeline.as_ref().unwrap().validate().is_ok());
        // Idempotente: no pisa un timeline existente.
        s.timeline_phase = 0.7;
        s.ensure_default_timeline(5000);
        assert_eq!(s.timeline.as_ref().unwrap().duration_ms, 2000);
        assert!((s.timeline_phase - 0.7).abs() < 1e-6);
        // Lineal fase 0.25 → round(0.25*47)=12.
        s.set_timeline_easing("linear");
        s.timeline_phase = 0.25;
        assert_eq!(s.timeline_frame(48), Some(12));
        // cubic_in_out deforma el tiempo: 0.25→0.0625 → round(2.9375)=3.
        s.set_timeline_easing("cubic_in_out");
        assert_eq!(s.timeline_frame(48), Some(3));
        // Extremos.
        s.timeline_phase = 0.0;
        assert_eq!(s.timeline_frame(48), Some(0));
        s.timeline_phase = 1.0;
        assert_eq!(s.timeline_frame(48), Some(47));
        assert_eq!(s.timeline_frame(0), None);
        assert_eq!(s.timeline_frame(1), Some(0));
        // Vínculo inverso: fotograma → fase.
        s.frame_idx = 47;
        s.sync_phase_from_frame();
        assert!((s.timeline_phase - 1.0).abs() < 1e-6);
        s.frame_idx = 0;
        s.sync_phase_from_frame();
        assert!((s.timeline_phase - 0.0).abs() < 1e-6);
        s.clear_timeline();
        assert!(s.timeline.is_none());
        assert_eq!(s.timeline_frame(48), None);
    }

    #[test]
    fn playback_fps_pinnea_doce_sin_selector() {
        // TODO(v3-fps): selector 6/12/24. Hoy fijo 12 → 83 ms.
        assert_eq!(PLAYBACK_FPS, 12);
        assert_eq!(REPRODUCCION_MS, 83);
        assert_eq!(REPRODUCCION_MS, 1000 / PLAYBACK_FPS as u64);
    }

    // ── AS3: hash de frames muestreado (caché de texturas) ───────────────
    fn estado_frames(w: usize, h: usize, n: usize, seed: u8) -> AnimPreviewState {
        AnimPreviewState {
            frames: (0..n)
                .map(|i| {
                    egui::ColorImage::new(
                        [w, h],
                        egui::Color32::from_rgba_unmultiplied(
                            seed.wrapping_add(i as u8),
                            10,
                            20,
                            255,
                        ),
                    )
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn frames_hash_determinista_y_detecta_cambios() {
        let a = estado_frames(64, 48, 3, 7);
        let b = estado_frames(64, 48, 3, 7);
        assert_eq!(a.frames_hash(), b.frames_hash());
        // Cambio en el píxel 0 (siempre muestreado) → hash distinto.
        let mut c = estado_frames(64, 48, 3, 7);
        c.frames[0].pixels[0] = egui::Color32::RED;
        assert_ne!(a.frames_hash(), c.frames_hash());
        // Distinto tamaño o cantidad → hash distinto.
        let d = estado_frames(64, 48, 4, 7);
        assert_ne!(a.frames_hash(), d.frames_hash());
        let e = estado_frames(32, 48, 3, 7);
        assert_ne!(a.frames_hash(), e.frames_hash());
        // Vacío: determinista, sin pánicos.
        let v = AnimPreviewState::default();
        assert_eq!(v.frames_hash(), AnimPreviewState::default().frames_hash());
    }

    #[test]
    fn frames_hash_muestrea_acotado_por_frame() {
        // 640×480 = 307200 píxeles; el muestreo toca ≤512+1 por frame.
        let w = 640_usize;
        let h = 480_usize;
        let total = w * h;
        let stride = (total / FRAME_HASH_SAMPLE_PIXELS).max(1);
        let tocados = total.div_ceil(stride);
        assert!(tocados <= FRAME_HASH_SAMPLE_PIXELS + 1, "tocados={tocados}");
        assert_eq!(FRAME_HASH_MAX_FRAMES, 6);
        assert_eq!(FRAME_HASH_SAMPLE_PIXELS, 512);
    }

    // ── v4: panel puro headless (ANIM-REVIVE) ────────────────────────────
    fn estado_concepto(concept: &str) -> AnimPreviewState {
        AnimPreviewState {
            template: "derivative-slope".into(),
            concept: concept.into(),
            ..Default::default()
        }
    }

    #[test]
    fn panel_puro_generate_valida_y_emite_evento() {
        let mut s = estado_concepto("derivada");
        s.set_live_param("x0", 2.5);
        assert!(can_generate(&s));
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Generate);
        assert_eq!(
            evs,
            vec![AnimPanelEvent::GenerateRequested {
                template: "derivative-slope".into(),
                concept: "derivada".into(),
            }]
        );
        // Estado: arranque fresco, sin progreso inventado (el hilo lo moverá).
        assert_eq!(s.progress, 0.0);
        assert!(!s.is_active());
        assert!(!s.playing);
        assert_eq!(s.frame_idx, 0);
        assert!(s.frames.is_empty());
        assert!(s.media_path.is_none());
        assert!(!s.params_dirty, "generar consume los params vivos");
        assert!(s.status.contains("solicitada"));
    }

    #[test]
    fn panel_puro_generate_con_concepto_vacio_falla() {
        let mut s = AnimPreviewState::default();
        assert!(!can_generate(&s));
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Generate);
        assert_eq!(evs.len(), 1);
        assert!(matches!(evs[0], AnimPanelEvent::ValidationFailed(_)));
        // El estado cae a Fallo visible (español), sin eventos con efecto.
        assert_eq!(estado_vista(&s), EstadoVista::Fallo);
        assert!(s.status.contains("Error"));
    }

    #[test]
    fn panel_puro_regenerate_conserva_frames_y_generate_limpia() {
        let frames = vec![egui::ColorImage::new([2, 2], Color32::BLACK); 3];
        let mut regen = AnimPreviewState {
            template: "derivative-slope".into(),
            concept: "derivada".into(),
            frames: frames.clone(),
            media_path: Some("/tmp/a.gif".into()),
            frame_idx: 2,
            ..Default::default()
        };
        let evs = apply_anim_panel_action(&mut regen, AnimPanelAction::Regenerate);
        assert!(matches!(evs[0], AnimPanelEvent::RegenerateRequested { .. }));
        assert_eq!(regen.frames.len(), 3, "regenerar conserva hasta el relevo");
        assert!(regen.media_path.is_some());
        assert_eq!(regen.frame_idx, 0);

        let mut gen = AnimPreviewState {
            template: "derivative-slope".into(),
            concept: "derivada".into(),
            frames,
            media_path: Some("/tmp/a.gif".into()),
            ..Default::default()
        };
        let evs = apply_anim_panel_action(&mut gen, AnimPanelAction::Generate);
        assert!(matches!(evs[0], AnimPanelEvent::GenerateRequested { .. }));
        assert!(gen.frames.is_empty(), "generar arranca fresco");
        assert!(gen.media_path.is_none());
    }

    #[test]
    fn panel_puro_cancel_cancela_generacion_y_detiene_repro() {
        // Generación en curso → resetea progreso + evento con flag.
        let mut s = estado_concepto("derivada");
        s.progress = 0.4;
        s.playing = true;
        assert!(can_cancel(&s));
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Cancel);
        assert_eq!(
            evs,
            vec![AnimPanelEvent::CancelRequested {
                was_generating: true
            }]
        );
        assert_eq!(s.progress, 0.0);
        assert!(!s.playing);
        assert!(s.status.contains("cancelada"));
        // Sin nada en curso → no-op (el botón va deshabilitado).
        assert!(!can_cancel(&s));
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Cancel);
        assert!(evs.is_empty());
        // Solo reproduciendo → detiene sin tocar progreso.
        let mut r = estado_concepto("derivada");
        r.frames = vec![egui::ColorImage::new([2, 2], Color32::BLACK); 2];
        r.playing = true;
        let evs = apply_anim_panel_action(&mut r, AnimPanelAction::Cancel);
        assert_eq!(
            evs,
            vec![AnimPanelEvent::CancelRequested {
                was_generating: false
            }]
        );
        assert!(!r.playing);
        assert!(r.status.contains("detenida"));
    }

    #[test]
    fn panel_puro_export_solo_con_archivo_listo() {
        // Sin nada (ni frames ni archivo): no-op (el botón va deshabilitado).
        let mut s = estado_concepto("derivada");
        assert!(!can_export(&s));
        assert!(apply_anim_panel_action(&mut s, AnimPanelAction::Export).is_empty());
        // Con archivo: evento con la ruta (la copia la hace el hilo de E/S).
        s.media_path = Some("/tmp/anim.gif".into());
        assert!(can_export(&s));
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Export);
        assert_eq!(
            evs,
            vec![AnimPanelEvent::ExportRequested {
                media_path: "/tmp/anim.gif".into()
            }]
        );
        // Exportar no muta el estado.
        assert!(s.media_path.is_some());
    }

    #[test]
    fn panel_puro_export_desde_frames_sin_archivo() {
        // AS4: el nativo genera frames sin archivo; Export pide codificar el
        // set actual (`media_path` vacío) en vez de ser no-op. Headless.
        let mut s = estado_concepto("derivada");
        s.frames = vec![egui::ColorImage::new([2, 2], Color32::BLACK); 4];
        assert!(s.media_path.is_none());
        assert!(can_export(&s), "frames listos deben habilitar exportar");
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Export);
        assert_eq!(
            evs,
            vec![AnimPanelEvent::ExportRequested {
                media_path: String::new()
            }]
        );
        // No muta: los frames siguen para el preview y el reintento.
        assert_eq!(s.frames.len(), 4);
        assert!(s.media_path.is_none());
    }

    #[test]
    fn panel_puro_play_frame_y_template_locales() {
        let mut s = estado_concepto("derivada");
        // Sin frames: toggle no-op.
        assert!(apply_anim_panel_action(&mut s, AnimPanelAction::TogglePlay).is_empty());
        assert!(apply_anim_panel_action(&mut s, AnimPanelAction::SelectFrame(3)).is_empty());
        s.frames = vec![egui::ColorImage::new([2, 2], Color32::BLACK); 5];
        // Toggle alterna con evento.
        assert_eq!(
            apply_anim_panel_action(&mut s, AnimPanelAction::TogglePlay),
            vec![AnimPanelEvent::PlaybackStarted]
        );
        assert!(s.playing);
        assert_eq!(
            apply_anim_panel_action(&mut s, AnimPanelAction::TogglePlay),
            vec![AnimPanelEvent::PlaybackPaused]
        );
        // SelectFrame clampea, pausa y sincroniza.
        s.playing = true;
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::SelectFrame(99));
        assert_eq!(evs, vec![AnimPanelEvent::FrameSelected(4)]);
        assert_eq!(s.frame_idx, 4);
        assert!(!s.playing);
        // Plantilla: cambio emite, vacío o igual es no-op.
        let evs = apply_anim_panel_action(
            &mut s,
            AnimPanelAction::SelectTemplate("integral-area".into()),
        );
        assert_eq!(
            evs,
            vec![AnimPanelEvent::TemplateSelected("integral-area".into())]
        );
        assert!(
            apply_anim_panel_action(&mut s, AnimPanelAction::SelectTemplate("".into())).is_empty()
        );
        assert!(apply_anim_panel_action(
            &mut s,
            AnimPanelAction::SelectTemplate("integral-area".into())
        )
        .is_empty());
        // Clear vacía todo con evento.
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::Clear);
        assert_eq!(evs, vec![AnimPanelEvent::Cleared]);
        assert_eq!(s.progress, 0.0);
        assert!(s.frames.is_empty());
    }

    #[test]
    fn combo_sincronizado_con_registro_nativo_once() {
        // Sync mecánico 11↔11: ComboBox == NATIVE_TEMPLATES (orden libre).
        let mut combo: Vec<&str> = PLANTILLAS_COMBO.to_vec();
        let mut nativo: Vec<&str> = crate::anim_native::NATIVE_TEMPLATES.to_vec();
        combo.sort_unstable();
        nativo.sort_unstable();
        assert_eq!(combo, nativo);
        assert_eq!(PLANTILLAS_COMBO.len(), 11);
    }
}

// ── AS4: tests de vista previa paramétrica (transporte + OOM) ────────────
#[cfg(test)]
mod parametric_preview_tests {
    use super::*;
    use grafito_anim::parametric::{FrameCount, ParamName, ParametricAnim, ParametricKind};
    use grafito_anim::Resolution;

    const IDS_PROHIBIDOS: &[&str] = &[
        "PlayPause",
        "Slider",
        "Button",
        "Tangent",
        "Select",
        "Parallel",
        "Midpoint",
        "Distance",
        "Angle",
        "Area",
        "Function",
        "Polygon",
        "Circle",
        "Line",
        "Point",
        "Vector",
        "Segment",
        "Ray",
        "Eraser",
        "Pencil",
    ];

    fn anim_prueba() -> ParametricAnim {
        ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x^2+p*x".to_string(),
            None,
            ParamName::try_new("p").unwrap(),
            -2.0,
            2.0,
            FrameCount::try_new(12).unwrap(),
            Resolution::try_new(96, 72).unwrap(),
        )
        .unwrap()
    }

    fn frames_tontos(w: usize, h: usize, n: usize) -> Vec<egui::ColorImage> {
        (0..n)
            .map(|i| {
                egui::ColorImage::new(
                    [w, h],
                    egui::Color32::from_rgba_unmultiplied(i as u8, 100, 150, 255),
                )
            })
            .collect()
    }

    #[test]
    fn instala_vista_previa_con_estado_visible() {
        let anim = anim_prueba();
        let mut s = AnimPreviewState::default();
        let info = set_parametric_frames(&mut s, &anim, frames_tontos(96, 72, 12)).unwrap();
        assert_eq!(info.frames, 12);
        assert_eq!(info.bytes, 96 * 72 * 4 * 12);
        assert_eq!(s.frames.len(), 12);
        assert_eq!(s.frame_idx, 0);
        assert!(!s.playing);
        // Estado visible con prosa humanizada.
        assert!(s.status.contains("deslizador"));
        assert!(s.status.contains("reproducir"));
        let st = parametric_transport_status(&s);
        assert_eq!(
            st,
            "fotograma 1 de 12 · en pausa — mové el deslizador para elegir fotograma"
        );
        for id in IDS_PROHIBIDOS {
            assert!(!s.status.contains(id), "status no debe traer {id}");
            assert!(!st.contains(id), "transporte no debe traer {id}");
        }
    }

    #[test]
    fn transporte_play_pause_scrub() {
        let anim = anim_prueba();
        let mut s = AnimPreviewState::default();
        set_parametric_frames(&mut s, &anim, frames_tontos(96, 72, 12)).unwrap();
        // play: alterna y emite evento.
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::TogglePlay);
        assert_eq!(evs, vec![AnimPanelEvent::PlaybackStarted]);
        assert!(s.playing);
        // tick avanza el fotograma.
        s.tick_playback();
        assert_eq!(s.frame_idx, 1);
        assert!(parametric_transport_status(&s).contains("fotograma 2 de 12"));
        assert!(parametric_transport_status(&s).contains("reproduciendo"));
        // pause: vuelve y emite evento.
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::TogglePlay);
        assert_eq!(evs, vec![AnimPanelEvent::PlaybackPaused]);
        assert!(!s.playing);
        // scrub: elige fotograma, pausa y sincroniza.
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::SelectFrame(5));
        assert_eq!(evs, vec![AnimPanelEvent::FrameSelected(5)]);
        assert_eq!(s.frame_idx, 5);
        assert!(!s.playing);
        // scrub fuera de rango clampea al último.
        let evs = apply_anim_panel_action(&mut s, AnimPanelAction::SelectFrame(999));
        assert_eq!(evs, vec![AnimPanelEvent::FrameSelected(11)]);
        assert_eq!(s.frame_idx, 11);
        assert!(parametric_transport_status(&s).contains("fotograma 12 de 12"));
    }

    #[test]
    fn oom_y_desajustes_rechazan_honesto() {
        let anim = anim_prueba();
        // Vacío.
        let mut s = AnimPreviewState::default();
        assert!(set_parametric_frames(&mut s, &anim, Vec::new()).is_err());
        assert!(s.frames.is_empty());
        // OOM: 60 sets chicos contra viewport 640×480 superan 64 MiB.
        let grande = ParametricAnim::try_new(
            ParametricKind::Sweep,
            "x+p".to_string(),
            None,
            ParamName::try_new("p").unwrap(),
            -2.0,
            2.0,
            FrameCount::try_new(24).unwrap(),
            Resolution::default(),
        )
        .unwrap();
        let err = set_parametric_frames(&mut s, &grande, frames_tontos(96, 72, 60)).unwrap_err();
        assert!(err.contains("excede el tope"), "err: {err}");
        assert!(s.frames.is_empty());
        // Conteo distinto.
        let err = set_parametric_frames(&mut s, &anim, frames_tontos(96, 72, 11)).unwrap_err();
        assert!(err.contains("11"), "err: {err}");
        // Tamaño distinto.
        let err = set_parametric_frames(&mut s, &anim, frames_tontos(64, 48, 12)).unwrap_err();
        assert!(err.contains("mide"), "err: {err}");
        assert!(s.frames.is_empty());
    }

    #[test]
    fn ventana_de_texturas_sigue_en_ocho() {
        // La caché de miniaturas es la ventana paginada de 8 (OOM GPU acotado).
        assert_eq!(MINIATURAS_POR_PAGINA, 8);
        let anim = anim_prueba();
        let mut s = AnimPreviewState::default();
        set_parametric_frames(&mut s, &anim, frames_tontos(96, 72, 12)).unwrap();
        s.frame_idx = 11;
        assert_eq!(s.inicio_ventana(), 8);
        assert_eq!(s.fin_ventana(), 12);
    }
}
