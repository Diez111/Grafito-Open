//! Prosa rioplatense del asistente (Q3: fuente única).
//!
//! Vive en `grafito-ui` (Piel) porque es puro y no depende de `grafito-app`
//! (DAG: `ui → app` prohibiría lo contrario): sólo usa `grafito-anim`
//! (paramétrico) y `grafito-assistant-types` (turnos), ambas deps ya
//! declaradas de `grafito-ui`.
//!
//! Contenido (movido verbatim, cero cambio de conducta):
//! - `humanize_control_name` / `humanize_prose_text` (+ helpers privados)
//!   desde `ui::assistant` — `assistant.rs` los re-exporta.
//! - `prosa_integral_explicita` / `append_canonical_integral_prose` desde
//!   `app::assistant` — `assistant.rs` los importa de acá.
//! - `ANIMATION_REFERENCE_SENTENCE`: literal canónico de la frase de
//!   referencia de la media (`app::anim_ui::animation_reference_sentence`
//!   delega acá para no duplicar el literal).
//!
//! Todo es puro (`&str -> String`, sin I/O ni spawn), sin `unwrap` en prod.

use grafito_assistant_types::{ConversationRole, ConversationTurn};

/// Frase de referencia para la prosa del turno (nombres humanos, jamás IDs).
///
/// Usa el mapa [`humanize_control_name`] solo en espíritu (deslizador,
/// reproducir, pausar): no contiene "PlayPause", "Slider", "Button" ni ningún
/// identificador literal de control. La UI ya humaniza el resto vía
/// [`humanize_prose_text`] al dibujar.
pub const ANIMATION_REFERENCE_SENTENCE: &str =
    "La animación está lista abajo: mové el deslizador para recorrer los fotogramas y usá reproducir o pausar para controlarla.";

/// Nombre humano en español para un identificador literal de control.
///
/// El modelo a veces escribe nombres técnicos en la prosa (`PlayPause`).
/// Esta tabla sólo contiene controles que existen de verdad (`Tool::name`,
/// `Icon::Play`/`Pause`): `PlayPause` no existe como control, así que su
/// texto dice cómo llegar (la animación del chat se reproduce sola).
/// Puro (`&str -> Option`), sin I/O.
pub fn humanize_control_name(id: &str) -> Option<&'static str> {
    match id {
        "PlayPause" => Some("reproducción — la animación del chat se reproduce sola, sin botón"),
        "Play" => Some("reproducir"),
        "Pause" => Some("pausar"),
        "Slider" => Some("deslizador"),
        "Button" => Some("botón"),
        "Eraser" => Some("borrador"),
        "Pencil" => Some("lápiz"),
        "Select" => Some("selección"),
        "Tangent" => Some("tangente"),
        "Perpendicular" => Some("perpendicular"),
        "Parallel" => Some("paralela"),
        "Midpoint" => Some("punto medio"),
        "Distance" => Some("distancia"),
        "Angle" => Some("ángulo"),
        "Area" => Some("área"),
        "Function" => Some("función"),
        "Polygon" => Some("polígono"),
        "Circle" => Some("círculo"),
        "Line" => Some("recta"),
        "Point" => Some("punto"),
        "Vector" => Some("vector"),
        "Segment" => Some("segmento"),
        "Ray" => Some("semirrecta"),
        _ => None,
    }
}

/// Reemplaza identificadores literales de controles en prosa por su nombre
/// humano. Orden longest-first (`PlayPause` antes que `Play`/`Pause`) y
/// reemplazos en minúsculas para no re-matchear. Puro, conserva UTF-8.
/// Además absorbe el sufijo GeoGebra `Id[param]` (ej. `Button[a]`): el
/// modelo a veces filtra `Button` dejando `[a]` suelto ("sin botón\[a]");
/// la prosa final jamás tiene corchetes (D2): queda "sin botón".
pub fn humanize_prose_text(text: &str) -> String {
    const KNOWN_IDS: &[&str] = &[
        "PlayPause",
        "Perpendicular",
        "Parallel",
        "Midpoint",
        "Distance",
        "Tangent",
        "Slider",
        "Button",
        "Eraser",
        "Pencil",
        "Select",
        "Angle",
        "Function",
        "Polygon",
        "Circle",
        "Segment",
        "Vector",
        "Pause",
        "Area",
        "Line",
        "Point",
        "Play",
        "Ray",
    ];
    let mut out = text.to_owned();
    for id in KNOWN_IDS {
        if out.contains(id) {
            if let Some(human) = humanize_control_name(id) {
                out = replace_control_with_optional_param(&out, id, human);
            }
        }
    }
    // N2: el modelo generaliza la sintaxis `Id[param]` del system prompt a la
    // palabra ya humana (`botón[a]` en vez de `Button[a]`): el pase D2 no la
    // ve porque busca `Button` exacto. Este barre la variante humana en
    // cualquier caja, con/sin tilde, y absorbe `[param]`.
    out = replace_human_button_params(&out);
    out
}

/// N2: reemplaza `botón`/`boton`/`button` en cualquier caja seguidos de un
/// opcional `[param]` por `botón`, sin dejar corchetes.
///
/// Cubre lo que el pase D2 (`Button` exacto) no ve: el modelo escribe la
/// palabra ya humana con sufijo GeoGebra (`botón[a]`, `BOTÓN[A]`, `boton[a]`).
/// Frontera honesta: si tras la raíz viene letra (`botones`) no toca nada.
/// Puro, UTF-8 seguro (chars, jamás índices byte), sin `unwrap`.
fn replace_human_button_params(text: &str) -> String {
    const MAX_PARAM_LEN: usize = 64;
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(stem) = match_button_stem(&chars[i..]) {
            let mut j = i + stem;
            if chars.get(j) == Some(&'[') {
                let mut k = j + 1;
                let mut seen = 0;
                while k < chars.len() && chars[k] != ']' && seen <= MAX_PARAM_LEN {
                    k += 1;
                    seen += 1;
                }
                if k < chars.len() && chars[k] == ']' {
                    j = k + 1;
                }
            }
            out.push_str("botón");
            i = j;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Raíz `boton`/`botón`/`button` case-insensitive (ASCII + Ó/ó) al inicio del
/// slice. Devuelve chars consumidos o `None`. Exige frontera no-letra detrás
/// para no romper `botones`. Pura, sin `unwrap`.
fn match_button_stem(chunk: &[char]) -> Option<usize> {
    let lower_at =
        |pos: usize| -> Option<char> { chunk.get(pos).and_then(|c| c.to_lowercase().next()) };
    let is_boton = lower_at(0) == Some('b')
        && lower_at(1) == Some('o')
        && lower_at(2) == Some('t')
        && matches!(
            (lower_at(3), lower_at(4)),
            (Some('o'), Some('n')) | (Some('ó'), Some('n'))
        );
    let is_button = lower_at(0) == Some('b')
        && lower_at(1) == Some('u')
        && lower_at(2) == Some('t')
        && lower_at(3) == Some('t')
        && lower_at(4) == Some('o')
        && lower_at(5) == Some('n');
    let stem = if is_boton {
        5
    } else if is_button {
        6
    } else {
        return None;
    };
    let boundary_ok = chunk.get(stem).is_none_or(|c| !c.is_alphabetic());
    boundary_ok.then_some(stem)
}

/// Reemplaza `id` y `id[lo que sea]` por `human` sin dejar corchetes.
///
/// Recorre por bytes con fronteras char (nunca corta scalars): si tras el
/// `id` viene `[`, absorbe hasta el `]` de cierre (acotado a 64 chars para
/// no comerse párrafos si falta el cierre). Pura, sin panic ni `unwrap`.
fn replace_control_with_optional_param(haystack: &str, id: &str, human: &str) -> String {
    const MAX_PARAM_LEN: usize = 64;
    let mut out = String::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(pos) = rest.find(id) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + id.len()..];
        if let Some(stripped) = after.strip_prefix('[') {
            // Busca `]` de cierre en los próximos 64 chars (límite honesto).
            let window_len: usize = stripped
                .char_indices()
                .take_while(|(offset, _)| *offset <= MAX_PARAM_LEN)
                .last()
                .map(|(offset, ch)| offset + ch.len_utf8())
                .unwrap_or(0);
            let window = &stripped[..window_len.min(stripped.len())];
            if let Some(close) = window.find(']') {
                out.push_str(human);
                rest = &stripped[close + 1..];
                continue;
            }
            // Sin cierre: deja el `[` como texto y sigue (jamás panic).
        }
        out.push_str(human);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Prosa rioplatense para integral explícita: nombra la función y el rango.
///
/// La usa el turno local-only para que la media nunca quede huérfana.
/// Sin "pedime otra" (ese marcador es solo de la canónica). Pura, sin I/O.
pub fn prosa_integral_explicita(expr: &str, pedido: &str) -> String {
    let (_, p0, p1) = grafito_anim::parametric::infer_area_anim(pedido)
        .map(|resuelto| {
            let anim = resuelto.anim();
            (anim.expr_a.clone(), anim.p0, anim.p1)
        })
        .unwrap_or_else(|_| {
            (
                expr.to_string(),
                grafito_anim::parametric::INTEGRAL_CANONICAL_P0,
                grafito_anim::parametric::INTEGRAL_CANONICAL_P1,
            )
        });
    format!("te muestro con f(x)={expr} en [{p0},{p1}].\n\n{ANIMATION_REFERENCE_SENTENCE}")
}

/// Agrega la declaración de la canónica al último turno del asistente.
///
/// Idempotente por contenido ("pedime otra" ya presente → no duplica).
/// Puro sobre el transcript (sin I/O ni spawn): el hilo de render no se
/// toca acá.
pub fn append_canonical_integral_prose(conversation: &mut [ConversationTurn]) {
    let Some(turno) = conversation.last_mut() else {
        return;
    };
    if turno.role != ConversationRole::Assistant || turno.content.contains("pedime otra") {
        return;
    }
    turno.content.push_str("\n\n");
    turno
        .content
        .push_str(grafito_anim::parametric::INTEGRAL_CANONICAL_PROSA);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize_control_name_maps_existing_controls() {
        // Bug 3: el modelo escribe `PlayPause` en la prosa.
        assert_eq!(
            humanize_control_name("PlayPause"),
            Some("reproducción — la animación del chat se reproduce sola, sin botón")
        );
        assert_eq!(humanize_control_name("Slider"), Some("deslizador"));
        assert_eq!(humanize_control_name("NoExiste"), None);
    }

    #[test]
    fn humanize_prose_text_replaces_ids_without_breaking_utf8() {
        // Ejemplo del screenshot: prosa con id crudo + tildes/emoji.
        let human = humanize_prose_text("Usá el control de reproducción PlayPause ⏯️ para ver");
        assert!(!human.contains("PlayPause"), "quedó crudo: {human}");
        assert!(human.contains("reproducción"), "falta humano: {human}");
        assert!(human.contains("⏯️"), "se rompió multibyte: {human}");
        // Ids encadenados: longest-first, sin re-matcheo.
        let both = humanize_prose_text("PlayPause y Play");
        assert!(!both.contains("PlayPause"));
        assert!(both.contains("reproducir"));
    }

    #[test]
    fn humanize_prose_text_absorbe_parametro_sin_dejar_corchetes() {
        // D2 bug del screenshot: "sin botón[a] sobre esa variable".
        let human = humanize_prose_text("sin Button[a] sobre esa variable");
        assert_eq!(human, "sin botón sobre esa variable", "prosa rota: {human}");
        assert!(!human.contains('['), "quedó corchete: {human}");
        assert!(!human.contains(']'), "quedó corchete: {human}");
        assert!(!human.contains("Button"), "quedó crudo: {human}");
        // Otros controles con parámetro GeoGebra también se absorben.
        let play = humanize_prose_text("tocá PlayPause[a] para ver");
        assert!(!play.contains('['), "quedó corchete: {play}");
        assert!(!play.contains("PlayPause"), "quedó crudo: {play}");
        let slider = humanize_prose_text("mové Slider[p, 0, 1] suave");
        assert!(!slider.contains('['), "quedó corchete: {slider}");
        assert!(slider.contains("deslizador"), "falta humano: {slider}");
        // Sin cierre: jamás panic, jamás corchete inventado de más.
        let broken = humanize_prose_text("sin Button[a sobre esa variable");
        assert!(!broken.contains("Button"), "quedó crudo: {broken}");
    }

    #[test]
    fn humanize_boton_humano_con_parametro_en_cualquier_caja() {
        // N2: el instalado D2 cubría `Button[a]` pero el modelo escribe la
        // palabra ya humana (`botón[a]`): seguía visible en la prosa.
        for raw in [
            "sin botón[a] sobre esa variable",
            "sin Button[a] sobre esa variable",
            "sin boton[a] sobre esa variable",
            "sin BOTÓN[A] sobre esa variable",
            "sin button[A] sobre esa variable",
            "sin Boton[a] sobre esa variable",
        ] {
            let human = humanize_prose_text(raw);
            assert_eq!(
                human, "sin botón sobre esa variable",
                "prosa rota: {raw} → {human}"
            );
            assert!(!human.contains('['), "quedó corchete: {human}");
            assert!(!human.contains(']'), "quedó corchete: {human}");
        }
        // Sin parámetro también se normaliza la caja a `botón`.
        assert_eq!(
            humanize_prose_text("tocá Button para ver"),
            "tocá botón para ver"
        );
        // Frontera honesta: el plural `botones` no se toca.
        let plural = humanize_prose_text("hay 2 o 3 botones para elegir");
        assert!(plural.contains("botones"), "rompió el plural: {plural}");
        // Sin cierre: jamás panic.
        let broken = humanize_prose_text("sin botón[a sobre esa variable");
        assert!(broken.contains("botón"), "perdió la palabra: {broken}");
    }

    #[test]
    fn integral_canonica_prosa_declara_y_no_duplica() {
        // La prosa declara la canónica en rioplatense.
        let mut conversacion = vec![ConversationTurn::assistant("La animación está lista.")];
        append_canonical_integral_prose(&mut conversacion);
        let texto = &conversacion.last().expect("turno").content;
        assert!(texto.contains("x²"), "{texto}");
        assert!(texto.contains("pedime otra"), "{texto}");
        // Idempotente: segunda pasada no duplica.
        append_canonical_integral_prose(&mut conversacion);
        assert_eq!(
            conversacion
                .last()
                .expect("turno")
                .content
                .matches("pedime otra")
                .count(),
            1
        );
        // Turno de usuario o vacío: no toca nada.
        let mut usuario = vec![ConversationTurn::user("hola")];
        append_canonical_integral_prose(&mut usuario);
        assert_eq!(usuario.last().expect("turno").content, "hola");
        let mut vacia: Vec<ConversationTurn> = Vec::new();
        append_canonical_integral_prose(&mut vacia);
        assert!(vacia.is_empty());
    }

    // ── Goldens Q3: igualdad byte a byte (candado anti-deriva) ──────────────

    #[test]
    fn golden_animation_reference_sentence_byte_identical() {
        assert_eq!(
            ANIMATION_REFERENCE_SENTENCE,
            "La animación está lista abajo: mové el deslizador para recorrer los fotogramas y usá reproducir o pausar para controlarla."
        );
        for id in ["PlayPause", "Slider", "Button", "Tangent", "Midpoint"] {
            assert!(
                !ANIMATION_REFERENCE_SENTENCE.contains(id),
                "la frase jamás trae IDs crudos: {id}"
            );
        }
    }

    #[test]
    fn golden_humanize_corpus_byte_identical() {
        // Derivación mecánica: `PlayPause` sin `[` → reemplazo directo.
        assert_eq!(
            humanize_prose_text("Usá el control de reproducción PlayPause ⏯️ para ver"),
            "Usá el control de reproducción reproducción — la animación del chat se reproduce sola, sin botón ⏯️ para ver"
        );
        assert_eq!(
            humanize_prose_text("PlayPause y Play"),
            "reproducción — la animación del chat se reproduce sola, sin botón y reproducir"
        );
        assert_eq!(humanize_prose_text(""), "");
        assert_eq!(humanize_prose_text("   "), "   ");
        assert_eq!(
            humanize_prose_text("mové Slider[p, 0, 1] suave"),
            "mové deslizador suave"
        );
    }

    #[test]
    fn golden_prosa_integral_explicita_fallback_byte_identical() {
        // "hola" no menciona área → rama fallback: expr tal cual + rango
        // canónico [0,2] + frase de referencia.
        assert_eq!(
            prosa_integral_explicita("x^3", "hola"),
            format!("te muestro con f(x)=x^3 en [0,2].\n\n{ANIMATION_REFERENCE_SENTENCE}")
        );
    }

    #[test]
    fn golden_prosa_integral_explicita_nombra_funcion_y_rango() {
        // Pedido explícito con función y rango: la prosa los nombra y jamás
        // trae el marcador canónico ("pedime otra" es solo canónico).
        let prosa = prosa_integral_explicita(
            "x^3",
            "animacion de la integral de f(x)=x^3 de 0 a 2 con animación",
        );
        assert_eq!(
            prosa,
            format!("te muestro con f(x)=x^3 en [0,2].\n\n{ANIMATION_REFERENCE_SENTENCE}")
        );
        assert!(
            !prosa.contains("pedime otra"),
            "el marcador es solo canónico: {prosa}"
        );
    }

    #[test]
    fn golden_append_canonical_exacta_byte_identical() {
        let mut conversacion = vec![ConversationTurn::assistant("La animación está lista.")];
        append_canonical_integral_prose(&mut conversacion);
        assert_eq!(
            conversacion.last().expect("turno").content,
            "La animación está lista.\n\nte muestro con f(x)=x² en [0,2]; pedime otra y la cambio"
        );
    }
}
