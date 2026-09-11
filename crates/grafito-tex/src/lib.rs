//! F3 LaTeX real offline (Fase B): `pulldown-latex` → `formulary` → SVG / tiny-skia.
//!
//! Pipeline puro (sin E/S, sin spawn, sin red): la entrada LaTeX se convierte
//! a MathML Core (`pulldown-latex` 0.8.0, MSRV 1.74.1, MIT), el MathML se
//! maqueta con las métricas OpenType MATH de la fuente embebida
//! (`formulary` 0.2.0 con `default-features=false` + `svg`, MSRV 1.87,
//! sin `rustybuzz`: sin shaping GPOS, avances y métricas MATH reales) y la
//! display-list (glyph-ids + posiciones + reglas) sale como SVG autocontenido
//! (glifos outlineados vía `ttf-parser`, sin referencias a fuentes) o como
//! píxeles RGBA vía `tiny-skia` 0.11.4 + `ttf-parser` 0.25.1 (mismas versiones
//! del lock de `grafito-app`, modelo `sk_anim_font`/`draw_text_block`).
//!
//! `math-core` (1.96) PROHIBIDO por MSRV: no se usa ni directa ni
//! transitivamente (verificado vía crates.io API: `pulldown-latex` 0.8.0 solo
//! trae `bumpalo`/`heck`/`inventory`, puro Rust).
//!
//! Contrato honesto: cualquier `Err` (entrada sobre presupuesto, parse,
//! fuente sin tabla MATH, SVG/bitmap sobre cota) lo mapea el llamador al
//! subset `draw_math` de `grafito-ui` + aviso visible. Jamás tofu: sin fuente
//! MATH no hay `Ok` con cajas vacías.
//!
//! Presupuestos espejo (ver `docs/architecture.md` §8): entrada ≤8 KiB,
//! SVG ≤64 KiB, bitmap ≤1 MiP, nodos ≤256 (paridad con
//! `MAX_MATH_PARSE_NODES` del subset).
//!
//! Tracking fino GPOS (módulo [`gpos`]): post-pase puro con `ttf-parser`
//! sobre la display-list (kerning PairPos `kern`/`dist` latn+DFLT + tabla
//! legacy `kern`; marcas MarkToBase/Ligature como consulta real). Corre
//! dentro de `mathml_to_layout`, así lo comparten `latex_to_svg` y
//! `latex_to_rgba`. Cotas `GPOS_MAX_*` anti-DoS; lo no soportado (SinglePos,
//! Cursive, Context/Chain, MarkToMark, `y_placement`) queda documentado en
//! `gpos` como límite honesto, jamás inventado.

pub mod gpos;

/// Entrada LaTeX máxima en bytes (8 KiB, espejo `MAX_EXPR_LENGTH`-style).
pub const TEX_INPUT_MAX_BYTES: usize = 8 * 1024;
/// SVG máximo en bytes (64 KiB, espejo `line_cap` de `grafito-anim`).
pub const TEX_SVG_MAX_BYTES: usize = 64 * 1024;
/// Bitmap máximo en píxeles (1 MiP, paridad con `AttachmentLimits`).
pub const TEX_BITMAP_MAX_PIXELS: usize = 1_048_576;
/// Nodos/eventos máximos del parse (256, paridad `MAX_MATH_PARSE_NODES`).
pub const TEX_MAX_NODES: usize = 256;
/// Tamaño de fuente por defecto del layout en px (display-list independiente
/// de resolución: el llamador re-maqueta si cambia el tamaño destino).
pub const TEX_DEFAULT_FONT_PX: f32 = 24.0;
/// Tamaño de fuente máximo aceptado en `latex_to_rgba` (cota anti-OOM).
pub const TEX_MAX_FONT_PX: f32 = 96.0;
/// Tamaño de fuente mínimo aceptado en `latex_to_rgba`.
pub const TEX_MIN_FONT_PX: f32 = 8.0;
/// Cola de mensajes de error en chars (UTF-8 seguro, sin cortar multibyte).
pub const TEX_ERROR_TAIL_CHARS: usize = 300;

/// Error tipado del pipeline offline (mensajes en español, sin panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TexError {
    /// Entrada vacía o solo espacios.
    Empty,
    /// Entrada sobre 8 KiB.
    TooLong { got: usize },
    /// Más de 256 nodos/eventos.
    TooManyNodes { got: usize },
    /// El parser LaTeX rechazó la entrada (cola del mensaje, 300 chars).
    Parse(String),
    /// MathML malformado o layout imposible (cola, 300 chars).
    Layout(String),
    /// La fuente embebida no trae tabla MATH (fallback honesto obligatorio).
    NoMathFont(String),
    /// SVG sobre 64 KiB.
    SvgTooBig { got: usize },
    /// Bitmap sobre 1 MiP.
    BitmapTooBig { got: usize },
    /// El raster no pudo construirse (cola, 300 chars).
    Bitmap(String),
}

impl std::fmt::Display for TexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "fórmula vacía"),
            Self::TooLong { got } => {
                write!(f, "fórmula de {got} bytes, máximo {TEX_INPUT_MAX_BYTES}")
            }
            Self::TooManyNodes { got } => {
                write!(f, "fórmula con {got} nodos, máximo {TEX_MAX_NODES}")
            }
            Self::Parse(detail) => write!(f, "no se pudo leer la fórmula: {detail}"),
            Self::Layout(detail) => write!(f, "no se pudo maquetar la fórmula: {detail}"),
            Self::NoMathFont(detail) => {
                write!(f, "sin fuente matemática (tabla MATH): {detail}")
            }
            Self::SvgTooBig { got } => {
                write!(f, "SVG de {got} bytes, máximo {TEX_SVG_MAX_BYTES}")
            }
            Self::BitmapTooBig { got } => {
                write!(f, "bitmap de {got} píxeles, máximo {TEX_BITMAP_MAX_PIXELS}")
            }
            Self::Bitmap(detail) => write!(f, "no se pudo rasterizar la fórmula: {detail}"),
        }
    }
}

impl std::error::Error for TexError {}

/// Cola de un mensaje en chars (nunca corta un multibyte a la mitad).
fn tail_chars(message: &str) -> String {
    let chars: Vec<char> = message.chars().collect();
    let total = chars.len();
    let inicio = total.saturating_sub(TEX_ERROR_TAIL_CHARS);
    chars[inicio..].iter().collect()
}

/// Bytes embebidos de STIX Two Math (OFL; ver `assets/fonts/OFL-NOTA.txt`).
pub fn embedded_math_font_bytes() -> &'static [u8] {
    include_bytes!("../assets/fonts/STIXTwoMath-Regular.ttf")
}

/// ¿La fuente embebida trae tabla MATH con constantes? Barato (solo lee el
/// directorio de tablas + cabecera MATH): el preflight honesto antes de
/// prometer LaTeX real. `false` → subset + aviso, jamás tofu.
pub fn math_font_available() -> bool {
    formulary::MathFont::probe(embedded_math_font_bytes(), 0)
}

/// Carga la fuente embebida verificando la tabla MATH. Sin ella →
/// `NoMathFont` (el llamador cae al subset, no a cajas vacías).
pub fn load_math_font() -> Result<formulary::MathFont<'static>, TexError> {
    let bytes = embedded_math_font_bytes();
    if !formulary::MathFont::probe(bytes, 0) {
        return Err(TexError::NoMathFont(
            "la fuente embebida no trae tabla MATH".to_string(),
        ));
    }
    formulary::MathFont::new(bytes, 0).map_err(|e| TexError::NoMathFont(tail_chars(&e.to_string())))
}

/// Valida el presupuesto de entrada (vacío / 8 KiB / NUL). Puro.
fn check_tex_input(source: &str) -> Result<(), TexError> {
    if source.trim().is_empty() {
        return Err(TexError::Empty);
    }
    let len = source.len();
    if len > TEX_INPUT_MAX_BYTES {
        return Err(TexError::TooLong { got: len });
    }
    if source.contains('\0') {
        return Err(TexError::Parse("la fórmula contiene NUL".to_string()));
    }
    Ok(())
}

/// LaTeX → MathML Core (`String` con `<math>…`). El renderer de
/// `pulldown-latex` sigue MathML Core con recuperación de errores, así que se
/// pre-escanean los eventos: cualquier `Err` del parser es `Parse` honesto
/// (el llamador usa el subset) en vez de MathML con huecos de color error.
/// Nodos acotados a 256 (anti-OOM). Puro.
pub fn latex_to_mathml(source: &str) -> Result<String, TexError> {
    check_tex_input(source)?;
    let storage = pulldown_latex::Storage::new();
    let parser = pulldown_latex::Parser::new(source, &storage);
    let mut events: Vec<pulldown_latex::Event<'_>> = Vec::new();
    for event in parser {
        match event {
            Ok(event) => {
                events.push(event);
                if events.len() > TEX_MAX_NODES {
                    return Err(TexError::TooManyNodes { got: events.len() });
                }
            }
            Err(error) => return Err(TexError::Parse(tail_chars(&error.to_string()))),
        }
    }
    if events.is_empty() {
        return Err(TexError::Empty);
    }
    let mut mathml = String::new();
    pulldown_latex::push_mathml(
        &mut mathml,
        events.into_iter().map(Ok::<_, pulldown_latex::ParserError>),
        pulldown_latex::RenderConfig::default(),
    )
    .map_err(|e| TexError::Parse(tail_chars(&e.to_string())))?;
    if !mathml.contains("<math") {
        return Err(TexError::Parse("sin elemento <math>".to_string()));
    }
    Ok(mathml)
}

/// MathML (salido de `latex_to_mathml`, ya acotado a 256 nodos) → display-list
/// `formulary::Layout` a `font_px`. Solo XML malformado es error duro
/// (`ParseError`); elementos inválidos caen a `mrow` + `warnings` (el layout
/// sigue siendo honesto). Aplica el post-pase GPOS (`gpos::apply_gpos`, con
/// y sin diálogos: misma cinta para SVG y RGBA). Puro.
fn mathml_to_layout(
    mathml: &str,
    font: &formulary::MathFont<'_>,
    font_px: f32,
) -> Result<formulary::Layout, TexError> {
    mathml_to_layout_con_gpos(mathml, font, font_px, true)
}

/// Núcleo con interruptor GPOS (los dorados lo usan en `false` como línea
/// base sin shaping; producción siempre en `true`).
fn mathml_to_layout_con_gpos(
    mathml: &str,
    font: &formulary::MathFont<'_>,
    font_px: f32,
    aplicar_gpos: bool,
) -> Result<formulary::Layout, TexError> {
    if !font_px.is_finite() || !(TEX_MIN_FONT_PX..=TEX_MAX_FONT_PX).contains(&font_px) {
        return Err(TexError::Layout(format!(
            "tamaño {font_px} fuera de {TEX_MIN_FONT_PX}..={TEX_MAX_FONT_PX}"
        )));
    }
    let tree =
        formulary::parse(mathml).map_err(|e| TexError::Layout(tail_chars(&e.to_string())))?;
    let mut laid = formulary::layout(
        &tree,
        font,
        &formulary::LayoutOptions { font_size: font_px },
    );
    if !laid.width.is_finite() || !laid.ascent.is_finite() || !laid.descent.is_finite() {
        return Err(TexError::Layout("métricas no finitas".to_string()));
    }
    if laid.width <= 0.0 {
        return Err(TexError::Layout("ancho nulo".to_string()));
    }
    if aplicar_gpos {
        // Misma fuente embebida que ya validó `load_math_font` (MATH ok);
        // si `Face::parse` fallara igual, es `Layout` honesto, no tofu.
        let cara = ttf_parser::Face::parse(embedded_math_font_bytes(), 0)
            .map_err(|e| TexError::Layout(tail_chars(&e.to_string())))?;
        let reporte = gpos::apply_gpos(&cara, &mut laid);
        if !laid.width.is_finite() || laid.width <= 0.0 {
            return Err(TexError::Layout("ancho no finito tras GPOS".to_string()));
        }
        // Un `completo: false` no es error (topes anti-DoS inalcanzables con
        // 256 nodos): el layout sigue válido, solo parcialmente sin ajustar.
        let _ = reporte;
    }
    Ok(laid)
}

/// LaTeX → SVG autocontenido (glifos como `<path>`, sin referencias a
/// fuentes). Requiere la fuente MATH; sin ella → `NoMathFont`. Cota 64 KiB.
/// Puro.
pub fn latex_to_svg(source: &str) -> Result<String, TexError> {
    let mathml = latex_to_mathml(source)?;
    let font = load_math_font()?;
    let laid = mathml_to_layout(&mathml, &font, TEX_DEFAULT_FONT_PX)?;
    let svg = formulary::svg::to_svg(&laid, &font);
    let len = svg.len();
    if len > TEX_SVG_MAX_BYTES {
        return Err(TexError::SvgTooBig { got: len });
    }
    Ok(svg)
}

/// Bitmap RGBA (no premultiplicado) con sus dimensiones en píxeles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexBitmap {
    /// Bytes RGBA rectos (`w*h*4`), fila a fila desde arriba.
    pub rgba: Vec<u8>,
    /// Ancho en píxeles.
    pub width: usize,
    /// Alto en píxeles.
    pub height: usize,
}

/// Outliner `ttf-parser` → `tiny-skia`: cada punto del contorno (unidades de
/// fuente, Y hacia arriba desde la baseline) se coloca en píxeles (Y hacia
/// abajo). `espejado` refleja sobre `origen_x + avance/2` (radicales RTL).
struct TintaContorno<'a> {
    trazo: &'a mut tiny_skia::PathBuilder,
    escala: f32,
    origen_x: f32,
    origen_y: f32,
    avance: f32,
    espejado: bool,
}

impl TintaContorno<'_> {
    fn coloca(&self, ox: f32, oy: f32) -> (f32, f32) {
        let mut x = self.origen_x + ox * self.escala;
        if self.espejado {
            x = 2.0 * self.origen_x + self.avance - x;
        }
        (x, self.origen_y - oy * self.escala)
    }
}

impl ttf_parser::OutlineBuilder for TintaContorno<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.coloca(x, y);
        self.trazo.move_to(px, py);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (px, py) = self.coloca(x, y);
        self.trazo.line_to(px, py);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p1x, p1y) = self.coloca(x1, y1);
        let (px, py) = self.coloca(x, y);
        self.trazo.quad_to(p1x, p1y, px, py);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p1x, p1y) = self.coloca(x1, y1);
        let (p2x, p2y) = self.coloca(x2, y2);
        let (px, py) = self.coloca(x, y);
        self.trazo.cubic_to(p1x, p1y, p2x, p2y, px, py);
    }

    fn close(&mut self) {
        self.trazo.close();
    }
}

/// Convierte un `formulary::Color` (u8 recto) a `tiny_skia::Color`.
fn tinta_color(color: Option<formulary::Color>, tinta: [u8; 4]) -> tiny_skia::Color {
    match color {
        Some(c) => tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a),
        None => tiny_skia::Color::from_rgba8(tinta[0], tinta[1], tinta[2], tinta[3]),
    }
}

/// Des-premultiplica los píxeles de `tiny-skia` (guarda premultiplicado) a
/// RGBA recto. Puro, sin overflow (`u32` intermedio + clamp).
fn enderezar_alfa(rgba: &mut [u8]) {
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a < 255 {
            let enderezar = |c: u8| -> u8 {
                let v = u32::from(c).saturating_mul(255) / a;
                v.min(255) as u8
            };
            px[0] = enderezar(px[0]);
            px[1] = enderezar(px[1]);
            px[2] = enderezar(px[2]);
        }
    }
}

/// LaTeX → bitmap RGBA recto a `font_px` (8..=96). Requiere la fuente MATH;
/// sin ella → `NoMathFont`. Cota 1 MiP. Puro (el llamador lo corre en hilo).
pub fn latex_to_rgba(source: &str, font_px: f32) -> Result<TexBitmap, TexError> {
    latex_to_rgba_con_tinta(source, font_px, [0, 0, 0, 255])
}

/// Idem con tinta por defecto `[r,g,b,a]` para ítems sin color propio.
pub fn latex_to_rgba_con_tinta(
    source: &str,
    font_px: f32,
    tinta: [u8; 4],
) -> Result<TexBitmap, TexError> {
    let mathml = latex_to_mathml(source)?;
    let font = load_math_font()?;
    let laid = mathml_to_layout(&mathml, &font, font_px)?;
    // Margen de 4 px (anti-recorte de itálicas/acentos, espejo del MARGIN=2u
    // del SVG a 24 px por defecto).
    let pad: f32 = 4.0;
    let wf = laid.width + 2.0 * pad;
    let hf = laid.ascent + laid.descent + 2.0 * pad;
    if !wf.is_finite() || !hf.is_finite() {
        return Err(TexError::Bitmap("dimensiones no finitas".to_string()));
    }
    if wf <= 0.0 || hf <= 0.0 {
        return Err(TexError::Bitmap("dimensiones nulas".to_string()));
    }
    // Rango sano antes del `as` (el `as u32` de un f32 fuera de rango es UB
    // lógico: se valida primero).
    if wf > 4096.0 || hf > 4096.0 {
        return Err(TexError::Bitmap("lado mayor a 4096".to_string()));
    }
    let w = wf.ceil() as usize;
    let h = hf.ceil() as usize;
    if w == 0 || h == 0 {
        return Err(TexError::Bitmap("dimensiones nulas".to_string()));
    }
    let pixeles = w
        .checked_mul(h)
        .ok_or_else(|| TexError::Bitmap("desborde de dimensiones".to_string()))?;
    if pixeles > TEX_BITMAP_MAX_PIXELS {
        return Err(TexError::BitmapTooBig { got: pixeles });
    }
    let cara = ttf_parser::Face::parse(embedded_math_font_bytes(), 0)
        .map_err(|e| TexError::Bitmap(tail_chars(&e.to_string())))?;
    let unidades = cara.units_per_em() as f32;
    if !unidades.is_finite() || unidades <= 0.0 {
        return Err(TexError::Bitmap("units_per_em nulo".to_string()));
    }
    let mut lienzo = tiny_skia::Pixmap::new(w as u32, h as u32)
        .ok_or_else(|| TexError::Bitmap("no se pudo crear el lienzo".to_string()))?;
    // La baseline queda a `pad + ascent` desde arriba (layout: y=0 en la
    // baseline, crece hacia abajo).
    let base_y = pad + laid.ascent;
    for item in &laid.items {
        match item {
            formulary::Item::Glyph {
                id,
                x,
                y,
                size,
                advance,
                color,
                mirrored,
            } => {
                if !size.is_finite() || *size <= 0.0 {
                    continue;
                }
                let escala = size / unidades;
                if !escala.is_finite() || escala <= 0.0 {
                    continue;
                }
                let gx = pad + x;
                let gy = base_y + y;
                let mut constructor = tiny_skia::PathBuilder::new();
                {
                    let mut tinta = TintaContorno {
                        trazo: &mut constructor,
                        escala,
                        origen_x: gx,
                        origen_y: gy,
                        avance: *advance,
                        espejado: *mirrored,
                    };
                    let _ = cara.outline_glyph(ttf_parser::GlyphId(id.0), &mut tinta);
                }
                let Some(senda) = constructor.finish() else {
                    // Glifo vacío (espacio): nada que pintar, honesto.
                    continue;
                };
                let mut pintura = tiny_skia::Paint::default();
                pintura.set_color(tinta_color(*color, tinta));
                lienzo.fill_path(
                    &senda,
                    &pintura,
                    tiny_skia::FillRule::Winding,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
            formulary::Item::Rule { x, y, w, h, color } => {
                if !w.is_finite() || !h.is_finite() || *w <= 0.0 || *h <= 0.0 {
                    continue;
                }
                let recto = tiny_skia::Rect::from_xywh(pad + x, base_y + y, *w, *h);
                let Some(recto) = recto else { continue };
                let mut pintura = tiny_skia::Paint::default();
                pintura.set_color(tinta_color(*color, tinta));
                lienzo.fill_rect(recto, &pintura, tiny_skia::Transform::identity(), None);
            }
            formulary::Item::Background { x, y, w, h, color } => {
                if !w.is_finite() || !h.is_finite() || *w <= 0.0 || *h <= 0.0 {
                    continue;
                }
                let recto = tiny_skia::Rect::from_xywh(pad + x, base_y + y, *w, *h);
                let Some(recto) = recto else { continue };
                let mut pintura = tiny_skia::Paint::default();
                pintura.set_color(tiny_skia::Color::from_rgba8(
                    color.r, color.g, color.b, color.a,
                ));
                lienzo.fill_rect(recto, &pintura, tiny_skia::Transform::identity(), None);
            }
            _ => {
                // Variante futura de la display-list (`#[non_exhaustive]`):
                // se ignora en vez de romper (el SVG sigue completo).
            }
        }
    }
    let mut rgba = lienzo.take();
    enderezar_alfa(&mut rgba);
    Ok(TexBitmap {
        rgba,
        width: w,
        height: h,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tex_tests {
    use super::{
        embedded_math_font_bytes, gpos, latex_to_mathml, latex_to_rgba, latex_to_svg,
        load_math_font, math_font_available, mathml_to_layout_con_gpos, TexError,
        TEX_BITMAP_MAX_PIXELS, TEX_DEFAULT_FONT_PX, TEX_INPUT_MAX_BYTES, TEX_MAX_FONT_PX,
        TEX_MAX_NODES, TEX_MIN_FONT_PX, TEX_SVG_MAX_BYTES,
    };

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(TEX_INPUT_MAX_BYTES, 8 * 1024);
        assert_eq!(TEX_SVG_MAX_BYTES, 64 * 1024);
        assert_eq!(TEX_BITMAP_MAX_PIXELS, 1_048_576);
        assert_eq!(TEX_MAX_NODES, 256);
        assert_eq!((TEX_MIN_FONT_PX, TEX_MAX_FONT_PX), (8.0, 96.0));
    }

    #[test]
    fn fuente_embebida_con_math() {
        assert!(!embedded_math_font_bytes().is_empty());
        assert!(math_font_available(), "STIX Two Math trae MATH");
        assert!(load_math_font().is_ok());
        // Basura o fuente sin MATH → probe falso (fallback honesto dado).
        assert!(!formulary::MathFont::probe(b"hola", 0));
        assert!(!formulary::MathFont::probe(&[0u8; 64], 0));
    }

    #[test]
    fn latex_simple_a_mathml() {
        let mathml = latex_to_mathml(r"x^2 + \frac{1}{2}");
        assert!(mathml.is_ok());
        if let Ok(mathml) = mathml {
            assert!(mathml.contains("<math"));
            assert!(mathml.contains("x"));
        }
    }

    #[test]
    fn entradas_invalidas_fallan_honesto() {
        assert_eq!(latex_to_mathml(""), Err(TexError::Empty));
        assert_eq!(latex_to_mathml("   \n "), Err(TexError::Empty));
        let larga = "x+".repeat(TEX_INPUT_MAX_BYTES);
        assert!(matches!(
            latex_to_mathml(&larga),
            Err(TexError::TooLong { .. })
        ));
        assert!(matches!(latex_to_mathml("x\0y"), Err(TexError::Parse(_))));
        // Fracciones anidadas más allá de 256 nodos → TooManyNodes o Parse.
        let mut honda = "x".to_string();
        for _ in 0..300 {
            honda = format!("\\frac{{{honda}}}{{1}}");
        }
        assert!(matches!(
            latex_to_mathml(&honda),
            Err(TexError::TooManyNodes { .. }) | Err(TexError::Parse(_))
        ));
    }

    #[test]
    fn latex_a_svg_autocontenido() {
        let svg = latex_to_svg(r"\frac{\alpha_i^2}{\sqrt{\beta}}");
        assert!(svg.is_ok());
        if let Ok(svg) = svg {
            assert!(svg.starts_with("<svg"));
            assert!(svg.len() <= TEX_SVG_MAX_BYTES);
            // Glifos outlineados: paths, sin referencias a fuentes.
            assert!(svg.contains("<path"));
        }
    }

    #[test]
    fn latex_a_rgba_pinta() {
        let bmp = latex_to_rgba(r"x^2 + 1", 24.0);
        assert!(bmp.is_ok());
        if let Ok(bmp) = bmp {
            assert!(bmp.width > 0 && bmp.height > 0);
            assert_eq!(bmp.rgba.len(), bmp.width * bmp.height * 4);
            assert!(
                bmp.width * bmp.height <= TEX_BITMAP_MAX_PIXELS,
                "dentro de 1 MiP"
            );
            // Hay tinta real (no lienzo vacío = jamás tofu vacío con Ok).
            assert!(
                bmp.rgba.chunks_exact(4).any(|px| px[3] > 0),
                "algún píxel con alfa"
            );
        }
        // Tamaño absurdo → Layout honesto.
        assert!(matches!(
            latex_to_rgba("x", f32::NAN),
            Err(TexError::Layout(_))
        ));
        assert!(matches!(
            latex_to_rgba("x", 500.0),
            Err(TexError::Layout(_))
        ));
    }

    /// Anchos pre/post GPOS a 24px (dorado interno; producción usa post).
    fn anchos_pre_post(src: &str) -> (formulary::Layout, formulary::Layout) {
        let mathml = latex_to_mathml(src).unwrap();
        let font = load_math_font().unwrap();
        let pre = mathml_to_layout_con_gpos(&mathml, &font, TEX_DEFAULT_FONT_PX, false).unwrap();
        let post = mathml_to_layout_con_gpos(&mathml, &font, TEX_DEFAULT_FONT_PX, true).unwrap();
        (pre, post)
    }

    #[test]
    fn gpos_valores_unidades_pineados() {
        let cara = ttf_parser::Face::parse(embedded_math_font_bytes(), 0).unwrap();
        let gid = |c: char| cara.glyph_index(c).map(|g| g.0).unwrap();
        // Dorados medidos en STIX Two Math embebida (GPOS `kern` latn, clase):
        // A→V −100, T→o −70 (unidades de diseño, upem 1000).
        assert_eq!(gpos::par_kern_unidades(&cara, gid('A'), gid('V')), -100);
        assert_eq!(gpos::par_kern_unidades(&cara, gid('T'), gid('o')), -70);
        assert_eq!(gpos::par_kern_unidades(&cara, gid('V'), gid('A')), -105);
        // Itálicas matemáticas sin cobertura GPOS: el espaciado ya lo da MATH.
        let mi_x = cara.glyph_index('\u{1D465}').map(|g| g.0).unwrap();
        let mi_v = cara.glyph_index('\u{1D476}').map(|g| g.0).unwrap();
        assert_eq!(gpos::par_kern_unidades(&cara, mi_x, mi_v), 0);
        // Sin tabla legacy `kern` en STIX.
        assert_eq!(gpos::legacy_kern_unidades(&cara, gid('A'), gid('V')), None);
        // Sin subtablas de marcas en STIX (ni base ni ligadura).
        assert_eq!(gpos::marca_base_delta_unidades(&cara, mi_x, 729), None);
        assert_eq!(gpos::marca_ligadura_delta_unidades(&cara, mi_x, 729), None);
    }

    #[test]
    fn gpos_cotas_pineadas() {
        assert_eq!(gpos::GPOS_MAX_LOOKUPS, 32);
        assert_eq!(gpos::GPOS_MAX_SUBTABLES_POR_LOOKUP, 16);
        assert_eq!(gpos::GPOS_MAX_ITEMS, 4096);
        assert_eq!(gpos::GPOS_MAX_PARES, 4096);
        assert_eq!(gpos::GPOS_MAX_KERN_SUBTABLES, 8);
    }

    #[test]
    fn gpos_av_muestra_delta_real() {
        let (pre, post) = anchos_pre_post(r"\mathrm{AV}");
        // −100/1000×24 = −2.4px exactos en el ancho y en la V.
        assert!(
            (pre.width - post.width - 2.4).abs() < 0.01,
            "pre={} post={}",
            pre.width,
            post.width
        );
        assert_eq!(post.items.len(), pre.items.len());
        for (a, b) in pre.items.iter().zip(post.items.iter()) {
            match (a, b) {
                (formulary::Item::Glyph { x: xa, .. }, formulary::Item::Glyph { x: xb, .. }) => {
                    // La A queda, la V retrocede 2.4.
                    if *xa == 0.0 {
                        assert!((xa - xb).abs() < 1e-6);
                    } else {
                        assert!((xa - xb - 2.4).abs() < 0.01, "xa={xa} xb={xb}");
                    }
                }
                _ => panic!("se esperaban glifos"),
            }
        }
        // El SVG comparte la cinta: la V sale en 14.83, no en 17.232.
        let svg = latex_to_svg(r"\mathrm{AV}").unwrap();
        assert!(svg.contains("14.83"), "V kernada en el SVG");
        assert!(!svg.contains("17.232"), "sin resto sin kern");
        // Extremo a extremo en el raster: 33.552+8→42 pre, 31.152+8→40 post.
        let bmp = latex_to_rgba(r"\mathrm{AV}", 24.0).unwrap();
        assert_eq!(bmp.width, 40);
    }

    #[test]
    fn gpos_to_muestra_delta_real() {
        let (pre, post) = anchos_pre_post(r"\text{To}");
        // −70/1000×24 = −1.68px.
        assert!(
            (pre.width - post.width - 1.68).abs() < 0.01,
            "pre={} post={}",
            pre.width,
            post.width
        );
    }

    #[test]
    fn gpos_x2_hat_vec_frac_sin_cambio_honesto() {
        // x^2: base + superíndice en otra corrida (otro y/size) + itálicas
        // sin cobertura GPOS → idéntico (MATH ya espacia).
        let (pre, post) = anchos_pre_post(r"x^2");
        assert_eq!(pre, post);
        // \hat{x} / \vec{v}: acentos overscript MATH (no combining marks) y
        // sin subtablas mark en la fuente → idéntico, sin inventar anclas.
        let (pre, post) = anchos_pre_post(r"\hat{x}");
        assert_eq!(pre, post);
        let (pre, post) = anchos_pre_post(r"\vec{v}");
        assert_eq!(pre, post);
        // \frac: numerador y denominador en corridas apiladas (distinto y),
        // sin pares en-corrida → idéntico.
        let (pre, post) = anchos_pre_post(r"\frac{1}{2}");
        assert_eq!(pre, post);
    }

    #[test]
    fn gpos_layout_vacio_y_tope_son_noop_seguro() {
        let cara = ttf_parser::Face::parse(embedded_math_font_bytes(), 0).unwrap();
        let mut vacio = formulary::Layout {
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
            items: Vec::new(),
        };
        let rep = gpos::apply_gpos(&cara, &mut vacio);
        assert_eq!(rep, gpos::GposReport::vacio(true));
        // Más allá de GPOS_MAX_ITEMS: no se toca nada, completo=false.
        let mut grande = formulary::Layout {
            width: 10.0,
            ascent: 10.0,
            descent: 2.0,
            items: Vec::new(),
        };
        for _ in 0..=gpos::GPOS_MAX_ITEMS {
            grande.items.push(formulary::Item::Glyph {
                id: formulary::GlyphId(3),
                x: 0.0,
                y: 0.0,
                size: 24.0,
                advance: 1.0,
                color: None,
                mirrored: false,
            });
        }
        let rep = gpos::apply_gpos(&cara, &mut grande);
        assert!(!rep.completo);
        assert_eq!(rep.pares_kern, 0);
        assert_eq!(grande.width, 10.0);
    }
}
