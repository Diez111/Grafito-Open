//! Animación didáctica dentro del turno del chat (sin ventana compañera).
//!
//! La ventana flotante "Animación didáctica" se eliminó: la animación la
//! genera EL ASISTENTE en su hilo (`assistant.rs::run_assistant_animation_with`,
//! con `CancellationToken` y progreso) y vive DENTRO del turno del chat como
//! `AssistantMedia` (`grafito-ui/src/assistant.rs::set_media`, reproductor
//! `draw_media_card` en el último turno). Nada de paneles/ventanas nuevas.
//!
//! Este módulo conserva SOLO lo necesario para que nada quede muerto:
//! heurística honesta sobre el texto del pedido (`wants_animation_request`),
//! validación del concepto (`animation_concept_from_request`, `Err` honesto
//! con qué pedir, jamás inventa) y frase de referencia con nombres humanos
//! (`animation_reference_sentence`, jamás IDs literales). Sin E/S, sin spawn,
//! sin egui: puro y headless-testeable. El trabajo pesado corre en threads,
//! la UI solo renderiza.

use grafito_assistant_types::TurnMediaRef;

/// ¿El pedido pide animación? Heurística honesta sobre el texto.
///
/// Cubre "con animación", "animalo/anímalo", "anima/animá/animar" y
/// "explica X con animación". Evita el falso positivo del sustantivo
/// "animal/animales" (token exacto, no verbo + clítico). Puro, sin I/O.
pub fn wants_animation_request(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("animaci") {
        return true;
    }
    for token in lower.split(|c: char| !c.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        if token == "animal" || token == "animales" {
            continue;
        }
        if token == "animalo"
            || token == "animala"
            || token == "animame"
            || token == "anímalo"
            || token == "anímala"
            || token == "anímame"
        {
            return true;
        }
        if token.contains("animar") {
            return true;
        }
        if token.contains("animá") || token.contains("animé") || token.contains("animó") {
            return true;
        }
        if token.starts_with("anima") {
            // "anima/animas/animan/animamos" (verbo). "animalito" y familia
            // (diminutivo del sustantivo) se excluyen para no inventar.
            if token.starts_with("animal")
                && token != "animalo"
                && token != "animala"
                && token != "animame"
            {
                continue;
            }
            return true;
        }
    }
    false
}

/// Palabras de relleno que no aportan concepto (para detectar ambigüedad).
const RELLENO_SIN_CONCEPTO: &[&str] = &[
    "explica",
    "explicame",
    "explicá",
    "explicame",
    "con",
    "de",
    "la",
    "el",
    "las",
    "los",
    "por",
    "favor",
    "porfa",
    "me",
    "una",
    "un",
    "que",
    "para",
    "hace",
    "haceme",
    "haz",
    "genera",
    "generame",
    "crea",
    "creame",
    "mostra",
    "mostrame",
    "muestra",
    "muestrame",
    "ver",
    "quiero",
    "porfavor",
];

/// Extrae el concepto a animar o devuelve `Err` honesto con qué pedir.
///
/// - Vacío → `Err` con ejemplo.
/// - Sin gatillo de animación → `Err` (agregar "con animación"/"animalo").
/// - Solo gatillo sin concepto ("animalo", "con animación", "explica con
///   animación") → `Err` honesto que pide concepto + ejemplo paramétrico.
/// - Con concepto ("explica la derivada con animación") → `Ok` con el texto
///   original recortado (para `detect_template_for_concept`). Jamás inventa.
pub fn animation_concept_from_request(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("pedido vacío: describí qué animar, por ejemplo «explica la derivada con animación» o «barrido de f(x)=x^2+p·x con p en [-2,2]»".to_string());
    }
    if !wants_animation_request(trimmed) {
        return Err("el pedido no pide animación: agregá «con animación» o «animalo», por ejemplo «explica la derivada con animación»".to_string());
    }
    let lower = trimmed.to_lowercase();
    let mut resto = lower.clone();
    // Gatillos largos primero (si no, "anim" partiría "animación" antes).
    for pat in [
        "con animación",
        "con animacion",
        "animación",
        "animacion",
        "anímalo",
        "anímala",
        "anímame",
        "animalo",
        "animala",
        "animame",
        "animar",
        "animá",
        "anima",
        "anim",
    ] {
        resto = resto.replace(pat, " ");
    }
    // Quita relleno para ver si queda concepto sustantivo.
    let mut sustantivo = resto.clone();
    for stop in RELLENO_SIN_CONCEPTO {
        sustantivo = sustantivo.replace(stop, " ");
    }
    let alfanum: usize = sustantivo.chars().filter(|c| c.is_alphanumeric()).count();
    if alfanum < 3 {
        return Err(format!(
            "no pude inferir qué animar desde «{}»: decime el concepto (derivada, integral, Pitágoras, …) y, si querés un barrido paramétrico, la expresión y el rango, por ejemplo «barrido de f(x)=x^2+p·x con p en [-2,2]»",
            trimmed.chars().take(120).collect::<String>()
        ));
    }
    Ok(trimmed.to_string())
}

/// Frase de referencia para la prosa del turno (nombres humanos, jamás IDs).
///
/// Q3: el literal canónico vive en `grafito_ui::prosa::ANIMATION_REFERENCE_SENTENCE`
/// (usa el mapa `humanize_control_name` de ui solo en espíritu: deslizador,
/// reproducir, pausar; sin "PlayPause"/"Slider"/"Button"). La UI ya humaniza
/// el resto vía `humanize_prose_text` al dibujar.
pub fn animation_reference_sentence() -> &'static str {
    grafito_ui::prosa::ANIMATION_REFERENCE_SENTENCE
}

// ── Easing por nombre del wire (F1 Manim-en-Rust) ──────────────────────────
// El easing NO vive en el protocolo (`Timeline::sample_with` recibe la fn):
// la Piel resuelve el nombre a la fn existente vía
// `grafito_ui::animation::easing::by_name` (desconocido → `linear` honesto).
// El scrub del deslizador aplica esta fn a la fracción del segmento antes
// del lerp — misma curva que `RateFunc` en los casos exactos (ver test de
// paridad en `grafito-ui/src/animation.rs`). Puro, sin egui, sin E/S.

/// Mapea un nombre del wire (`EASING_NAMES`) a su easing de la Piel.
///
/// Desconocido o vacío → `linear` honesto (no inventa curva). Puro.
pub fn easing_fn_for_name(name: &str) -> fn(f32) -> f32 {
    grafito_ui::animation::easing::by_name(name)
}

// ── Retención diferida de texturas egui (fix use-after-free wgpu) ───────────
// El render GPU va un frame atrás: destruir una textura gestionada por egui
// (`TextureHandle` drop → `TexturesDelta::free` → `renderer.free_texture` →
// `wgpu::Texture::destroy`) en el mismo frame en que deja de usarse corre el
// riesgo de que un submit en vuelo todavía la referencie. El síntoma real es
// `Validation Error — Texture with 'egui_texid_Managed(N)' label has been
// destroyed` en `Queue::submit` al instalar la 2ª animación (la 1ª ok).
//
// Protocolo: ninguna textura se destruye en el mismo frame en que deja de
// usarse. Al reemplazar/evictar, el handle viejo se `retire`a a esta cola y
// se libera recién tras `TEXTURE_GRACE_FRAMES` ticks (un tick = un frame
// dibujado). `tick` devuelve los items listos para dropear; el caller los
// deja caer fuera de la cola. Puro, sin egui, sin I/O: headless-testeable.
// Los wirings con `TextureHandle` (`teaching_ui::ensure_textures`,
// `render_2d::FillTextureCacheStore`, `grafito-ui assistant::set_media` que
// replica esta máquina porque la Piel no puede depender de la app) siguen
// este mismo protocolo; ver sus tests de ciclo de vida.
//
// Presupuesto: la retención suma como máximo los sets retirados aún en
// gracia (estado estable: 1 set viejo; ráfagas transitorias acotadas por el
// OOM ya existente de cada caché). N≥2 por diseño, no por suerte.

/// Frames de gracia antes de liberar una textura retirada.
///
/// 3 = 1 frame de submit en vuelo + 1 de margen + 1 de redondeo de
/// `request_repaint`. Nunca bajar de 2.
pub const TEXTURE_GRACE_FRAMES: u32 = 3;

/// Tope de handles retenidos (M3-7): ráfagas de reemplazos sin dibujar
/// (10× `ensure` sin `tick`) no pueden crecer sin cota. Al superar el
/// tope, `retire` evicta el más viejo primero (el más próximo a expirar;
/// best-effort bajo presión, documentado). 96 = 2 playlists completas.
pub const RETENTION_MAX_PENDING: usize = 96;

const _: () = assert!(TEXTURE_GRACE_FRAMES >= 2);

#[derive(Debug)]
struct RetentionEntry<T> {
    item: T,
    frames_left: u32,
}

/// Cola de retiro con gracia por frames para handles de textura.
///
/// Genérica para no depender de egui acá: el caller instancia con
/// `egui::TextureHandle` y dropea lo que `tick` devuelve.
#[derive(Debug, Default)]
pub struct RetentionQueue<T> {
    entries: Vec<RetentionEntry<T>>,
}

impl<T> RetentionQueue<T> {
    /// Cola vacía.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Retira un handle: no se libera hasta `TEXTURE_GRACE_FRAMES` ticks.
    ///
    /// Acotado (M3-7): si ya hay `RETENTION_MAX_PENDING` retenidos, evicta
    /// el más viejo (drop inmediato, best-effort bajo presión) para que
    /// `pending()` nunca supere el tope.
    pub fn retire(&mut self, item: T) {
        if self.entries.len() >= RETENTION_MAX_PENDING {
            self.entries.remove(0);
        }
        self.entries.push(RetentionEntry {
            item,
            frames_left: TEXTURE_GRACE_FRAMES,
        });
    }

    /// Retira un lote completo (p. ej. el set de frames de una animación).
    pub fn retire_all(&mut self, items: Vec<T>) {
        for item in items {
            self.retire(item);
        }
    }

    /// Avanza un frame y devuelve los items cuya gracia expiró (el caller
    /// los dropea: ahí recién se destruye la textura GPU).
    pub fn tick(&mut self) -> Vec<T> {
        for entry in &mut self.entries {
            entry.frames_left = entry.frames_left.saturating_sub(1);
        }
        let mut ready = Vec::new();
        let mut still_pending = Vec::new();
        for entry in self.entries.drain(..) {
            if entry.frames_left == 0 {
                ready.push(entry.item);
            } else {
                still_pending.push(entry);
            }
        }
        self.entries = still_pending;
        ready
    }

    /// Cuántos handles siguen retenidos (aún no liberables).
    pub fn pending(&self) -> usize {
        self.entries.len()
    }

    /// Verdadero si no hay nada retenido.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── P0-UI historial Thumb+Replay (contrato app-side, puro sin egui) ─────────
// El modelo W1 (`assistant-types`: `ConversationTurn.media: Option<TurnMediaRef>`,
// `turn_media_map()`, `trim_conversation()`) pega al turno un thumb RGBA de
// 96×96 + los campos del replay. La Piel (`grafito-ui/src/assistant.rs`)
// dibuja una mini-card por turno no-last con ese thumb (1 `TextureHandle`
// chico por turno) + botón [Ver de nuevo] → `AssistantUiAction::ReplayMedia`.
// El slot vivo (`set_media`) sigue siendo el único reproductor full.
//
// Gate por dueño (P0, lado Piel en `live_slot_kind_for_last_turn`): el slot
// vivo es global y no sabe a qué turno pertenece, así que el ÚLTIMO turno
// solo lo usa si no tiene `media` propia; si ya tiene la suya, la Piel le
// dibuja SU mini-card (mismo `ReplayMedia{turn_idx}`, resuelto acá abajo sin
// importar que sea el último: `history_replay_request` no excluye al último
// a propósito) y jamás el slot. El slot nunca contradice el `turn.media`
// visible. Las mini-cards de turnos viejos (`is_history_mini_card`) quedan
// intactas.
//
// LRU vs FIFO (P1-mini-card): el caché de thumbs de la Piel es LRU real por
// `last_used` (con ≤6 turnos vivos << 96 casi nunca evicta, pero bajo presión
// conserva el más usado, no el índice más chico). La `RetentionQueue` de este
// módulo, en cambio, es FIFO a propósito: no es un caché sino una cola de
// gracia (cada batch debe liberarse en orden tras `TEXTURE_GRACE_FRAMES`
// ticks; reordenarla por uso rompería el invariante de liberación).
//
// Cancelled vs Failed (P1-export): cancelar no es error. La Piel distingue
// `MediaExportState::Cancelled` (neutro, conserva el player) de `Failed`
// (esconde toolbar/slider/export, solo error + [Reintentar]); el diálogo
// marca neutro (`mark_cancelled`) en vez del viejo `mark_failed` con
// "exportación cancelada".
// Todo lo de acá es puro y headless-testeable: la app resuelve el pedido de
// replay contra la conversación y reinyecta frames por el camino existente
// (`set_media`), sin guardar `Vec<ColorImage>` completo por turno en memoria
// (48 frames a 480px ~30 MiB = OOM).
//
// Presupuestos intactos (no se redefinen, se pinean en tests): GIF 64 frames,
// 8 M píxeles totales, archivo ≤5 MiB; retención 96 con gracia 3.

/// Lado del thumb histórico en píxeles (paridad con `TURN_MEDIA_THUMB_SIDE_PX`).
pub const HISTORY_THUMB_SIDE_PX: usize = 96;

/// Bytes RGBA exactos del thumb histórico (96×96×4).
pub const HISTORY_THUMB_EXPECTED_RGBA_BYTES: usize =
    HISTORY_THUMB_SIDE_PX * HISTORY_THUMB_SIDE_PX * 4;

/// Tope de thumbs históricos cacheados (1 textura por turno; el historial
/// tiene como máximo `MAX_CONVERSATION_TURNS` = 6 turnos, 6 << 96).
/// Paridad con `RETENTION_MAX_PENDING`.
pub const HISTORY_THUMB_MAX_TEXTURES: usize = 96;

/// Pedido de replay resuelto contra un turno del historial.
///
/// Puro: la app lo construye con `history_replay_request` y reinyecta los
/// frames por el camino existente (`set_media`); la UI solo emitió la
/// intención `ReplayMedia{turn_idx}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryReplayRequest {
    /// Índice del turno en la conversación al momento del click.
    pub turn_idx: usize,
    /// Título visible del historial.
    pub title: String,
    /// Plantilla nativa para regenerar los frames.
    pub template: String,
    /// Concepto para regenerar los frames.
    pub concept: String,
    /// Frames declarados por el turno (1..=64, ya validado).
    pub frame_count: u8,
}

/// ¿El thumb trae exactamente los RGBA de 96×96?
///
/// `TurnMediaRef::validate` acepta cualquier largo ≤36 KiB; para subirlo como
/// `ColorImage` se exige el tamaño exacto (sin adivinar dimensiones).
pub fn history_thumb_rgba_valid(thumb: &[u8]) -> bool {
    thumb.len() == HISTORY_THUMB_EXPECTED_RGBA_BYTES
}

/// ¿Este turno lleva tarjeta histórica? Solo turnos con media que no son el
/// último: el último usa el slot vivo (`set_media`/`draw_media_card`).
pub fn is_history_mini_card(turn_idx: usize, turn_count: usize, has_media: bool) -> bool {
    has_media && turn_count > 0 && turn_idx < turn_count && turn_idx.saturating_add(1) != turn_count
}

/// Resuelve un pedido de replay contra el turno indicado.
///
/// `None` honesto si el índice está fuera de rango, no hay media, la media no
/// valida o el thumb no trae los RGBA exactos. Puro, sin I/O.
pub fn history_replay_request(
    turn_idx: usize,
    turn_count: usize,
    media: Option<&TurnMediaRef>,
) -> Option<HistoryReplayRequest> {
    let media = media?;
    if turn_idx >= turn_count {
        return None;
    }
    if media.validate().is_err() || !history_thumb_rgba_valid(&media.thumb) {
        return None;
    }
    Some(HistoryReplayRequest {
        turn_idx,
        title: media.title.clone(),
        template: media.template.clone(),
        concept: media.concept.clone(),
        frame_count: media.frame_count,
    })
}

/// Huella FNV-1a 64 del `TurnMediaRef` para el caché de thumbs.
///
/// Paridad con `grafito_ui::assistant::history_thumb_fingerprint` (la Piel no
/// puede depender de la app, DAG `ui → app`, por eso se duplica el algoritmo;
/// ambas se pinean con el mismo vector en sus tests).
pub fn history_thumb_fingerprint(media: &TurnMediaRef) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    const FIELD_SEPARATOR: u8 = 0xff;
    let mut hash = FNV_OFFSET_BASIS;
    for field in [&media.title, &media.template, &media.concept] {
        for byte in field.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= u64::from(FIELD_SEPARATOR);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for byte in &media.thumb {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash ^= u64::from(media.frame_count);
    hash.wrapping_mul(FNV_PRIME)
}

// ── M2 Piel: upscale card/overlay, letterbox del thumb e intents de chat ───
// Helpers puros (sin egui, sin I/O) que la Piel espeja del otro lado del DAG
// (`ui → app` prohíbe que `grafito-ui` importe de acá): el algoritmo se
// duplica en `grafito-ui/src/assistant.rs` (`MAX_PREVIEW_UPSCALE`,
// `history_thumb_uv_rect`) y ambas copias se pinean con los mismos vectores
// en sus tests. El parseo de los intents lo hace otro frente (parser del
// chat); acá solo vive el vocabulario visible + la política documentada.

/// Cap de upscale del preview inline vs overlay libre (M2-1).
///
/// La card inline capea a 1.5× por nitidez: los frames nativos salen a ~480px
/// y los paneles miden 300..520, así que el caso común escala ≤1.1× y el cap
/// solo muerde texturas chicas (tests/thumbnails), donde más de 1.5× pixela
/// feo aun con filtrado lineal. El overlay ("Ver grande") permite upscale
/// libre: el usuario lo abrió a propósito y pide ver grande. Diferencia
/// intencional con motivo, no bug: `Some(cap)` inline, `None` en overlay.
/// Puro, sin I/O.
pub fn preview_upscale_cap(in_overlay: bool) -> Option<f32> {
    if in_overlay {
        None
    } else {
        Some(grafito_ui::assistant::MAX_PREVIEW_UPSCALE)
    }
}

/// Recorte UV centrado para el thumb histórico (M2-2).
///
/// El thumb serializado es RGBA 96×96 cuadrado por construcción: el caso real
/// siempre da `(0,0,1,1)` y el rect cuadrado de la mini-card jamás deforma.
/// Si la fuente algún día no fuera cuadrada, recorta el eje largo centrado
/// (letterbox por recorte, jamás estiramiento). Devuelve
/// `(min_u, min_v, max_u, max_v)` en 0..=1. Puro, sin `unwrap`.
pub fn history_thumb_uv_crop(src_w: u32, src_h: u32) -> (f32, f32, f32, f32) {
    let w = src_w.max(1) as f32;
    let h = src_h.max(1) as f32;
    if w == h {
        return (0.0, 0.0, 1.0, 1.0);
    }
    if w > h {
        let keep = (h / w).clamp(0.0, 1.0);
        let margin = (1.0 - keep) / 2.0;
        (margin, 0.0, 1.0 - margin, 1.0)
    } else {
        let keep = (w / h).clamp(0.0, 1.0);
        let margin = (1.0 - keep) / 2.0;
        (0.0, margin, 1.0, 1.0 - margin)
    }
}

/// Ejemplos de intents de animación invocables por texto en el chat (M2-4).
///
/// El parseo lo hace otro frente (parser del chat → `AssistantUiAction` /
/// setters de la Piel); esta lista es el vocabulario que la Piel muestra
/// (`grafito-ui::assistant::MEDIA_CHAT_INTENTS_HINT`, paridad de texto) y que
/// el parser acepta. Puro.
pub fn media_chat_intent_examples() -> &'static [&'static str] {
    &["exportar mp4 720p", "velocidad 2x", "órbita", "reintentar"]
}

// ── Diálogo Exportar profesional (Piel, `fn render(&Estado) -> Frame`) ─────
// El estado vivo es `grafito_ui::assistant::MediaExportDialog` (ver su
// `draw` en `grafito-ui`): el duplicado local (`AnimExportDialog` +
// `draw_anim_export_dialog`) se eliminó con el audio (W1). Este módulo
// conserva heurística de pedidos, texturas y reproductor de la card.

#[cfg(test)]
mod tests {
    use super::*;

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
        "Play",
        "Pause",
    ];

    #[test]
    fn heuristica_cubre_pedidos_con_animacion() {
        assert!(wants_animation_request("explica la derivada con animación"));
        assert!(wants_animation_request("Explica X con animación"));
        assert!(wants_animation_request("animalo"));
        assert!(wants_animation_request("anímalo"));
        assert!(wants_animation_request("anima la parábola"));
        assert!(wants_animation_request("animá la integral"));
        assert!(wants_animation_request("quiero animar la tangente"));
        assert!(!wants_animation_request("hola, ¿cómo estás?"));
        assert!(!wants_animation_request("quiero ver un animal"));
        assert!(!wants_animation_request("animales en el zoológico"));
        assert!(!wants_animation_request("derivá x^2"));
    }

    #[test]
    fn concepto_valido_pasa_y_ambiguo_falla_honesto() {
        let ok = animation_concept_from_request("explica la derivada con animación").unwrap();
        assert!(ok.contains("derivada"));
        assert!(animation_concept_from_request(
            "barrido de f(x)=x^2+p·x con p en [-2,2] con animación"
        )
        .is_ok());
        for ambiguo in [
            "animalo",
            "con animación",
            "anima",
            "explica con animación",
            "   ",
            "",
        ] {
            let err = animation_concept_from_request(ambiguo).unwrap_err();
            assert!(
                err.contains("qué animar") || err.contains("vacío") || err.contains("no pide"),
                "ambiguo {ambiguo:?} debe guiar, fue: {err}"
            );
            // El error guía con ejemplo, jamás inventa frames.
            assert!(err.contains("por ejemplo") || err.contains("agregá"));
        }
    }

    #[test]
    fn referencia_sin_ids_literales() {
        let frase = animation_reference_sentence();
        assert!(frase.contains("deslizador"));
        assert!(frase.contains("reproducir"));
        assert!(frase.contains("pausar"));
        for id in IDS_PROHIBIDOS {
            assert!(!frase.contains(id), "frase no debe traer {id}");
        }
    }

    // ── F1: easing por nombre del wire ───────────────────────────────────
    #[test]
    fn easing_por_nombre_cubre_los_8_y_falla_a_linear() {
        // Los 8 `EASING_NAMES` resuelven a fn finita con endpoints 0/1.
        for name in grafito_anim::protocol::EASING_NAMES {
            let f = easing_fn_for_name(name);
            for t in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
                assert!(f(t).is_finite(), "{name}({t}) finito");
            }
            assert!((f(0.0)).abs() < 1e-6, "{name}(0)==0");
            assert!((f(1.0) - 1.0).abs() < 1e-6, "{name}(1)==1");
        }
        // Desconocido/vacío → linear (el fallback honesto de la Piel).
        let lin = grafito_ui::animation::easing::linear;
        assert_eq!(easing_fn_for_name("wiggle-mal")(0.3), lin(0.3));
        assert_eq!(easing_fn_for_name("")(0.7), lin(0.7));
        // El scrub usa la fn sobre la fracción del segmento: cubic_in_out
        // es monótono y suaviza extremos (0.5 exacto por simetría).
        let f = easing_fn_for_name("cubic_in_out");
        assert!((f(0.5) - 0.5).abs() < 1e-6);
        assert!(f(0.25) < 0.25 && f(0.75) > 0.75, "in-out frena extremos");
    }

    #[test]
    fn retencion_gracia_minima_documentada() {
        // N≥2 verificado a compile-time (`const _: () = assert!(...)`
        // arriba: clippy `assertions_on_constants` prohíbe re-chequear la
        // const en runtime). Acá se pinnea el valor para que un cambio
        // silencioso falle fuerte.
        assert_eq!(TEXTURE_GRACE_FRAMES, 3);
    }

    #[test]
    fn retencion_no_libera_hasta_gracia_instalar_b_con_a_en_vuelo() {
        // Ciclo de vida del crash: media A instalada → media B la reemplaza
        // → submit en vuelo todavía referencia A → A no se destruye hasta
        // la gracia.
        let mut cola = RetentionQueue::new();
        cola.retire(10_u64);
        assert_eq!(cola.pending(), 1);
        // 1er y 2do frame: A sigue viva aunque B ya esté instalada.
        assert!(cola.tick().is_empty(), "frame 1: A retenida");
        assert_eq!(cola.pending(), 1);
        assert!(cola.tick().is_empty(), "frame 2: A retenida");
        assert_eq!(cola.pending(), 1);
        // 3er frame: gracia expirada, recién ahí se libera.
        let listos = cola.tick();
        assert_eq!(listos, vec![10_u64]);
        assert!(cola.is_empty());
    }

    #[test]
    fn retencion_prune_bajo_presion_no_toca_en_vuelo() {
        // Evicción LRU bajo presión: se retiran 8 y llegan 2 más antes de
        // que expire la gracia; nada se libera antes de tiempo y el orden
        // de liberación respeta el orden de retiro.
        let mut cola = RetentionQueue::new();
        for id in 0_u64..8 {
            cola.retire(id);
        }
        assert!(cola.tick().is_empty());
        cola.retire(8_u64);
        cola.retire(9_u64);
        assert_eq!(cola.pending(), 10);
        assert!(cola.tick().is_empty(), "en-vuelo intacto en frame 2");
        let listos = cola.tick();
        let mut ordenados = listos.clone();
        ordenados.sort_unstable();
        assert_eq!(ordenados, (0_u64..8).collect::<Vec<_>>());
        assert_eq!(cola.pending(), 2, "los 2 tardíos siguen en gracia");
        let ultimos = cola.tick();
        let mut ultimos_ordenados = ultimos.clone();
        ultimos_ordenados.sort_unstable();
        assert_eq!(ultimos_ordenados, vec![8_u64, 9_u64]);
        assert!(cola.is_empty());
    }

    #[test]
    fn retencion_lote_completo_y_cola_vacia() {
        let mut cola: RetentionQueue<u64> = RetentionQueue::new();
        assert!(cola.is_empty());
        assert!(cola.tick().is_empty());
        cola.retire_all(vec![1_u64, 2_u64, 3_u64]);
        assert_eq!(cola.pending(), 3);
        let _ = cola.tick();
        let _ = cola.tick();
        let listos = cola.tick();
        assert_eq!(listos.len(), 3);
        assert!(cola.is_empty());
    }

    // ── M3-7: cota pending<=96 ──────────────────────────────────────────
    #[test]
    fn retencion_acota_pending_96_y_drena_con_tick() {
        assert_eq!(RETENTION_MAX_PENDING, 96);
        let mut cola = RetentionQueue::new();
        // Ráfaga sin dibujar: 200 retiros sin un solo tick.
        for id in 0_u64..200 {
            cola.retire(id);
        }
        assert_eq!(
            cola.pending(),
            RETENTION_MAX_PENDING,
            "pending nunca supera el tope"
        );
        // Evicción del más viejo: sobreviven los últimos 96.
        let mut vistos: Vec<u64> = Vec::new();
        for _ in 0..TEXTURE_GRACE_FRAMES {
            vistos.extend(cola.tick());
        }
        vistos.sort_unstable();
        let esperados: Vec<u64> = (104_u64..200).collect();
        assert_eq!(vistos, esperados, "evicta viejos, conserva recientes");
        assert!(cola.is_empty());
    }

    #[test]
    fn retencion_10_reemplazos_sin_tick_no_crece_sin_cota() {
        // 10× reemplazo de set (48 thumbs) sin tick: 480 retiros.
        let mut cola = RetentionQueue::new();
        for ronda in 0_u64..10 {
            cola.retire_all((0_u64..48).map(|i| ronda * 1000 + i).collect());
            assert!(
                cola.pending() <= RETENTION_MAX_PENDING,
                "ronda {ronda}: pending {} > tope",
                cola.pending()
            );
        }
        assert_eq!(cola.pending(), RETENTION_MAX_PENDING);
    }

    // ── P0-UI historial Thumb+Replay ─────────────────────────────────────
    fn muestra_historial(title: &str) -> TurnMediaRef {
        TurnMediaRef::new(
            title,
            "derivada",
            "pendiente de la tangente",
            vec![128_u8; HISTORY_THUMB_EXPECTED_RGBA_BYTES],
            8,
        )
    }

    #[test]
    fn historial_mini_card_solo_no_last_con_media() {
        // 4 turnos: mini-card en 1, el último con media usa el slot vivo.
        assert!(is_history_mini_card(1, 4, true));
        assert!(!is_history_mini_card(3, 4, true), "el último es slot vivo");
        assert!(!is_history_mini_card(0, 4, false), "sin media no hay card");
        assert!(!is_history_mini_card(0, 0, true), "vacío nunca");
        assert!(!is_history_mini_card(5, 4, true), "fuera de rango nunca");
    }

    #[test]
    fn thumb_rgba_exige_96x96_exactos() {
        assert!(history_thumb_rgba_valid(&vec![
            0_u8;
            HISTORY_THUMB_EXPECTED_RGBA_BYTES
        ]));
        assert!(!history_thumb_rgba_valid(&[]));
        assert!(!history_thumb_rgba_valid(&vec![
            0_u8;
            HISTORY_THUMB_EXPECTED_RGBA_BYTES
                - 1
        ]));
        assert!(!history_thumb_rgba_valid(&vec![
            0_u8;
            HISTORY_THUMB_EXPECTED_RGBA_BYTES
                + 1
        ]));
    }

    #[test]
    fn replay_request_valida_y_rechaza_honesto() {
        let media = muestra_historial("Tangente móvil");
        let pedido = history_replay_request(1, 4, Some(&media)).expect("válido resuelve");
        assert_eq!(pedido.turn_idx, 1);
        assert_eq!(pedido.title, "Tangente móvil");
        assert_eq!(pedido.template, "derivada");
        assert_eq!(pedido.frame_count, 8);
        assert!(history_replay_request(1, 4, None).is_none(), "sin media");
        assert!(
            history_replay_request(9, 4, Some(&media)).is_none(),
            "índice fuera de rango"
        );
        let sin_titulo = TurnMediaRef::new(
            "",
            "derivada",
            "concepto",
            vec![1_u8; HISTORY_THUMB_EXPECTED_RGBA_BYTES],
            8,
        );
        assert!(
            history_replay_request(0, 2, Some(&sin_titulo)).is_none(),
            "media inválida no resuelve"
        );
        let thumb_corto = TurnMediaRef::new("t", "derivada", "concepto", vec![1_u8; 100], 8);
        assert!(
            history_replay_request(0, 2, Some(&thumb_corto)).is_none(),
            "thumb no-96×96 no resuelve"
        );
    }

    #[test]
    fn replay_del_ultimo_turno_resuelve_para_el_gate_por_dueno() {
        // Gate por dueño (P0, lado Piel): el último turno con media propia
        // muestra SU mini-card con el mismo `ReplayMedia{turn_idx}`. Este
        // resolver no excluye al último a propósito: si lo excluyera, el
        // botón del último turno emitiría un pedido que jamás resuelve.
        let media = muestra_historial("Integral");
        let pedido = history_replay_request(3, 4, Some(&media))
            .expect("el último con media resuelve replay");
        assert_eq!(pedido.turn_idx, 3);
        assert_eq!(pedido.title, "Integral");
        assert!(
            !is_history_mini_card(3, 4, true),
            "el último no es mini-card de historial (usa slot o su propia card)"
        );
    }

    #[test]
    fn fingerprint_determinista_y_sensible_a_campos() {
        let base = muestra_historial("Tangente");
        assert_eq!(
            history_thumb_fingerprint(&base),
            history_thumb_fingerprint(&base)
        );
        let otro_titulo = muestra_historial("Integral");
        assert_ne!(
            history_thumb_fingerprint(&base),
            history_thumb_fingerprint(&otro_titulo),
            "cambia el título, cambia la huella"
        );
        let otros_frames = TurnMediaRef::new(
            "Tangente",
            "derivada",
            "pendiente de la tangente",
            vec![128_u8; HISTORY_THUMB_EXPECTED_RGBA_BYTES],
            9,
        );
        assert_ne!(
            history_thumb_fingerprint(&base),
            history_thumb_fingerprint(&otros_frames),
            "cambia el frame_count, cambia la huella"
        );
    }

    #[test]
    fn dos_animaciones_turno_viejo_conserva_mini_card_con_replay_y_trim_invariante() {
        use grafito_assistant_types::{
            attach_turn_media, trim_conversation, turn_media_map, ConversationTurn,
            MAX_CONVERSATION_TURNS,
        };
        // Escenario usuario: pide una animación, luego pide otra. La anterior
        // no debe desaparecer: queda mini-card con replay en su turno.
        let mut conversacion = vec![
            ConversationTurn::user("explica la derivada con animación"),
            ConversationTurn::assistant("tangente lista"),
            ConversationTurn::user("ahora la integral con animación"),
            ConversationTurn::assistant("integral lista"),
        ];
        attach_turn_media(&mut conversacion, 1, muestra_historial("Tangente"))
            .expect("pega media al turno viejo");
        attach_turn_media(&mut conversacion, 3, muestra_historial("Integral"))
            .expect("pega media al turno nuevo");
        assert_eq!(turn_media_map(&conversacion).len(), 2);
        // El viejo es mini-card (no-last con media) y su replay resuelve.
        assert!(is_history_mini_card(1, conversacion.len(), true));
        let vieja = turn_media_map(&conversacion)
            .remove(&1)
            .expect("el turno viejo conserva su media");
        let pedido = history_replay_request(1, conversacion.len(), Some(&vieja))
            .expect("el replay del turno viejo resuelve");
        assert_eq!(pedido.title, "Tangente");
        assert_eq!(pedido.template, "derivada");
        assert_eq!(pedido.frame_count, 8);
        // El último usa el slot vivo, jamás mini-card.
        assert!(!is_history_mini_card(3, conversacion.len(), true));
        // Trim: crece más allá del tope y el invariante se mantiene (pares
        // user→assistant completos, media pegada al turno, sin reindexado).
        while conversacion.len() < MAX_CONVERSATION_TURNS + 2 {
            let n = conversacion.len();
            conversacion.push(ConversationTurn::user(format!("q{n}")));
            conversacion.push(ConversationTurn::assistant(format!("r{n}")));
        }
        trim_conversation(&mut conversacion);
        assert_eq!(conversacion.len(), MAX_CONVERSATION_TURNS);
        for par in conversacion.chunks_exact(2) {
            assert!(matches!(
                par[0].role,
                grafito_assistant_types::ConversationRole::User
            ));
            assert!(matches!(
                par[1].role,
                grafito_assistant_types::ConversationRole::Assistant
            ));
            assert!(par[0].validate().is_ok() && par[1].validate().is_ok());
        }
        for media in turn_media_map(&conversacion).values() {
            assert_eq!(media.thumb.len(), HISTORY_THUMB_EXPECTED_RGBA_BYTES);
        }
    }

    #[test]
    fn consts_historial_en_paridad_y_gif_intacto() {
        assert_eq!(HISTORY_THUMB_SIDE_PX, 96);
        assert_eq!(
            HISTORY_THUMB_EXPECTED_RGBA_BYTES,
            96 * 96 * 4,
            "36 KiB por thumb, jamás Vec<ColorImage> por turno"
        );
        assert_eq!(HISTORY_THUMB_MAX_TEXTURES, 96, "6 turnos << 96");
        assert_eq!(TEXTURE_GRACE_FRAMES, 3);
        assert_eq!(RETENTION_MAX_PENDING, 96);
        // Presupuestos GIF intactos (no redefinidos acá, pineados).
        assert_eq!(crate::anim_native::GIF_EXPORT_MAX_TOTAL_PIXELS, 8_000_000);
        assert_eq!(
            grafito_assistant_types::TURN_MEDIA_MAX_FRAMES,
            64,
            "64 frames por replay"
        );
        assert_eq!(
            grafito_assistant_types::TURN_MEDIA_THUMB_MAX_BYTES,
            HISTORY_THUMB_EXPECTED_RGBA_BYTES,
            "tope del tipo = RGBA 96×96"
        );
    }

    // ── M2 Piel: caps, letterbox e intents ──────────────────────────────
    #[test]
    fn upscale_card_capea_y_overlay_libre_con_motivo() {
        // M2-1: la card inline capea a 1.5× (nitidez), el overlay es libre
        // ("ver grande" a pedido). Diferencia intencional pineada.
        assert_eq!(
            preview_upscale_cap(false),
            Some(grafito_ui::assistant::MAX_PREVIEW_UPSCALE)
        );
        assert_eq!(preview_upscale_cap(false), Some(1.5));
        assert_eq!(preview_upscale_cap(true), None);
    }

    #[test]
    fn thumb_uv_cuadrado_completo_y_no_cuadrado_recorta_centrado() {
        // M2-2: 96×96 (el caso real) da UV completo: jamás deforma.
        assert_eq!(history_thumb_uv_crop(96, 96), (0.0, 0.0, 1.0, 1.0));
        // Apaisado: recorta U centrado, V intacta.
        assert_eq!(history_thumb_uv_crop(192, 96), (0.25, 0.0, 0.75, 1.0));
        // Retrato: recorta V centrada, U intacta.
        assert_eq!(history_thumb_uv_crop(96, 192), (0.0, 0.25, 1.0, 0.75));
        // Degenerado no panica: cae al cuadrado completo.
        assert_eq!(history_thumb_uv_crop(0, 0), (0.0, 0.0, 1.0, 1.0));
    }

    #[test]
    fn intents_de_chat_cubren_los_4_ejemplos_visibles() {
        // M2-4: vocabulario que la Piel muestra y el parser acepta.
        let ejemplos = media_chat_intent_examples();
        assert_eq!(ejemplos.len(), 4);
        assert!(ejemplos.contains(&"exportar mp4 720p"));
        assert!(ejemplos.contains(&"velocidad 2x"));
        assert!(ejemplos.contains(&"órbita"));
        assert!(ejemplos.contains(&"reintentar"));
    }
}
