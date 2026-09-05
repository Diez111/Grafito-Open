//! Modelo intermedio entre el XML de GeoGebra y los comandos Grafito.

/// Un `<element type label>` con los hijos que nos importan.
#[derive(Debug, Clone, Default)]
pub(crate) struct GgbElemento {
    pub tipo: String,
    pub etiqueta: String,
    pub coords: Option<[f64; 4]>,
    pub valor: Option<f64>,
    pub deslizador: Option<(f64, f64)>,
    pub matrix: Option<[f64; 6]>,
    pub eigen: Option<[f64; 4]>,
    pub vector_start: Option<[f64; 2]>,
    pub texto: Option<String>,
    pub es_celda_hoja: bool,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct GgbComando {
    pub nombre: String,
    pub entradas: Vec<String>,
    pub salidas: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct GgbExpresion {
    pub etiqueta: String,
    pub exp: String,
    pub tipo: String,
}
#[derive(Debug, Clone, Copy)]
pub(crate) enum ItemOrden {
    Elemento(usize),
    Comando(usize),
}
#[derive(Debug, Clone, Default)]
pub(crate) struct Construccion {
    pub elementos: Vec<GgbElemento>,
    pub comandos: Vec<GgbComando>,
    pub expresiones: Vec<GgbExpresion>,
    pub orden: Vec<ItemOrden>,
    pub con_script: bool,
    pub con_cas: bool,
    pub hoja_celdas: Vec<Vec<String>>,
}
impl Construccion {
    #[allow(dead_code)]
    pub(crate) fn has_hoja(&self) -> bool {
        !self.hoja_celdas.is_empty()
    }
}
