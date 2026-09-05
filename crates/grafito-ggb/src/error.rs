//! Errores tipados
use std::fmt::{Display, Formatter, Result as FmtResult};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GgbError {
    Vacio,
    DemasiadoGrande {
        bytes: u64,
        limite: u64,
    },
    DemasiadasEntradas {
        encontradas: u64,
        limite: u64,
    },
    ZipInvalido {
        detalle: String,
    },
    EntradaPeligrosa {
        nombre: String,
        motivo: &'static str,
    },
    MetodoNoSoportado {
        entrada: String,
        metodo: String,
    },
    BombaZip {
        entrada: String,
        detalle: String,
    },
    XmlFaltante,
    XmlDemasiadoGrande {
        bytes: u64,
        limite: u64,
    },
    XmlMalformado {
        detalle: String,
    },
    LimiteElementos {
        encontrados: usize,
        limite: usize,
    },
}
impl GgbError {
    pub(crate) fn recorta(detalle: &str) -> String {
        const MAX: usize = 256;
        if detalle.chars().count() > MAX {
            detalle.chars().take(MAX).collect()
        } else {
            detalle.to_string()
        }
    }
}
impl Display for GgbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Vacio => write!(f, "ggb: archivo vacío"),
            Self::DemasiadoGrande { bytes, limite } => write!(
                f,
                "ggb: archivo de {bytes} B supera el límite de {limite} B"
            ),
            Self::DemasiadasEntradas {
                encontradas,
                limite,
            } => write!(
                f,
                "ggb: {encontradas} entradas ZIP superan el límite de {limite}"
            ),
            Self::ZipInvalido { detalle } => write!(f, "ggb: ZIP inválido: {detalle}"),
            Self::EntradaPeligrosa { nombre, motivo } => {
                write!(f, "ggb: entrada peligrosa '{nombre}': {motivo}")
            }
            Self::MetodoNoSoportado { entrada, metodo } => write!(
                f,
                "ggb: '{entrada}' usa compresión '{metodo}': solo almacenado/deflate"
            ),
            Self::BombaZip { entrada, detalle } => {
                write!(f, "ggb: posible ZIP-bomb en '{entrada}': {detalle}")
            }
            Self::XmlFaltante => write!(f, "ggb: falta geogebra.xml en el archivo"),
            Self::XmlDemasiadoGrande { bytes, limite } => write!(
                f,
                "ggb: geogebra.xml de {bytes} B supera el límite de {limite} B"
            ),
            Self::XmlMalformado { detalle } => write!(f, "ggb: XML malformado: {detalle}"),
            Self::LimiteElementos {
                encontrados,
                limite,
            } => write!(
                f,
                "ggb: {encontrados} elementos superan el límite de {limite}"
            ),
        }
    }
}
impl std::error::Error for GgbError {}
