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
}
