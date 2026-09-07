//! Raster liviano de fórmulas para el frente F2c (cerebro puro, sin egui/wgpu).
//!
//! Parsea el subset que ya exporta `document_to_mathml`
//! (`crates/grafito-app/src/export.rs:5771`): un `<math><mrow>` con cero o más
//! `<mtext>etiqueta: expresión</mtext>` y comentarios honestos
//! `<!-- omitido: … -->`. El resto de tags se ignora (no se inventa semántica
//! CAS: la expresión viaja como texto plano, igual que en el export).
//!
//! Fuente de bloques propia 5×7 (`GLYPH_W`×`GLYPH_H`), dibujada a mano para
//! este frente (no es Computer Modern ni DejaVu; es solo legible para el
//! canvas y el export honesto). Cubre ASCII matemático (`A-Z a-z 0-9
//! +-*/=()[]<>,.:;!?'“”%^&|_~#@$` + espacio) más `² ∑ ∫ √ π ≤ ≥ × ÷ · −`.
//! Decisión documentada (pedido F2c): **glifo ausente se omite con aviso en
//! logs (`log::warn!`), jamás `□` ni tofu silencioso**. Se avanza un espacio
//! en blanco para no colapsar el texto, se cuenta en `omitted_glyphs` y se
//! expone al llamador. Nada de texto falso: si no hay `<mtext>`, el bitmap
//! sale vacío (`is_empty`) en vez de dibujar algo inventado.
//!
//! Cotas (todo acotado): entrada MathML ≤ `MAX_TEX_MTEXT_BYTES` (8 KiB),
//! líneas ≤ `MAX_TEX_LINES` (64), caracteres por línea ≤
//! `MAX_TEX_LINE_CHARS` (256), píxeles ≤ `MAX_TEX_BITMAP_PIXELS` (1 MiP,
//! alineado con `AttachmentLimits max_pixels`). Sin `unwrap` en prod,
//! sin `panic`, MSRV 1.92.

/// Ancho de glifo en píxeles (5 columnas de tinta).
pub const GLYPH_W: u32 = 5;
/// Alto de glifo en píxeles (7 filas de tinta).
pub const GLYPH_H: u32 = 7;
/// Avance horizontal por carácter (5 + 1 de aire).
pub const GLYPH_ADVANCE: u32 = 6;
/// Avance vertical por línea (7 + 3 de aire).
pub const LINE_ADVANCE: u32 = 10;
/// Entrada MathML máxima (igual orden que `MAX_EXPR_LENGTH` × 4).
pub const MAX_TEX_MTEXT_BYTES: usize = 8_192;
/// Líneas máximas rasterizadas por documento.
pub const MAX_TEX_LINES: usize = 64;
/// Caracteres máximos por línea (se trunca, no se rechaza).
pub const MAX_TEX_LINE_CHARS: usize = 256;
/// Píxeles máximos del bitmap (1 MiP, igual que `AttachmentLimits`).
pub const MAX_TEX_BITMAP_PIXELS: usize = 1_048_576;

/// Bitmap monocromo liviano: 0 = fondo, 255 = tinta. Row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexBitmap {
    /// Ancho en píxeles (0 si vacío).
    pub width: u32,
    /// Alto en píxeles (0 si vacío).
    pub height: u32,
    /// `width*height` bytes (0/255).
    pub pixels: Vec<u8>,
}

impl TexBitmap {
    /// Bitmap vacío (nada que rasterizar: honesto, no inventa).
    pub fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }

    /// ¿Sin píxeles?
    pub fn is_empty(&self) -> bool {
        self.pixels.is_empty() || self.width == 0 || self.height == 0
    }

    /// Píxel en `(x, y)` o `None` si está fuera de cota. Sin `panic`.
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?;
        self.pixels.get(idx).copied()
    }

    /// Cantidad de píxeles con tinta (para tests y presupuestos).
    pub fn ink_pixels(&self) -> usize {
        self.pixels.iter().filter(|p| **p > 0).count()
    }
}

/// Resultado del raster: bitmap + cuántos glifos se omitieron con aviso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexRasterOutcome {
    /// Bitmap resultante (vacío si no había `<mtext>`).
    pub bitmap: TexBitmap,
    /// Líneas extraídas de `<mtext>` (ya sin escapes, truncadas a cota).
    pub lines: Vec<String>,
    /// Glifos omitidos por falta de cobertura (cada uno avisado con `log::warn!`).
    pub omitted_glyphs: usize,
}

/// Desescapa las 5 entidades XML que emite `document_to_mathml` vía `escape_xml`.
fn unescape_xml(text: &str) -> String {
    // Orden importa: `&amp;` último al escapar, primero al desescapar.
    // Implementación manual acotada (sin regex, sin `unwrap`).
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            let rest = &text[i..];
            if let Some(end) = rest.find(';') {
                if end <= 7 {
                    let entity = &rest[..end + 1];
                    let decoded = match entity {
                        "&amp;" => Some("&"),
                        "&lt;" => Some("<"),
                        "&gt;" => Some(">"),
                        "&quot;" => Some("\""),
                        "&apos;" => Some("'"),
                        _ => None,
                    };
                    if let Some(decoded) = decoded {
                        out.push_str(decoded);
                        i += entity.len();
                        continue;
                    }
                }
            }
        }
        if let Some(ch) = rest_char(text, i) {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

/// Carácter en el offset de bytes `i`, o `None` si el borde es inválido.
/// No parte UTF-8: si el slice no es boundary, avanza de a un byte.
fn rest_char(text: &str, i: usize) -> Option<char> {
    text.get(i..)?.chars().next()
}

/// Extrae los contenidos de `<mtext>…</mtext>` del subset MathML.
/// Puro y acotado: entrada > `MAX_TEX_MTEXT_BYTES` → `Err`; sin `<mtext>` →
/// `Vec` vacío (honesto). Cada línea se desescapa, se recorta a
/// `MAX_TEX_LINE_CHARS` por caracteres (nunca parte un `char`) y se capan las
/// líneas a `MAX_TEX_LINES`. Los comentarios `<!-- … -->` y tags desconocidos
/// se ignoran (igual que el export: resto → texto plano honesto).
pub fn extract_mtext_lines(mathml: &str) -> Result<Vec<String>, String> {
    if mathml.len() > MAX_TEX_MTEXT_BYTES {
        return Err(format!(
            "MathML supera {MAX_TEX_MTEXT_BYTES} bytes ({} recibidos)",
            mathml.len()
        ));
    }
    let mut lines = Vec::new();
    let mut search_from = 0;
    while lines.len() < MAX_TEX_LINES {
        let Some(open_rel) = mathml.get(search_from..).and_then(|s| s.find("<mtext>")) else {
            break;
        };
        let content_start = search_from + open_rel + "<mtext>".len();
        let Some(close_rel) = mathml.get(content_start..).and_then(|s| s.find("</mtext>")) else {
            break;
        };
        let content_end = content_start + close_rel;
        let raw = mathml.get(content_start..content_end).unwrap_or("");
        let decoded = unescape_xml(raw);
        let trimmed = decoded.trim();
        // Una entrada `<mtext>` puede traer varias líneas si la etiqueta tenía
        // saltos (no debería, pero se tolera sin inventar).
        for part in trimmed.split('\n') {
            if lines.len() >= MAX_TEX_LINES {
                break;
            }
            let clean: String = part
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect();
            let clean = clean.trim();
            if clean.is_empty() {
                continue;
            }
            let kept = if clean.chars().count() > MAX_TEX_LINE_CHARS {
                log::warn!("tex_raster: línea truncada a {MAX_TEX_LINE_CHARS} caracteres");
                clean.chars().take(MAX_TEX_LINE_CHARS).collect()
            } else {
                clean.to_string()
            };
            lines.push(kept);
        }
        search_from = content_end + "</mtext>".len();
    }
    Ok(lines)
}

/// Filas de tinta 5×7 para un carácter soportado. Cada `u8` usa los 5 bits
/// bajos (bit 4 = columna izquierda). `None` = sin cobertura → el llamador
/// omite con `log::warn!` (jamás `□`).
fn glyph_rows(ch: char) -> Option<[u8; 7]> {
    // Fuente de bloques propia 5×7. Mayúsculas y minúsculas comparten dibujo
    // salvo `x y z` que llevan descendente propio para no confundir fórmulas.
    let rows = match ch {
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '0' | 'O' | 'o' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' | 'a' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' | 'b' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' | 'c' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' | 'd' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'E' | 'e' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' | 'f' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' | 'g' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' | 'h' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' | 'i' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' | 'j' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' | 'k' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' | 'l' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' | 'm' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' | 'n' => [0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11],
        'P' | 'p' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' | 'q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' | 'r' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' | 's' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' | 't' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' | 'u' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' | 'v' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' | 'w' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'x' => [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11],
        'Y' | 'y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        'Z' | 'z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '+' => [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        '*' => [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '=' => [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '[' => [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E],
        ']' => [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E],
        '{' => [0x02, 0x04, 0x04, 0x08, 0x04, 0x04, 0x02],
        '}' => [0x08, 0x04, 0x04, 0x02, 0x04, 0x04, 0x08],
        '<' => [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02],
        '>' => [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x04, 0x04, 0x08],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        ':' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00],
        ';' => [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x04, 0x08],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '\'' => [0x04, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x0A, 0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00],
        '%' => [0x19, 0x1A, 0x02, 0x04, 0x08, 0x0B, 0x13],
        '^' => [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F],
        '#' => [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
        '$' => [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04],
        '&' => [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D],
        '|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        '~' => [0x00, 0x00, 0x0A, 0x15, 0x00, 0x00, 0x00],
        '@' => [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0E],
        // Símbolos matemáticos pedidos por F2c.
        '²' => [0x0C, 0x12, 0x02, 0x0C, 0x10, 0x1E, 0x00],
        '∑' => [0x1F, 0x10, 0x08, 0x04, 0x08, 0x10, 0x1F],
        '∫' => [0x02, 0x04, 0x04, 0x04, 0x04, 0x04, 0x08],
        '√' => [0x01, 0x01, 0x02, 0x15, 0x0A, 0x04, 0x00],
        'π' => [0x1F, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x00],
        '≤' => [0x02, 0x04, 0x08, 0x10, 0x00, 0x1F, 0x00],
        '≥' => [0x08, 0x04, 0x02, 0x01, 0x00, 0x1F, 0x00],
        '×' => [0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x00],
        '÷' => [0x00, 0x04, 0x00, 0x1F, 0x00, 0x04, 0x00],
        '·' => [0x00, 0x00, 0x00, 0x0C, 0x0C, 0x00, 0x00],
        '−' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        _ => return None,
    };
    Some(rows)
}

/// ¿El carácter tiene glifo propio? (para tests y para el aviso honesto).
pub fn has_glyph(ch: char) -> bool {
    glyph_rows(ch).is_some()
}

/// Rasteriza líneas de texto plano a bitmap. Cada glifo ausente se omite con
/// `log::warn!` (una vez por carácter distinto por llamada), se avanza un
/// espacio en blanco y se cuenta en el valor de retorno. Sin `panic`:
/// dimensiones con `checked_*`, píxeles capados a `MAX_TEX_BITMAP_PIXELS`
/// (si se excede, se devuelve `Err` honesto en vez de truncar a escondidas).
pub fn raster_lines_to_bitmap(lines: &[String]) -> Result<(TexBitmap, usize), String> {
    if lines.is_empty() {
        return Ok((TexBitmap::empty(), 0));
    }
    let capped_lines = lines.len().min(MAX_TEX_LINES);
    let mut max_chars: usize = 0;
    for line in lines.iter().take(capped_lines) {
        max_chars = max_chars.max(line.chars().count().min(MAX_TEX_LINE_CHARS));
    }
    if max_chars == 0 {
        return Ok((TexBitmap::empty(), 0));
    }
    let width = (max_chars as u32)
        .checked_mul(GLYPH_ADVANCE)
        .and_then(|w| w.checked_sub(GLYPH_ADVANCE - GLYPH_W))
        .ok_or_else(|| "tex_raster: ancho desbordado".to_string())?;
    let height = (capped_lines as u32)
        .checked_mul(LINE_ADVANCE)
        .and_then(|h| h.checked_sub(LINE_ADVANCE - GLYPH_H))
        .ok_or_else(|| "tex_raster: alto desbordado".to_string())?;
    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| "tex_raster: píxeles desbordados".to_string())?;
    if pixel_count > MAX_TEX_BITMAP_PIXELS {
        return Err(format!(
            "tex_raster: {pixel_count} píxeles exceden el máximo {MAX_TEX_BITMAP_PIXELS}"
        ));
    }
    let mut pixels = vec![0u8; pixel_count];
    let mut omitted: usize = 0;
    let mut warned: Vec<char> = Vec::new();
    for (line_idx, line) in lines.iter().take(capped_lines).enumerate() {
        let y0 = (line_idx as u32).saturating_mul(LINE_ADVANCE);
        for (col_idx, ch) in line.chars().take(MAX_TEX_LINE_CHARS).enumerate() {
            let x0 = (col_idx as u32).saturating_mul(GLYPH_ADVANCE);
            let Some(rows) = glyph_rows(ch) else {
                omitted += 1;
                if !warned.contains(&ch) {
                    warned.push(ch);
                    log::warn!(
                        "tex_raster: glifo sin cobertura omitido: {ch:?} (línea {})",
                        line_idx + 1
                    );
                }
                continue;
            };
            for (dy, row) in rows.iter().enumerate() {
                for dx in 0..GLYPH_W {
                    // Bit 4 = columna izquierda (ver `glyph_rows`).
                    let bit = 4u32.saturating_sub(dx);
                    if (row >> bit) & 1 == 0 {
                        continue;
                    }
                    let x = x0.saturating_add(dx);
                    let y = y0.saturating_add(dy as u32);
                    if x >= width || y >= height {
                        continue;
                    }
                    let idx = (y as usize)
                        .checked_mul(width as usize)
                        .and_then(|base| base.checked_add(x as usize));
                    if let Some(idx) = idx {
                        if let Some(slot) = pixels.get_mut(idx) {
                            *slot = 255;
                        }
                    }
                }
            }
        }
    }
    Ok((
        TexBitmap {
            width,
            height,
            pixels,
        },
        omitted,
    ))
}

/// Entrada principal F2c: MathML del subset → bitmap honesto.
/// `Err` solo por cotas (entrada gigante o bitmap gigante); sin `<mtext>` →
/// bitmap vacío con 0 omitidos (no se inventa nada).
pub fn render_mathml_subset_to_bitmap(mathml: &str) -> Result<TexRasterOutcome, String> {
    let lines = extract_mtext_lines(mathml)?;
    if lines.is_empty() {
        return Ok(TexRasterOutcome {
            bitmap: TexBitmap::empty(),
            lines: Vec::new(),
            omitted_glyphs: 0,
        });
    }
    let (bitmap, omitted) = raster_lines_to_bitmap(&lines)?;
    Ok(TexRasterOutcome {
        bitmap,
        lines,
        omitted_glyphs: omitted,
    })
}

/// Atajo honesto para texto plano matemático (una o varias líneas `\n`).
/// Misma fuente y misma política de omisión con aviso que el subset MathML.
pub fn render_plain_math_to_bitmap(text: &str) -> Result<TexRasterOutcome, String> {
    if text.len() > MAX_TEX_MTEXT_BYTES {
        return Err(format!(
            "texto supera {MAX_TEX_MTEXT_BYTES} bytes ({} recibidos)",
            text.len()
        ));
    }
    let mut lines = Vec::new();
    for part in text.split('\n').take(MAX_TEX_LINES) {
        let clean: String = part
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        let clean = clean.trim();
        if clean.is_empty() {
            continue;
        }
        let kept = if clean.chars().count() > MAX_TEX_LINE_CHARS {
            log::warn!("tex_raster: línea truncada a {MAX_TEX_LINE_CHARS} caracteres");
            clean.chars().take(MAX_TEX_LINE_CHARS).collect()
        } else {
            clean.to_string()
        };
        lines.push(kept);
    }
    if lines.is_empty() {
        return Ok(TexRasterOutcome {
            bitmap: TexBitmap::empty(),
            lines: Vec::new(),
            omitted_glyphs: 0,
        });
    }
    let (bitmap, omitted) = raster_lines_to_bitmap(&lines)?;
    Ok(TexRasterOutcome {
        bitmap,
        lines,
        omitted_glyphs: omitted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mtext_recupera_funciones_y_puntos_con_escapes() {
        let mathml = "<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">\n<mrow>\n<mtext>f: x*x+1</mtext>\n<mtext>A = (1, 2)</mtext>\n<!-- omitido: Circle (solo funciones y puntos en MathML) -->\n</mrow>\n</math>\n";
        let lines = extract_mtext_lines(mathml).expect("subset válido");
        assert_eq!(
            lines,
            vec!["f: x*x+1".to_string(), "A = (1, 2)".to_string()]
        );
    }

    #[test]
    fn extract_mtext_desescapa_y_vacio_es_honesto() {
        let hostile = "<math><mrow><mtext>&lt;b&gt;&amp;&quot;</mtext></mrow></math>";
        let lines = extract_mtext_lines(hostile).expect("hostil válido");
        assert_eq!(lines, vec!["<b>&\"".to_string()]);
        let empty = "<math><mrow>\n</mrow>\n</math>\n";
        let lines = extract_mtext_lines(empty).expect("vacío válido");
        assert!(lines.is_empty());
    }

    #[test]
    fn raster_cubre_ascii_matematico_y_simbolos_pedidos() {
        for ch in ['x', '²', '∑', '∫', '√', 'π', '=', '+', '(', '1'] {
            assert!(has_glyph(ch), "falta glifo pedido: {ch:?}");
        }
        let outcome = render_plain_math_to_bitmap("y = x²\n∑ ∫ √ π").expect("raster válido");
        assert!(!outcome.bitmap.is_empty());
        assert!(outcome.bitmap.ink_pixels() > 0);
        assert_eq!(outcome.omitted_glyphs, 0);
        // Dos líneas: alto = 2*10-3 = 17.
        assert_eq!(
            outcome.bitmap.height,
            2 * LINE_ADVANCE - (LINE_ADVANCE - GLYPH_H)
        );
    }

    #[test]
    fn raster_omite_con_aviso_y_jamas_dibuja_tofu() {
        // 😀 no tiene cobertura: se omite, se cuenta, el resto sigue.
        let outcome = render_plain_math_to_bitmap("A😀B").expect("raster válido");
        assert_eq!(outcome.omitted_glyphs, 1);
        let ab = render_plain_math_to_bitmap("AB").expect("raster válido");
        // El omitido avanza un espacio en blanco (no colapsa): más ancho…
        assert_eq!(
            outcome.bitmap.width,
            3 * GLYPH_ADVANCE - (GLYPH_ADVANCE - GLYPH_W)
        );
        assert_eq!(
            ab.bitmap.width,
            2 * GLYPH_ADVANCE - (GLYPH_ADVANCE - GLYPH_W)
        );
        // …pero la misma tinta (jamás □ ni tofu).
        assert!(outcome.bitmap.ink_pixels() == ab.bitmap.ink_pixels());
    }

    #[test]
    fn raster_respeta_cotas_sin_panic() {
        let big = "x".repeat(MAX_TEX_MTEXT_BYTES + 1);
        assert!(render_plain_math_to_bitmap(&big).is_err());
        assert!(extract_mtext_lines(&big).is_err());
        let many = vec!["y = x".to_string(); MAX_TEX_LINES + 10];
        let (bitmap, _) = raster_lines_to_bitmap(&many).expect("capa líneas");
        assert_eq!(
            bitmap.height,
            (MAX_TEX_LINES as u32) * LINE_ADVANCE - (LINE_ADVANCE - GLYPH_H)
        );
    }

    #[test]
    fn mathml_sin_mtext_da_bitmap_vacio_honesto() {
        let mathml = "<math><mrow><!-- omitido: Circle --></mrow></math>";
        let outcome = render_mathml_subset_to_bitmap(mathml).expect("vacío válido");
        assert!(outcome.bitmap.is_empty());
        assert!(outcome.lines.is_empty());
        assert_eq!(outcome.omitted_glyphs, 0);
    }
}
