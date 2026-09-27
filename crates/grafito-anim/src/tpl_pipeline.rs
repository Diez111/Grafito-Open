//! Pipeline de render: calidad 4K/60fps (post-proceso puro sobre RGBA).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias nuevas
//! (solo `std`). Todo CPU, funciones puras, sin pánicos en código productivo
//! (`unwrap_used = deny`: ningún `unwrap`/`expect` fuera de tests).
//!
//! El tipo propio [`RgbaFrame`] es compatible con
//! `egui::ColorImage::from_rgba_unmultiplied([w, h], pixels)`: mismo layout
//! (RGBA sin premultiplicar, fila tras fila, `w*h*4` bytes). La Piel lo
//! convierte con `from_rgba_unmultiplied([f.width as usize, f.height as usize],
//! f.pixels)` tras validar con [`RgbaFrame::try_new`].
//!
//! ## Etapas
//!
//! 1. **SSAA x2/x4** ([`downsample_box`]): el renderer dibuja grande (x2/x4 por
//!    lado) y se promedia cada bloque con filtro box (promedio recto, como el
//!    resolve de MSAA/SSAA clásico). Fuentes: LearnOpenGL "Anti Aliasing"
//!    (SSAA = render a mayor resolución + downsample), UCSD CSE 167 clase 16
//!    (SSAA promedia las muestras del píxel), NVIDIA GPU Gems 2 cap. 21
//!    (downsample de tiles supersampleados; el box 1px es el promedio recto),
//!    Unigine docs "Supersampling" (render grande → downsample con filtro).
//! 2. **FXAA-lite** ([`fxaa_lite`]): aproximación honesta y DOCUMENTADA, no el
//!    FXAA real de Lottes/NVIDIA (ese estima dirección de borde por luma y
//!    muestrea a lo largo del borde). Acá: si el rango de luma en 3×3 supera
//!    el umbral, el píxel se reemplaza por el promedio 3×3; si no, se copia.
//!    Suaviza escalones sin difuminar planos. Barato, sin subpíxel.
//! 3. **Motion blur por acumulación** ([`motion_blur`]): promedio ponderado de
//!    N subframes (`Σ c·w / Σ w`, redondeo al entero más cercano). Es la
//!    técnica del accumulation buffer (Haeberli–Akeley 1990; precursor Korein–
//!    Badler 1983) y del "accumulation motion blur" de Blender EEVEE / Unity
//!    HDRP (N pasos temporales acumulados; cada paso reevalúa la escena).
//!    Los pesos triangulares ([`pesos_triangulares`]) imitan el obturador con
//!    rampa; los uniformes ([`pesos_uniformes`]) el obturador abierto parejo
//!    (como el promedio simple de Brooks–Barron para sintetizar blur).
//! 4. **GIF**: downscale ([`reescala_vecino`], vecino más cercano) + paleta
//!    uniforme 3-3-2 ([`cuantiza_332`], 256 colores exactos). Honesto: el
//!    muestreo de color real (median-cut, neuquant, `ffmpeg palettegen` +
//!    `paletteuse` con dithering) NO está acá; el 3-3-2 uniforme causa banding
//!    en degradados y es solo para previsualizar tamaño. GIF admite 256
//!    colores por frame (paleta local; el truco multicapa tipo gifski apila
//!    frames con transparencia para ~1000-3000 colores, fuera de alcance).
//!    WebM/VP9 (perfil 0 8-bit 4:2:0) acepta 4K/60fps por niveles (luma
//!    samples/s + bitrate); el tamaño codificado NO se estima acá (depende del
//!    codec): [`Preset::estima_bytes_rgba`] estima RAM sin comprimir.
//!
//! ## Presets ([`Preset`]) y topes espejo de `protocol.rs`
//!
//! | Preset | Canvas | fps | Uso |
//! |---|---|---|---|
//! | `Preview720p30` | 1280×720 | 30 | vista previa rápida |
//! | `Full1080p30` | 1920×1080 | 30 | entrega Full HD |
//! | `Ultra4K30` | 3840×2160 | 30 | entrega 4K (a 30fps: 60fps NO cabe, ver abajo) |
//! | `ShortGif` | 480×270 | 12 | GIF corto (12fps × 4s = 48 ≤ 64) |
//!
//! Espejos (NO editar acá el protocolo; si cambian allá, cambiar acá):
//!
//! | Constante acá | Origen (`protocol.rs` / `engine.rs`) | Valor |
//! |---|---|---|
//! | [`TOPE_FRAMES_CORTO`] | `PREVIEW_SHORT_MAX_FRAMES` (`protocol.rs:347`) | 64 |
//! | [`TOPE_FRAMES_LARGO`] | `VIDEO_LONGFORM_MAX_FRAMES` (`protocol.rs:351`) | 1500 |
//! | [`TOPE_CHUNK_BYTES`] | `LONGFORM_CHUNK_MAX_BYTES` (`protocol.rs:355`) | 64 MiB |
//! | [`TOPE_DURACION_MS`] | `MAX_TIMELINE_DURATION_MS` (`protocol.rs:937`) | 60 000 |
//! | [`TOPE_LADO_MAX`] | `Resolution::try_new` (`protocol.rs:230`) | 4096 |
//! | `DEFAULT_JOB_TIMEOUT_SECS` (solo informe) | `engine.rs:25` | 90 s |
//!
//! ## Informe de topes que frenan 4K/60fps (propuesta, SIN editar `protocol.rs`)
//!
//! Cuentas (RGBA = `w*h*4`):
//!
//! - Frame 4K: 3840×2160×4 = 33 177 600 B (≈31,6 MiB).
//! - Chunk 64 MiB / frame 4K = **2 frames por chunk** (composición absurda).
//! - 60 s a 60 fps = **3600 frames > 1500** (tope largo).
//! - 60 s a 30 fps = 1800 frames > 1500 (el tope largo ya frena los 60 s
//!   actuales a 30 fps; a 60 fps frena el doble).
//! - GIF 64 frames a 12 fps = 5,3 s; un GIF 4K es inviable por peso de todos
//!   modos (va downscaleado a 480p por diseño).
//! - `Resolution` 4096: 3840×2160 cabe justo; DCI 4096×2160 va AL límite.
//! - Job 90 s: un render 4K/60 real (3600 frames + encode) no entra; el margen
//!   de 30 s sobre 60 s de timeline se calculó para 720p/30.
//!
//! Propuesta numerada (para el dueño de `protocol.rs` / `engine.rs`):
//!
//! 1. `VIDEO_LONGFORM_MAX_FRAMES`: 1500 → **3600** (60 s a 60 fps exactos).
//!    Ideal con holgura de parte: 7200 (120 s a 60 fps, a la par del punto 4).
//! 2. `LONGFORM_CHUNK_MAX_BYTES`: 64 MiB → **256 MiB** (8 frames 4K por chunk;
//!    512 MiB darían 16 pero ya presionan la RAM del player). Alternativa sin
//!    subir RAM: drenaje a disco cada 2 frames (ya previsto en el contrato
//!    P0.1) pineado como obligatorio para `Ultra4K`.
//! 3. `PREVIEW_SHORT_MAX_FRAMES`: 64 → **180** (6 s a 30 fps) para previews
//!    WebM/MP4 cortos; GIF queda en 64 por peso (o 120 solo con downscale ≤
//!    480p + paleta, nunca a 720p+).
//! 4. `MAX_TIMELINE_DURATION_MS` + `AnimDuration` 60 s → **120 s** (120 000),
//!    con `DEFAULT_JOB_TIMEOUT_SECS` 90 s → **300 s** (`MAX_JOB_TIMEOUT_SECS`
//!    600 ya lo admite; es solo subir el default para `Ultra4K`).
//! 5. `Resolution` 4096 → **8192** por lado SOLO con gate de memoria previo
//!    (un frame 8K RGBA = 256 MiB; sin gate es OOM garantizado). Si no hay
//!    gate, mantener 4096 (4K UHD cabe).
//!
//! [`Preset::chequea`] reporta qué tope frena cada pedido sin panics
//! ([`Freno`]): los tests pinean que `Ultra4K30` largo frena por chunk y por
//! frames, y que `ShortGif` cabe en el tope corto.

/// Tope de frames del preview corto. Espejo de
/// `crate::protocol::PREVIEW_SHORT_MAX_FRAMES` (64).
pub const TOPE_FRAMES_CORTO: usize = 64;
/// Tope de frames del video largo. Espejo de
/// `crate::protocol::VIDEO_LONGFORM_MAX_FRAMES` (1500).
pub const TOPE_FRAMES_LARGO: usize = 1500;
/// Tope de bytes por chunk en RAM. Espejo de
/// `crate::protocol::LONGFORM_CHUNK_MAX_BYTES` (64 MiB).
pub const TOPE_CHUNK_BYTES: usize = 64 * 1024 * 1024;
/// Duración máxima del timeline. Espejo de
/// `crate::protocol::MAX_TIMELINE_DURATION_MS` (60 000 ms).
pub const TOPE_DURACION_MS: u64 = 60_000;
/// Lado máximo del canvas. Espejo de `Resolution::try_new` (4096).
pub const TOPE_LADO_MAX: u32 = 4096;
/// Bytes por píxel RGBA.
pub const BYTES_POR_PIXEL: usize = 4;
/// Factores SSAA soportados por [`downsample_box`].
pub const SSAA_FACTORS: &[u32] = &[1, 2, 4];
/// Umbral de luma por defecto de [`fxaa_lite`] (0..=255).
pub const FXAA_LUMA_UMBRAL_DEFAULT: u8 = 24;

// ── Frame ────────────────────────────────────────────────────────────────

/// Frame RGBA sin premultiplicar, fila tras fila (`w*h*4` bytes).
///
/// Compatible con `egui::ColorImage::from_rgba_unmultiplied([w, h], pixels)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    /// Ancho en píxeles (> 0).
    pub width: u32,
    /// Alto en píxeles (> 0).
    pub height: u32,
    /// Píxeles RGBA (`width*height*4` bytes exactos).
    pub pixels: Vec<u8>,
}

impl RgbaFrame {
    /// Constructor validado. `None` si algún lado es 0, si el largo no es
    /// `w*h*4` exacto o si el producto desborda.
    pub fn try_new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let n = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(BYTES_POR_PIXEL)?;
        if pixels.len() != n {
            return None;
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Frame sólido de un color `[r, g, b, a]`. `None` si desborda.
    pub fn solido(width: u32, height: u32, color: [u8; 4]) -> Option<Self> {
        if width == 0 || height == 0 {
            return None;
        }
        let n = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(BYTES_POR_PIXEL)?;
        let mut pixels = Vec::with_capacity(n);
        for _ in 0..(n / BYTES_POR_PIXEL) {
            pixels.extend_from_slice(&color);
        }
        Some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Extensión `[w, h]` lista para `ColorImage::from_rgba_unmultiplied`.
    pub fn extension(&self) -> [usize; 2] {
        [self.width as usize, self.height as usize]
    }

    /// Bytes RGBA del frame (`w*h*4`). `None` si desborda.
    pub fn bytes_len(&self) -> Option<usize> {
        (self.width as usize)
            .checked_mul(self.height as usize)?
            .checked_mul(BYTES_POR_PIXEL)
    }

    /// Píxel en `(x, y)`. `None` si está fuera.
    pub fn pixel_en(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(BYTES_POR_PIXEL)?;
        let p = self.pixels.get(i..i + BYTES_POR_PIXEL)?;
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Luma (0..=255) del píxel `(x, y)` con aprox. entera
    /// `(77R + 150G + 29B) >> 8`. `None` si está fuera.
    pub fn luma_en(&self, x: u32, y: u32) -> Option<u8> {
        let p = self.pixel_en(x, y)?;
        Some(luma_de(p[0], p[1], p[2]))
    }
}

/// Luma entera de un RGB (Rec. 601 aproximada). Pura.
fn luma_de(r: u8, g: u8, b: u8) -> u8 {
    (((u16::from(r) * 77) + (u16::from(g) * 150) + (u16::from(b) * 29)) >> 8) as u8
}

// ── 1. Supersampling SSAA ────────────────────────────────────────────────

/// Downsample con filtro box: promedia cada bloque `factor×factor`.
///
/// El renderer dibuja a `width×height` grande y esto lo baja a
/// `width/factor × height/factor` (el SSAA clásico: render grande → promedio).
/// Solo `factor` 1 (clon), 2 o 4; `None` honesto si el factor no es válido, si
/// los lados no son divisibles o si el destino desborda.
pub fn downsample_box(src: &RgbaFrame, factor: u32) -> Option<RgbaFrame> {
    if factor == 1 {
        return Some(src.clone());
    }
    if factor != 2 && factor != 4 {
        return None;
    }
    if !src.width.is_multiple_of(factor) || !src.height.is_multiple_of(factor) {
        return None;
    }
    let dst_w = src.width / factor;
    let dst_h = src.height / factor;
    if dst_w == 0 || dst_h == 0 {
        return None;
    }
    let f = factor as usize;
    let n = f.checked_mul(f)?;
    let total = (dst_w as usize)
        .checked_mul(dst_h as usize)?
        .checked_mul(BYTES_POR_PIXEL)?;
    let mut pixels = Vec::with_capacity(total);
    let src_w = src.width as usize;
    for dy in 0..(dst_h as usize) {
        for dx in 0..(dst_w as usize) {
            let mut acc = [0u32; 4];
            for oy in 0..f {
                for ox in 0..f {
                    let sx = dx * f + ox;
                    let sy = dy * f + oy;
                    let i = sy
                        .checked_mul(src_w)?
                        .checked_add(sx)?
                        .checked_mul(BYTES_POR_PIXEL)?;
                    let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
                    for (slot, valor) in acc.iter_mut().zip(p.iter()) {
                        *slot += u32::from(*valor);
                    }
                }
            }
            let m = n as u32;
            // Promedio con redondeo al más cercano (mitades hacia arriba).
            for suma in acc {
                pixels.push(((suma + m / 2) / m) as u8);
            }
        }
    }
    RgbaFrame::try_new(dst_w, dst_h, pixels)
}

/// Upscale por vecino más cercano (replica cada píxel `factor×factor`).
///
/// Helper para sintetizar el "render grande" del SSAA en tests o para ampliar
/// miniaturas; NO es parte del pipeline de calidad (no antialisa, preserva el
/// escalón). Solo 1, 2 o 4; `None` si desborda o el factor no es válido.
pub fn upscale_nearest(src: &RgbaFrame, factor: u32) -> Option<RgbaFrame> {
    if factor == 1 {
        return Some(src.clone());
    }
    if factor != 2 && factor != 4 {
        return None;
    }
    let dst_w = src.width.checked_mul(factor)?;
    let dst_h = src.height.checked_mul(factor)?;
    let total = (dst_w as usize)
        .checked_mul(dst_h as usize)?
        .checked_mul(BYTES_POR_PIXEL)?;
    let mut pixels = Vec::with_capacity(total);
    let f = factor as usize;
    for y in 0..(src.height as usize) {
        for _ in 0..f {
            for x in 0..(src.width as usize) {
                let i = y
                    .checked_mul(src.width as usize)?
                    .checked_add(x)?
                    .checked_mul(BYTES_POR_PIXEL)?;
                let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
                for _ in 0..f {
                    pixels.extend_from_slice(p);
                }
            }
        }
    }
    RgbaFrame::try_new(dst_w, dst_h, pixels)
}

// ── FXAA-lite ────────────────────────────────────────────────────────────

/// FXAA-lite: suaviza solo bordes de alto contraste.
///
/// Para cada píxel se mide el rango de luma en su vecindario 3×3 (bordes
/// replicados); si `max - min > umbral`, se emite el promedio 3×3, si no se
/// copia el original. Ver [`fxaa_lite`] para el umbral por defecto (24).
/// `None` si el frame no se puede reconstruir (no debería: mismo tamaño).
///
/// Diferencias honestas con el FXAA real: sin estimación de dirección del
/// borde, sin muestreo anisotrópico, sin subpíxel; es un box selectivo.
pub fn fxaa_lite_con_umbral(src: &RgbaFrame, umbral: u8) -> Option<RgbaFrame> {
    let w = src.width as usize;
    let h = src.height as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let total = w.checked_mul(h)?.checked_mul(BYTES_POR_PIXEL)?;
    if src.pixels.len() != total {
        return None;
    }
    let mut pixels = Vec::with_capacity(total);
    for y in 0..h {
        for x in 0..w {
            let mut min_l = 255u8;
            let mut max_l = 0u8;
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for oy in 0..3 {
                for ox in 0..3 {
                    let nx = (x as i64 + ox as i64 - 1).clamp(0, w as i64 - 1) as usize;
                    let ny = (y as i64 + oy as i64 - 1).clamp(0, h as i64 - 1) as usize;
                    let i = nx
                        .checked_add(ny.checked_mul(w)?)?
                        .checked_mul(BYTES_POR_PIXEL)?;
                    let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
                    let l = luma_de(p[0], p[1], p[2]);
                    if l < min_l {
                        min_l = l;
                    }
                    if l > max_l {
                        max_l = l;
                    }
                    for (slot, valor) in acc.iter_mut().zip(p.iter()) {
                        *slot += u32::from(*valor);
                    }
                    n += 1;
                }
            }
            if max_l.saturating_sub(min_l) > umbral {
                for suma in acc {
                    pixels.push(((suma + n / 2) / n) as u8);
                }
            } else {
                let i = x
                    .checked_add(y.checked_mul(w)?)?
                    .checked_mul(BYTES_POR_PIXEL)?;
                let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
                pixels.extend_from_slice(p);
            }
        }
    }
    RgbaFrame::try_new(src.width, src.height, pixels)
}

/// FXAA-lite con el umbral por defecto ([`FXAA_LUMA_UMBRAL_DEFAULT`] = 24).
pub fn fxaa_lite(src: &RgbaFrame) -> Option<RgbaFrame> {
    fxaa_lite_con_umbral(src, FXAA_LUMA_UMBRAL_DEFAULT)
}

// ── 2. Motion blur por acumulación ───────────────────────────────────────

/// Acumulación temporal: promedio ponderado `Σ c·w / Σ w` por canal (alfa
/// incluido), con redondeo al entero más cercano.
///
/// Requiere: lista no vacía, `frames.len() == weights.len()`, todos del mismo
/// tamaño, pesos finitos y no negativos con suma finita > 0. Cualquier otra
/// cosa → `None` honesto (sin pánicos).
pub fn motion_blur(frames: &[RgbaFrame], weights: &[f32]) -> Option<RgbaFrame> {
    if frames.is_empty() || frames.len() != weights.len() {
        return None;
    }
    let primero = frames.first()?;
    let (w, h) = (primero.width, primero.height);
    if w == 0 || h == 0 {
        return None;
    }
    for f in frames.iter() {
        if f.width != w || f.height != h {
            return None;
        }
        if f.bytes_len()? != primero.bytes_len()? {
            return None;
        }
    }
    let mut suma_w = 0.0f32;
    for peso in weights.iter() {
        if !peso.is_finite() || *peso < 0.0 {
            return None;
        }
        suma_w += *peso;
    }
    if !suma_w.is_finite() || suma_w <= 0.0 {
        return None;
    }
    let n = primero.pixels.len();
    let mut pixels = Vec::with_capacity(n);
    for i in 0..n {
        let mut acc = 0.0f32;
        for (f, peso) in frames.iter().zip(weights.iter()) {
            let c = f.pixels.get(i)?;
            acc += f32::from(*c) * *peso;
        }
        let v = (acc / suma_w).round().clamp(0.0, 255.0) as u8;
        pixels.push(v);
    }
    RgbaFrame::try_new(w, h, pixels)
}

/// Acumulación con pesos parejos (obturador abierto uniforme).
pub fn motion_blur_uniform(frames: &[RgbaFrame]) -> Option<RgbaFrame> {
    let pesos = pesos_uniformes(frames.len())?;
    motion_blur(frames, &pesos)
}

/// Pesos parejos `[1, …, 1]` (largo `n`). `None` si `n == 0`.
pub fn pesos_uniformes(n: usize) -> Option<Vec<f32>> {
    if n == 0 {
        return None;
    }
    Some(vec![1.0; n])
}

/// Pesos triangulares simétricos `1, 2, …, pico, …, 2, 1` (imitan obturador
/// con rampa de apertura/cierre). `n == 1` → `[1]`. `None` si `n == 0`.
pub fn pesos_triangulares(n: usize) -> Option<Vec<f32>> {
    if n == 0 {
        return None;
    }
    let mut pesos = Vec::with_capacity(n);
    for i in 0..n {
        let v = (i + 1).min(n - i) as f32;
        pesos.push(v);
    }
    Some(pesos)
}

// ── 3. Downscale + paleta GIF ────────────────────────────────────────────

/// Reescala a `dst_w×dst_h` por vecino más cercano (`sx = x*src_w/dst_w`).
///
/// Honesto: preserva escalones (no antialisa). Para bajar 4K→GIF es lo
/// correcto en costo, pero una producción con tiempo usaría box/bilineal.
/// `None` si algún destino es 0 o si desborda.
pub fn reescala_vecino(src: &RgbaFrame, dst_w: u32, dst_h: u32) -> Option<RgbaFrame> {
    if dst_w == 0 || dst_h == 0 {
        return None;
    }
    let src_w = src.width as usize;
    let src_h = src.height as usize;
    let total = (dst_w as usize)
        .checked_mul(dst_h as usize)?
        .checked_mul(BYTES_POR_PIXEL)?;
    let mut pixels = Vec::with_capacity(total);
    for y in 0..(dst_h as usize) {
        let sy = (y.checked_mul(src_h)?) / (dst_h as usize);
        for x in 0..(dst_w as usize) {
            let sx = (x.checked_mul(src_w)?) / (dst_w as usize);
            let i = sy
                .checked_mul(src_w)?
                .checked_add(sx)?
                .checked_mul(BYTES_POR_PIXEL)?;
            let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
            pixels.extend_from_slice(p);
        }
    }
    RgbaFrame::try_new(dst_w, dst_h, pixels)
}

/// Índice de paleta 3-3-2 (R 3 bits, G 3 bits, B 2 bits = 256 colores).
pub fn indice_332(r: u8, g: u8, b: u8) -> u8 {
    (r & 0xE0) | ((g >> 3) & 0x1C) | (b >> 6)
}

/// Expande un índice 3-3-2 a RGB replicando bits (roundtrip exacto:
/// `indice_332(color_332(i)) == i`).
pub fn color_332(idx: u8) -> [u8; 3] {
    let r = idx >> 5;
    let g = (idx >> 2) & 0x07;
    let b = idx & 0x03;
    let r8 = (r << 5) | (r << 2) | (r >> 1);
    let g8 = (g << 5) | (g << 2) | (g >> 1);
    let b8 = (b << 6) | (b << 4) | (b << 2) | b;
    [r8, g8, b8]
}

/// Cuantiza a la paleta uniforme 3-3-2 (el alfa se preserva).
///
/// Muestreo de color SIMPLE y documentado: cada píxel cae al bucket uniforme
/// más cercano, sin dithering. Causa banding en degradados; un encoder real
/// usaría median-cut/neuquant + `palettegen/paletteuse`. `None` si el frame
/// no se puede reconstruir.
pub fn cuantiza_332(src: &RgbaFrame) -> Option<RgbaFrame> {
    let total = src.bytes_len()?;
    if src.pixels.len() != total {
        return None;
    }
    let mut pixels = Vec::with_capacity(total);
    let mut i = 0;
    while i < total {
        let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
        let rgb = color_332(indice_332(p[0], p[1], p[2]));
        pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], p[3]]);
        i += BYTES_POR_PIXEL;
    }
    RgbaFrame::try_new(src.width, src.height, pixels)
}

/// Cantidad de colores 3-3-2 distintos del frame (≤ 256). `None` si el
/// buffer no coincide con las dimensiones.
pub fn cuenta_colores_332(src: &RgbaFrame) -> Option<usize> {
    let total = src.bytes_len()?;
    if src.pixels.len() != total {
        return None;
    }
    let mut vistos = [false; 256];
    let mut i = 0;
    while i < total {
        let p = src.pixels.get(i..i + BYTES_POR_PIXEL)?;
        vistos[usize::from(indice_332(p[0], p[1], p[2]))] = true;
        i += BYTES_POR_PIXEL;
    }
    Some(vistos.iter().filter(|v| **v).count())
}

// ── Presets + estimación ─────────────────────────────────────────────────

/// Presets de entrega del pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// 1280×720 a 30 fps: vista previa rápida.
    Preview720p30,
    /// 1920×1080 a 30 fps: entrega Full HD.
    Full1080p30,
    /// 3840×2160 a 30 fps: entrega 4K (60 fps NO cabe en los topes actuales).
    Ultra4K30,
    /// 480×270 a 12 fps: GIF corto (12 × 4 s = 48 ≤ 64).
    ShortGif,
}

impl Preset {
    /// Ancho del preset.
    pub const fn ancho(self) -> u32 {
        match self {
            Self::Preview720p30 => 1280,
            Self::Full1080p30 => 1920,
            Self::Ultra4K30 => 3840,
            Self::ShortGif => 480,
        }
    }

    /// Alto del preset.
    pub const fn alto(self) -> u32 {
        match self {
            Self::Preview720p30 => 720,
            Self::Full1080p30 => 1080,
            Self::Ultra4K30 => 2160,
            Self::ShortGif => 270,
        }
    }

    /// Cuadros por segundo del preset.
    pub const fn fps(self) -> u32 {
        match self {
            Self::Preview720p30 | Self::Full1080p30 | Self::Ultra4K30 => 30,
            Self::ShortGif => 12,
        }
    }

    /// ¿Es salida GIF (tope corto de 64)?
    pub const fn es_gif(self) -> bool {
        match self {
            Self::ShortGif => true,
            Self::Preview720p30 | Self::Full1080p30 | Self::Ultra4K30 => false,
        }
    }

    /// Tope de frames del formato (corto 64 / largo 1500, espejo de
    /// `ExportFormat::max_frames`).
    pub const fn tope_frames(self) -> usize {
        match self {
            Self::ShortGif => TOPE_FRAMES_CORTO,
            Self::Preview720p30 | Self::Full1080p30 | Self::Ultra4K30 => TOPE_FRAMES_LARGO,
        }
    }

    /// Frames para `duracion_ms` a los fps del preset (entera hacia abajo).
    /// `fps` 0 no ocurre en estos presets; `duracion_ms` 0 → 0.
    pub fn frames_para(self, duracion_ms: u64) -> u64 {
        duracion_ms
            .saturating_mul(u64::from(self.fps()))
            .checked_div(1000)
            .unwrap_or(0)
    }

    /// Bytes RGBA en RAM para `frames` cuadros. `None` si desborda.
    pub fn estima_bytes_rgba(self, frames: usize) -> Option<u64> {
        estima_bytes(self.ancho(), self.alto(), frames)
    }

    /// Bytes aproximados del GIF indexado (1 B/px + 768 B de paleta global).
    /// `None` si desborda. El GIF real comprime con LZW: esto es cota superior
    /// honesta del raster sin comprimir.
    pub fn estima_bytes_gif(self, frames: usize) -> Option<u64> {
        let px = (u64::from(self.ancho())).checked_mul(u64::from(self.alto()))?;
        let raster = px.checked_mul(frames as u64)?;
        raster.checked_add(768)
    }

    /// ¿Cuántos frames del preset entran en un chunk de `TOPE_CHUNK_BYTES`?
    /// 0 honesto si el frame desborda o no entra ni uno.
    pub fn max_frames_por_chunk(self) -> usize {
        let bpf = (self.ancho() as usize)
            .checked_mul(self.alto() as usize)
            .and_then(|v| v.checked_mul(BYTES_POR_PIXEL))
            .unwrap_or(0);
        if bpf == 0 {
            return 0;
        }
        TOPE_CHUNK_BYTES / bpf
    }

    /// Chequea `frames` contra los topes: ¿entra en UN chunk y dentro del
    /// tope de frames del formato? Nunca panics.
    pub fn chequea(self, frames: usize) -> Chequeo {
        let tope = self.tope_frames();
        if frames == 0 || frames > tope {
            return Chequeo {
                cabe: false,
                freno: Some(Freno::TopeFrames {
                    pedidos: frames,
                    tope,
                }),
                por_chunk: self.max_frames_por_chunk(),
                bytes_rgba: self.estima_bytes_rgba(frames),
            };
        }
        let por_chunk = self.max_frames_por_chunk();
        if por_chunk == 0 || frames > por_chunk {
            return Chequeo {
                cabe: false,
                freno: Some(Freno::Chunk {
                    pedidos: frames,
                    por_chunk,
                    tope_bytes: TOPE_CHUNK_BYTES,
                }),
                por_chunk,
                bytes_rgba: self.estima_bytes_rgba(frames),
            };
        }
        Chequeo {
            cabe: true,
            freno: None,
            por_chunk,
            bytes_rgba: self.estima_bytes_rgba(frames),
        }
    }
}

/// Qué tope frena un pedido (ver [`Preset::chequea`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freno {
    /// Fuera del tope de frames del formato (0 o más que corto/largo).
    TopeFrames {
        /// Frames pedidos.
        pedidos: usize,
        /// Tope del formato.
        tope: usize,
    },
    /// No entra en un chunk de RAM (hay que componer por partes y drenar).
    Chunk {
        /// Frames pedidos.
        pedidos: usize,
        /// Frames que entran por chunk.
        por_chunk: usize,
        /// Tope de bytes del chunk.
        tope_bytes: usize,
    },
}

/// Resultado de [`Preset::chequea`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chequeo {
    /// ¿Entra en un chunk y dentro del tope de frames?
    pub cabe: bool,
    /// Tope que lo frena (`None` si cabe).
    pub freno: Option<Freno>,
    /// Frames del preset que entran por chunk.
    pub por_chunk: usize,
    /// Bytes RGBA estimados (`None` si desborda).
    pub bytes_rgba: Option<u64>,
}

/// Estima los bytes RGBA de un set (`w*h*4*frames`). `None` si desborda.
/// Pura, sin allocs (paridad con `protocol::estimate_chunk_bytes`, en `u64`
/// para que 4K largo no sature `u32`).
pub fn estima_bytes(ancho: u32, alto: u32, frames: usize) -> Option<u64> {
    (u64::from(ancho))
        .checked_mul(u64::from(alto))?
        .checked_mul(BYTES_POR_PIXEL as u64)?
        .checked_mul(frames as u64)
}

/// ¿Cuántos frames de `w`×`h` entran en `chunk_bytes`? Pura
/// (paridad con `protocol::max_chunk_frames`).
pub fn max_frames_en_chunk(ancho: u32, alto: u32, chunk_bytes: usize) -> usize {
    let bpf = (ancho as usize)
        .checked_mul(alto as usize)
        .and_then(|v| v.checked_mul(BYTES_POR_PIXEL))
        .unwrap_or(0);
    if bpf == 0 {
        return 0;
    }
    chunk_bytes / bpf
}

// ── Tests inline ─────────────────────────────────────────────────────────

#[cfg(test)]
mod pruebas_pipeline {
    use super::*;

    /// Luma media del frame (helper de métrica, tests).
    fn luma_media(f: &RgbaFrame) -> f64 {
        let mut suma = 0u64;
        let mut n = 0u64;
        let mut i = 0;
        while i < f.pixels.len() {
            suma += u64::from(luma_de(f.pixels[i], f.pixels[i + 1], f.pixels[i + 2]));
            n += 1;
            i += BYTES_POR_PIXEL;
        }
        if n == 0 {
            return 0.0;
        }
        suma as f64 / n as f64
    }

    /// Varianza de luma (métrica simple de contraste global).
    fn varianza_luma(f: &RgbaFrame) -> f64 {
        let media = luma_media(f);
        let mut acc = 0.0f64;
        let mut n = 0u64;
        let mut i = 0;
        while i < f.pixels.len() {
            let d = f64::from(luma_de(f.pixels[i], f.pixels[i + 1], f.pixels[i + 2])) - media;
            acc += d * d;
            n += 1;
            i += BYTES_POR_PIXEL;
        }
        if n == 0 {
            return 0.0;
        }
        acc / n as f64
    }

    /// Salto máximo: mayor |luma(a) − luma(b)| entre vecinos horizontales y
    /// verticales. Un escalón duro da 255; una rampa SSAA lo parte en
    /// escalones menores (el pico baja aunque la variación total se conserve).
    fn salto_maximo(f: &RgbaFrame) -> u8 {
        let w = f.width as usize;
        let h = f.height as usize;
        if w == 0 || h == 0 {
            return 0;
        }
        let luma = |x: usize, y: usize| {
            let i = (y * w + x) * BYTES_POR_PIXEL;
            luma_de(f.pixels[i], f.pixels[i + 1], f.pixels[i + 2])
        };
        let mut maximo = 0u8;
        for y in 0..h {
            for x in 0..w {
                let a = luma(x, y);
                if x + 1 < w {
                    maximo = maximo.max(a.abs_diff(luma(x + 1, y)));
                }
                if y + 1 < h {
                    maximo = maximo.max(a.abs_diff(luma(x, y + 1)));
                }
            }
        }
        maximo
    }

    /// Patrón de prueba: diagonal dura (blanco si x > y, negro si no).
    ///
    /// El `>` estricto deja la arista x == y justo en la esquina de los
    /// bloques 2×2 del SSAA: cada bloque diagonal mezcla 1 blanco + 3 negros
    /// y el box x2 emite gris 64 (rampa real, no identidad).
    fn patron_diagonal(w: u32, h: u32) -> RgbaFrame {
        let mut px = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for y in 0..h {
            for x in 0..w {
                if x > y {
                    px.extend_from_slice(&[255, 255, 255, 255]);
                } else {
                    px.extend_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
        RgbaFrame::try_new(w, h, px).unwrap()
    }

    #[test]
    fn frame_valida_largo_exacto() {
        assert!(RgbaFrame::try_new(0, 4, vec![]).is_none());
        assert!(RgbaFrame::try_new(4, 0, vec![]).is_none());
        assert!(RgbaFrame::try_new(2, 2, vec![0; 15]).is_none());
        assert!(RgbaFrame::try_new(2, 2, vec![0; 17]).is_none());
        let f = RgbaFrame::try_new(2, 2, vec![9; 16]).unwrap();
        assert_eq!(f.extension(), [2, 2]);
        assert_eq!(f.bytes_len().unwrap(), 16);
        assert_eq!(f.pixel_en(1, 0).unwrap(), [9, 9, 9, 9]);
        assert!(f.pixel_en(2, 0).is_none());
        assert!(f.pixel_en(0, 2).is_none());
        assert!(RgbaFrame::solido(2, 3, [1, 2, 3, 4]).unwrap().pixels.len() == 24);
        assert!(RgbaFrame::solido(0, 3, [0, 0, 0, 0]).is_none());
    }

    #[test]
    fn ssaa_caja_promedia_exacta() {
        // 4×4: bloque sup-izq blanco, resto negro → al bajar x2 el (0,0)
        // queda blanco y el resto negro (promedios exactos por bloque).
        let mut px = vec![0u8; 4 * 4 * 4];
        for y in 0..2 {
            for x in 0..2 {
                let i = (y * 4 + x) * 4;
                px[i] = 255;
                px[i + 1] = 255;
                px[i + 2] = 255;
                px[i + 3] = 255;
            }
        }
        // Alfa del resto a opaco para promediar solo color.
        for i in (0..px.len()).step_by(4) {
            px[i + 3] = 255;
        }
        let grande = RgbaFrame::try_new(4, 4, px).unwrap();
        let chico = downsample_box(&grande, 2).unwrap();
        assert_eq!((chico.width, chico.height), (2, 2));
        assert_eq!(chico.pixel_en(0, 0).unwrap(), [255, 255, 255, 255]);
        assert_eq!(chico.pixel_en(1, 0).unwrap(), [0, 0, 0, 255]);
        assert_eq!(chico.pixel_en(0, 1).unwrap(), [0, 0, 0, 255]);
        // Damero 2×2 dentro de un bloque → gris 128 (redondeo de 127.5).
        let damero = RgbaFrame::try_new(
            2,
            2,
            vec![
                255, 255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255,
            ],
        )
        .unwrap();
        let uno = downsample_box(&damero, 2).unwrap();
        assert_eq!((uno.width, uno.height), (1, 1));
        assert_eq!(uno.pixel_en(0, 0).unwrap(), [128, 128, 128, 255]);
        // Bordes honestos.
        assert!(downsample_box(&grande, 3).is_none());
        assert!(downsample_box(&grande, 0).is_none());
        assert!(downsample_box(&patron_diagonal(3, 3), 2).is_none());
        assert_eq!(downsample_box(&grande, 1).unwrap(), grande);
        // x4 sobre 4×4 → 1×1.
        let px4 = RgbaFrame::solido(4, 4, [100, 150, 200, 255]).unwrap();
        let uno4 = downsample_box(&px4, 4).unwrap();
        assert_eq!(uno4.pixel_en(0, 0).unwrap(), [100, 150, 200, 255]);
    }

    #[test]
    fn ssaa_reduce_aliasing_en_diagonal() {
        // Aliasing: diagonal dura muestreada a 32×32. SSAA: la MISMA escena
        // geométrica muestreada a 64×64 (render grande real, no upscale del
        // alias: un upscale nearest + box sería identidad) y box x2.
        let alias = patron_diagonal(32, 32);
        let grande = patron_diagonal(64, 64);
        let suave = downsample_box(&grande, 2).unwrap();
        assert_eq!((suave.width, suave.height), (32, 32));
        assert_eq!(salto_maximo(&alias), 255);
        // La rampa gris existe: bloques diagonales 1B+3N → gris 64.
        let mut intermedios = 0;
        for y in 0..32 {
            for x in 0..32 {
                let l = suave.luma_en(x, y).unwrap();
                if l > 16 && l < 240 {
                    intermedios += 1;
                }
            }
        }
        assert!(intermedios > 0, "SSAA no generó rampa de grises");
        // Métrica simple: el pico del escalón se parte (255 → 191 o menos).
        // La energía media NO sirve de métrica (la variación total por fila
        // se conserva al repartir el salto: 255 = 64 + 191).
        assert!(
            salto_maximo(&suave) < salto_maximo(&alias),
            "pico {} vs {}",
            salto_maximo(&suave),
            salto_maximo(&alias)
        );
        assert!(varianza_luma(&suave) <= varianza_luma(&alias));
    }

    #[test]
    fn fxaa_suaviza_borde_y_respeta_plano() {
        // Plano sólido: intacto.
        let plano = RgbaFrame::solido(8, 8, [40, 80, 120, 255]).unwrap();
        assert_eq!(fxaa_lite(&plano).unwrap(), plano);
        // Diagonal: cambia al menos un píxel del borde y mantiene tamaño.
        let diag = patron_diagonal(16, 16);
        let fx = fxaa_lite(&diag).unwrap();
        assert_eq!((fx.width, fx.height), (16, 16));
        assert!(fx.pixels != diag.pixels, "FXAA-lite no tocó el borde");
        assert!(salto_maximo(&fx) < salto_maximo(&diag));
        // Umbral máximo = casi identidad (solo bordes de 255 exacto mezclan).
        let fx_max = fxaa_lite_con_umbral(&diag, 255).unwrap();
        assert_eq!(fx_max.pixels, diag.pixels);
    }

    #[test]
    fn motion_blur_promedia_exacto() {
        let negro = RgbaFrame::solido(2, 1, [0, 0, 0, 255]).unwrap();
        let blanco = RgbaFrame::solido(2, 1, [255, 255, 255, 255]).unwrap();
        // Uniforme: (0 + 255) / 2 = 127.5 → 128.
        let gris = motion_blur_uniform(&[negro.clone(), blanco.clone()]).unwrap();
        assert_eq!(gris.pixel_en(0, 0).unwrap(), [128, 128, 128, 255]);
        // Ponderado [3, 1]: 63.75 → 64 (los pesos se normalizan).
        let p = motion_blur(&[negro.clone(), blanco.clone()], &[3.0, 1.0]).unwrap();
        assert_eq!(p.pixel_en(0, 0).unwrap(), [64, 64, 64, 255]);
        // Pesos sin normalizar dan lo mismo que normalizados.
        let q = motion_blur(&[negro.clone(), blanco.clone()], &[0.75, 0.25]).unwrap();
        assert_eq!(q, p);
        // Un solo frame con peso cualquiera = identidad.
        let solo = motion_blur(std::slice::from_ref(&blanco), &[5.0]).unwrap();
        assert_eq!(solo, blanco);
        // Triangulares: 3 frames negro/gris/blanco con [1,2,1] → 128.
        let medio = RgbaFrame::solido(2, 1, [128, 128, 128, 255]).unwrap();
        let tri = motion_blur(
            &[negro.clone(), medio.clone(), blanco.clone()],
            &pesos_triangulares(3).unwrap(),
        )
        .unwrap();
        assert_eq!(tri.pixel_en(0, 0).unwrap(), [128, 128, 128, 255]);
        assert_eq!(pesos_triangulares(4).unwrap(), vec![1.0, 2.0, 2.0, 1.0]);
        assert_eq!(pesos_uniformes(3).unwrap(), vec![1.0, 1.0, 1.0]);
        // Bordes honestos.
        assert!(motion_blur(&[], &[]).is_none());
        assert!(motion_blur(std::slice::from_ref(&negro), &[1.0, 2.0]).is_none());
        assert!(motion_blur(&[negro.clone(), blanco.clone()], &[1.0, f32::NAN]).is_none());
        assert!(motion_blur(&[negro.clone(), blanco.clone()], &[-1.0, 2.0]).is_none());
        assert!(motion_blur(&[negro.clone(), blanco.clone()], &[0.0, 0.0]).is_none());
        let chico = RgbaFrame::solido(1, 1, [0, 0, 0, 255]).unwrap();
        assert!(motion_blur(&[negro.clone(), chico], &[1.0, 1.0]).is_none());
        assert!(pesos_uniformes(0).is_none());
        assert!(pesos_triangulares(0).is_none());
    }

    #[test]
    fn gif_332_cabe_en_256_colores() {
        // Roundtrip exacto de la paleta.
        for i in [0u8, 1, 42, 127, 200, 255] {
            let rgb = color_332(i);
            assert_eq!(indice_332(rgb[0], rgb[1], rgb[2]), i, "i={i}");
        }
        // Degradado de 1024 colores distintos → ≤ 256 tras cuantizar.
        let mut px = Vec::with_capacity(32 * 32 * 4);
        for k in 0..(32 * 32) {
            let v = k as u8;
            px.extend_from_slice(&[v, (k * 7) as u8, (k * 13) as u8, 255]);
        }
        let foto = RgbaFrame::try_new(32, 32, px).unwrap();
        let q = cuantiza_332(&foto).unwrap();
        assert_eq!((q.width, q.height), (32, 32));
        // Alfa preservado.
        assert!(q.pixels.iter().skip(3).step_by(4).all(|a| *a == 255));
        let ncol = cuenta_colores_332(&q).unwrap();
        assert!(ncol <= 256 && ncol > 0, "colores={ncol}");
        // Reescala 4K→GIF: vecino más cercano mapea esquinas exactas.
        let grande = RgbaFrame::solido(3840, 2160, [10, 20, 30, 255]).unwrap();
        let mini = reescala_vecino(&grande, 480, 270).unwrap();
        assert_eq!((mini.width, mini.height), (480, 270));
        assert_eq!(mini.pixel_en(0, 0).unwrap(), [10, 20, 30, 255]);
        assert_eq!(mini.pixel_en(479, 269).unwrap(), [10, 20, 30, 255]);
        assert!(reescala_vecino(&grande, 0, 270).is_none());
    }

    #[test]
    fn presets_dentro_de_topes_o_reportan_freno() {
        // Paridad pineada con protocol.rs: 1280×720→18, 4K→2, 640×480→54.
        assert_eq!(Preset::Preview720p30.max_frames_por_chunk(), 18);
        assert_eq!(Preset::Ultra4K30.max_frames_por_chunk(), 2);
        assert_eq!(max_frames_en_chunk(640, 480, TOPE_CHUNK_BYTES), 54);
        assert_eq!(Preset::Full1080p30.max_frames_por_chunk(), 8);
        // Bytes exactos: frame 4K = 33 177 600 B.
        assert_eq!(Preset::Ultra4K30.estima_bytes_rgba(1).unwrap(), 33_177_600);
        assert_eq!(estima_bytes(640, 480, 54).unwrap(), 66_355_200);
        assert!(estima_bytes(640, 480, 54).unwrap() <= TOPE_CHUNK_BYTES as u64);
        assert!(estima_bytes(640, 480, 55).unwrap() > TOPE_CHUNK_BYTES as u64);
        // ShortGif cabe: 4 s a 12 fps = 48 ≤ 64 y entra en un chunk.
        let gif_4s = Preset::ShortGif.frames_para(4000);
        assert_eq!(gif_4s, 48);
        let ok = Preset::ShortGif.chequea(gif_4s as usize);
        assert!(ok.cabe && ok.freno.is_none(), "{ok:?}");
        // ShortGif largo (8 s = 96) frena por tope corto.
        let freno_gif = Preset::ShortGif.chequea(96);
        assert!(!freno_gif.cabe);
        assert_eq!(
            freno_gif.freno,
            Some(Freno::TopeFrames {
                pedidos: 96,
                tope: 64
            })
        );
        // Full1080p 2 s = 60 frames: dentro del tope largo pero NO en un chunk.
        let fhd = Preset::Full1080p30.chequea(60);
        assert!(!fhd.cabe);
        assert_eq!(
            fhd.freno,
            Some(Freno::Chunk {
                pedidos: 60,
                por_chunk: 8,
                tope_bytes: TOPE_CHUNK_BYTES
            })
        );
        // Ultra4K 60 s a 30 fps = 1800 > 1500: frena por tope largo (y aunque
        // cupiera, el chunk de 2 lo frenaría igual).
        assert_eq!(Preset::Ultra4K30.frames_para(60_000), 1800);
        let freno_4k = Preset::Ultra4K30.chequea(1800);
        assert!(!freno_4k.cabe);
        assert_eq!(
            freno_4k.freno,
            Some(Freno::TopeFrames {
                pedidos: 1800,
                tope: 1500
            })
        );
        // 60 fps honesto de 60 s = 3600: lo que pide el informe (tope 1500).
        assert_eq!(60_000u64.saturating_mul(60) / 1000, 3600);
        // Cero frames es Err honesto.
        assert!(!Preset::Preview720p30.chequea(0).cabe);
    }
}
