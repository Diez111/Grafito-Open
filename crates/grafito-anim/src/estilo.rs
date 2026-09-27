//! Estilo visual unificado de `grafito-anim` (ola de uniformidad visual).
//!
//! Unifica los colores, grosores, tipografías y duraciones que los 9 módulos
//! `tpl_*.rs` + `textanim.rs` hardcodeaban por separado. Todo es `pub const`
//! o función pura sin dependencias (solo `std` implícito): este archivo
//! compila standalone (`rustc --test estilo.rs`).
//!
//! ## Relevamiento (valores observados, no inventados)
//!
//! | Token | Valor | Fuente |
//! |---|---|---|
//! | Fondo noche | `[15, 19, 28, 255]` | `tpl_linalg.rs:436`, `tpl_graphs.rs:1244` |
//! | Barra noche | `[10, 14, 22, 255]` | `tpl_linalg.rs:437`, `tpl_graphs.rs:1245` |
//! | Grilla | `[36, 46, 64, 255]` | `tpl_linalg.rs:438`, `tpl_graphs.rs:1246` |
//! | Grilla fantasma | `[24, 31, 45, 255]` | `tpl_linalg.rs:439` (solo linalg) |
//! | Ejes | `[110, 130, 165, 255]` | `tpl_linalg.rs:440`, `tpl_graphs.rs:1247` |
//! | Texto primario | `[235, 238, 245, 255]` | `tpl_linalg.rs:441`, `tpl_graphs.rs:1248` |
//! | Acento amarillo | `[255, 205, 70, 255]` | `tpl_linalg.rs:442`, `tpl_graphs.rs:1249` |
//! | Éxito verde | `[88, 196, 130, 255]` | `tpl_linalg.rs:443`, `tpl_graphs.rs:1250` |
//! | Peligro rojo | `[250, 110, 110, 255]` | `tpl_linalg.rs:444`, `tpl_graphs.rs:1251` |
//! | Acento azul | `[110, 170, 255, 255]` | `tpl_linalg.rs:445`, `tpl_graphs.rs:1252` |
//! | Acento naranja | `[255, 160, 70, 255]` | `tpl_linalg.rs:446`, `tpl_graphs.rs:1253` |
//! | Texto secundario | `[140, 150, 170, 255]` | `tpl_linalg.rs:447`, `tpl_graphs.rs:1254` |
//! | Gris oscuro / arista | `[52, 62, 82]` / `[74, 88, 112]` | `tpl_graphs.rs:1255-1256` (solo grafos) |
//! | Fondo noche-violeta | `[8, 10, 20, 255]` | `tpl_4d.rs:1098` |
//! | Rejilla violeta | `[30, 34, 58, 255]` | `tpl_4d.rs:1100` |
//! | Cian / violeta / magenta / verde / ámbar | ver consts | `tpl_4d.rs:1102-1106` |
//! | Fondo día | `[255, 255, 255]` | `guion.rs:80` (`FONDO_POR_DEFECTO`) |
//! | Trazo grilla | grosor `0` (= 1 px) | `tpl_linalg.rs:788`, `tpl_4d.rs:1272` |
//! | Trazo vectores | grosor `2` | `tpl_linalg.rs:1158` |
//! | Trazo aristas 4D | grosor `3` | `tpl_4d.rs:1728` |
//! | Nodo linalg | disco `r = 4` | `tpl_linalg.rs:1165` |
//! | Nodo grafos | `(min/48).clamp(3, 9)` + halo `+2` | `tpl_graphs.rs:1693-1695,1399` |
//! | Vértices 4D | disco `r = 2/3` | `tpl_4d.rs:1620,1735` |
//! | Cabeza de flecha | `(8 + 2·grosor).min(24)` | `tpl_linalg.rs:660` |
//! | Fuente título | `32.0` px en canvas `640×480` | `tpl_edo.rs:154`, `tpl_3d.rs:137`, `tpl_chaos.rs:160`, `tpl_stats.rs:285` |
//! | Métrica texto | avance `0.6em`, línea `1.2em` | `textanim.rs:93-95` |
//! | Subtítulo | escala `0.7`, separación `0.5em` | `textanim.rs:97-99` |
//! | Etiqueta bitmap | fuente `5×7`, escala `1`/`2` (corte en `300` px) | `tpl_linalg.rs:1059-1065`, `tpl_graphs.rs:1684-1690`, `tpl_4d.rs:1480-1486` |
//! | Rótulo | alto `7·esc + 8`, texto en `(6, 4)` | `tpl_linalg.rs:1068-1076` (idéntico en graphs/4d) |
//! | Atenuado karaoke | `0.35` | `textanim.rs:101` (`KARAOKE_ATENUADO`) |
//! | Duración escena | default `4000` ms, rango `100..=60000` | `tpl_am2.rs:73,69-71` |
//! | Run anims/pasos | `100..=60000` ms | `anims.rs:37`, `guion.rs:76-78`, `protocol.rs:2345-2347` |
//! | Fases | setup 20 % / construcción 60 % / hold 20 % | `tpl_am1.rs:27-28,238-240` |
//! | Paso largo | `8000` ms + espera `200` ms; pregunta `2600` ms | `guion.rs:1263-1266,1257` |
//! | Long-form | `1500` frames = 50 s a 30 fps | `tpl_linalg.rs:71`, `tpl_graphs.rs:82` |
//!
//! Los módulos sin raster propio (`am1`, `am2`, `edo`, `stats`, `3d`, `caos`)
//! no definen color: dibujan `Mobject`/títulos y la Piel pone el color. Sus
//! acentos acá son convención nueva dentro de las dos paletas relevadas
//! (marcados como tal); el resto pinea valores observados.
//!
//! Estética: escandinava como `grafito-ui/src/tokens.rs` (Inter, base 4,
//! restraint); en raster se traduce a hairline de 1 px, grilla tenue y un
//! solo acento por escena.

/// Píxel RGBA de los rasters CPU (8 bits por canal, fila-mayor,
/// paridad `tpl_linalg::RgbaFrame` / `tpl_graphs` / `tpl_4d`).
pub type Rgba = [u8; 4];

// ═══════════════════════════════════════════════════════════
// Paleta
// ═══════════════════════════════════════════════════════════

/// Fondo noche 3b1b (`tpl_linalg.rs:436`, `tpl_graphs.rs:1244`).
pub const FONDO_NOCHE: Rgba = [15, 19, 28, 255];
/// Barra de rótulo noche (`tpl_linalg.rs:437`, `tpl_graphs.rs:1245`).
pub const BARRA_NOCHE: Rgba = [10, 14, 22, 255];
/// Grilla noche (`tpl_linalg.rs:438`, `tpl_graphs.rs:1246`).
pub const GRILLA_NOCHE: Rgba = [36, 46, 64, 255];
/// Grilla fantasma (ghost: la grilla deformada bajo la original,
/// `tpl_linalg.rs:439`; solo linalg la usaba).
pub const GRILLA_FANTASMA: Rgba = [24, 31, 45, 255];
/// Ejes (`tpl_linalg.rs:440`, `tpl_graphs.rs:1247`).
pub const EJES_NOCHE: Rgba = [110, 130, 165, 255];
/// Texto primario sobre noche (`tpl_linalg.rs:441`, `tpl_graphs.rs:1248`).
pub const TEXTO_PRIMARIO: Rgba = [235, 238, 245, 255];
/// Texto secundario / etiquetas tenues (`tpl_linalg.rs:447`, `tpl_graphs.rs:1254`).
pub const TEXTO_SECUNDARIO: Rgba = [140, 150, 170, 255];
/// Peligro / error (`tpl_linalg.rs:444`, `tpl_graphs.rs:1251`).
pub const PELIGRO: Rgba = [250, 110, 110, 255];
/// Éxito / convergencia (`tpl_linalg.rs:443`, `tpl_graphs.rs:1250`).
pub const EXITO: Rgba = [88, 196, 130, 255];
/// Amarillo de destaque (autovectores, `tpl_linalg.rs:442`).
pub const AMARILLO: Rgba = [255, 205, 70, 255];
/// Azul de destaque (`tpl_linalg.rs:445`).
pub const AZUL: Rgba = [110, 170, 255, 255];
/// Naranja de vectores (`tpl_linalg.rs:446,1158`).
pub const NARANJA: Rgba = [255, 160, 70, 255];
/// Gris oscuro de grafos (`tpl_graphs.rs:1255`; solo grafos).
pub const GRIS_OSCURO: Rgba = [52, 62, 82, 255];
/// Arista tenue de grafos (`tpl_graphs.rs:1256`; solo grafos).
pub const ARISTA: Rgba = [74, 88, 112, 255];

/// Fondo noche-violeta 4D (`tpl_4d.rs:1098`).
pub const FONDO_4D: Rgba = [8, 10, 20, 255];
/// Barra noche-violeta (`tpl_4d.rs:1099`).
pub const BARRA_4D: Rgba = [5, 7, 15, 255];
/// Rejilla noche-violeta (`tpl_4d.rs:1100`).
pub const REJILLA_4D: Rgba = [30, 34, 58, 255];
/// Blanco noche-violeta (`tpl_4d.rs:1101`).
pub const BLANCO_4D: Rgba = [232, 236, 248, 255];
/// Cian de aristas 4D (`tpl_4d.rs:1102`).
pub const CIAN_4D: Rgba = [90, 220, 255, 255];
/// Violeta 4D (`tpl_4d.rs:1103`).
pub const VIOLETA_4D: Rgba = [168, 140, 255, 255];
/// Magenta 4D (`tpl_4d.rs:1104`).
pub const MAGENTA_4D: Rgba = [255, 120, 200, 255];
/// Verde 4D (`tpl_4d.rs:1105`).
pub const VERDE_4D: Rgba = [110, 230, 150, 255];
/// Ámbar 4D (vértices destacados, `tpl_4d.rs:1106,1867`).
pub const AMBAR_4D: Rgba = [255, 200, 90, 255];

/// Fondo día (claro) del guion (`guion.rs:80`, `[u8; 3]` allá; acá con alfa).
pub const FONDO_DIA: Rgba = [255, 255, 255, 255];

/// Dominio didáctico (un acento por dominio, pedido de la ola).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dominio {
    /// Álgebra lineal (raster noche; `tpl_linalg`).
    Linalg,
    /// Análisis 1 estilo 3b1b (`tpl_am1`).
    Am1,
    /// Análisis 2 (`tpl_am2`).
    Am2,
    /// EDOs y Fourier (`tpl_edo`).
    Edo,
    /// Estadística y ML (`tpl_stats`).
    Stats,
    /// Superficies 3D (`tpl_3d`).
    TresD,
    /// Politopos 4D (raster noche-violeta; `tpl_4d`).
    CuatroD,
    /// Grafos y redes (raster noche; `tpl_graphs`).
    Grafos,
    /// Caos y fractales (`tpl_chaos`).
    Caos,
}

impl Dominio {
    /// Acento del dominio. Observados: `Linalg` (naranja de vectores,
    /// `tpl_linalg.rs:1158`), `Grafos` (amarillo de destaque, misma paleta),
    /// `CuatroD` (cian de aristas, `tpl_4d.rs:1102`). El resto es convención
    /// nueva dentro de las dos paletas relevadas (esos módulos no tienen
    /// raster propio: el color lo pone la Piel sobre `Mobject`).
    pub const fn acento(self) -> Rgba {
        match self {
            Self::Linalg => NARANJA,
            Self::Am1 => AMARILLO,
            Self::Am2 => AZUL,
            Self::Edo => EXITO,
            Self::Stats => VIOLETA_4D,
            Self::TresD => CIAN_4D,
            Self::CuatroD => CIAN_4D,
            Self::Grafos => AMARILLO,
            Self::Caos => MAGENTA_4D,
        }
    }

    /// Nombre kebab del dominio (para rótulos y logs).
    pub const fn nombre(self) -> &'static str {
        match self {
            Self::Linalg => "linalg",
            Self::Am1 => "am1",
            Self::Am2 => "am2",
            Self::Edo => "edo",
            Self::Stats => "stats",
            Self::TresD => "3d",
            Self::CuatroD => "4d",
            Self::Grafos => "grafos",
            Self::Caos => "caos",
        }
    }
}

/// Paleta completa de una escena (fondo + estructura + texto + semáforo).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paleta {
    /// Fondo del frame.
    pub fondo: Rgba,
    /// Barra del rótulo.
    pub barra: Rgba,
    /// Grilla / retícula.
    pub grilla: Rgba,
    /// Ejes.
    pub ejes: Rgba,
    /// Texto primario / etiquetas.
    pub texto: Rgba,
    /// Texto secundario / hints.
    pub texto_secundario: Rgba,
    /// Peligro / error.
    pub peligro: Rgba,
    /// Éxito / convergencia.
    pub exito: Rgba,
}

impl Paleta {
    /// Noche 3b1b compartida por `tpl_linalg` y `tpl_graphs`.
    pub const NOCHE: Self = Self {
        fondo: FONDO_NOCHE,
        barra: BARRA_NOCHE,
        grilla: GRILLA_NOCHE,
        ejes: EJES_NOCHE,
        texto: TEXTO_PRIMARIO,
        texto_secundario: TEXTO_SECUNDARIO,
        peligro: PELIGRO,
        exito: EXITO,
    };
    /// Noche-violeta de `tpl_4d` (`tpl_4d.rs:1098-1106`).
    pub const NOCHE_VIOLETA: Self = Self {
        fondo: FONDO_4D,
        barra: BARRA_4D,
        grilla: REJILLA_4D,
        ejes: EJES_NOCHE,
        texto: BLANCO_4D,
        texto_secundario: TEXTO_SECUNDARIO,
        peligro: PELIGRO,
        exito: VERDE_4D,
    };
}

// ═══════════════════════════════════════════════════════════
// Trazo (px a 720p: lado menor = 720)
// ═══════════════════════════════════════════════════════════

/// Fino: grilla/ejes/rejilla (`segmento` grosor `0` = 1 px;
/// `tpl_linalg.rs:788`, `tpl_4d.rs:1272-1273`). Fijo en todo lienzo,
/// como en los rasters (paridad hairline `tokens::STROKE_HAIRLINE`).
pub const TRAZO_FINO_PX: i32 = 1;
/// Medio: vectores, flechas y vértices chicos (grosor `2`;
/// `tpl_linalg.rs:1158`, disco `r = 2` en `tpl_4d.rs:1620`).
pub const TRAZO_MEDIO_PX: i32 = 2;
/// Grueso: aristas 4D y vértices destacados (grosor `3`;
/// `tpl_4d.rs:1728,1735,1867`). El disco `r = 4` de `tpl_linalg.rs:1165`
/// queda dentro del rango de nodo (`NODO_MIN..=NODO_MAX`).
pub const TRAZO_GRUESO_PX: i32 = 3;
/// Radio mínimo de nodo (`tpl_graphs.rs:1693`).
pub const NODO_MIN_PX: i32 = 3;
/// Radio máximo de nodo (`tpl_graphs.rs:1693`; a 720p se clampa acá:
/// `720 / 48 = 15 → 9`).
pub const NODO_MAX_PX: i32 = 9;
/// Radio de nodo a 720p (lado menor 720; pineado, no calculado).
pub const NODO_A_720P_PX: i32 = 9;
/// Halo del nodo seleccionado: `radio + 2` (`tpl_graphs.rs:1399`).
pub const HALO_NODO_PX: i32 = 2;
/// Base de la cabeza de flecha (`tpl_linalg.rs:660`).
pub const FLECHA_BASE_PX: f64 = 8.0;
/// La cabeza crece `2 px` por punto de grosor (`tpl_linalg.rs:660`).
pub const FLECHA_POR_GROSOR: f64 = 2.0;
/// Tope de la cabeza de flecha (`tpl_linalg.rs:660`).
pub const FLECHA_MAX_PX: f64 = 24.0;

/// Alfa del elemento fantasma (grilla deformada, curva previa).
/// Derivado de lo observado: `GRILLA_FANTASMA ≈ FONDO + 0.45·(GRILLA − FONDO)`
/// por canal (ver test `fantasma_es_grilla_al_45`); pineado a dos decimales.
pub const ALFA_FANTASMA: f32 = 0.45;
/// Alfa del hatch (trama diagonal de áreas). Sin precedente en los rasters
/// (usaban color fantasma directo); convención nueva por debajo del
/// atenuado de karaoke para que la trama no compita con el trazo.
pub const ALFA_HATCH: f32 = 0.5;
/// Alfa del elemento no activo (paridad `textanim::KARAOKE_ATENUADO`,
/// `textanim.rs:101`).
pub const ALFA_ATENUADO: f32 = 0.35;

/// Radio de nodo según el lado menor del lienzo
/// (`(min / 48).clamp(3, 9)`, `tpl_graphs.rs:1693-1695`). Pura.
pub const fn radio_nodo(lado_menor: u32) -> i32 {
    let r = (lado_menor / 48) as i32;
    if r < NODO_MIN_PX {
        NODO_MIN_PX
    } else if r > NODO_MAX_PX {
        NODO_MAX_PX
    } else {
        r
    }
}

/// Radio con halo de selección (`radio + 2`, `tpl_graphs.rs:1399`). Pura.
pub const fn radio_con_halo(radio: i32) -> i32 {
    radio + HALO_NODO_PX
}

/// Largo de la cabeza de flecha (`(8 + 2·grosor).min(24)`,
/// `tpl_linalg.rs:660`; `grosor` negativo se trata como 1). Pura.
pub fn cabeza_flecha(grosor: i32) -> f64 {
    let g = if grosor < 1 { 1.0 } else { grosor as f64 };
    (FLECHA_BASE_PX + FLECHA_POR_GROSOR * g).min(FLECHA_MAX_PX)
}

/// Mezcla `src` sobre `fondo` con alfa `0..1` (paridad `tpl_4d::mezcla`,
/// `tpl_4d.rs:1156`; alfa no finito → 0). Pura, sin pánicos.
pub fn mezcla_rgba(src: Rgba, fondo: Rgba, alfa: f32) -> Rgba {
    let a = if alfa.is_finite() {
        f64::from(alfa.clamp(0.0, 1.0))
    } else {
        0.0
    };
    let mut out = [0u8; 4];
    let mut k = 0;
    while k < 4 {
        let v = a * f64::from(src[k]) + (1.0 - a) * f64::from(fondo[k]);
        out[k] = v.round().clamp(0.0, 255.0) as u8;
        k += 1;
    }
    out
}

// ═══════════════════════════════════════════════════════════
// Tipo (misma convención que `textanim::Titulo`)
// ═══════════════════════════════════════════════════════════

/// Fuente de referencia del título: `32.0` px (la que usan los cuatro
/// `titulo_para`: `tpl_edo.rs:154`, `tpl_3d.rs:137`, `tpl_chaos.rs:160`,
/// `tpl_stats.rs:285`).
pub const TITULO_PX_REFERENCIA: f32 = 32.0;
/// Alto de canvas donde esos 32 px aplican (`640×480` en los tests de
/// `titulo_para`, p. ej. `tpl_edo.rs:1294`).
pub const TITULO_ALTO_REFERENCIA: f32 = 480.0;
/// Fracción del alto del canvas para el título (`32/480`; misma convención
/// de centrado medido que `textanim::Titulo::layout`).
pub const TITULO_FRACCION_ALTO: f32 = TITULO_PX_REFERENCIA / TITULO_ALTO_REFERENCIA;
/// Piso de fuente (paridad `textanim::TEXTANIM_FONT_MIN_PX`, `textanim.rs:88`).
pub const FUENTE_MIN_PX: f32 = 8.0;
/// Tope de fuente (paridad `textanim::TEXTANIM_FONT_MAX_PX`, `textanim.rs:90`).
pub const FUENTE_MAX_PX: f32 = 96.0;
/// Avance monoespaciado por glifo en em (paridad `textanim::TEXTO_AVANCE_POR_EM`,
/// `textanim.rs:93`; los valores vivos son monoespaciados).
pub const AVANCE_POR_EM: f32 = 0.6;
/// Alto de línea en em (paridad `textanim::TEXTO_ALTO_LINEA_POR_EM`).
pub const ALTO_LINEA_POR_EM: f32 = 1.2;
/// Escala del subtítulo (paridad `textanim::SUBTITULO_ESCALA`).
pub const SUBTITULO_ESCALA: f32 = 0.7;
/// Separación título↔subtítulo en em (paridad `textanim::SUBTITULO_SEPARACION_POR_EM`).
pub const SUBTITULO_SEPARACION_POR_EM: f32 = 0.5;
/// Ancho del glifo bitmap de etiquetas (`tpl_linalg.rs:821`, fuente 5×7).
pub const GLIFO_ETIQUETA_W: i32 = 5;
/// Alto del glifo bitmap de etiquetas.
pub const GLIFO_ETIQUETA_H: i32 = 7;
/// Corte de escala de etiqueta: lado menor `>= 300` → escala 2, si no 1
/// (`tpl_linalg.rs:1059-1065`; idéntico en graphs/4d).
pub const ETIQUETA_CORTE_PX: u32 = 300;
/// Escala de etiqueta a 720p (lado menor 720 `>= 300` → 2; pineado).
pub const ETIQUETA_ESCALA_720P: usize = 2;
/// Alto del rótulo a 720p (`7·2 + 8 = 22`; pineado).
pub const ROTULO_ALTO_720P: i32 = 22;
/// Padding del rótulo (`+ 8`, `tpl_linalg.rs:1069`).
pub const ROTULO_PAD_PX: i32 = 8;
/// Origen del texto del rótulo (`texto(buf, w, h, 6, 4, …)`, `tpl_linalg.rs:1071`).
pub const ROTULO_TEXTO_X: i32 = 6;
/// Origen Y del texto del rótulo.
pub const ROTULO_TEXTO_Y: i32 = 4;

/// Título en px para un alto de canvas (`alto · 32/480`, clamp `8..=96`;
/// a 480 da los 32 px observados). Pura.
pub fn titulo_px(alto_canvas: u32) -> f32 {
    let v = alto_canvas as f32 * TITULO_FRACCION_ALTO;
    v.clamp(FUENTE_MIN_PX, FUENTE_MAX_PX)
}

/// Subtítulo en px para una fuente de título (`0.7·font`, `textanim.rs:351`). Pura.
pub fn subtitulo_px(font_px: f32) -> f32 {
    font_px * SUBTITULO_ESCALA
}

/// Escala entera de la etiqueta bitmap (`1`/`2` con corte en 300 px,
/// `tpl_linalg.rs:1059-1065`). Pura.
pub const fn escala_etiqueta(lado_menor: u32) -> usize {
    if lado_menor >= ETIQUETA_CORTE_PX {
        2
    } else {
        1
    }
}

/// Alto de la barra de rótulo (`7·esc + 8` con `esc` clamp `1..=4`,
/// `tpl_linalg.rs:1069`). Pura.
pub const fn alto_rotulo(escala: usize) -> i32 {
    let esc = if escala < 1 {
        1
    } else if escala > 4 {
        4
    } else {
        escala as i32
    };
    GLIFO_ETIQUETA_H * esc + ROTULO_PAD_PX
}

/// Ancho monoespaciado de un valor vivo (`chars · 0.6em · font`;
/// misma medición que `textanim::Titulo::layout`). Pura.
pub fn ancho_valor(chars: usize, font_px: f32) -> f32 {
    chars as f32 * AVANCE_POR_EM * font_px
}

// ═══════════════════════════════════════════════════════════
// Duración (ms; setup/anim/hold + presets por familia)
// ═══════════════════════════════════════════════════════════

/// Duración mínima de escena/run (`tpl_am2.rs:69`, `anims.rs:37`,
/// `guion.rs:76`, `protocol.rs:2345`).
pub const DUR_MIN_MS: u64 = 100;
/// Duración máxima de escena/preset local (`tpl_am2.rs:71`; tope local: el
/// protocolo admite timelines de 120 s).
pub const DUR_MAX_MS: u64 = 60_000;
/// Total por defecto (`tpl_am2.rs:73`, `AM2_DUR_DEFAULT_MS`).
pub const DUR_TOTAL_DEFAULT_MS: u64 = 4_000;
/// Setup por defecto (20 % de 4000; fases `tpl_am1.rs:27-28`).
pub const DUR_SETUP_DEFAULT_MS: u64 = 800;
/// Animación por defecto (60 % de 4000).
pub const DUR_ANIM_DEFAULT_MS: u64 = 2_400;
/// Hold por defecto (20 % de 4000).
pub const DUR_HOLD_DEFAULT_MS: u64 = 800;
/// Numerador de setup/hold (1/5 = 20 %, `tpl_am1.rs:238-240`).
pub const FASE_QUINTO: u64 = 5;
/// Preset breve: pregunta larga (`guion.rs:1257`, `PREGUNTA_LARGO_RUN_MS`).
pub const RITMO_BREVE_MS: u64 = 2_600;
/// Preset estándar: escena por defecto (`tpl_am2.rs:73`).
pub const RITMO_ESTANDAR_MS: u64 = 4_000;
/// Preset largo: paso long-form (`guion.rs:1263`, `PACING_LARGO_RUN_MS[0]`).
pub const RITMO_LARGO_MS: u64 = 8_000;
/// Espera entre pasos largos (`guion.rs:1266`, `PACING_LARGO_WAIT_MS[0]`).
pub const RITMO_ESPERA_MS: u64 = 200;
/// Frames por segundo de referencia (long-form 50 s → 1500 frames).
pub const FPS_REFERENCIA: u32 = 30;

/// Ritmo de una escena (preset de la ola; los totales pinean valores
/// observados: 2600 pregunta, 4000 escena, 8000 paso largo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ritmo {
    /// Pregunta / cápsula breve (2600 ms).
    Breve,
    /// Escena estándar (4000 ms).
    Estandar,
    /// Paso long-form denso (8000 ms).
    Largo,
}

impl Ritmo {
    /// Total del preset en ms. Pura.
    pub const fn total_ms(self) -> u64 {
        match self {
            Self::Breve => RITMO_BREVE_MS,
            Self::Estandar => RITMO_ESTANDAR_MS,
            Self::Largo => RITMO_LARGO_MS,
        }
    }

    /// Setup (20 % del total). Pura.
    pub const fn setup_ms(self) -> u64 {
        self.total_ms() / FASE_QUINTO
    }

    /// Hold final (20 % del total). Pura.
    pub const fn hold_ms(self) -> u64 {
        self.total_ms() / FASE_QUINTO
    }

    /// Construcción (el resto: 60 % + redondeo entero). Pura.
    pub const fn anim_ms(self) -> u64 {
        self.total_ms() - self.setup_ms() - self.hold_ms()
    }

    /// Ritmo sugerido por dominio. Convención nueva: `Stats` (12 pasos de
    /// descenso, `tpl_stats.rs:91`) y `Caos` (zoom hasta span `1e-12`,
    /// `tpl_chaos.rs:205`) piden tiempo largo; el resto va al default de
    /// 4000 ms (`tpl_am2.rs:73`).
    pub const fn para_dominio(dominio: Dominio) -> Self {
        match dominio {
            Dominio::Stats | Dominio::Caos => Self::Largo,
            _ => Self::Estandar,
        }
    }
}

/// Reparte un total en `(setup, anim, hold)` 20/60/20 con aritmética entera
/// (`setup = total/5`, `hold = total/5`, `anim` = resto; paridad
/// `tpl_am1::alpha_fase`). Pura.
pub const fn reparte(total_ms: u64) -> (u64, u64, u64) {
    let setup = total_ms / FASE_QUINTO;
    let hold = total_ms / FASE_QUINTO;
    (setup, total_ms - setup - hold, hold)
}

/// ¿La duración está en `100..=60000` ms? (paridad `tpl_am2::valida_duracion`,
/// `tpl_am2.rs:299-305`). Pura.
pub const fn dura_valida(ms: u64) -> bool {
    ms >= DUR_MIN_MS && ms <= DUR_MAX_MS
}

/// Frames para una duración a `fps` (`ms·fps/1000`, saturado; `fps` 0 → 0;
/// paridad `tpl_linalg.rs:358`, `tpl_graphs.rs:878`: 50 s a 30 fps = 1500). Pura.
pub const fn frames_para_ms(duracion_ms: u64, fps: u32) -> u64 {
    if fps == 0 {
        0
    } else {
        duracion_ms.saturating_mul(fps as u64) / 1_000
    }
}

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn paleta_noche_pineada() {
        assert_eq!(FONDO_NOCHE, [15, 19, 28, 255]);
        assert_eq!(BARRA_NOCHE, [10, 14, 22, 255]);
        assert_eq!(GRILLA_NOCHE, [36, 46, 64, 255]);
        assert_eq!(GRILLA_FANTASMA, [24, 31, 45, 255]);
        assert_eq!(EJES_NOCHE, [110, 130, 165, 255]);
        assert_eq!(TEXTO_PRIMARIO, [235, 238, 245, 255]);
        assert_eq!(TEXTO_SECUNDARIO, [140, 150, 170, 255]);
        assert_eq!(PELIGRO, [250, 110, 110, 255]);
        assert_eq!(EXITO, [88, 196, 130, 255]);
        assert_eq!(AMARILLO, [255, 205, 70, 255]);
        assert_eq!(AZUL, [110, 170, 255, 255]);
        assert_eq!(NARANJA, [255, 160, 70, 255]);
        assert_eq!(GRIS_OSCURO, [52, 62, 82, 255]);
        assert_eq!(ARISTA, [74, 88, 112, 255]);
        assert_eq!(FONDO_DIA, [255, 255, 255, 255]);
    }

    #[test]
    fn paleta_4d_pineada() {
        assert_eq!(FONDO_4D, [8, 10, 20, 255]);
        assert_eq!(BARRA_4D, [5, 7, 15, 255]);
        assert_eq!(REJILLA_4D, [30, 34, 58, 255]);
        assert_eq!(BLANCO_4D, [232, 236, 248, 255]);
        assert_eq!(CIAN_4D, [90, 220, 255, 255]);
        assert_eq!(VIOLETA_4D, [168, 140, 255, 255]);
        assert_eq!(MAGENTA_4D, [255, 120, 200, 255]);
        assert_eq!(VERDE_4D, [110, 230, 150, 255]);
        assert_eq!(AMBAR_4D, [255, 200, 90, 255]);
    }

    #[test]
    fn paletas_estructuradas() {
        assert_eq!(Paleta::NOCHE.fondo, FONDO_NOCHE);
        assert_eq!(Paleta::NOCHE.barra, BARRA_NOCHE);
        assert_eq!(Paleta::NOCHE.grilla, GRILLA_NOCHE);
        assert_eq!(Paleta::NOCHE.ejes, EJES_NOCHE);
        assert_eq!(Paleta::NOCHE.texto, TEXTO_PRIMARIO);
        assert_eq!(Paleta::NOCHE.texto_secundario, TEXTO_SECUNDARIO);
        assert_eq!(Paleta::NOCHE.peligro, PELIGRO);
        assert_eq!(Paleta::NOCHE.exito, EXITO);
        assert_eq!(Paleta::NOCHE_VIOLETA.fondo, FONDO_4D);
        assert_eq!(Paleta::NOCHE_VIOLETA.barra, BARRA_4D);
        assert_eq!(Paleta::NOCHE_VIOLETA.grilla, REJILLA_4D);
        assert_eq!(Paleta::NOCHE_VIOLETA.texto, BLANCO_4D);
        assert_eq!(Paleta::NOCHE_VIOLETA.exito, VERDE_4D);
        // Toda paleta es opaca (restraint escandinavo: sin translucidez).
        for p in [Paleta::NOCHE, Paleta::NOCHE_VIOLETA] {
            for c in [
                p.fondo,
                p.barra,
                p.grilla,
                p.ejes,
                p.texto,
                p.texto_secundario,
                p.peligro,
                p.exito,
            ] {
                assert_eq!(c[3], 255);
            }
        }
    }

    #[test]
    fn acento_por_dominio_pineado() {
        assert_eq!(Dominio::Linalg.acento(), NARANJA);
        assert_eq!(Dominio::Am1.acento(), AMARILLO);
        assert_eq!(Dominio::Am2.acento(), AZUL);
        assert_eq!(Dominio::Edo.acento(), EXITO);
        assert_eq!(Dominio::Stats.acento(), VIOLETA_4D);
        assert_eq!(Dominio::TresD.acento(), CIAN_4D);
        assert_eq!(Dominio::CuatroD.acento(), CIAN_4D);
        assert_eq!(Dominio::Grafos.acento(), AMARILLO);
        assert_eq!(Dominio::Caos.acento(), MAGENTA_4D);
        assert_eq!(Dominio::Linalg.nombre(), "linalg");
        assert_eq!(Dominio::Am1.nombre(), "am1");
        assert_eq!(Dominio::Am2.nombre(), "am2");
        assert_eq!(Dominio::Edo.nombre(), "edo");
        assert_eq!(Dominio::Stats.nombre(), "stats");
        assert_eq!(Dominio::TresD.nombre(), "3d");
        assert_eq!(Dominio::CuatroD.nombre(), "4d");
        assert_eq!(Dominio::Grafos.nombre(), "grafos");
        assert_eq!(Dominio::Caos.nombre(), "caos");
    }

    #[test]
    fn trazos_pineados() {
        assert_eq!(TRAZO_FINO_PX, 1);
        assert_eq!(TRAZO_MEDIO_PX, 2);
        assert_eq!(TRAZO_GRUESO_PX, 3);
        assert!(TRAZO_FINO_PX < TRAZO_MEDIO_PX);
        assert!(TRAZO_MEDIO_PX < TRAZO_GRUESO_PX);
        assert_eq!((NODO_MIN_PX, NODO_MAX_PX), (3, 9));
        assert_eq!(NODO_A_720P_PX, 9);
        assert_eq!(HALO_NODO_PX, 2);
        assert_eq!(
            (FLECHA_BASE_PX, FLECHA_POR_GROSOR, FLECHA_MAX_PX),
            (8.0, 2.0, 24.0)
        );
    }

    #[test]
    fn radio_nodo_clampa_como_graphs() {
        assert_eq!(radio_nodo(0), 3);
        assert_eq!(radio_nodo(64), 3);
        assert_eq!(radio_nodo(360), 7);
        assert_eq!(radio_nodo(480), 9);
        assert_eq!(radio_nodo(720), 9);
        assert_eq!(radio_nodo(4096), 9);
        assert_eq!(radio_con_halo(7), 9);
        assert_eq!(radio_con_halo(NODO_MAX_PX), 11);
    }

    #[test]
    fn cabeza_flecha_como_linalg() {
        assert!((cabeza_flecha(2) - 12.0).abs() < 1e-12);
        assert!((cabeza_flecha(1) - 10.0).abs() < 1e-12);
        assert!((cabeza_flecha(100) - 24.0).abs() < 1e-12);
        assert!((cabeza_flecha(0) - 10.0).abs() < 1e-12);
        assert!((cabeza_flecha(-5) - 10.0).abs() < 1e-12);
    }

    #[test]
    fn alfas_pineados() {
        assert_eq!(ALFA_FANTASMA, 0.45);
        assert_eq!(ALFA_HATCH, 0.5);
        assert_eq!(ALFA_ATENUADO, 0.35);
        assert!(ALFA_ATENUADO < ALFA_FANTASMA);
        assert!(ALFA_FANTASMA < ALFA_HATCH);
    }

    #[test]
    fn fantasma_es_grilla_al_45_sobre_fondo() {
        let g = mezcla_rgba(GRILLA_NOCHE, FONDO_NOCHE, ALFA_FANTASMA);
        for k in 0..3 {
            let d = (i32::from(g[k]) - i32::from(GRILLA_FANTASMA[k])).abs();
            assert!(d <= 1, "canal {k}: {g:?} vs {:?}", GRILLA_FANTASMA);
        }
        assert_eq!(mezcla_rgba(GRILLA_NOCHE, FONDO_NOCHE, 1.0), GRILLA_NOCHE);
        assert_eq!(
            mezcla_rgba(GRILLA_NOCHE, FONDO_NOCHE, 0.0)[..3],
            FONDO_NOCHE[..3]
        );
        assert_eq!(
            mezcla_rgba(AZUL, FONDO_NOCHE, f32::NAN)[..3],
            FONDO_NOCHE[..3]
        );
    }

    #[test]
    fn tipo_con_convencion_textanim() {
        assert_eq!(TITULO_PX_REFERENCIA, 32.0);
        assert_eq!(TITULO_ALTO_REFERENCIA, 480.0);
        assert_eq!(FUENTE_MIN_PX, 8.0);
        assert_eq!(FUENTE_MAX_PX, 96.0);
        assert_eq!(AVANCE_POR_EM, 0.6);
        assert_eq!(ALTO_LINEA_POR_EM, 1.2);
        assert_eq!(SUBTITULO_ESCALA, 0.7);
        assert_eq!(SUBTITULO_SEPARACION_POR_EM, 0.5);
        assert_eq!((GLIFO_ETIQUETA_W, GLIFO_ETIQUETA_H), (5, 7));
        assert_eq!(ETIQUETA_CORTE_PX, 300);
        assert_eq!(ETIQUETA_ESCALA_720P, 2);
        assert_eq!(ROTULO_ALTO_720P, 22);
        assert_eq!((ROTULO_PAD_PX, ROTULO_TEXTO_X, ROTULO_TEXTO_Y), (8, 6, 4));
        // La fracción reproduce los 32 px a 480 de alto.
        assert!((TITULO_FRACCION_ALTO * 480.0 - 32.0).abs() < 1e-6);
        assert_eq!(titulo_px(480), 32.0);
        assert!((titulo_px(720) - 48.0).abs() < 1e-3);
        assert_eq!(titulo_px(64), 8.0);
        assert_eq!(titulo_px(4096), 96.0);
        assert!((subtitulo_px(32.0) - 22.4).abs() < 1e-4);
        assert!((ancho_valor(4, 48.0) - 4.0 * 0.6 * 48.0).abs() < 1e-4);
    }

    #[test]
    fn etiqueta_y_rotulo_como_rasters() {
        assert_eq!(escala_etiqueta(299), 1);
        assert_eq!(escala_etiqueta(300), 2);
        assert_eq!(escala_etiqueta(720), ETIQUETA_ESCALA_720P);
        assert_eq!(alto_rotulo(1), 15);
        assert_eq!(alto_rotulo(2), ROTULO_ALTO_720P);
        assert_eq!(alto_rotulo(0), 15);
        assert_eq!(alto_rotulo(99), 36);
    }

    #[test]
    fn duracion_por_defecto_20_60_20() {
        assert_eq!(DUR_MIN_MS, 100);
        assert_eq!(DUR_MAX_MS, 60_000);
        assert_eq!(DUR_TOTAL_DEFAULT_MS, 4_000);
        assert_eq!(DUR_SETUP_DEFAULT_MS, 800);
        assert_eq!(DUR_ANIM_DEFAULT_MS, 2_400);
        assert_eq!(DUR_HOLD_DEFAULT_MS, 800);
        assert_eq!(
            DUR_SETUP_DEFAULT_MS + DUR_ANIM_DEFAULT_MS + DUR_HOLD_DEFAULT_MS,
            DUR_TOTAL_DEFAULT_MS
        );
        assert_eq!(reparte(DUR_TOTAL_DEFAULT_MS), (800, 2_400, 800));
        assert_eq!(Ritmo::Estandar.total_ms(), 4_000);
        assert_eq!(Ritmo::Estandar.setup_ms(), 800);
        assert_eq!(Ritmo::Estandar.anim_ms(), 2_400);
        assert_eq!(Ritmo::Estandar.hold_ms(), 800);
    }

    #[test]
    fn presets_por_familia_pineados() {
        assert_eq!(RITMO_BREVE_MS, 2_600);
        assert_eq!(RITMO_ESTANDAR_MS, 4_000);
        assert_eq!(RITMO_LARGO_MS, 8_000);
        assert_eq!(RITMO_ESPERA_MS, 200);
        assert_eq!(Ritmo::Breve.total_ms(), 2_600);
        assert_eq!(Ritmo::Largo.total_ms(), 8_000);
        assert_eq!(reparte(RITMO_BREVE_MS), (520, 1_560, 520));
        assert_eq!(reparte(RITMO_LARGO_MS), (1_600, 4_800, 1_600));
        assert_eq!(Ritmo::para_dominio(Dominio::Stats), Ritmo::Largo);
        assert_eq!(Ritmo::para_dominio(Dominio::Caos), Ritmo::Largo);
        assert_eq!(Ritmo::para_dominio(Dominio::Linalg), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::Am1), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::Am2), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::Edo), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::TresD), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::CuatroD), Ritmo::Estandar);
        assert_eq!(Ritmo::para_dominio(Dominio::Grafos), Ritmo::Estandar);
    }

    #[test]
    fn valida_y_frames_como_rasters() {
        assert!(dura_valida(100));
        assert!(dura_valida(4_000));
        assert!(dura_valida(60_000));
        assert!(!dura_valida(99));
        assert!(!dura_valida(60_001));
        assert_eq!(frames_para_ms(50_000, 30), 1_500);
        assert_eq!(frames_para_ms(4_000, 30), 120);
        assert_eq!(frames_para_ms(1_000, 0), 0);
        assert_eq!(frames_para_ms(u64::MAX, 30), u64::MAX / 1_000);
    }
}
