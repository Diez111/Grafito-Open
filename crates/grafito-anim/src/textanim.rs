//! Animaciones de texto estilo Manim, en CPU y sin red.
//!
//! Cubre el requisito estrella (títulos centrados que se ven bien) más tres
//! animaciones clásicas de Manim, con tipos propios (sin egui, sin wgpu, sin
//! I/O) y presupuestos pineados:
//!
//! - [`Titulo`]: bloque centrado real (ancho medido, anclaje center/middle,
//!   multi-línea con interlineado, canvas 64..4096) + subtítulo opcional.
//! - [`WriteTexto`]: escritura por glifo (reveal progresivo con alfa por
//!   índice + cursor opcional), como el `Write(Text(..))` de Manim.
//! - [`TransformMatchingTexto`]: morph entre dos textos alineando glifos
//!   comunes por char (LCS determinista) con fade para el resto, como
//!   `TransformMatchingTex` (que aísla subcadenas con `{{..}}` y mapea con
//!   `key_map`; acá el "aislado" es el char).
//! - [`KaraokeTexto`]: highlight por palabra con tiempos para voiceover,
//!   con el mismo formato conceptual de `captions.rs` (sin tocarlo).
//!
//! ## Paridad Manim (fuentes)
//!
//! - `Write`: revela el objeto de texto de forma progresiva
//!   (`docs.manim.community`, guía "Using Text"; ejemplo canónico
//!   `self.play(Write(text))`).
//! - `TransformMatchingTex`: transforma intentando casar subcadenas
//!   renderizadas entre dos `Tex` (`manim.animation.transform_matching_parts`,
//!   v0.21.0: "A transformation trying to transform rendered LaTeX strings").
//! - `Text`/`Tex`: el texto se mide ya tipografiado; el centrado sale del
//!   ancho medido, no de adivinar.
//!
//! ## Aproximación monoespaciada (honesta, pineada por test)
//!
//! `grafito-tex` (`crates/grafito-tex/src/lib.rs`) es offline con STIX
//! embebida, pero su layout con GPOS (`mathml_to_layout_con_gpos`) es
//! `fn` privada y sus salidas públicas (`latex_to_svg`, `latex_to_rgba`) son
//! solo para math, no para texto plano; además `grafito-anim` no depende de
//! `grafito-tex` (sin dependencias nuevas por contrato). Por eso la medición
//! es propia y monoespaciada: avance `0.6em` por glifo, línea `1.2em`
//! ([`TEXTO_AVANCE_POR_EM`], [`TEXTO_ALTO_LINEA_POR_EM`]). El centrado es
//! exacto respecto del ancho medido (`offset == (canvas - texto)/2` pineado),
//! así que el título se ve bien cualquiera sea la fuente real del raster.
//!
//! ## Formato karaoke (espejo conceptual de `captions.rs`, sin tocarlo)
//!
//! | Regla | `captions.rs` | Acá |
//! |---|---|---|
//! | Texto | `1..=200` chars | [`TEXTANIM_MAX_CHARS`] `1..=200` |
//! | Ventana | `start < end`, `end <= 60_000` | igual ([`KARAOKE_MAX_DUR_MS`]) |
//! | Palabras | ordenadas, sin solape, sin `{}`/controles | igual |
//! | Reparto | división entera + resto a las primeras, 1 ms mínimo | [`KaraokeTexto::reparto`] igual |
//! | ASS | `{\k<cs>}` por palabra | el llamador emite ASS con `formatea_ass_ts`; acá solo el timing |
//!
//! ## Samplers sin allocs
//!
//! Los métodos de muestreo por frame (`muestra_en`, `alfa_en`, `activa_en`,
//! `revelados`, `cursor_*`) solo hacen aritmética sobre buffers del llamador:
//! ningún `Vec`, `String` ni `format!` en el camino caliente. La única
//! asignación vive en los constructores (`try_new`/`reparto`/`layout`).
//!
//! ## Presupuestos
//!
//! Texto `1..=200` chars ([`TEXTANIM_MAX_CHARS`], paridad
//! `captions::CAPTION_MAX_CHARS`), `1..=8` líneas visuales
//! ([`TEXTANIM_MAX_LINEAS`]), karaoke `1..=64` palabras
//! ([`TEXTANIM_MAX_PALABRAS`], holgura sobre
//! `guion::VOICEOVER_MAX_PALABRAS = 40`), canvas `64..=4096` por lado
//! (paridad `protocol::Resolution`), fuente `8..=96` px (paridad
//! `grafito-tex::TEX_MIN/MAX_FONT_PX`), karaoke dentro de 60 s
//! (paridad timeline). Todo lo que excede es `Err` honesto en rioplatense.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! (solo `std`). Sin `unwrap`/`expect` en producción.

use std::fmt::{Display, Formatter, Result as FmtResult};

// ── Presupuestos ─────────────────────────────────────────────────────────

/// Chars máximos por texto (paridad `captions::CAPTION_MAX_CHARS`; anti-OOM).
pub const TEXTANIM_MAX_CHARS: usize = 200;
/// Líneas visuales máximas (`split('\n')`; anti-OOM del layout).
pub const TEXTANIM_MAX_LINEAS: usize = 8;
/// Palabras con timing máximas del karaoke (holgura sobre
/// `guion::VOICEOVER_MAX_PALABRAS = 40`).
pub const TEXTANIM_MAX_PALABRAS: usize = 64;
/// Lado mínimo del canvas en px (paridad `protocol::Resolution`).
pub const TEXTANIM_CANVAS_MIN: u32 = 64;
/// Lado máximo del canvas en px (paridad `protocol::Resolution`).
pub const TEXTANIM_CANVAS_MAX: u32 = 4096;
/// Fuente mínima en px (paridad `grafito-tex::TEX_MIN_FONT_PX`).
pub const TEXTANIM_FONT_MIN_PX: f32 = 8.0;
/// Fuente máxima en px (paridad `grafito-tex::TEX_MAX_FONT_PX`).
pub const TEXTANIM_FONT_MAX_PX: f32 = 96.0;
/// Avance monoespaciado por glifo, en em (APROXIMACIÓN documentada arriba:
/// `grafito-tex` no expone posiciones de texto plano como lib).
pub const TEXTO_AVANCE_POR_EM: f32 = 0.6;
/// Alto de línea (interlineado), en em.
pub const TEXTO_ALTO_LINEA_POR_EM: f32 = 1.2;
/// Escala del subtítulo respecto de la fuente del título.
pub const SUBTITULO_ESCALA: f32 = 0.7;
/// Separación título↔subtítulo, en em de la fuente del título.
pub const SUBTITULO_SEPARACION_POR_EM: f32 = 0.5;
/// Alfa de las palabras no activas del karaoke (la activa va a 1.0).
pub const KARAOKE_ATENUADO: f32 = 0.35;
/// Fin máximo del karaoke en ms (60 s, paridad timeline/captions).
pub const KARAOKE_MAX_DUR_MS: u32 = 60_000;

// ── Error ────────────────────────────────────────────────────────────────

/// Error honesto de las animaciones de texto (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAnimError {
    /// Texto vacío o solo blancos.
    TextoVacio,
    /// Texto que excede el tope de chars.
    TextoLargo { got: usize, max: usize },
    /// Texto con controles o NUL (salvo `\n`/`\t` donde aplique).
    TextoConControl,
    /// Más líneas visuales que el tope.
    DemasiadasLineas { got: usize, max: usize },
    /// Fuente fuera de `8..=96` o no finita.
    FuenteInvalida,
    /// Canvas fuera de `64..=4096` por lado.
    CanvasFueraDeRango { detalle: String },
    /// Salto de línea/tab donde solo va una línea.
    SaltoNoSoportado,
    /// Ventana `start..end` con `start >= end`.
    VentanaInvalida { start: u32, end: u32 },
    /// Fin fuera de los 60 s.
    FueraDeRango { end: u32, max: u32 },
    /// Palabra con timing inválida.
    PalabraInvalida { motivo: String },
    /// Más palabras que el tope.
    DemasiadasPalabras { got: usize, max: usize },
    /// Reparto automático imposible en la duración dada.
    RepartoImposible { motivo: String },
    /// Buffer del sampler con largo distinto del esperado.
    BufferCorto { got: usize, want: usize },
}

impl Display for TextAnimError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::TextoVacio => write!(f, "texto vacío: pasame al menos 1 carácter"),
            Self::TextoLargo { got, max } => {
                write!(f, "texto de {got} chars (válido 1..={max})")
            }
            Self::TextoConControl => {
                write!(f, "texto con carácter de control: usá texto plano")
            }
            Self::DemasiadasLineas { got, max } => {
                write!(f, "{got} líneas exceden el tope de {max}: partí el título")
            }
            Self::FuenteInvalida => write!(
                f,
                "fuente fuera de {TEXTANIM_FONT_MIN_PX}..={TEXTANIM_FONT_MAX_PX} px o no finita"
            ),
            Self::CanvasFueraDeRango { detalle } => {
                write!(f, "canvas inválido ({detalle}): usá 64..=4096 por lado")
            }
            Self::SaltoNoSoportado => write!(
                f,
                "acá va una sola línea (sin saltos ni tabs): usá una animación por línea"
            ),
            Self::VentanaInvalida { start, end } => write!(
                f,
                "ventana {start}..{end} inválida: el inicio debe ser menor que el fin"
            ),
            Self::FueraDeRango { end, max } => {
                write!(f, "fin {end} ms fuera de rango (válido ..={max})")
            }
            Self::PalabraInvalida { motivo } => write!(f, "palabra inválida: {motivo}"),
            Self::DemasiadasPalabras { got, max } => {
                write!(f, "{got} palabras exceden el tope de {max}: partí el texto")
            }
            Self::RepartoImposible { motivo } => write!(f, "reparto imposible: {motivo}"),
            Self::BufferCorto { got, want } => write!(
                f,
                "buffer de {got} celdas para {want} glifos: pasalo del largo exacto"
            ),
        }
    }
}

impl std::error::Error for TextAnimError {}

// ── Easing propio (sin importar scene.rs: este módulo es autocontenido) ──

/// Easing del reveal/morph (paridad con `scene::RateFunc`: `Smooth` es el
/// smoothstep `3t²-2t³` de Manim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEasing {
    /// Suave Manim (`3t²-2t³`).
    #[default]
    Smooth,
    /// Progresión constante.
    Linear,
}

impl TextEasing {
    /// Aplica el easing a `t` (clamp 0..1 + guardia finita, sin pánicos).
    pub fn aplica(self, t: f64) -> f64 {
        let t = if t.is_finite() {
            t.clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self {
            Self::Linear => t,
            Self::Smooth => t * t * (3.0 - 2.0 * t),
        }
    }
}

// ── Validadores compartidos (construcción; pueden alocar) ─────────────────

/// Valida el canvas (paridad `protocol::Resolution` 64..=4096).
fn valida_canvas(w: u32, h: u32) -> Result<(), TextAnimError> {
    if !(TEXTANIM_CANVAS_MIN..=TEXTANIM_CANVAS_MAX).contains(&w)
        || !(TEXTANIM_CANVAS_MIN..=TEXTANIM_CANVAS_MAX).contains(&h)
    {
        return Err(TextAnimError::CanvasFueraDeRango {
            detalle: format!("{w}x{h}"),
        });
    }
    Ok(())
}

/// Valida la fuente (paridad `grafito-tex` 8..=96, finita).
fn valida_fuente(px: f32) -> Result<(), TextAnimError> {
    if !px.is_finite() || !(TEXTANIM_FONT_MIN_PX..=TEXTANIM_FONT_MAX_PX).contains(&px) {
        return Err(TextAnimError::FuenteInvalida);
    }
    Ok(())
}

/// Valida el texto base: recorta bordes, exige `1..=200` chars y rechaza
/// controles salvo `\n`/`\t` (el `\0` cae acá: también es control).
/// Devuelve el recortado (sin `\r`: pega con `\n` pelado).
fn valida_texto_base(texto: &str) -> Result<String, TextAnimError> {
    let t = texto.trim();
    if t.is_empty() {
        return Err(TextAnimError::TextoVacio);
    }
    let n = t.chars().count();
    if n > TEXTANIM_MAX_CHARS {
        return Err(TextAnimError::TextoLargo {
            got: n,
            max: TEXTANIM_MAX_CHARS,
        });
    }
    if t.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(TextAnimError::TextoConControl);
    }
    Ok(t.to_string())
}

/// Colapsa blancos a un espacio (misma cuenta que `captions::normaliza_texto`).
fn normaliza(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ── 1. Titulo ─────────────────────────────────────────────────────────────

/// Título centrado para el requisito estrella: el layout se mide con el
/// avance monoespaciado y el bloque se ancla center/middle sobre el canvas.
///
/// El subtítulo (opcional) va en una línea al 70 % debajo del título.
#[derive(Debug, Clone, PartialEq)]
pub struct Titulo {
    texto: String,
    subtitulo: Option<String>,
    font_px: f32,
    canvas_w: u32,
    canvas_h: u32,
}

impl Titulo {
    /// Constructor validado (todo `Err` honesto, sin pánicos).
    pub fn try_new(
        titulo: &str,
        subtitulo: Option<&str>,
        font_px: f32,
        canvas_w: u32,
        canvas_h: u32,
    ) -> Result<Self, TextAnimError> {
        valida_canvas(canvas_w, canvas_h)?;
        valida_fuente(font_px)?;
        let texto = valida_texto_base(titulo)?;
        let n_lineas = texto.split('\n').count();
        if n_lineas > TEXTANIM_MAX_LINEAS {
            return Err(TextAnimError::DemasiadasLineas {
                got: n_lineas,
                max: TEXTANIM_MAX_LINEAS,
            });
        }
        let subtitulo = match subtitulo {
            None => None,
            Some(s) => {
                let t = valida_texto_base(s)?;
                if t.contains('\n') || t.contains('\t') {
                    return Err(TextAnimError::SaltoNoSoportado);
                }
                Some(t)
            }
        };
        Ok(Self {
            texto,
            subtitulo,
            font_px,
            canvas_w,
            canvas_h,
        })
    }

    /// Texto del título (recortado).
    pub fn texto(&self) -> &str {
        &self.texto
    }

    /// Subtítulo, si hay.
    pub fn subtitulo(&self) -> Option<&str> {
        self.subtitulo.as_deref()
    }

    /// Fuente en px.
    pub fn font_px(&self) -> f32 {
        self.font_px
    }

    /// Canvas en px.
    pub fn canvas(&self) -> (u32, u32) {
        (self.canvas_w, self.canvas_h)
    }

    /// Mide y centra el bloque (construcción: puede alocar; el resultado es
    /// el que el raster usa después sin recalcular).
    pub fn layout(&self) -> TituloLayout {
        let avance = self.font_px * TEXTO_AVANCE_POR_EM;
        let alto = self.font_px * TEXTO_ALTO_LINEA_POR_EM;
        let cw = self.canvas_w as f32;
        let ch = self.canvas_h as f32;
        let mut lineas = Vec::new();
        let mut ancho_max = 0.0f32;
        for parte in self.texto.split('\n') {
            let w = parte.chars().count() as f32 * avance;
            if w > ancho_max {
                ancho_max = w;
            }
            lineas.push((parte.to_string(), w));
        }
        let alto_texto = lineas.len() as f32 * alto;
        let sub_datos = self.subtitulo.as_ref().map(|s| {
            let fuente = self.font_px * SUBTITULO_ESCALA;
            let av = fuente * TEXTO_AVANCE_POR_EM;
            let al = fuente * TEXTO_ALTO_LINEA_POR_EM;
            let w = s.chars().count() as f32 * av;
            (s.clone(), w, av, al)
        });
        let separacion = self.font_px * SUBTITULO_SEPARACION_POR_EM;
        let (ancho_bloque, alto_bloque) = match &sub_datos {
            None => (ancho_max, alto_texto),
            Some((_, w, _, al)) => (ancho_max.max(*w), alto_texto + separacion + *al),
        };
        // Anclaje center/middle exacto: el raster no re-deriva nada.
        let ox = (cw - ancho_bloque) / 2.0;
        let oy = (ch - alto_bloque) / 2.0;
        let mut lineas_out = Vec::with_capacity(lineas.len());
        for (i, (texto, w)) in lineas.into_iter().enumerate() {
            lineas_out.push(LineaLayout {
                x: ox + (ancho_bloque - w) / 2.0,
                y: oy + i as f32 * alto,
                ancho: w,
                alto,
                avance,
                texto,
            });
        }
        let subtitulo = sub_datos.map(|(texto, w, av, al)| LineaLayout {
            x: ox + (ancho_bloque - w) / 2.0,
            y: oy + alto_texto + separacion,
            ancho: w,
            alto: al,
            avance: av,
            texto,
        });
        let desborda = ancho_bloque > cw || alto_bloque > ch;
        TituloLayout {
            bloque_w: ancho_bloque,
            bloque_h: alto_bloque,
            offset_x: ox,
            offset_y: oy,
            lineas: lineas_out,
            subtitulo,
            desborda,
        }
    }
}

/// Una línea ya medida y posicionada (origen = esquina sup-izq).
#[derive(Debug, Clone, PartialEq)]
pub struct LineaLayout {
    /// Texto de la línea.
    pub texto: String,
    /// Ancho medido en px (`chars × avance`).
    pub ancho: f32,
    /// X de la esquina sup-izq (centrada dentro del bloque).
    pub x: f32,
    /// Y de la esquina sup-izq.
    pub y: f32,
    /// Alto de línea en px.
    pub alto: f32,
    /// Avance por glifo en px.
    pub avance: f32,
}

/// Bloque de título medido y centrado sobre el canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct TituloLayout {
    /// Ancho del bloque (máximo entre título y subtítulo).
    pub bloque_w: f32,
    /// Alto del bloque (título + separación + subtítulo si hay).
    pub bloque_h: f32,
    /// X del bloque: `(canvas_w - bloque_w)/2` exacto.
    pub offset_x: f32,
    /// Y del bloque: `(canvas_h - bloque_h)/2` exacto.
    pub offset_y: f32,
    /// Líneas del título, cada una centrada en el bloque.
    pub lineas: Vec<LineaLayout>,
    /// Subtítulo centrado, si hay.
    pub subtitulo: Option<LineaLayout>,
    /// `true` si el bloque excede el canvas (el raster recorta; el offset
    /// sigue exacto, no se finge otro centrado).
    pub desborda: bool,
}

/// Caja de un glifo del título (para el raster o el cursor del `Write`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlifoCaja {
    /// Char del glifo.
    pub ch: char,
    /// Índice de línea.
    pub linea: usize,
    /// Columna dentro de la línea.
    pub columna: usize,
    /// X de la esquina sup-izq.
    pub x: f32,
    /// Y de la esquina sup-izq.
    pub y: f32,
    /// Ancho (= avance).
    pub w: f32,
    /// Alto (= alto de línea).
    pub h: f32,
}

impl TituloLayout {
    /// Cajas de los glifos del título (construcción, no camino caliente).
    /// El subtítulo es decorativo: su métrica vive en
    /// [`TituloLayout::subtitulo`].
    pub fn cajas_glifos(&self) -> Vec<GlifoCaja> {
        let mut out = Vec::new();
        for (li, linea) in self.lineas.iter().enumerate() {
            for (col, ch) in linea.texto.chars().enumerate() {
                out.push(GlifoCaja {
                    ch,
                    linea: li,
                    columna: col,
                    x: linea.x + col as f32 * linea.avance,
                    y: linea.y,
                    w: linea.avance,
                    h: linea.alto,
                });
            }
        }
        out
    }
}

// ── 2. WriteTexto ─────────────────────────────────────────────────────────

/// Escritura por glifo estilo Manim `Write`: el progreso global `alpha`
/// 0..1 (con easing) se reparte en alfas por índice; el cursor opcional
/// marca la columna continua `eased × n`.
///
/// Solo una línea (sin `\n` ni `\t`): el multi-línea se compone con un
/// `WriteTexto` por línea (ver [`Titulo::layout`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTexto {
    glifos: Vec<char>,
    con_cursor: bool,
    easing: TextEasing,
}

impl WriteTexto {
    /// Constructor validado (texto `1..=200` chars, una línea).
    pub fn try_new(
        texto: &str,
        con_cursor: bool,
        easing: TextEasing,
    ) -> Result<Self, TextAnimError> {
        let t = valida_texto_base(texto)?;
        if t.contains('\n') || t.contains('\t') {
            return Err(TextAnimError::SaltoNoSoportado);
        }
        Ok(Self {
            glifos: t.chars().collect(),
            con_cursor,
            easing,
        })
    }

    /// Cantidad de glifos.
    pub fn len(&self) -> usize {
        self.glifos.len()
    }

    /// ¿Sin glifos? (nunca tras `try_new`, pero existe por contrato).
    pub fn is_empty(&self) -> bool {
        self.glifos.is_empty()
    }

    /// ¿Lleva cursor?
    pub fn con_cursor(&self) -> bool {
        self.con_cursor
    }

    /// Easing del reveal.
    pub fn easing(&self) -> TextEasing {
        self.easing
    }

    /// Texto completo.
    pub fn texto(&self) -> String {
        self.glifos.iter().collect()
    }

    /// Glifo en `i` (`None` honesto si está fuera).
    pub fn glifo_en(&self, i: usize) -> Option<char> {
        self.glifos.get(i).copied()
    }

    /// Progreso con easing aplicado (clamp 0..1, no finito → 0).
    pub fn eased(&self, alpha: f64) -> f64 {
        self.easing.aplica(alpha)
    }

    /// Alfa del glifo `i` en el progreso `alpha` (0..1; fuera de rango → 0).
    /// Sin allocs.
    pub fn alfa_en(&self, indice: usize, alpha: f64) -> f32 {
        if indice >= self.glifos.len() {
            return 0.0;
        }
        let e = self.eased(alpha);
        if e <= 0.0 {
            return 0.0;
        }
        if e >= 1.0 {
            return 1.0;
        }
        let pos = e * self.glifos.len() as f64;
        (pos - indice as f64).clamp(0.0, 1.0) as f32
    }

    /// Llena `salida` (largo exacto `len()`) con los alfas del frame.
    /// Sin allocs: el único camino de muestreo por frame.
    pub fn muestra_en(&self, alpha: f64, salida: &mut [f32]) -> Result<(), TextAnimError> {
        if salida.len() != self.glifos.len() {
            return Err(TextAnimError::BufferCorto {
                got: salida.len(),
                want: self.glifos.len(),
            });
        }
        let e = self.eased(alpha);
        let n = self.glifos.len() as f64;
        for (i, celda) in salida.iter_mut().enumerate() {
            let a = if e <= 0.0 {
                0.0
            } else if e >= 1.0 {
                1.0
            } else {
                (e * n - i as f64).clamp(0.0, 1.0)
            };
            *celda = a as f32;
        }
        Ok(())
    }

    /// Glifos con alfa > 0 (`ceil(eased × n)`; 0 en `alpha = 0`, `n` en 1).
    /// Sin allocs.
    pub fn revelados(&self, alpha: f64) -> usize {
        let n = self.glifos.len();
        if n == 0 {
            return 0;
        }
        let e = self.eased(alpha);
        if e <= 0.0 {
            return 0;
        }
        if e >= 1.0 {
            return n;
        }
        ((e * n as f64).ceil() as usize).min(n)
    }

    /// Glifos completos (alfa == 1; `floor(eased × n)`). Sin allocs.
    pub fn completos(&self, alpha: f64) -> usize {
        let n = self.glifos.len();
        if n == 0 {
            return 0;
        }
        let e = self.eased(alpha);
        if e <= 0.0 {
            return 0;
        }
        if e >= 1.0 {
            return n;
        }
        ((e * n as f64).floor() as usize).min(n)
    }

    /// Columna continua del cursor (`eased × n`; `None` si no lleva cursor).
    /// El raster la convierte a px con `origen_x + col × avance`. Sin allocs.
    pub fn cursor_columna(&self, alpha: f64) -> Option<f32> {
        if !self.con_cursor {
            return None;
        }
        Some((self.eased(alpha) * self.glifos.len() as f64) as f32)
    }

    /// X del cursor en px (`None` si no lleva cursor o métrica no finita).
    /// Sin allocs.
    pub fn cursor_x(&self, alpha: f64, avance: f32, origen_x: f32) -> Option<f32> {
        let col = self.cursor_columna(alpha)?;
        if !avance.is_finite() || !origen_x.is_finite() {
            return None;
        }
        let x = origen_x + col * avance;
        if x.is_finite() {
            Some(x)
        } else {
            None
        }
    }
}

// ── 3. TransformMatchingTexto ─────────────────────────────────────────────

/// Morph entre dos textos estilo `TransformMatchingTex`: los glifos comunes
/// (subsecuencia común más larga, determinista) se morphan (crossfade
/// 1→0 / 0→1 con las posiciones que el raster interpola vía los mapas) y el
/// resto hace fade out/in.
///
/// La alineación se precomputa una vez en `try_new` (O(n×m) con
/// n,m ≤ 200); el muestreo no aloca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformMatchingTexto {
    desde: Vec<char>,
    hasta: Vec<char>,
    /// Por índice de `desde`: índice emparejado en `hasta` (`None` = fade out).
    desde_hacia: Vec<Option<usize>>,
    /// Por índice de `hasta`: índice emparejado en `desde` (`None` = fade in).
    hasta_desde: Vec<Option<usize>>,
    emparejados: usize,
    easing: TextEasing,
}

/// LCS determinista: DP por sufijos + backtrack que ante empate avanza en
/// `desde` (se queda con el match más temprano de `hasta`). Puro.
fn lcs_mapas(desde: &[char], hasta: &[char]) -> (Vec<Option<usize>>, Vec<Option<usize>>, usize) {
    let n = desde.len();
    let m = hasta.len();
    let fila = m + 1;
    let mut dp = vec![0usize; (n + 1) * fila];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * fila + j] = if desde[i] == hasta[j] {
                dp[(i + 1) * fila + j + 1] + 1
            } else {
                dp[(i + 1) * fila + j].max(dp[i * fila + j + 1])
            };
        }
    }
    let mut a = vec![None; n];
    let mut b = vec![None; m];
    let mut k = 0usize;
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if desde[i] == hasta[j] {
            a[i] = Some(j);
            b[j] = Some(i);
            k += 1;
            i += 1;
            j += 1;
        } else if dp[(i + 1) * fila + j] >= dp[i * fila + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    (a, b, k)
}

impl TransformMatchingTexto {
    /// Constructor validado (ambos `1..=200` chars, una línea cada uno) con
    /// la alineación precomputada.
    pub fn try_new(desde: &str, hasta: &str, easing: TextEasing) -> Result<Self, TextAnimError> {
        let a = valida_texto_base(desde)?;
        let b = valida_texto_base(hasta)?;
        for t in [&a, &b] {
            if t.contains('\n') || t.contains('\t') {
                return Err(TextAnimError::SaltoNoSoportado);
            }
        }
        let desde_v: Vec<char> = a.chars().collect();
        let hasta_v: Vec<char> = b.chars().collect();
        let (desde_hacia, hasta_desde, emparejados) = lcs_mapas(&desde_v, &hasta_v);
        Ok(Self {
            desde: desde_v,
            hasta: hasta_v,
            desde_hacia,
            hasta_desde,
            emparejados,
            easing,
        })
    }

    /// Largo del texto origen.
    pub fn len_desde(&self) -> usize {
        self.desde.len()
    }

    /// Largo del texto destino.
    pub fn len_hasta(&self) -> usize {
        self.hasta.len()
    }

    /// Glifos emparejados (largo del LCS).
    pub fn emparejados(&self) -> usize {
        self.emparejados
    }

    /// Easing del morph.
    pub fn easing(&self) -> TextEasing {
        self.easing
    }

    /// Texto origen.
    pub fn texto_desde(&self) -> String {
        self.desde.iter().collect()
    }

    /// Texto destino.
    pub fn texto_hasta(&self) -> String {
        self.hasta.iter().collect()
    }

    /// Índice en `hasta` emparejado con `desde[i]` (`None` = fade out o
    /// fuera de rango). Sin allocs.
    pub fn hacia_desde(&self, i: usize) -> Option<usize> {
        self.desde_hacia.get(i).copied().flatten()
    }

    /// Índice en `desde` emparejado con `hasta[j]` (`None` = fade in o
    /// fuera de rango). Sin allocs.
    pub fn hacia_hasta(&self, j: usize) -> Option<usize> {
        self.hasta_desde.get(j).copied().flatten()
    }

    /// Progreso con easing aplicado. Sin allocs.
    pub fn eased(&self, alpha: f64) -> f64 {
        self.easing.aplica(alpha)
    }

    /// Alfa del glifo origen `i` (`1 - eased`; fuera de rango → 0).
    /// Sin allocs.
    pub fn alfa_desde(&self, i: usize, alpha: f64) -> f32 {
        if i >= self.desde.len() {
            return 0.0;
        }
        (1.0 - self.eased(alpha)) as f32
    }

    /// Alfa del glifo destino `j` (`eased`; fuera de rango → 0). Sin allocs.
    pub fn alfa_hasta(&self, j: usize, alpha: f64) -> f32 {
        if j >= self.hasta.len() {
            return 0.0;
        }
        self.eased(alpha) as f32
    }

    /// Llena ambos buffers (largos exactos `len_desde`/`len_hasta`) con los
    /// alfas del frame. Sin allocs.
    pub fn muestra_en(
        &self,
        alpha: f64,
        buf_desde: &mut [f32],
        buf_hasta: &mut [f32],
    ) -> Result<(), TextAnimError> {
        if buf_desde.len() != self.desde.len() {
            return Err(TextAnimError::BufferCorto {
                got: buf_desde.len(),
                want: self.desde.len(),
            });
        }
        if buf_hasta.len() != self.hasta.len() {
            return Err(TextAnimError::BufferCorto {
                got: buf_hasta.len(),
                want: self.hasta.len(),
            });
        }
        let e = self.eased(alpha) as f32;
        for celda in buf_desde.iter_mut() {
            *celda = 1.0 - e;
        }
        for celda in buf_hasta.iter_mut() {
            *celda = e;
        }
        Ok(())
    }
}

// ── 4. KaraokeTexto ───────────────────────────────────────────────────────

/// Palabra con su ventana en ms (espejo de `captions::CaptionSegment`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PalabraVoz {
    /// Texto de la palabra.
    pub texto: String,
    /// Inicio en ms.
    pub inicio_ms: u32,
    /// Fin en ms (`> inicio`, `<= 60_000`).
    pub fin_ms: u32,
}

/// Highlight por palabra con tiempos para voiceover: la palabra activa va a
/// alfa 1.0 y el resto a [`KARAOKE_ATENUADO`]. El muestreo no aloca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KaraokeTexto {
    texto: String,
    palabras: Vec<PalabraVoz>,
}

impl KaraokeTexto {
    /// Constructor validado con timings explícitos (ventanas ordenadas sin
    /// solape dentro de 60 s; palabras sin `{}`/controles, paridad captions).
    pub fn try_new(texto: &str, palabras: Vec<(String, u32, u32)>) -> Result<Self, TextAnimError> {
        let texto_ok = valida_texto_base(texto)?;
        if palabras.is_empty() {
            return Err(TextAnimError::PalabraInvalida {
                motivo: "sin palabras: el karaoke necesita al menos 1".to_string(),
            });
        }
        if palabras.len() > TEXTANIM_MAX_PALABRAS {
            return Err(TextAnimError::DemasiadasPalabras {
                got: palabras.len(),
                max: TEXTANIM_MAX_PALABRAS,
            });
        }
        let mut prev_fin: Option<u32> = None;
        let mut voces = Vec::with_capacity(palabras.len());
        for (palabra, inicio, fin) in palabras {
            let p = palabra.trim();
            if p.is_empty() {
                return Err(TextAnimError::PalabraInvalida {
                    motivo: "palabra vacía".to_string(),
                });
            }
            if p.chars().count() > TEXTANIM_MAX_CHARS {
                return Err(TextAnimError::PalabraInvalida {
                    motivo: format!("palabra de más de {TEXTANIM_MAX_CHARS} chars"),
                });
            }
            if p.contains(['{', '}']) || p.chars().any(|c| c.is_control()) {
                return Err(TextAnimError::PalabraInvalida {
                    motivo: format!("palabra {p:?} con control o llaves"),
                });
            }
            if inicio >= fin {
                return Err(TextAnimError::PalabraInvalida {
                    motivo: format!("{p:?}: inicio {inicio} no menor que fin {fin}"),
                });
            }
            if fin > KARAOKE_MAX_DUR_MS {
                return Err(TextAnimError::FueraDeRango {
                    end: fin,
                    max: KARAOKE_MAX_DUR_MS,
                });
            }
            if let Some(previo) = prev_fin {
                if inicio < previo {
                    return Err(TextAnimError::PalabraInvalida {
                        motivo: format!("{p:?} arranca en {inicio} antes del fin previo {previo}"),
                    });
                }
            }
            prev_fin = Some(fin);
            voces.push(PalabraVoz {
                texto: p.to_string(),
                inicio_ms: inicio,
                fin_ms: fin,
            });
        }
        Ok(Self {
            texto: texto_ok,
            palabras: voces,
        })
    }

    /// Reparto automático como `captions::voiceover_segments`: las palabras
    /// del texto (blancos colapsados) se reparten proporcionalmente en
    /// `[0, duracion_ms]` (división entera con el resto a las primeras, 1 ms
    /// mínimo por palabra). `Err` si la ventana no alcanza o pasa los 60 s.
    pub fn reparto(texto: &str, duracion_ms: u32) -> Result<Self, TextAnimError> {
        let normalizado = normaliza(texto);
        if normalizado.is_empty() {
            return Err(TextAnimError::TextoVacio);
        }
        if normalizado.chars().count() > TEXTANIM_MAX_CHARS {
            return Err(TextAnimError::TextoLargo {
                got: normalizado.chars().count(),
                max: TEXTANIM_MAX_CHARS,
            });
        }
        if normalizado.chars().any(|c| c.is_control()) {
            return Err(TextAnimError::TextoConControl);
        }
        if duracion_ms == 0 {
            return Err(TextAnimError::VentanaInvalida {
                start: 0,
                end: duracion_ms,
            });
        }
        if duracion_ms > KARAOKE_MAX_DUR_MS {
            return Err(TextAnimError::FueraDeRango {
                end: duracion_ms,
                max: KARAOKE_MAX_DUR_MS,
            });
        }
        let partes: Vec<&str> = normalizado.split(' ').collect();
        if partes.len() > TEXTANIM_MAX_PALABRAS {
            return Err(TextAnimError::DemasiadasPalabras {
                got: partes.len(),
                max: TEXTANIM_MAX_PALABRAS,
            });
        }
        if duracion_ms < partes.len() as u32 {
            return Err(TextAnimError::RepartoImposible {
                motivo: format!(
                    "ventana de {duracion_ms} ms muy corta para {} palabras: dale al menos 1 ms por palabra",
                    partes.len()
                ),
            });
        }
        let base = duracion_ms / partes.len() as u32;
        let resto = duracion_ms % partes.len() as u32;
        let mut cursor = 0u32;
        let mut voces = Vec::with_capacity(partes.len());
        for (k, palabra) in partes.iter().enumerate() {
            let extra = if (k as u32) < resto { 1 } else { 0 };
            let tramo = base.saturating_add(extra);
            let fin = cursor.saturating_add(tramo);
            voces.push(PalabraVoz {
                texto: (*palabra).to_string(),
                inicio_ms: cursor,
                fin_ms: fin,
            });
            cursor = fin;
        }
        Ok(Self {
            texto: normalizado,
            palabras: voces,
        })
    }

    /// Texto completo (normalizado en `reparto`, recortado en `try_new`).
    pub fn texto(&self) -> &str {
        &self.texto
    }

    /// Palabras con timing.
    pub fn palabras(&self) -> &[PalabraVoz] {
        &self.palabras
    }

    /// Cantidad de palabras.
    pub fn len(&self) -> usize {
        self.palabras.len()
    }

    /// ¿Sin palabras? (nunca tras construir, pero existe por contrato).
    pub fn is_empty(&self) -> bool {
        self.palabras.is_empty()
    }

    /// Duración total (fin de la última palabra).
    pub fn duracion_ms(&self) -> u32 {
        self.palabras.last().map_or(0, |p| p.fin_ms)
    }

    /// Índice de la palabra activa en `t_ms` (`inicio <= t < fin`; `None` en
    /// huecos o fuera de la pista). Barrido lineal (≤64 palabras). Sin allocs.
    pub fn activa_en(&self, t_ms: u32) -> Option<usize> {
        for (i, p) in self.palabras.iter().enumerate() {
            if t_ms >= p.inicio_ms && t_ms < p.fin_ms {
                return Some(i);
            }
        }
        None
    }

    /// Alfa de la palabra `i` en `t_ms` (1.0 activa, [`KARAOKE_ATENUADO`]
    /// el resto, 0.0 fuera de rango). Sin allocs.
    pub fn alfa_en(&self, i: usize, t_ms: u32) -> f32 {
        if i >= self.palabras.len() {
            return 0.0;
        }
        match self.activa_en(t_ms) {
            Some(a) if a == i => 1.0,
            _ => KARAOKE_ATENUADO,
        }
    }

    /// Llena `salida` (largo exacto `len()`) con los alfas en `t_ms`.
    /// Sin allocs.
    pub fn muestra_en(&self, t_ms: u32, salida: &mut [f32]) -> Result<(), TextAnimError> {
        if salida.len() != self.palabras.len() {
            return Err(TextAnimError::BufferCorto {
                got: salida.len(),
                want: self.palabras.len(),
            });
        }
        let activa = self.activa_en(t_ms);
        for (i, celda) in salida.iter_mut().enumerate() {
            *celda = if Some(i) == activa {
                1.0
            } else {
                KARAOKE_ATENUADO
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(TEXTANIM_MAX_CHARS, 200);
        assert_eq!(TEXTANIM_MAX_LINEAS, 8);
        assert_eq!(TEXTANIM_MAX_PALABRAS, 64);
        assert_eq!((TEXTANIM_CANVAS_MIN, TEXTANIM_CANVAS_MAX), (64, 4096));
        assert_eq!((TEXTANIM_FONT_MIN_PX, TEXTANIM_FONT_MAX_PX), (8.0, 96.0));
        assert_eq!(TEXTO_AVANCE_POR_EM, 0.6);
        assert_eq!(TEXTO_ALTO_LINEA_POR_EM, 1.2);
        assert_eq!(KARAOKE_MAX_DUR_MS, 60_000);
    }

    #[test]
    fn titulo_centrado_offset_exact() {
        let t = Titulo::try_new("Hola", None, 48.0, 640, 480).unwrap();
        let l = t.layout();
        let avance = 48.0 * TEXTO_AVANCE_POR_EM;
        assert!((l.bloque_w - 4.0 * avance).abs() < 1e-4);
        // Requisito estrella: offset exacto por construcción.
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
        assert_eq!(l.offset_y, (480.0 - l.bloque_h) / 2.0);
        assert!((l.offset_x - 262.4).abs() < 0.05, "offset_x={}", l.offset_x);
        assert!(!l.desborda);
        assert_eq!(l.lineas.len(), 1);
    }

    #[test]
    fn titulo_multilinea_centra_cada_linea() {
        let t = Titulo::try_new("ab\nabcdef", None, 20.0, 640, 480).unwrap();
        let l = t.layout();
        assert_eq!(l.lineas.len(), 2);
        let avance = 20.0 * TEXTO_AVANCE_POR_EM;
        assert!((l.bloque_w - 6.0 * avance).abs() < 1e-4);
        for linea in &l.lineas {
            assert_eq!(linea.x, l.offset_x + (l.bloque_w - linea.ancho) / 2.0);
        }
        assert!((l.lineas[1].y - l.lineas[0].y - 20.0 * TEXTO_ALTO_LINEA_POR_EM).abs() < 1e-4);
    }

    #[test]
    fn titulo_con_subtitulo_centrado_y_debajo() {
        let t = Titulo::try_new("Hola", Some("mundo"), 40.0, 800, 600).unwrap();
        let l = t.layout();
        let sub = l.subtitulo.as_ref().unwrap();
        assert!(sub.avance < l.lineas[0].avance);
        assert!(sub.y > l.lineas[0].y);
        assert_eq!(sub.x, l.offset_x + (l.bloque_w - sub.ancho) / 2.0);
        assert!(!l.desborda);
    }

    #[test]
    fn titulo_largo_desborda_pero_offset_exact() {
        let largo = "x".repeat(TEXTANIM_MAX_CHARS);
        let t = Titulo::try_new(&largo, None, TEXTANIM_FONT_MAX_PX, 640, 480).unwrap();
        let l = t.layout();
        assert!(l.bloque_w > 640.0);
        assert!(l.desborda);
        assert_eq!(l.offset_x, (640.0 - l.bloque_w) / 2.0);
    }

    #[test]
    fn titulo_rechaza_bordes() {
        assert!(matches!(
            Titulo::try_new("", None, 24.0, 640, 480),
            Err(TextAnimError::TextoVacio)
        ));
        assert!(matches!(
            Titulo::try_new("   ", None, 24.0, 640, 480),
            Err(TextAnimError::TextoVacio)
        ));
        let largo = "a".repeat(TEXTANIM_MAX_CHARS + 1);
        assert!(matches!(
            Titulo::try_new(&largo, None, 24.0, 640, 480),
            Err(TextAnimError::TextoLargo { .. })
        ));
        assert!(matches!(
            Titulo::try_new("hola", None, 0.0, 640, 480),
            Err(TextAnimError::FuenteInvalida)
        ));
        assert!(matches!(
            Titulo::try_new("hola", None, f32::NAN, 640, 480),
            Err(TextAnimError::FuenteInvalida)
        ));
        assert!(matches!(
            Titulo::try_new("hola", None, 24.0, 63, 480),
            Err(TextAnimError::CanvasFueraDeRango { .. })
        ));
        assert!(matches!(
            Titulo::try_new("hola", None, 24.0, 640, 4097),
            Err(TextAnimError::CanvasFueraDeRango { .. })
        ));
        assert!(Titulo::try_new("h", None, 8.0, 64, 64).is_ok());
        assert!(Titulo::try_new("h", None, 96.0, 4096, 4096).is_ok());
        assert!(Titulo::try_new("h", Some("a\nb"), 24.0, 640, 480).is_err());
        let muchas = (0..TEXTANIM_MAX_LINEAS + 1)
            .map(|_| "a")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            Titulo::try_new(&muchas, None, 24.0, 4096, 4096),
            Err(TextAnimError::DemasiadasLineas { .. })
        ));
    }

    #[test]
    fn layout_monoespaciado_pineado_como_aproximacion() {
        // `grafito-tex` no expone posiciones de texto plano como lib
        // (`mathml_to_layout_con_gpos` es privada y `grafito-anim` no depende
        // del crate): avance fijo 0.6em, pineado acá como aproximación.
        let t = Titulo::try_new("abc", None, 24.0, 640, 480).unwrap();
        let l = t.layout();
        let cajas = l.cajas_glifos();
        assert_eq!(cajas.len(), 3);
        let avance = 24.0 * TEXTO_AVANCE_POR_EM;
        for (i, c) in cajas.iter().enumerate() {
            assert!((c.x - (l.lineas[0].x + i as f32 * avance)).abs() < 1e-4);
            assert_eq!((c.columna, c.linea), (i, 0));
        }
    }

    #[test]
    fn write_reveal_progresivo_y_cursor() {
        let w = WriteTexto::try_new("hola", true, TextEasing::Linear).unwrap();
        assert_eq!(w.len(), 4);
        assert!(!w.is_empty());
        let mut buf = [0.0f32; 4];
        w.muestra_en(0.0, &mut buf).unwrap();
        assert_eq!(buf, [0.0, 0.0, 0.0, 0.0]);
        w.muestra_en(0.5, &mut buf).unwrap();
        assert_eq!(buf, [1.0, 1.0, 0.0, 0.0]);
        w.muestra_en(1.0, &mut buf).unwrap();
        assert_eq!(buf, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(w.revelados(0.0), 0);
        assert_eq!(w.revelados(0.5), 2);
        assert_eq!(w.revelados(1.0), 4);
        assert_eq!(w.completos(0.5), 2);
        assert_eq!(w.cursor_columna(0.0), Some(0.0));
        assert_eq!(w.cursor_columna(0.5), Some(2.0));
        assert_eq!(w.cursor_columna(1.0), Some(4.0));
        let avance = 24.0 * TEXTO_AVANCE_POR_EM;
        assert_eq!(w.cursor_x(0.5, avance, 100.0), Some(100.0 + 2.0 * avance));
        let s = WriteTexto::try_new("hola", false, TextEasing::Linear).unwrap();
        assert_eq!(s.cursor_columna(0.5), None);
        assert_eq!(s.cursor_x(0.5, avance, 100.0), None);
        // Parcial por índice: 0.625×4 = 2.5 → tercer glifo a 0.5.
        assert!((w.alfa_en(2, 0.625) - 0.5).abs() < 1e-6);
        assert_eq!(w.alfa_en(9, 1.0), 0.0);
        let mut corto = [0.0f32; 3];
        assert!(matches!(
            w.muestra_en(0.5, &mut corto),
            Err(TextAnimError::BufferCorto { .. })
        ));
        assert!(WriteTexto::try_new("a\nb", true, TextEasing::Linear).is_err());
    }

    #[test]
    fn write_smooth_monotono_y_centro_exacto() {
        let w = WriteTexto::try_new("hola mundo", true, TextEasing::Smooth).unwrap();
        let mut prev = [0.0f32; 10];
        w.muestra_en(0.0, &mut prev).unwrap();
        for k in 0..=20 {
            let alpha = f64::from(k) * 0.05;
            let mut buf = [0.0f32; 10];
            w.muestra_en(alpha, &mut buf).unwrap();
            for i in 0..10 {
                assert!(
                    buf[i] + 1e-6 >= prev[i],
                    "monótono en {i} con alpha={alpha}"
                );
            }
            prev = buf;
        }
        assert!((TextEasing::Smooth.aplica(0.5) - 0.5).abs() < 1e-12);
        assert_eq!(TextEasing::Linear.aplica(0.3), 0.3);
    }

    #[test]
    fn matching_alinea_comunes_y_fondea_resto() {
        let m = TransformMatchingTexto::try_new("abc", "adc", TextEasing::Linear).unwrap();
        assert_eq!(m.emparejados(), 2);
        assert_eq!(m.hacia_desde(0), Some(0));
        assert_eq!(m.hacia_desde(1), None);
        assert_eq!(m.hacia_desde(2), Some(2));
        assert_eq!(m.hacia_hasta(0), Some(0));
        assert_eq!(m.hacia_hasta(1), None);
        assert_eq!(m.hacia_hasta(2), Some(2));
        let mut bd = [0.0f32; 3];
        let mut bh = [0.0f32; 3];
        m.muestra_en(0.0, &mut bd, &mut bh).unwrap();
        assert_eq!(bd, [1.0, 1.0, 1.0]);
        assert_eq!(bh, [0.0, 0.0, 0.0]);
        m.muestra_en(1.0, &mut bd, &mut bh).unwrap();
        assert_eq!(bd, [0.0, 0.0, 0.0]);
        assert_eq!(bh, [1.0, 1.0, 1.0]);
        m.muestra_en(0.5, &mut bd, &mut bh).unwrap();
        assert_eq!(bd, [0.5, 0.5, 0.5]);
        assert_eq!(bh, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn matching_identico_y_sin_nada_en_comun() {
        let m = TransformMatchingTexto::try_new("hola", "hola", TextEasing::Linear).unwrap();
        assert_eq!(m.emparejados(), 4);
        for i in 0..4 {
            assert_eq!(m.hacia_desde(i), Some(i));
        }
        let m = TransformMatchingTexto::try_new("abc", "xyz", TextEasing::Linear).unwrap();
        assert_eq!(m.emparejados(), 0);
        assert_eq!(m.hacia_desde(0), None);
        assert!(TransformMatchingTexto::try_new("", "a", TextEasing::Linear).is_err());
        let largo = "a".repeat(TEXTANIM_MAX_CHARS + 1);
        assert!(TransformMatchingTexto::try_new(&largo, "a", TextEasing::Linear).is_err());
        let m = TransformMatchingTexto::try_new("ab", "abc", TextEasing::Linear).unwrap();
        assert_eq!((m.len_desde(), m.len_hasta()), (2, 3));
        let mut bd = [0.0f32; 2];
        let mut mal = [0.0f32; 2];
        assert!(m.muestra_en(0.5, &mut bd, &mut mal).is_err());
    }

    #[test]
    fn karaoke_reparto_activa_y_atenua() {
        let k = KaraokeTexto::reparto("hola mundo", 1000).unwrap();
        assert_eq!(k.len(), 2);
        assert!(!k.is_empty());
        assert_eq!(k.duracion_ms(), 1000);
        assert_eq!(k.palabras()[0].inicio_ms, 0);
        assert_eq!(k.palabras()[0].fin_ms, 500);
        assert_eq!(k.palabras()[1].inicio_ms, 500);
        assert_eq!(k.activa_en(0), Some(0));
        assert_eq!(k.activa_en(499), Some(0));
        assert_eq!(k.activa_en(500), Some(1));
        assert_eq!(k.activa_en(1000), None);
        let mut buf = [0.0f32; 2];
        k.muestra_en(250, &mut buf).unwrap();
        assert_eq!(buf, [1.0, KARAOKE_ATENUADO]);
        k.muestra_en(750, &mut buf).unwrap();
        assert_eq!(buf, [KARAOKE_ATENUADO, 1.0]);
        k.muestra_en(750, &mut buf).unwrap();
        assert_eq!(buf, [KARAOKE_ATENUADO, 1.0]);
        assert_eq!(k.alfa_en(9, 100), 0.0);
    }

    #[test]
    fn karaoke_rechaza_bordes() {
        assert!(matches!(
            KaraokeTexto::reparto("", 1000),
            Err(TextAnimError::TextoVacio)
        ));
        assert!(matches!(
            KaraokeTexto::reparto("a b", 1),
            Err(TextAnimError::RepartoImposible { .. })
        ));
        assert!(matches!(
            KaraokeTexto::reparto("hola", 60_001),
            Err(TextAnimError::FueraDeRango { .. })
        ));
        let r = KaraokeTexto::try_new(
            "hola mundo",
            vec![
                ("hola".to_string(), 0, 600),
                ("mundo".to_string(), 500, 1000),
            ],
        );
        assert!(matches!(r, Err(TextAnimError::PalabraInvalida { .. })));
        let k = KaraokeTexto::try_new(
            "hola mundo",
            vec![
                ("hola".to_string(), 0, 400),
                ("mundo".to_string(), 600, 1000),
            ],
        )
        .unwrap();
        assert_eq!(k.activa_en(500), None);
        let mut corto = [0.0f32; 1];
        assert!(matches!(
            k.muestra_en(100, &mut corto),
            Err(TextAnimError::BufferCorto { .. })
        ));
    }
}
