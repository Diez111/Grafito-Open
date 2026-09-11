//! Subtítulos deterministas para el voiceover mínimo (P1-core).
//!
//! El voiceover reparte la narración de cada [`PasoGuion`](crate::guion::PasoGuion)
//! en su ventana de tiempo; este módulo la baja a pistas de subtítulos
//! ([`CaptionTrack`]) serializables en dos formatos:
//! - SRT (RFC: numeración 1-based, `HH:MM:SS,mmm`, ≤2 renglones, tags escapados)
//! - ASS v4+ (estilo `Caption`: blanco `#FFFFFF`, highlight amarillo `#FFD700`
//!   por palabra vía karaoke `{\k}`, outline 3, MarginV 80, fontsize 56
//!   relativo al player 720p —la Piel lo escala—).
//!
//! Presupuestos: texto `1..=200` chars, segmento dentro de 60 s
//! (`end_ms <= 60_000`), pista `0..=2000` segmentos, salida
//! `<= 256 KiB` en ambos formatos.
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red. Todo `Err` en
//! español, sin pánicos (sin `unwrap` en prod).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::guion::PasoGuion;

/// Chars máximos del texto de un segmento (anti-OOM de wire).
pub const CAPTION_MAX_CHARS: usize = 200;
/// Segmentos máximos por pista.
pub const CAPTION_MAX_SEGMENTS: usize = 2000;
/// Fin máximo de un segmento en ms (60 s, paridad con el timeline).
pub const CAPTION_MAX_END_MS: u32 = 60_000;
/// Palabras con timing máximas por segmento (anti-OOM; el voiceover real
/// trae `<= 40` por validación del guion).
pub const CAPTION_MAX_PALABRAS: usize = 500;
/// Cota de salida de `to_srt`/`to_ass` en bytes (256 KiB).
pub const CAPTION_MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// Renglones máximos por bloque SRT.
pub const SRT_MAX_LINEAS: usize = 2;
/// Estilo ASS: fontsize relativo al player (la Piel lo escala).
pub const ASS_FONTSIZE: u32 = 56;
/// Estilo ASS: outline.
pub const ASS_OUTLINE: u32 = 3;
/// Estilo ASS: margen vertical.
pub const ASS_MARGIN_V: u32 = 80;

/// Error honesto de subtítulos (todo en español, sin pánicos).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CaptionError {
    /// Texto vacío o solo espacios.
    #[error("subtítulo vacío: pasame al menos 1 carácter")]
    TextoVacio,
    /// Texto con controles o NUL.
    #[error("subtítulo con carácter de control: usá texto plano")]
    TextoConControl,
    /// Texto que excede 200 chars.
    #[error("subtítulo de {got} chars (válido 1..={max})")]
    TextoLargo { got: usize, max: usize },
    /// Ventana inválida (`start < end` exigido).
    #[error("ventana {start}..{end} inválida: el inicio debe ser menor que el fin")]
    VentanaInvalida { start: u32, end: u32 },
    /// Fin fuera de los 60 s.
    #[error("fin {end} ms fuera de 60 s (válido ..={max})")]
    FueraDeRango { end: u32, max: u32 },
    /// Palabra con timing inválida.
    #[error("palabra inválida: {motivo}")]
    PalabraInvalida { motivo: String },
    /// Demasiadas palabras en un segmento.
    #[error("{got} palabras exceden el tope de {max}: partí el segmento")]
    DemasiadasPalabras { got: usize, max: usize },
    /// Demasiados segmentos en la pista.
    #[error("{got} segmentos exceden el tope de {max}: partí la pista")]
    DemasiadosSegmentos { got: usize, max: usize },
    /// Segmentos desordenados o solapados.
    #[error("segmento {indice} desordenado o solapado: ordená por inicio sin solapar")]
    Desorden { indice: usize },
    /// Salida que excede 256 KiB.
    #[error("salida de {bytes} bytes excede 256 KiB: partí la pista")]
    SalidaMuyGrande { bytes: usize },
    /// Reparto de palabras imposible en la ventana.
    #[error("reparto imposible: {motivo}")]
    RepartoImposible { motivo: String },
    /// Ventanas desparejas con los pasos.
    #[error("tenés {duraciones} ventanas para {pasos} pasos: pasalas 1 a 1")]
    VentanasDesparejas { pasos: usize, duraciones: usize },
}

fn palabra_invalida(motivo: String) -> CaptionError {
    CaptionError::PalabraInvalida { motivo }
}

fn reparto_imposible(motivo: String) -> CaptionError {
    CaptionError::RepartoImposible { motivo }
}

/// Cuenta palabras separadas por blancos (misma cuenta que valida el
/// voiceover del guion). Pura.
pub fn cuenta_palabras(texto: &str) -> usize {
    texto.split_whitespace().count()
}

/// Colapsa blancos (`\n`, `\t`, dobles) a un solo espacio y recorta
/// bordes. Pura, determinista.
pub fn normaliza_texto(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Un segmento de subtítulo: frase + ventana + karaoke opcional por palabra.
///
/// `palabras` trae `(texto, inicio_ms, fin_ms)` por palabra dentro de
/// `[start_ms, end_ms]`; vacío = frase entera sin karaoke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionSegment {
    /// Frase (`1..=200` chars).
    pub texto: String,
    /// Inicio en ms (`< end_ms`).
    pub start_ms: u32,
    /// Fin en ms (`<= 60_000`).
    pub end_ms: u32,
    /// Karaoke por palabra (vacío = frase entera).
    #[serde(default)]
    pub palabras: Vec<(String, u32, u32)>,
}

impl CaptionSegment {
    /// Constructor validado.
    pub fn try_new(
        texto: String,
        start_ms: u32,
        end_ms: u32,
        palabras: Vec<(String, u32, u32)>,
    ) -> Result<Self, CaptionError> {
        let seg = Self {
            texto,
            start_ms,
            end_ms,
            palabras,
        };
        seg.validate()?;
        Ok(seg)
    }

    /// Frase sin karaoke (atajo para la Piel).
    pub fn frase(texto: String, start_ms: u32, end_ms: u32) -> Result<Self, CaptionError> {
        Self::try_new(texto, start_ms, end_ms, Vec::new())
    }

    /// Validación estricta (todo `Err` en español, sin pánicos).
    pub fn validate(&self) -> Result<(), CaptionError> {
        let texto = self.texto.trim();
        if texto.is_empty() {
            return Err(CaptionError::TextoVacio);
        }
        if texto.contains('\0')
            || texto
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(CaptionError::TextoConControl);
        }
        let largo = texto.chars().count();
        if largo > CAPTION_MAX_CHARS {
            return Err(CaptionError::TextoLargo {
                got: largo,
                max: CAPTION_MAX_CHARS,
            });
        }
        if self.start_ms >= self.end_ms {
            return Err(CaptionError::VentanaInvalida {
                start: self.start_ms,
                end: self.end_ms,
            });
        }
        if self.end_ms > CAPTION_MAX_END_MS {
            return Err(CaptionError::FueraDeRango {
                end: self.end_ms,
                max: CAPTION_MAX_END_MS,
            });
        }
        if self.palabras.len() > CAPTION_MAX_PALABRAS {
            return Err(CaptionError::DemasiadasPalabras {
                got: self.palabras.len(),
                max: CAPTION_MAX_PALABRAS,
            });
        }
        let mut previo_fin: Option<u32> = None;
        for (palabra, inicio, fin) in &self.palabras {
            let p = palabra.trim();
            if p.is_empty() {
                return Err(palabra_invalida("palabra vacía".to_string()));
            }
            if p.chars().count() > CAPTION_MAX_CHARS {
                return Err(palabra_invalida(format!(
                    "palabra de más de {CAPTION_MAX_CHARS} chars"
                )));
            }
            if p.contains(['{', '}', '\0']) || p.chars().any(|c| c.is_control()) {
                return Err(palabra_invalida(format!(
                    "palabra {p:?} con control o llaves"
                )));
            }
            if inicio < &self.start_ms || fin > &self.end_ms {
                return Err(palabra_invalida(format!(
                    "{p:?} ({inicio}..{fin}) fuera de {}..{}",
                    self.start_ms, self.end_ms
                )));
            }
            if inicio >= fin {
                return Err(palabra_invalida(format!(
                    "{p:?}: inicio {inicio} no menor que fin {fin}"
                )));
            }
            if let Some(previo) = previo_fin {
                if *inicio < previo {
                    return Err(palabra_invalida(format!(
                        "{p:?} arranca en {inicio} antes del fin previo {previo}"
                    )));
                }
            }
            previo_fin = Some(*fin);
        }
        Ok(())
    }
}

/// Pista de subtítulos: segmentos ordenados sin solapar (`0..=2000`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CaptionTrack {
    /// Segmentos en orden de reproducción.
    #[serde(default)]
    pub segments: Vec<CaptionSegment>,
}

impl CaptionTrack {
    /// Constructor validado.
    pub fn try_new(segments: Vec<CaptionSegment>) -> Result<Self, CaptionError> {
        let pista = Self { segments };
        pista.validate()?;
        Ok(pista)
    }

    /// Pista vacía (silencio total, válida).
    pub fn vacia() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// ¿Sin segmentos?
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Cantidad de segmentos.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Validación estricta: tope, cada segmento, orden sin solapar.
    pub fn validate(&self) -> Result<(), CaptionError> {
        if self.segments.len() > CAPTION_MAX_SEGMENTS {
            return Err(CaptionError::DemasiadosSegmentos {
                got: self.segments.len(),
                max: CAPTION_MAX_SEGMENTS,
            });
        }
        let mut previo_fin: Option<u32> = None;
        for (indice, seg) in self.segments.iter().enumerate() {
            seg.validate()?;
            if let Some(previo) = previo_fin {
                if seg.start_ms < previo {
                    return Err(CaptionError::Desorden { indice });
                }
            }
            previo_fin = Some(seg.end_ms);
        }
        Ok(())
    }

    /// Baja la pista a SRT (RFC: numeración 1-based, `HH:MM:SS,mmm`,
    /// ≤2 renglones, tags escapados). Cota `<= 256 KiB`.
    pub fn to_srt(&self) -> Result<String, CaptionError> {
        self.validate()?;
        let mut out = String::new();
        for (i, seg) in self.segments.iter().enumerate() {
            out.push_str(&(i + 1).to_string());
            out.push('\n');
            out.push_str(&formatea_srt_ts(seg.start_ms));
            out.push_str(" --> ");
            out.push_str(&formatea_srt_ts(seg.end_ms));
            out.push('\n');
            for linea in envuelve_dos_lineas(&seg.texto) {
                out.push_str(&escapa_srt(&linea));
                out.push('\n');
            }
            out.push('\n');
        }
        if out.len() > CAPTION_MAX_OUTPUT_BYTES {
            return Err(CaptionError::SalidaMuyGrande { bytes: out.len() });
        }
        Ok(out)
    }

    /// Baja la pista a ASS v4+ (estilo `Caption`: blanco `#FFFFFF`,
    /// highlight amarillo `#FFD700` por palabra vía karaoke `{\k}`,
    /// outline 3, MarginV 80, fontsize relativo). Sin `palabras` la
    /// frase va entera. Cota `<= 256 KiB`.
    pub fn to_ass(&self) -> Result<String, CaptionError> {
        self.validate()?;
        let mut out = String::from(
            "[Script Info]\nScriptType: v4.00+\nWrapStyle: 0\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709\n\n\
             [V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n",
        );
        out.push_str(&format!(
            "Style: Caption,Arial,{ASS_FONTSIZE},&H00FFFFFF,&H0000D7FF,&H00000000,&H80000000,0,0,0,0,100,100,0,0,1,{ASS_OUTLINE},0,2,20,20,{ASS_MARGIN_V},1\n\n\
             [Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n"
        ));
        for seg in &self.segments {
            let texto = if seg.palabras.is_empty() {
                escapa_ass(&normaliza_texto(&seg.texto))
            } else {
                karaoke_ass(&seg.palabras)
            };
            out.push_str(&format!(
                "Dialogue: 0,{}, {},Caption,,0,0,0,,{}\n",
                formatea_ass_ts(seg.start_ms),
                formatea_ass_ts(seg.end_ms),
                texto
            ));
        }
        if out.len() > CAPTION_MAX_OUTPUT_BYTES {
            return Err(CaptionError::SalidaMuyGrande { bytes: out.len() });
        }
        Ok(out)
    }
}

/// Formatea ms a `HH:MM:SS,mmm` (SRT). Pura.
pub fn formatea_srt_ts(ms: u32) -> String {
    let total_s = ms / 1000;
    let resto_ms = ms % 1000;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h:02}:{m:02}:{s:02},{resto_ms:03}")
}

/// Formatea ms a `h:mm:ss.cc` (ASS, centésimas por piso). Pura.
pub fn formatea_ass_ts(ms: u32) -> String {
    let cs = ms / 10;
    let cc = cs % 100;
    let total_s = cs / 100;
    let s = total_s % 60;
    let total_m = total_s / 60;
    let m = total_m % 60;
    let h = total_m / 60;
    format!("{h}:{m:02}:{s:02}.{cc:02}")
}

/// Escapa tags para SRT (`&` primero para no re-escapar). Pura.
pub fn escapa_srt(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escapa texto para ASS (sin bloques `{}` falsos: las llaves van a
/// paréntesis; la barra a `/`; el salto a espacio). Pura.
pub fn escapa_ass(texto: &str) -> String {
    texto
        .replace('\\', "/")
        .replace('{', "(")
        .replace('}', ")")
        .replace(['\n', '\r'], " ")
}

/// Parte la frase en ≤2 renglones balanceados por caracteres (las
/// palabras no se cortan; 1 palabra = 1 renglón). Pura, determinista.
pub fn envuelve_dos_lineas(texto: &str) -> Vec<String> {
    let normalizado = normaliza_texto(texto);
    let palabras: Vec<&str> = normalizado.split(' ').filter(|w| !w.is_empty()).collect();
    if palabras.len() <= 1 {
        return vec![palabras.join(" ")];
    }
    let total: usize = palabras.iter().map(|w| w.chars().count()).sum();
    let mut mejor = 1usize;
    let mut mejor_diff = usize::MAX;
    let mut izq = 0usize;
    for i in 1..palabras.len() {
        izq += palabras[i - 1].chars().count();
        let der = total.saturating_sub(izq);
        let diff = izq.abs_diff(der);
        if diff < mejor_diff {
            mejor_diff = diff;
            mejor = i;
        }
    }
    vec![palabras[..mejor].join(" "), palabras[mejor..].join(" ")]
}

/// Arma el karaoke ASS por palabra (`{\k<cs>}palabra `, `cs` por piso
/// con mínimo 1). Las palabras ya vienen validadas en orden. Pura.
fn karaoke_ass(palabras: &[(String, u32, u32)]) -> String {
    let mut out = String::new();
    for (palabra, inicio, fin) in palabras {
        let cs = fin.saturating_sub(*inicio) / 10;
        let cs = cs.max(1);
        out.push_str(&format!("{{\\k{cs}}}{} ", escapa_ass(palabra.trim())));
    }
    // El último espacio separa del fin de línea; si quedó colgando se
    // recorta sin tocar el karaoke.
    out.trim_end().to_string()
}

/// Baja el voiceover de los pasos a pista de subtítulos.
///
/// Reparte las palabras del `voiceover` de cada paso proporcionalmente
/// a su ventana (`duraciones_ms`, misma técnica del reparto de frames:
/// división entera con el resto a las primeras palabras, 1 ms mínimo
/// por palabra), acumulando en orden. El paso sin `voiceover` se saltea
/// (hueco silencioso legítimo, el cursor igual avanza).
///
/// Exige `pasos.len() == duraciones_ms.len()` y pista dentro de 60 s.
/// Pura, sin I/O, sin pánicos.
pub fn voiceover_segments(
    pasos: &[PasoGuion],
    duraciones_ms: &[u32],
) -> Result<CaptionTrack, CaptionError> {
    if pasos.len() != duraciones_ms.len() {
        return Err(CaptionError::VentanasDesparejas {
            pasos: pasos.len(),
            duraciones: duraciones_ms.len(),
        });
    }
    let mut segmentos = Vec::new();
    let mut cursor: u64 = 0;
    for (paso, dur) in pasos.iter().zip(duraciones_ms.iter()) {
        let inicio = cursor;
        cursor = cursor.saturating_add(u64::from(*dur));
        let Some(voz) = paso.voiceover.as_deref() else {
            continue;
        };
        let texto = normaliza_texto(voz);
        if texto.is_empty() {
            continue;
        }
        let palabras: Vec<&str> = texto.split(' ').filter(|w| !w.is_empty()).collect();
        if palabras.is_empty() {
            continue;
        }
        let dur_u64 = u64::from(*dur);
        if dur_u64 < palabras.len() as u64 {
            return Err(reparto_imposible(format!(
                "ventana de {dur} ms muy corta para {} palabras: dale al menos 1 ms por palabra",
                palabras.len()
            )));
        }
        let fin = inicio.saturating_add(dur_u64);
        if fin > u64::from(CAPTION_MAX_END_MS) {
            return Err(CaptionError::FueraDeRango {
                end: fin.min(u64::from(u32::MAX)) as u32,
                max: CAPTION_MAX_END_MS,
            });
        }
        let base = *dur / palabras.len() as u32;
        let resto = *dur % palabras.len() as u32;
        let mut cursor_palabra = inicio;
        let mut con_tiempos = Vec::with_capacity(palabras.len());
        for (k, palabra) in palabras.iter().enumerate() {
            let extra = if (k as u32) < resto { 1 } else { 0 };
            let tramo = base.saturating_add(extra);
            let fin_palabra = cursor_palabra.saturating_add(u64::from(tramo));
            con_tiempos.push((
                (*palabra).to_string(),
                cursor_palabra.min(u64::from(u32::MAX)) as u32,
                fin_palabra.min(u64::from(u32::MAX)) as u32,
            ));
            cursor_palabra = fin_palabra;
        }
        segmentos.push(CaptionSegment {
            texto,
            start_ms: inicio.min(u64::from(u32::MAX)) as u32,
            end_ms: fin.min(u64::from(u32::MAX)) as u32,
            palabras: con_tiempos,
        });
    }
    CaptionTrack::try_new(segmentos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segmento(texto: &str, inicio: u32, fin: u32) -> CaptionSegment {
        CaptionSegment {
            texto: texto.to_string(),
            start_ms: inicio,
            end_ms: fin,
            palabras: Vec::new(),
        }
    }

    #[test]
    fn segmento_valida_bordes() {
        assert!(segmento("hola", 0, 1000).validate().is_ok());
        assert!(segmento("", 0, 1000).validate().is_err());
        assert!(segmento("   ", 0, 1000).validate().is_err());
        assert!(segmento("a".repeat(201).as_str(), 0, 1000)
            .validate()
            .is_err());
        assert!(segmento("a".repeat(200).as_str(), 0, 1000)
            .validate()
            .is_ok());
        assert!(segmento("hola", 1000, 1000).validate().is_err());
        assert!(segmento("hola", 2000, 1000).validate().is_err());
        assert!(segmento("hola", 0, 60_001).validate().is_err());
        assert!(segmento("hola", 0, 60_000).validate().is_ok());
        // Palabras: fuera de ventana, invertidas y desordenadas fallan.
        let fuera = CaptionSegment {
            texto: "hola mundo".to_string(),
            start_ms: 0,
            end_ms: 1000,
            palabras: vec![
                ("hola".to_string(), 0, 500),
                ("mundo".to_string(), 900, 1100),
            ],
        };
        assert!(fuera.validate().is_err());
        let invertida = CaptionSegment {
            texto: "hola".to_string(),
            start_ms: 0,
            end_ms: 1000,
            palabras: vec![("hola".to_string(), 500, 500)],
        };
        assert!(invertida.validate().is_err());
    }

    #[test]
    fn pista_valida_tope_orden_y_solape() {
        assert!(CaptionTrack::vacia().validate().is_ok());
        let a = segmento("uno", 0, 1000);
        let b = segmento("dos", 1000, 2000);
        assert!(CaptionTrack::try_new(vec![a.clone(), b.clone()]).is_ok());
        // Solape: el segundo arranca antes del fin del primero.
        let solapado = segmento("dos", 999, 2000);
        assert!(CaptionTrack::try_new(vec![a.clone(), solapado]).is_err());
        // Desorden.
        assert!(CaptionTrack::try_new(vec![b, a]).is_err());
        // Tope 2000.
        let muchos: Vec<CaptionSegment> = (0..2001)
            .map(|i| segmento("x", i * 10, i * 10 + 5))
            .collect();
        assert!(CaptionTrack::try_new(muchos).is_err());
    }

    #[test]
    fn srt_numeracion_tiempos_dos_renglones_y_escape() {
        let pista = CaptionTrack::try_new(vec![
            segmento("hola", 1000, 3500),
            CaptionSegment {
                texto: "esta frase es bastante larga y se parte en dos renglones seguro"
                    .to_string(),
                start_ms: 4000,
                end_ms: 8000,
                palabras: Vec::new(),
            },
            segmento("a <b> & c", 9000, 9500),
        ])
        .unwrap();
        let srt = pista.to_srt().unwrap();
        assert!(srt.starts_with("1\n00:00:01,000 --> 00:00:03,500\nhola\n\n"));
        assert!(srt.contains("2\n00:00:04,000 --> 00:00:08,000\n"));
        // El segmento largo va en exactamente 2 renglones.
        let bloque2 = srt.split("\n\n").nth(1).unwrap();
        assert_eq!(
            bloque2.lines().count(),
            4,
            "nro + tiempos + 2 renglones: {bloque2}"
        );
        // Tags escapados (el wrap lo parte en 2 renglones, cada mitad escapa).
        assert!(srt.contains("a &lt;b&gt;"));
        assert!(srt.contains("&amp; c"));
        assert!(!srt.contains("a <b>"));
    }

    #[test]
    fn ass_estilo_caption_y_karaoke() {
        // Sin palabras: frase entera con el estilo Caption.
        let pista = CaptionTrack::try_new(vec![segmento("hola mundo", 1000, 3500)]).unwrap();
        let ass = pista.to_ass().unwrap();
        assert!(ass.contains("ScriptType: v4.00+"));
        assert!(ass.contains("Style: Caption,Arial,56,&H00FFFFFF,&H0000D7FF,"));
        assert!(ass.contains(",3,0,2,20,20,80,1"));
        assert!(ass.contains("Dialogue: 0,0:00:01.00, 0:00:03.50,Caption,,0,0,0,,hola mundo"));
        // Con palabras: karaoke por palabra con highlight amarillo.
        let con_voz = CaptionSegment {
            texto: "hola mundo".to_string(),
            start_ms: 0,
            end_ms: 1000,
            palabras: vec![
                ("hola".to_string(), 0, 400),
                ("mundo".to_string(), 400, 1000),
            ],
        };
        let pista = CaptionTrack::try_new(vec![con_voz]).unwrap();
        let ass = pista.to_ass().unwrap();
        assert!(ass.contains("{\\k40}hola {\\k60}mundo"));
    }

    #[test]
    fn salidas_acotan_a_256kib() {
        // 2000 segmentos de 200 chars superan 256 KiB en ambos formatos.
        let muchos: Vec<CaptionSegment> = (0..2000)
            .map(|i| segmento("a".repeat(200).as_str(), i * 30, i * 30 + 25))
            .collect();
        let pista = CaptionTrack::try_new(muchos).unwrap();
        assert!(pista.to_srt().is_err());
        assert!(pista.to_ass().is_err());
    }

    #[test]
    fn timestamps_puros() {
        assert_eq!(formatea_srt_ts(0), "00:00:00,000");
        assert_eq!(formatea_srt_ts(61_500), "00:01:01,500");
        assert_eq!(formatea_ass_ts(0), "0:00:00.00");
        assert_eq!(formatea_ass_ts(61_500), "0:01:01.50");
        // Dos líneas balanceadas sin cortar palabras.
        let lineas = envuelve_dos_lineas("una dos tres cuatro cinco seis");
        assert_eq!(lineas.len(), 2);
        assert_eq!(envuelve_dos_lineas("sola"), vec!["sola".to_string()]);
    }
}
