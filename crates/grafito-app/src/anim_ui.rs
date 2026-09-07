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
/// Usa el mapa `humanize_control_name` de ui solo en espíritu (deslizador,
/// reproducir, pausar): no contiene "PlayPause", "Slider", "Button" ni ningún
/// identificador literal de control. La UI ya humaniza el resto vía
/// `humanize_prose_text` al dibujar.
pub fn animation_reference_sentence() -> &'static str {
    "La animación está lista abajo: mové el deslizador para recorrer los fotogramas y usá reproducir o pausar para controlarla."
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
}
