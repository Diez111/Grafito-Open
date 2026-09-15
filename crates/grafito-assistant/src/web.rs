//! Búsqueda web opt-in del asistente (modo "Buscar en internet").
//!
//! Sin dependencias nuevas ni claves de API: usa DuckDuckGo HTML
//! (`html.duckduckgo.com/html/`) como backend primario y la Instant Answer API
//! (`api.duckduckgo.com/?format=json`) como respaldo cuando el HTML no trae
//! resultados. Todo sale por `crate::shared_http_client()` (bloqueante, sin
//! redirects) y corre siempre en un worker, nunca en la UI.
//!
//! Reglas:
//! - Query saneada (trim, cap [`WEB_SEARCH_QUERY_MAX_CHARS`], sin NUL).
//! - Resultados acotados a [`WEB_SEARCH_MAX_RESULTS`] con snippet capado.
//! - URLs sólo `http(s)`; se decodifica el redirect `uddg=` de DDG.
//! - El contexto inyectable al prompt se acota a
//!   [`WEB_SEARCH_CONTEXT_MAX_CHARS`].
//! - Sin red (`assistant-net` off) o fallo de transporte: `Err` honesto, jamás
//!   resultados vacíos silenciosos.
//!
//! La tool del agente (`web_search`) y el pre-flight del modo chat consumen
//! este módulo; el gating por preferencia del usuario vive en la app.
//!
//! El parseo es puro y determinista (sin red): [`parse_ddg_html`],
//! [`parse_ddg_api`], [`decode_uddg`] y [`sanitize_html_text`] se testean por
//! separado. El transporte vive en `web_search_with_html_endpoint`, que acepta
//! endpoints inyectables para que los tests usen stubs TCP locales.

/// Máximo de resultados devueltos por búsqueda.
pub const WEB_SEARCH_MAX_RESULTS: usize = 5;
/// Cap de caracteres de la query aceptada.
pub const WEB_SEARCH_QUERY_MAX_CHARS: usize = 256;
/// Cap de caracteres de cada snippet.
pub const WEB_SEARCH_SNIPPET_MAX_CHARS: usize = 400;
/// Cap de caracteres del bloque de contexto inyectable al prompt.
pub const WEB_SEARCH_CONTEXT_MAX_CHARS: usize = 4_096;
/// Timeout de la búsqueda completa.
pub const WEB_SEARCH_TIMEOUT_MS: u64 = 8_000;

/// Cap de caracteres del título de un resultado. Más chico que el snippet a
/// propósito: el título entra en una sola línea del bloque de contexto.
const WEB_SEARCH_TITLE_MAX_CHARS: usize = 200;
/// Cap de caracteres de una URL aceptada en un resultado.
const WEB_SEARCH_URL_MAX_CHARS: usize = 2_000;
/// Cap del cuerpo de respuesta que se lee de cada backend (OOM-safe).
#[cfg(feature = "assistant-net")]
const WEB_SEARCH_RESPONSE_MAX_BYTES: usize = 1 << 20;
/// Endpoint HTML de DuckDuckGo (backend primario, sin JS).
#[cfg(feature = "assistant-net")]
const WEB_SEARCH_HTML_ENDPOINT: &str = "https://html.duckduckgo.com/html/";
/// Endpoint de la Instant Answer API de DuckDuckGo (respaldo).
#[cfg(feature = "assistant-net")]
const WEB_SEARCH_API_ENDPOINT: &str = "https://api.duckduckgo.com/";
/// UA explícito: el cliente compartido no manda ninguno y DDG corta clientes
/// sin identificación.
#[cfg(feature = "assistant-net")]
const WEB_SEARCH_USER_AGENT: &str =
    "Mozilla/5.0 (compatible; Grafito/1.1; +https://www.grafito.org/)";

/// Un resultado de búsqueda web ya saneado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearchResult {
    /// Título sin HTML ni entidades.
    pub title: String,
    /// URL final http(s).
    pub url: String,
    /// Resumen textual sin HTML ni entidades.
    pub snippet: String,
}

/// Busca `query` en la web y devuelve hasta [`WEB_SEARCH_MAX_RESULTS`]
/// resultados saneados.
///
/// La query se recorta a [`WEB_SEARCH_QUERY_MAX_CHARS`] (sin partir chars);
/// query vacía o con NUL devuelve `Err`. Primero consulta el backend HTML y,
/// si no trae resultados, cae a la Instant Answer API. Sin resultados en
/// ninguno de los dos no es error (`Ok(vec![])`); sólo se devuelve `Err` si
/// fallan los dos (transporte o status no-2xx).
#[cfg(feature = "assistant-net")]
pub fn web_search(query: &str) -> Result<Vec<WebSearchResult>, String> {
    web_search_with_html_endpoint(
        WEB_SEARCH_HTML_ENDPOINT,
        WEB_SEARCH_API_ENDPOINT,
        query,
        std::time::Duration::from_millis(WEB_SEARCH_TIMEOUT_MS),
    )
}

/// Sin red: `Err` honesto (el build no puede buscar).
#[cfg(not(feature = "assistant-net"))]
pub fn web_search(_query: &str) -> Result<Vec<WebSearchResult>, String> {
    Err("web search is disabled in this build (feature assistant-net is off)".into())
}

/// Igual que [`web_search`] pero con endpoints inyectables y timeout explícito.
///
/// Pensado como costura de tests: los tests levantan stubs TCP en
/// `127.0.0.1:0` y verifican el parseo y el fallback sin tocar la red real.
/// Cada backend usa el mismo `timeout` (el peor caso son dos intentos).
#[cfg(feature = "assistant-net")]
pub(crate) fn web_search_with_html_endpoint(
    html_endpoint: &str,
    api_endpoint: &str,
    query: &str,
    timeout: Duration,
) -> Result<Vec<WebSearchResult>, String> {
    let query = sanitize_web_query(query)?;
    if timeout.is_zero() {
        return Err("web search timeout must be greater than zero".into());
    }

    let mut failures: Vec<String> = Vec::new();

    let html_url = endpoint_with_query(html_endpoint, &query);
    match fetch_web_search_body(&html_url, timeout) {
        Ok(body) => {
            let results = parse_ddg_html(&body);
            if !results.is_empty() {
                return Ok(results);
            }
        }
        Err(error) => failures.push(format!("html backend: {error}")),
    }

    let api_url = format!(
        "{}&format=json&no_html=1",
        endpoint_with_query(api_endpoint, &query)
    );
    match fetch_web_search_body(&api_url, timeout) {
        Ok(body) => return Ok(parse_ddg_api(&body)),
        Err(error) => failures.push(format!("instant answer backend: {error}")),
    }

    if failures.len() < 2 {
        // El backend HTML respondió sin resultados: eso no es un fallo de
        // búsqueda aunque la Instant Answer API no haya podido responder.
        return Ok(Vec::new());
    }
    Err(format!(
        "web search failed on every backend: {}",
        failures.join("; ")
    ))
}

/// Formatea los resultados como bloque de contexto acotado para el prompt.
///
/// Bloque en español con lista numerada (`N. Título — URL` + snippet
/// indentado), capado a [`WEB_SEARCH_CONTEXT_MAX_CHARS`] chars: si sobra
/// texto se corta en un límite de char y se agrega `…`. Sin resultados (o sin
/// resultados utilizables) devuelve `String` vacío. Los campos se vuelven a
/// sanear por las dudas: un resultado con URL no http(s) se descarta y el
/// texto pierde tags/entidades.
pub fn format_web_context(query: &str, results: &[WebSearchResult]) -> String {
    if results.is_empty() {
        return String::new();
    }

    let display_query = sanitize_display_query(query);
    let mut body = String::new();
    let mut shown = 0usize;
    for result in results.iter().take(WEB_SEARCH_MAX_RESULTS) {
        let Some(url) = sanitize_url(&result.url) else {
            continue;
        };
        let title = sanitize_html_text_capped(&result.title, WEB_SEARCH_TITLE_MAX_CHARS);
        if title.is_empty() {
            continue;
        }
        let snippet = sanitize_html_text_capped(&result.snippet, WEB_SEARCH_SNIPPET_MAX_CHARS);
        shown += 1;
        body.push_str(&format!("{shown}. {title} — {url}"));
        if !snippet.is_empty() {
            body.push_str(&format!("\n   {snippet}"));
        }
        body.push('\n');
    }
    if shown == 0 {
        return String::new();
    }

    let context = format!("Resultados de búsqueda web para \"{display_query}\":\n{body}");
    truncate_chars_with_ellipsis(context.trim_end(), WEB_SEARCH_CONTEXT_MAX_CHARS)
}

/// Extrae los resultados del HTML de `html.duckduckgo.com/html/`.
///
/// Empareja cada título (`class="result__a"`, href `uddg=` decodificado a URL
/// final) con el primer snippet (`class="result__snippet"`) que aparece entre
/// ese título y el siguiente. Título o URL inválidos descartan el resultado.
/// Tolera `<b>` dentro del texto, entidades HTML, hrefs directos y resultados
/// sin snippet. Devuelve como mucho [`WEB_SEARCH_MAX_RESULTS`] resultados.
pub fn parse_ddg_html(html: &str) -> Vec<WebSearchResult> {
    let titles = collect_anchors(html, "result__a");
    let snippets = collect_anchors(html, "result__snippet");

    let mut results = Vec::new();
    for (index, title) in titles.iter().enumerate() {
        if results.len() >= WEB_SEARCH_MAX_RESULTS {
            break;
        }
        let Some(url) = title.href.as_deref().and_then(decode_uddg) else {
            continue;
        };
        let next_start = titles
            .get(index + 1)
            .map(|next| next.start)
            .unwrap_or(usize::MAX);
        let snippet = snippets
            .iter()
            .find(|candidate| candidate.start > title.start && candidate.start < next_start)
            .map(|candidate| {
                sanitize_html_text_capped(&candidate.raw_text, WEB_SEARCH_SNIPPET_MAX_CHARS)
            })
            .unwrap_or_default();
        let title_text = sanitize_html_text_capped(&title.raw_text, WEB_SEARCH_TITLE_MAX_CHARS);
        if title_text.is_empty() {
            continue;
        }
        results.push(WebSearchResult {
            title: title_text,
            url,
            snippet,
        });
    }
    results
}

/// Extrae resultados de la Instant Answer API (`format=json&no_html=1`).
///
/// Si hay `Abstract` + `AbstractURL` válidos, el abstract va primero (título:
/// `Heading`, o el propio abstract si no hay heading). Después aplana un nivel
/// de `RelatedTopics` (items `{Text, FirstURL}` o `{Name, Topics:[...]}`).
/// Total capado a [`WEB_SEARCH_MAX_RESULTS`].
pub fn parse_ddg_api(json: &str) -> Vec<WebSearchResult> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };

    let mut results = Vec::new();
    let heading = value
        .get("Heading")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let abstract_text = value
        .get("Abstract")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let abstract_url = value
        .get("AbstractURL")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if !abstract_text.trim().is_empty() {
        if let Some(url) = decode_uddg(abstract_url) {
            let title_source = if heading.trim().is_empty() {
                abstract_text
            } else {
                heading
            };
            let title = sanitize_html_text_capped(title_source, WEB_SEARCH_TITLE_MAX_CHARS);
            if !title.is_empty() {
                results.push(WebSearchResult {
                    title,
                    url,
                    snippet: sanitize_html_text_capped(abstract_text, WEB_SEARCH_SNIPPET_MAX_CHARS),
                });
            }
        }
    }

    if let Some(topics) = value
        .get("RelatedTopics")
        .and_then(|value| value.as_array())
    {
        for topic in topics {
            if results.len() >= WEB_SEARCH_MAX_RESULTS {
                break;
            }
            push_api_topic(topic, &mut results);
        }
    }
    results
}

/// Decodifica un href de resultado a una URL final http(s).
///
/// Acepta el redirect de DDG (`//duckduckgo.com/l/?uddg=...`, también en
/// variante absoluta o path relativo) y decodifica el percent-encoding del
/// valor de `uddg` (en `uddg` el `+` es literal, no espacio). Los hrefs
/// http(s) directos se devuelven tal cual. Cualquier otro esquema
/// (`javascript:`, `data:`, `ftp:`, ...) o href sin URL final devuelve `None`.
/// Las entidades HTML del atributo (`&amp;`) se decodifican antes de parsear.
pub fn decode_uddg(href: &str) -> Option<String> {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return None;
    }
    let decoded = decode_html_entities(trimmed);
    let lower = decoded.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
    {
        return None;
    }

    let relative = lower.starts_with("//") || lower.starts_with('/');
    let ddg_redirect = lower.starts_with("//duckduckgo.com/l/")
        || lower.starts_with("https://duckduckgo.com/l/")
        || lower.starts_with("http://duckduckgo.com/l/");
    if relative || ddg_redirect {
        let value = query_param(&decoded, "uddg")?;
        return sanitize_url(&percent_decode(&value));
    }
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return sanitize_url(&decoded);
    }
    None
}

/// Saneador de texto HTML: quita tags `<...>`, decodifica entidades, colapsa
/// espacios/saltos repetidos y capa a [`WEB_SEARCH_SNIPPET_MAX_CHARS`] chars.
///
/// El cap es por chars (nunca parte un multibyte). Para títulos se usa un cap
/// interno de `WEB_SEARCH_TITLE_MAX_CHARS` (200) vía el mismo camino.
pub fn sanitize_html_text(raw: &str) -> String {
    sanitize_html_text_capped(raw, WEB_SEARCH_SNIPPET_MAX_CHARS)
}

// ---------------------------------------------------------------------------
// Parseo puro (sin red)
// ---------------------------------------------------------------------------

/// Un tag `<a ...>` (o similar) con el `href` crudo y su texto interno.
struct RawAnchor {
    /// Offset del marker de clase dentro del HTML (ordena el documento).
    start: usize,
    href: Option<String>,
    raw_text: String,
}

/// Junta los tags que contienen `marker` como clase, en orden de documento.
fn collect_anchors(html: &str, marker: &str) -> Vec<RawAnchor> {
    let mut anchors = Vec::new();
    let mut search_from = 0usize;
    while let Some(relative) = html[search_from..].find(marker) {
        let marker_pos = search_from + relative;
        search_from = marker_pos + marker.len();

        // Evita falsos positivos dentro de tokens más largos (p. ej.
        // `result__abstract` contiene `result__a`).
        if html
            .as_bytes()
            .get(marker_pos + marker.len())
            .map(|byte| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'-')
            .unwrap_or(false)
        {
            continue;
        }
        let Some(tag_start) = html[..marker_pos].rfind('<') else {
            continue;
        };
        let Some(tag_len) = scan_tag_len(&html[tag_start..]) else {
            continue;
        };
        let tag_end = tag_start + tag_len;
        let tag = &html[tag_start..tag_end];
        let Some(name) = tag_name(tag) else {
            continue;
        };
        let href = attribute_value(tag, "href");
        let after = &html[tag_end..];
        let closing = format!("</{name}>");
        let (raw_text, next_from) = match find_ascii_case_insensitive(after, &closing) {
            Some(close_pos) => (&after[..close_pos], tag_end + close_pos + closing.len()),
            None => (after, html.len()),
        };
        anchors.push(RawAnchor {
            start: marker_pos,
            href,
            raw_text: raw_text.to_string(),
        });
        search_from = next_from;
    }
    anchors
}

/// Empuja un item de `RelatedTopics` (hoja o grupo de un nivel).
fn push_api_topic(topic: &serde_json::Value, results: &mut Vec<WebSearchResult>) {
    if results.len() >= WEB_SEARCH_MAX_RESULTS {
        return;
    }
    if let Some(text) = topic.get("Text").and_then(|value| value.as_str()) {
        let first_url = topic
            .get("FirstURL")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        push_api_leaf(text, first_url, results);
        return;
    }
    if let Some(children) = topic.get("Topics").and_then(|value| value.as_array()) {
        for child in children {
            if results.len() >= WEB_SEARCH_MAX_RESULTS {
                break;
            }
            if let Some(text) = child.get("Text").and_then(|value| value.as_str()) {
                let first_url = child
                    .get("FirstURL")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                push_api_leaf(text, first_url, results);
            }
        }
    }
}

/// Agrega una hoja `{Text, FirstURL}` de la Instant Answer API.
fn push_api_leaf(text: &str, first_url: &str, results: &mut Vec<WebSearchResult>) {
    let Some(url) = decode_uddg(first_url) else {
        return;
    };
    let title = sanitize_html_text_capped(text, WEB_SEARCH_TITLE_MAX_CHARS);
    if title.is_empty() {
        return;
    }
    results.push(WebSearchResult {
        title,
        url,
        snippet: sanitize_html_text_capped(text, WEB_SEARCH_SNIPPET_MAX_CHARS),
    });
}

fn sanitize_html_text_capped(raw: &str, max_chars: usize) -> String {
    let without_tags = strip_html_tags(raw);
    let decoded = decode_html_entities(&without_tags);
    let collapsed = collapse_whitespace(&decoded);
    truncate_chars(&collapsed, max_chars).to_string()
}

/// Quita tags `<...>`; un `<` sin `>` próximo queda como texto literal.
fn strip_html_tags(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut index = 0usize;
    while index < raw.len() {
        if raw.as_bytes()[index] == b'<' {
            if let Some(close) = raw[index..].find('>') {
                index += close + 1;
                continue;
            }
        }
        let Some(character) = raw[index..].chars().next() else {
            break;
        };
        out.push(character);
        index += character.len_utf8();
    }
    out
}

/// Decodifica las entidades HTML básicas y numéricas (`&#39;`, `&#x27;`).
fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let bytes = rest.as_bytes();
        // Las entidades HTML son ASCII: un `;` dentro de los primeros 12
        // bytes siempre cae en un límite de char válido.
        let semicolon = bytes
            .iter()
            .enumerate()
            .skip(1)
            .take(11)
            .find(|(_, byte)| **byte == b';')
            .map(|(offset, _)| offset);
        let Some(semicolon) = semicolon else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        match decode_html_entity(&rest[1..semicolon]) {
            Some(character) => {
                out.push(character);
                rest = &rest[semicolon + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        "hellip" => Some('…'),
        "mdash" => Some('—'),
        "ndash" => Some('–'),
        _ => {
            let digits = entity.strip_prefix('#')?;
            let code = if let Some(hex) = digits
                .strip_prefix('x')
                .or_else(|| digits.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                digits.parse::<u32>().ok()?
            };
            char::from_u32(code)
        }
    }
}

/// Colapsa espacios/saltos repetidos a un único espacio y hace trim.
fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_space = false;
    for character in input.chars() {
        if character.is_whitespace() {
            if !out.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(character);
        }
    }
    out
}

/// Devuelve el prefijo de `text` con hasta `max_chars` chars (sin partir
/// multibyte).
fn truncate_chars(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

/// Corta a `max_chars` chars y agrega `…` si hubo recorte (el resultado total
/// nunca supera `max_chars`).
fn truncate_chars_with_ellipsis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut truncated = truncate_chars(text, max_chars.saturating_sub(1)).to_string();
    truncated.push('…');
    truncated
}

/// Query tal cual para mostrar en el encabezado del contexto (sin `Err`).
fn sanitize_display_query(query: &str) -> String {
    let collapsed = collapse_whitespace(query);
    truncate_chars(&collapsed, WEB_SEARCH_QUERY_MAX_CHARS).to_string()
}

/// Sólo deja URLs http(s) sin NUL ni controles, capadas a
/// `WEB_SEARCH_URL_MAX_CHARS` chars.
fn sanitize_url(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    if trimmed.chars().any(char::is_control) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return None;
    }
    Some(truncate_chars(trimmed, WEB_SEARCH_URL_MAX_CHARS).to_string())
}

/// Primer valor de `key` en el query string de `url` (comparación exacta).
fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=') {
            if name == key {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Percent-decoding a mano (`%XX`); no convierte `+` (eso es sólo para
/// `application/x-www-form-urlencoded`, no para `uddg`).
fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(byte);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// `find` ASCII case-insensitive sin asignar ni exigir límites de char en la
/// aguja (sólo se usa con agujas ASCII).
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|start| haystack[*start..*start + needle.len()].eq_ignore_ascii_case(needle))
}

/// Largo del tag que arranca en `input` (incluye el `>`), respetando comillas.
fn scan_tag_len(input: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    for (index, byte) in input.bytes().enumerate() {
        match (quote, byte) {
            (None, b'"') | (None, b'\'') => quote = Some(byte),
            (Some(open), close) if open == close => quote = None,
            (None, b'>') => return Some(index + 1),
            _ => {}
        }
    }
    None
}

/// Nombre del tag (`a`, `div`, ...) o `None` si no es un tag.
fn tag_name(tag: &str) -> Option<&str> {
    let rest = tag.strip_prefix('<')?;
    let length = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric())
        .count();
    if length == 0 {
        return None;
    }
    rest.get(..length)
}

/// Valor de un atributo del tag, con comillas simples/dobles o sin comillas.
fn attribute_value(tag: &str, name: &str) -> Option<String> {
    let mut search_from = 0usize;
    while search_from < tag.len() {
        let relative = find_ascii_case_insensitive(&tag[search_from..], name)?;
        let position = search_from + relative;
        let before_ok = position == 0
            || tag.as_bytes()[position - 1].is_ascii_whitespace()
            || tag.as_bytes()[position - 1] == b'<';
        if before_ok {
            let after = tag[position + name.len()..].trim_start();
            if let Some(value_part) = after.strip_prefix('=') {
                let value_part = value_part.trim_start();
                let quote = *value_part.as_bytes().first()?;
                if quote == b'"' || quote == b'\'' {
                    let rest = &value_part[1..];
                    let close = rest.find(quote as char)?;
                    return Some(rest[..close].to_string());
                }
                let end = value_part
                    .find(|character: char| character.is_whitespace() || character == '>')
                    .unwrap_or(value_part.len());
                return Some(value_part[..end].to_string());
            }
        }
        search_from = position + name.len();
    }
    None
}

// ---------------------------------------------------------------------------
// Transporte (sólo con `assistant-net`)
// ---------------------------------------------------------------------------

#[cfg(feature = "assistant-net")]
use std::time::Duration;

/// Valida/recorta la query: vacía o con NUL es `Err`, > cap se recorta por
/// chars (sin partir multibyte).
#[cfg(feature = "assistant-net")]
fn sanitize_web_query(query: &str) -> Result<String, String> {
    if query.contains('\0') {
        return Err("web search query contains a NUL character".into());
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("web search query is empty".into());
    }
    Ok(truncate_chars(trimmed, WEB_SEARCH_QUERY_MAX_CHARS).to_string())
}

/// GET al backend con el timeout pedido; acepta cualquier status 2xx
/// (incluye 202) y capa el cuerpo a `WEB_SEARCH_RESPONSE_MAX_BYTES`.
#[cfg(feature = "assistant-net")]
fn fetch_web_search_body(url: &str, timeout: Duration) -> Result<String, String> {
    use std::io::Read;

    let client = crate::shared_http_client()?;
    let response = client
        .get(url)
        .header("User-Agent", WEB_SEARCH_USER_AGENT)
        .timeout(timeout)
        .send()
        .map_err(|error| format!("web search request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("web search backend responded with HTTP {status}"));
    }
    let mut bytes = Vec::new();
    response
        .take(WEB_SEARCH_RESPONSE_MAX_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("web search response could not be read: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Arma `endpoint?q=...` contemplando `?`/`&` ya presentes en el endpoint.
#[cfg(feature = "assistant-net")]
fn endpoint_with_query(endpoint: &str, query: &str) -> String {
    let separator = match endpoint.chars().last() {
        Some('?') | Some('&') => "",
        _ if endpoint.contains('?') => "&",
        _ => "?",
    };
    format!("{endpoint}{separator}q={}", percent_encode_query(query))
}

/// Percent-encoding de query string (`%XX` sobre los bytes UTF-8).
#[cfg(feature = "assistant-net")]
fn percent_encode_query(input: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0F) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "assistant-net")]
    use std::time::Duration;

    /// Fixture con el snippet real verificado en el box (href `uddg=` +
    /// `&amp;rut=`) más dos resultados inventados: uno con entidades/`<b>` y
    /// otro sin snippet.
    const DDG_HTML_FIXTURE: &str = r#"<!DOCTYPE html>
<html><body>
<div class="serp__results">
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.grafito.org%2F&amp;rut=3cf4">Grafito</a>
      </h2>
      <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.grafito.org%2F&amp;rut=3cf4">Pizarra <b>geométrica</b> con cerebro en Rust &amp; piel egui. 1&nbsp;&lt; 2 &amp;&amp; 3 &gt; 2</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fdocs%3Fpage%3D1%26lang%3Des">Documentación de ejemplo</a>
      </h2>
      <a class="result__snippet" href="https://example.com/docs?page=1&amp;lang=es">Snippet con &#x27;comillas&#x27; y &#39;ap&#243;strofes&#39;.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title">
        <a rel="nofollow" class="result__a" href="https://www.rust-lang.org/">Rust</a>
      </h2>
    </div>
  </div>
</div>
</body></html>"#;

    #[test]
    fn parse_ddg_html_extracts_results_and_sanitizes_text() {
        let results = parse_ddg_html(DDG_HTML_FIXTURE);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Grafito");
        assert_eq!(results[0].url, "https://www.grafito.org/");
        assert_eq!(
            results[0].snippet,
            "Pizarra geométrica con cerebro en Rust & piel egui. 1 < 2 && 3 > 2"
        );
        assert_eq!(results[1].title, "Documentación de ejemplo");
        assert_eq!(results[1].url, "https://example.com/docs?page=1&lang=es");
        assert_eq!(results[1].snippet, "Snippet con 'comillas' y 'apóstrofes'.");
        assert_eq!(results[2].title, "Rust");
        assert_eq!(results[2].url, "https://www.rust-lang.org/");
        assert!(results[2].snippet.is_empty());
        assert!(!results[0].title.contains('<'));
        assert!(!results[0].snippet.contains("<b>"));
    }

    #[test]
    fn parse_ddg_html_returns_empty_without_results() {
        let html = "<html><body><div class=\"no-results\">No results.</div>\
                    <a class=\"result__url\" href=\"https://example.com/\">example.com</a></body></html>";
        assert!(parse_ddg_html(html).is_empty());
    }

    #[test]
    fn decode_uddg_unwraps_relative_redirect() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.grafito.org%2F&rut=3cf4";
        assert_eq!(
            decode_uddg(href).as_deref(),
            Some("https://www.grafito.org/")
        );
        let escaped = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.grafito.org%2F&amp;rut=3cf4";
        assert_eq!(
            decode_uddg(escaped).as_deref(),
            Some("https://www.grafito.org/")
        );
    }

    #[test]
    fn decode_uddg_keeps_direct_https() {
        let href = "https://example.com/a?b=1";
        assert_eq!(decode_uddg(href).as_deref(), Some(href));
    }

    #[test]
    fn decode_uddg_rejects_javascript_data_and_other_schemes() {
        assert_eq!(decode_uddg("javascript:alert(1)"), None);
        assert_eq!(decode_uddg("JavaScript:alert(1)"), None);
        assert_eq!(decode_uddg("data:text/html,<b>x</b>"), None);
        assert_eq!(decode_uddg("ftp://example.com/x"), None);
        assert_eq!(decode_uddg(""), None);
        assert_eq!(decode_uddg("/sin-uddg"), None);
    }

    #[test]
    fn decode_uddg_does_not_turn_literal_plus_into_space() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa+b";
        assert_eq!(
            decode_uddg(href).as_deref(),
            Some("https://example.com/a+b")
        );
    }

    #[test]
    fn parse_ddg_api_uses_abstract_first() {
        let json = r#"{
            "Heading": "Grafito",
            "Abstract": "Pizarra geométrica con cerebro en Rust.",
            "AbstractURL": "https://www.grafito.org/",
            "RelatedTopics": [
                {"Text": "Derivada - Wikipedia", "FirstURL": "https://es.wikipedia.org/wiki/Derivada"}
            ]
        }"#;
        let results = parse_ddg_api(json);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Grafito");
        assert_eq!(results[0].url, "https://www.grafito.org/");
        assert_eq!(
            results[0].snippet,
            "Pizarra geométrica con cerebro en Rust."
        );
        assert_eq!(results[1].title, "Derivada - Wikipedia");
        assert_eq!(results[1].url, "https://es.wikipedia.org/wiki/Derivada");
    }

    #[test]
    fn parse_ddg_api_flattens_nested_related_topics() {
        let json = r#"{
            "RelatedTopics": [
                {"Text": "Área - Wikipedia", "FirstURL": "https://es.wikipedia.org/wiki/%C3%81rea"},
                {"Name": "Matemática", "Topics": [
                    {"Text": "Derivada - Wikipedia", "FirstURL": "https://es.wikipedia.org/wiki/Derivada"},
                    {"Text": "Integral - Wikipedia", "FirstURL": "https://es.wikipedia.org/wiki/Integral"}
                ]}
            ]
        }"#;
        let results = parse_ddg_api(json);
        let urls: Vec<&str> = results.iter().map(|result| result.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://es.wikipedia.org/wiki/%C3%81rea",
                "https://es.wikipedia.org/wiki/Derivada",
                "https://es.wikipedia.org/wiki/Integral",
            ]
        );
    }

    #[test]
    fn parse_ddg_api_caps_results() {
        let topics: Vec<String> = (0..9)
            .map(|index| {
                format!(r#"{{"Text": "Tema {index}", "FirstURL": "https://example.com/{index}"}}"#)
            })
            .collect();
        let json = format!(r#"{{"RelatedTopics": [{}]}}"#, topics.join(","));
        let results = parse_ddg_api(&json);
        assert_eq!(results.len(), WEB_SEARCH_MAX_RESULTS);
        assert_eq!(results[0].title, "Tema 0");
        assert_eq!(results[4].title, "Tema 4");
    }

    #[test]
    fn parse_ddg_api_ignores_malformed_payloads() {
        assert!(parse_ddg_api("").is_empty());
        assert!(parse_ddg_api("not json").is_empty());
        assert!(parse_ddg_api("{}").is_empty());
        assert!(parse_ddg_api(r#"{"Abstract": "sin url"}"#).is_empty());
    }

    #[test]
    fn sanitize_html_text_strips_tags_entities_and_extra_spaces() {
        let raw = "  <b>Hola</b>\n\n  mundo &amp;  <i>compañía</i>\t<...>  ";
        assert_eq!(sanitize_html_text(raw), "Hola mundo & compañía");
        assert_eq!(sanitize_html_text("1 < 2 sin cierre"), "1 < 2 sin cierre");
    }

    #[test]
    fn sanitize_html_text_caps_by_chars_without_splitting_multibyte() {
        let raw = "á".repeat(WEB_SEARCH_SNIPPET_MAX_CHARS + 50);
        let sanitized = sanitize_html_text(&raw);
        assert_eq!(sanitized.chars().count(), WEB_SEARCH_SNIPPET_MAX_CHARS);
    }

    #[test]
    fn format_web_context_is_empty_without_results() {
        assert!(format_web_context("grafito", &[]).is_empty());
    }

    #[test]
    fn format_web_context_lists_results_in_spanish() {
        let results = parse_ddg_html(DDG_HTML_FIXTURE);
        let context = format_web_context("grafito", &results);
        assert!(
            context.starts_with(
                "Resultados de búsqueda web para \"grafito\":\n1. Grafito — https://www.grafito.org/\n   Pizarra"
            ),
            "encabezado inesperado: {context}"
        );
        assert!(context
            .contains("2. Documentación de ejemplo — https://example.com/docs?page=1&lang=es"));
        assert!(context.contains("3. Rust — https://www.rust-lang.org/"));
    }

    #[test]
    fn format_web_context_sanitizes_urls_and_text() {
        let html = r#"<a class="result__a" href="javascript:alert(1)">Malicioso</a>
            <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fseguro">Seguro</a>
            <a class="result__snippet">Texto <b>con</b> tags &amp; entidades</a>"#;
        let results = parse_ddg_html(html);
        assert_eq!(results.len(), 1);
        let context = format_web_context("seguro", &results);
        assert!(context.contains("1. Seguro — https://example.com/seguro"));
        assert!(context.contains("Texto con tags & entidades"));
        assert!(!context.contains("javascript:"));
        assert!(!context.contains("<b>"));
        assert!(!context.contains("Malicioso"));
    }

    #[test]
    fn format_web_context_drops_results_with_unsafe_urls() {
        let results = vec![
            WebSearchResult {
                title: "Malo".into(),
                url: "javascript:alert(1)".into(),
                snippet: "x".into(),
            },
            WebSearchResult {
                title: "Bueno".into(),
                url: "https://example.com/".into(),
                snippet: "ok".into(),
            },
        ];
        let context = format_web_context("q", &results);
        assert!(context.contains("1. Bueno — https://example.com/"));
        assert!(!context.contains("javascript:"));
        assert!(!context.contains("Malo"));
    }

    #[test]
    fn format_web_context_caps_at_context_budget() {
        let results: Vec<WebSearchResult> = (0..WEB_SEARCH_MAX_RESULTS)
            .map(|index| WebSearchResult {
                title: format!("Título {index} {}", "á".repeat(300)),
                url: format!("https://example.com/{index}/{}", "b".repeat(600)),
                snippet: "😀".repeat(300),
            })
            .collect();
        let context = format_web_context("consulta", &results);
        assert_eq!(context.chars().count(), WEB_SEARCH_CONTEXT_MAX_CHARS);
        assert!(context.ends_with('…'));
    }

    // -----------------------------------------------------------------------
    // Transporte con stub TCP local
    // -----------------------------------------------------------------------

    /// Stub HTTP mínimo: acepta una conexión, lee la request y escribe
    /// `HTTP/1.1 <status_line>` + body. Devuelve el endpoint y el hilo con la
    /// request cruda para poder inspeccionarla.
    #[cfg(feature = "assistant-net")]
    fn spawn_stub_server(
        status_line: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("stub binds");
        let address = listener.local_addr().expect("stub addr");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("stub accepts");
            let mut buffer = [0u8; 4096];
            let bytes = stream.read(&mut buffer).expect("stub reads");
            let request = String::from_utf8_lossy(&buffer[..bytes]).into_owned();
            write!(
                stream,
                "HTTP/1.1 {status_line}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("stub writes");
            request
        });
        (format!("http://{address}/search"), handle)
    }

    /// Endpoint en un puerto reservado y liberado: conexión rechazada segura.
    #[cfg(feature = "assistant-net")]
    fn closed_local_endpoint() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserva puerto");
        let address = listener.local_addr().expect("addr");
        drop(listener);
        format!("http://{address}/search")
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_with_html_endpoint_parses_stubbed_html() {
        let (html_endpoint, html_server) = spawn_stub_server("200 OK", DDG_HTML_FIXTURE);
        let results = web_search_with_html_endpoint(
            &html_endpoint,
            &closed_local_endpoint(),
            "grafito",
            Duration::from_secs(2),
        )
        .expect("stubbed html search succeeds");
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].title, "Grafito");
        assert_eq!(results[0].url, "https://www.grafito.org/");
        let request = html_server.join().expect("html stub joins");
        assert!(
            request.starts_with("GET /search?q=grafito "),
            "petición inesperada: {request}"
        );
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_falls_back_to_api_when_html_is_empty() {
        let (html_endpoint, html_server) =
            spawn_stub_server("200 OK", "<html><body>sin resultados</body></html>");
        let api_body = r#"{"Heading":"Grafito","Abstract":"Pizarra geométrica.","AbstractURL":"https://www.grafito.org/","RelatedTopics":[]}"#;
        let (api_endpoint, api_server) = spawn_stub_server("200 OK", api_body);
        let results = web_search_with_html_endpoint(
            &html_endpoint,
            &api_endpoint,
            "grafito",
            Duration::from_secs(2),
        )
        .expect("stubbed fallback succeeds");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Grafito");
        assert_eq!(results[0].url, "https://www.grafito.org/");
        html_server.join().expect("html stub joins");
        let api_request = api_server.join().expect("api stub joins");
        assert!(
            api_request.contains("/search?q=grafito&format=json&no_html=1"),
            "petición api inesperada: {api_request}"
        );
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_returns_empty_when_both_backends_have_no_results() {
        let (html_endpoint, html_server) =
            spawn_stub_server("200 OK", "<html><body>nada</body></html>");
        let (api_endpoint, api_server) = spawn_stub_server("200 OK", r#"{"RelatedTopics":[]}"#);
        let results = web_search_with_html_endpoint(
            &html_endpoint,
            &api_endpoint,
            "zzz",
            Duration::from_secs(2),
        )
        .expect("sin resultados no es error");
        assert!(results.is_empty());
        html_server.join().expect("html stub joins");
        api_server.join().expect("api stub joins");
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_errors_when_both_backends_fail() {
        let error = web_search_with_html_endpoint(
            &closed_local_endpoint(),
            &closed_local_endpoint(),
            "grafito",
            Duration::from_millis(500),
        )
        .expect_err("ambos backends caídos debe ser Err");
        assert!(
            error.contains("web search failed on every backend"),
            "error honesto: {error}"
        );
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_errors_when_both_backends_return_error_status() {
        let (html_endpoint, html_server) = spawn_stub_server("503 Service Unavailable", "down");
        let (api_endpoint, api_server) = spawn_stub_server("503 Service Unavailable", "down");
        let error = web_search_with_html_endpoint(
            &html_endpoint,
            &api_endpoint,
            "grafito",
            Duration::from_secs(2),
        )
        .expect_err("status de error en ambos backends debe ser Err");
        assert!(error.contains("503"), "error honesto: {error}");
        html_server.join().expect("html stub joins");
        api_server.join().expect("api stub joins");
    }

    #[cfg(feature = "assistant-net")]
    #[test]
    fn web_search_query_validation_rejects_empty_and_nul_and_caps_long() {
        assert!(sanitize_web_query("").is_err());
        assert!(sanitize_web_query("   ").is_err());
        assert!(sanitize_web_query("grafito\0malo").is_err());
        assert_eq!(
            sanitize_web_query("  grafito  ").expect("query válida"),
            "grafito"
        );
        let long = "á".repeat(WEB_SEARCH_QUERY_MAX_CHARS + 40);
        let capped = sanitize_web_query(&long).expect("se recorta, no falla");
        assert_eq!(capped.chars().count(), WEB_SEARCH_QUERY_MAX_CHARS);
        let error = web_search_with_html_endpoint(
            "http://127.0.0.1:9/",
            "http://127.0.0.1:9/",
            "   ",
            Duration::from_secs(1),
        )
        .expect_err("query vacía debe fallar antes de tocar la red");
        assert!(error.contains("empty"), "error honesto: {error}");
    }
}
