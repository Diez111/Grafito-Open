//! Posicionamiento GPOS puro con `ttf-parser` (sin `rustybuzz`, sin deps nuevas).
//!
//! `formulary` (feature `svg`, sin `shaping`) maqueta por cmap + avances +
//! tabla MATH: métricas reales pero sin tracking fino GPOS. Este módulo aplica
//! un post-pase acotado sobre la display-list (`formulary::Layout`) que
//! comparten `latex_to_svg` y `latex_to_rgba`:
//!
//! - Kerning horizontal (`kern`/`dist` vía GPOS PairPos formato 1 y 2, más
//!   tabla legacy `kern`): entre glifos consecutivos de una misma corrida
//!   (mismo `y` y `size`, sin espejado) que se tocan exactamente
//!   (`x₂ == x₁ + advance₁`). El `dx` desplaza la cinta `x ≥ xₖ` (glifos,
//!   reglas y fondos) y achica `width` en la misma medida.
//! - Marcas (MarkToBase / MarkToLigature): consulta real de anclas; solo se
//!   aplica si la marca es combining de verdad (avance 0 en el mismo origen
//!   que la base). Con la STIX Two Math embebida este camino es no-op
//!   verificado (la fuente no trae esas subtablas y los acentos de formulary
//!   son overscripts MATH, no combining marks): sin ajuste antes que ajuste
//!   inventado.
//!
//! ## Límites honestos (verificado contra STIX Two Math embebida)
//!
//! - L1: la fuente no trae script `math` en GPOS (solo `DFLT`/`latn`/`cyrl`/
//!   `grek`); se usan `latn` + `DFLT`. Sin script matemático no hay lookups
//!   específicos de math que perderse: los 4 lookups son `kern` alcanzables
//!   desde `latn`/`DFLT`.
//! - L2: los 7 subtables GPOS son todos PairPos; no hay SinglePos, Cursive,
//!   Context/Chain ni Mark*: el motor los salta explícitamente. Si otra
//!   fuente los trajera, el kerning resultante sería parcial (documentado,
//!   no inventado).
//! - L3: de `ValueRecord` solo se aplica `x_advance` del 1º + `x_placement`
//!   del 2º (todo lo medido en STIX trae `x_placement/y_*` en cero).
//!   `y_placement` se ignora para no romper el contrato de baseline.
//! - L4: el `dx` por par se acota a `|dx| <= size` (anti-fuente-loca).
//! - L5: reglas que cruzan el corte de kern (barra de `\frac` sobre numerador
//!   kernado) conservan su ancho: overhang acotado por L4, no re-maqueta.
//! - L6: corridas con combining marks no saltean marcas en PairPos
//!   (`ignore_marks`): inalcanzable con lo que emite formulary hoy.
//!
//! ## Cotas anti-DoS
//!
//! Todo bucle está acotado por constante (`GPOS_MAX_*`): lookups, subtables,
//! ítems y pares. `PairSet::get` es búsqueda binaria y `ClassMatrix::get` es
//! O(1) (ambos de `ttf-parser`); acá no hay loops sobre datos de la fuente
//! salvo iterar subtables con tope.

/// Tope de lookups GPOS recolectados por cara (la STIX trae 4).
pub const GPOS_MAX_LOOKUPS: usize = 32;
/// Tope de subtables inspeccionados por lookup.
pub const GPOS_MAX_SUBTABLES_POR_LOOKUP: u16 = 16;
/// Tope de ítems de display-list procesados (con 256 nodos es inalcanzable).
pub const GPOS_MAX_ITEMS: usize = 4096;
/// Tope de pares evaluados por layout.
pub const GPOS_MAX_PARES: usize = 4096;
/// Tope de subtables legacy `kern` inspeccionados.
pub const GPOS_MAX_KERN_SUBTABLES: u32 = 8;
/// Tolerancia de contigüidad: fracción de `size` (0.1px a 24px; espaciados
/// reales TeX como thin-space ≈ 4px quedan muy por encima).
pub const GPOS_ABUT_EPS_FRACTION: f32 = 0.004;

/// Resultado del post-pase (para dorados y diagnóstico del llamador).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GposReport {
    /// Pares de kerning con `dx != 0` aplicados.
    pub pares_kern: u32,
    /// Suma de `dx` aplicados, en unidades de layout (px a `font_px`).
    pub dx_total: f32,
    /// Marcas combining reubicadas por ancla.
    pub marcas: u32,
    /// `false` si se tocó algún tope (o la cara no da métricas sanas): el
    /// layout puede estar parcialmente sin ajustar, jamás roto.
    pub completo: bool,
}

impl GposReport {
    /// Reporte vacío: nada que ajustar (o nada ajustado).
    pub const fn vacio(completo: bool) -> Self {
        Self {
            pares_kern: 0,
            dx_total: 0.0,
            marcas: 0,
            completo,
        }
    }
}

/// Etiquetas de feature GPOS de kerning que se recolectan.
fn es_feature_kern(tag: ttf_parser::Tag) -> bool {
    tag == ttf_parser::Tag::from_bytes(b"kern") || tag == ttf_parser::Tag::from_bytes(b"dist")
}

/// Etiquetas de feature GPOS de marcas que se recolectan.
fn es_feature_marca(tag: ttf_parser::Tag) -> bool {
    tag == ttf_parser::Tag::from_bytes(b"mark")
        || tag == ttf_parser::Tag::from_bytes(b"mkmk")
        || tag == ttf_parser::Tag::from_bytes(b"abvm")
        || tag == ttf_parser::Tag::from_bytes(b"blwm")
}

/// Índices de lookup (dedup, ordenados) alcanzables desde `scripts` para las
/// features cuyo tag pasa `filtro`. Con tope `GPOS_MAX_LOOKUPS`.
fn lookups_para(
    gpos: &ttf_parser::opentype_layout::LayoutTable<'_>,
    scripts: &[ttf_parser::Tag],
    filtro: fn(ttf_parser::Tag) -> bool,
) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for script_tag in scripts {
        let Some(script) = gpos.scripts.find(*script_tag) else {
            continue;
        };
        // Lenguaje por defecto + explícitos (STIX: solo default, sin langs).
        let mut sistemas: Vec<ttf_parser::opentype_layout::LanguageSystem<'_>> = Vec::new();
        if let Some(defecto) = script.default_language {
            sistemas.push(defecto);
        }
        for i in 0..script.languages.len() {
            if let Some(sistema) = script.languages.get(i) {
                sistemas.push(sistema);
            }
        }
        for sistema in sistemas {
            for fi in sistema.feature_indices.into_iter() {
                let Some(feature) = gpos.features.get(fi) else {
                    continue;
                };
                if !filtro(feature.tag) {
                    continue;
                }
                for li in feature.lookup_indices.into_iter() {
                    if out.len() >= GPOS_MAX_LOOKUPS {
                        return ordena_dedup(out);
                    }
                    if !out.contains(&li) {
                        out.push(li);
                    }
                }
            }
        }
    }
    ordena_dedup(out)
}

/// Respaldo global si `latn`+`DFLT` no existen: features `kern`/`dist` (o
/// marcas) de toda la tabla, con el mismo tope.
fn lookups_global(
    gpos: &ttf_parser::opentype_layout::LayoutTable<'_>,
    filtro: fn(ttf_parser::Tag) -> bool,
) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::new();
    for i in 0..gpos.features.len() {
        let Some(feature) = gpos.features.get(i) else {
            continue;
        };
        if !filtro(feature.tag) {
            continue;
        }
        for li in feature.lookup_indices.into_iter() {
            if out.len() >= GPOS_MAX_LOOKUPS {
                return ordena_dedup(out);
            }
            if !out.contains(&li) {
                out.push(li);
            }
        }
    }
    ordena_dedup(out)
}

/// Ordena y deduplica índices de lookup (aplicación determinista).
fn ordena_dedup(mut indices: Vec<u16>) -> Vec<u16> {
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// Subtables PairPos recolectados (máx. `GPOS_MAX_LOOKUPS` en total).
fn recolecta_pares<'a>(
    cara: &ttf_parser::Face<'a>,
    lookups: &[u16],
) -> Vec<ttf_parser::gpos::PairAdjustment<'a>> {
    let mut pares = Vec::new();
    let Some(gpos) = cara.tables().gpos else {
        return pares;
    };
    for li in lookups.iter().take(GPOS_MAX_LOOKUPS) {
        let Some(lookup) = gpos.lookups.get(*li) else {
            continue;
        };
        let n = lookup.subtables.len().min(GPOS_MAX_SUBTABLES_POR_LOOKUP);
        for j in 0..n {
            if pares.len() >= GPOS_MAX_LOOKUPS {
                return pares;
            }
            let Some(sub) = lookup
                .subtables
                .get::<ttf_parser::gpos::PositioningSubtable>(j)
            else {
                continue;
            };
            // L2: Single/Cursive/Context/Chain/Mark* se saltan acá (kerning
            // es solo PairPos); las marcas van por `recolecta_marcas`.
            if let ttf_parser::gpos::PositioningSubtable::Pair(par) = sub {
                pares.push(par);
            }
        }
    }
    pares
}

/// Subtables de marcas recolectados (MarkToBase + MarkToLigature; MarkToMark
/// queda fuera, ver L6 y docs del módulo).
#[allow(clippy::type_complexity)]
fn recolecta_marcas<'a>(
    cara: &ttf_parser::Face<'a>,
    lookups: &[u16],
) -> (
    Vec<ttf_parser::gpos::MarkToBaseAdjustment<'a>>,
    Vec<ttf_parser::gpos::MarkToLigatureAdjustment<'a>>,
) {
    let mut bases = Vec::new();
    let mut ligaduras = Vec::new();
    let Some(gpos) = cara.tables().gpos else {
        return (bases, ligaduras);
    };
    for li in lookups.iter().take(GPOS_MAX_LOOKUPS) {
        let Some(lookup) = gpos.lookups.get(*li) else {
            continue;
        };
        let n = lookup.subtables.len().min(GPOS_MAX_SUBTABLES_POR_LOOKUP);
        for j in 0..n {
            if bases.len() + ligaduras.len() >= GPOS_MAX_LOOKUPS {
                return (bases, ligaduras);
            }
            let Some(sub) = lookup
                .subtables
                .get::<ttf_parser::gpos::PositioningSubtable>(j)
            else {
                continue;
            };
            match sub {
                ttf_parser::gpos::PositioningSubtable::MarkToBase(m) => bases.push(m),
                ttf_parser::gpos::PositioningSubtable::MarkToLigature(m) => ligaduras.push(m),
                _ => {}
            }
        }
    }
    (bases, ligaduras)
}

/// Kerning GPOS (PairPos) entre dos glifos, en unidades de diseño (fiel al
/// orden de lookups: se suman, como HarfBuzz). L3: `x_advance` del 1º +
/// `x_placement` del 2º; el resto se ignora (cero en STIX, verificado).
pub fn par_kern_unidades(cara: &ttf_parser::Face<'_>, primero: u16, segundo: u16) -> i32 {
    let [latn, dflt] = [
        ttf_parser::Tag::from_bytes(b"latn"),
        ttf_parser::Tag::from_bytes(b"DFLT"),
    ];
    let Some(gpos) = cara.tables().gpos else {
        return 0;
    };
    let mut indices = lookups_para(&gpos, &[latn, dflt], es_feature_kern);
    if indices.is_empty() {
        indices = lookups_global(&gpos, es_feature_kern);
    }
    let pares = recolecta_pares(cara, &indices);
    kern_en_pares(&pares, primero, segundo)
}

/// Suma de `dx` sobre una lista ya recolectada (núcleo testeable).
fn kern_en_pares(
    pares: &[ttf_parser::gpos::PairAdjustment<'_>],
    primero: u16,
    segundo: u16,
) -> i32 {
    let g1 = ttf_parser::GlyphId(primero);
    let g2 = ttf_parser::GlyphId(segundo);
    let mut dx: i32 = 0;
    for par in pares {
        match *par {
            ttf_parser::gpos::PairAdjustment::Format1 { coverage, sets } => {
                let Some(idx) = coverage.get(g1) else {
                    continue;
                };
                let Some(set) = sets.get(idx) else {
                    continue;
                };
                if let Some((r1, r2)) = set.get(g2) {
                    dx = dx.saturating_add(i32::from(r1.x_advance) + i32::from(r2.x_placement));
                }
            }
            ttf_parser::gpos::PairAdjustment::Format2 {
                coverage,
                classes,
                matrix,
            } => {
                // Conservador: sin cobertura del 1º no se toca (la clase 0 es
                // "sin clase" y su celda puede traer basura honesta).
                if coverage.get(g1).is_none() {
                    continue;
                }
                let c1 = classes.0.get(g1);
                let c2 = classes.1.get(g2);
                if let Some((r1, r2)) = matrix.get((c1, c2)) {
                    dx = dx.saturating_add(i32::from(r1.x_advance) + i32::from(r2.x_placement));
                }
            }
        }
    }
    dx
}

/// Kerning legacy tabla `kern` (subtables horizontales simples). STIX no la
/// trae (`None`); se soporta por si la fuente embebida cambia.
pub fn legacy_kern_unidades(
    cara: &ttf_parser::Face<'_>,
    primero: u16,
    segundo: u16,
) -> Option<i16> {
    let kern = cara.tables().kern?;
    let g1 = ttf_parser::GlyphId(primero);
    let g2 = ttf_parser::GlyphId(segundo);
    for sub in kern
        .subtables
        .into_iter()
        .take(GPOS_MAX_KERN_SUBTABLES as usize)
    {
        if !sub.horizontal || sub.variable || sub.has_cross_stream || sub.has_state_machine {
            continue;
        }
        if let Some(valor) = sub.glyphs_kerning(g1, g2) {
            return Some(valor);
        }
    }
    None
}

/// Delta de anclas MarkToBase (`ancla_base - ancla_marca`), en unidades de
/// diseño. `None` = sin subtable aplicable (caso STIX: siempre).
pub fn marca_base_delta_unidades(
    cara: &ttf_parser::Face<'_>,
    base: u16,
    marca: u16,
) -> Option<(i16, i16)> {
    let [latn, dflt] = [
        ttf_parser::Tag::from_bytes(b"latn"),
        ttf_parser::Tag::from_bytes(b"DFLT"),
    ];
    let gpos = cara.tables().gpos?;
    let mut indices = lookups_para(&gpos, &[latn, dflt], es_feature_marca);
    if indices.is_empty() {
        indices = lookups_global(&gpos, es_feature_marca);
    }
    let (bases, _) = recolecta_marcas(cara, &indices);
    let gb = ttf_parser::GlyphId(base);
    let gm = ttf_parser::GlyphId(marca);
    for sub in &bases {
        let Some(mi) = sub.mark_coverage.get(gm) else {
            continue;
        };
        let Some((clase, ancla_marca)) = sub.marks.get(mi) else {
            continue;
        };
        let Some(bi) = sub.base_coverage.get(gb) else {
            continue;
        };
        let Some(ancla_base) = sub.anchors.get(bi, clase) else {
            continue;
        };
        return Some((
            ancla_base.x.saturating_sub(ancla_marca.x),
            ancla_base.y.saturating_sub(ancla_marca.y),
        ));
    }
    None
}

/// Delta de anclas MarkToLigature (componente 0), en unidades de diseño.
pub fn marca_ligadura_delta_unidades(
    cara: &ttf_parser::Face<'_>,
    ligadura: u16,
    marca: u16,
) -> Option<(i16, i16)> {
    let [latn, dflt] = [
        ttf_parser::Tag::from_bytes(b"latn"),
        ttf_parser::Tag::from_bytes(b"DFLT"),
    ];
    let gpos = cara.tables().gpos?;
    let mut indices = lookups_para(&gpos, &[latn, dflt], es_feature_marca);
    if indices.is_empty() {
        indices = lookups_global(&gpos, es_feature_marca);
    }
    let (_, ligaduras) = recolecta_marcas(cara, &indices);
    let gl = ttf_parser::GlyphId(ligadura);
    let gm = ttf_parser::GlyphId(marca);
    for sub in &ligaduras {
        let Some(mi) = sub.mark_coverage.get(gm) else {
            continue;
        };
        let Some((clase, ancla_marca)) = sub.marks.get(mi) else {
            continue;
        };
        let Some(li) = sub.ligature_coverage.get(gl) else {
            continue;
        };
        let Some(matriz) = sub.ligature_array.get(li) else {
            continue;
        };
        // Componente 0 (fila 0): la ligadura aislada de formulary no expone
        // componentes; otro `row` sería inventar el target.
        let Some(ancla_base) = matriz.get(0, clase) else {
            continue;
        };
        return Some((
            ancla_base.x.saturating_sub(ancla_marca.x),
            ancla_base.y.saturating_sub(ancla_marca.y),
        ));
    }
    None
}

/// ¿El ítem es glifo con métricas finitas? (Reglas/fondos/variantes futuras
/// no participan en pares pero sí en el desplazamiento de cinta.)
fn es_glifo_finito(item: &formulary::Item) -> bool {
    match *item {
        formulary::Item::Glyph {
            x,
            y,
            size,
            advance,
            ..
        } => {
            x.is_finite() && y.is_finite() && size.is_finite() && size > 0.0 && advance.is_finite()
        }
        _ => false,
    }
}

/// Aplica el post-pase GPOS al layout (kerning en cinta + marcas combining).
/// Puro salvo lecturas de la cara (sin I/O). Nunca deja el layout roto: ante
/// cualquier tope o métrica no finita devuelve `completo: false` y lo ya
/// aplicado sigue siendo válido (desplazamientos de cinta consistentes).
pub fn apply_gpos(cara: &ttf_parser::Face<'_>, layout: &mut formulary::Layout) -> GposReport {
    let mut reporte = GposReport::vacio(true);
    if layout.items.len() > GPOS_MAX_ITEMS {
        reporte.completo = false;
        return reporte;
    }
    let upem = f32::from(cara.units_per_em());
    if !upem.is_finite() || upem <= 0.0 {
        reporte.completo = false;
        return reporte;
    }
    // Toda `x` que se va a desplazar tiene que ser finita (el `as u32` del
    // raster no perdona NaN; acá se detecta antes).
    for item in &layout.items {
        let x = match *item {
            formulary::Item::Glyph { x, .. }
            | formulary::Item::Rule { x, .. }
            | formulary::Item::Background { x, .. } => x,
            _ => continue,
        };
        if !x.is_finite() {
            reporte.completo = false;
            return reporte;
        }
    }
    // Índices de glifos ordenados por x (cinta de lectura).
    let mut orden: Vec<usize> = Vec::new();
    for (i, item) in layout.items.iter().enumerate() {
        if es_glifo_finito(item) {
            orden.push(i);
        }
    }
    orden.sort_by(|a, b| {
        let xa = match layout.items[*a] {
            formulary::Item::Glyph { x, .. } => x,
            _ => f32::NAN,
        };
        let xb = match layout.items[*b] {
            formulary::Item::Glyph { x, .. } => x,
            _ => f32::NAN,
        };
        xa.total_cmp(&xb)
    });

    let [latn, dflt] = [
        ttf_parser::Tag::from_bytes(b"latn"),
        ttf_parser::Tag::from_bytes(b"DFLT"),
    ];
    let pares: Vec<ttf_parser::gpos::PairAdjustment<'_>> = match cara.tables().gpos {
        Some(gpos) => {
            let mut indices = lookups_para(&gpos, &[latn, dflt], es_feature_kern);
            if indices.is_empty() {
                indices = lookups_global(&gpos, es_feature_kern);
            }
            recolecta_pares(cara, &indices)
        }
        None => Vec::new(),
    };

    let mut pares_vistos: usize = 0;
    let mut anterior: Option<usize> = None;
    for actual in orden {
        let (id_previo, x_previo, y_previo, tam_previo, adv_previo, espejado_previo) =
            match anterior {
                Some(p) => datos_glifo(&layout.items[p]),
                None => (None, 0.0, 0.0, 0.0, 0.0, true),
            };
        let (id_actual, x_actual, y_actual, tam_actual, _, espejado_actual) =
            datos_glifo(&layout.items[actual]);
        let en_corrida = match (id_previo, id_actual) {
            (Some(_), Some(_)) => {
                y_previo.to_bits() == y_actual.to_bits()
                    && tam_previo.to_bits() == tam_actual.to_bits()
                    && !espejado_previo
                    && !espejado_actual
            }
            _ => false,
        };
        if en_corrida {
            let (idp, ida) = match (id_previo, id_actual) {
                (Some(p), Some(a)) => (p, a),
                _ => (0, 0),
            };
            // Marcas combining reales (avance 0, mismo origen): anclas GPOS.
            // Con STIX/formulary este camino no dispara (ver docs); si
            // dispara, el delta es de anclas medidas, no inventadas.
            let avance_actual = match layout.items[actual] {
                formulary::Item::Glyph { advance, .. } => advance,
                _ => f32::NAN,
            };
            if avance_actual == 0.0 && x_actual == x_previo && y_actual == y_previo {
                let delta = marca_base_delta_unidades(cara, idp, ida)
                    .or_else(|| marca_ligadura_delta_unidades(cara, idp, ida));
                if let Some((mdx, mdy)) = delta {
                    let escala = tam_actual / upem;
                    let dx = f32::from(mdx) * escala;
                    let dy = f32::from(mdy) * escala;
                    if dx.is_finite() && dy.is_finite() {
                        if let formulary::Item::Glyph { x, y, .. } = &mut layout.items[actual] {
                            *x += dx;
                            *y -= dy; // anclas en Y-up, layout en Y-down
                            reporte.marcas += 1;
                        }
                    }
                }
                anterior = Some(actual);
                continue;
            }
            // Kerning: solo si se tocan exactamente (puerta anti-invento).
            let hueco = x_actual - (x_previo + adv_previo);
            let eps = GPOS_ABUT_EPS_FRACTION * tam_actual;
            if hueco.abs() <= eps {
                pares_vistos += 1;
                if pares_vistos > GPOS_MAX_PARES {
                    reporte.completo = false;
                    return reporte;
                }
                let mut du = kern_en_pares(&pares, idp, ida);
                if let Some(legado) = legacy_kern_unidades(cara, idp, ida) {
                    du = du.saturating_add(i32::from(legado));
                }
                if du != 0 {
                    let escala = tam_actual / upem;
                    let mut dx = du as f32 * escala;
                    if !dx.is_finite() {
                        reporte.completo = false;
                        return reporte;
                    }
                    // L4: anti-fuente-loca.
                    dx = dx.clamp(-tam_actual, tam_actual);
                    if dx != 0.0 {
                        desplaza_cinta(&mut layout.items, x_actual, dx);
                        layout.width += dx;
                        reporte.pares_kern += 1;
                        reporte.dx_total += dx;
                    }
                }
            }
        }
        anterior = Some(actual);
    }
    reporte
}

/// Datos de un glifo para comparar corridas (id, x, y, size, advance,
/// mirrored). `id` es `None` si el ítem no es glifo.
fn datos_glifo(item: &formulary::Item) -> (Option<u16>, f32, f32, f32, f32, bool) {
    match *item {
        formulary::Item::Glyph {
            id,
            x,
            y,
            size,
            advance,
            mirrored,
            ..
        } => (Some(id.0), x, y, size, advance, mirrored),
        _ => (None, 0.0, 0.0, 0.0, 0.0, true),
    }
}

/// Desplaza en `dx` todo ítem con `x >= corte` (cinta). L5: las reglas que
/// cruzan el corte conservan su ancho (documentado, no re-maqueta).
fn desplaza_cinta(items: &mut [formulary::Item], corte: f32, dx: f32) {
    for item in items.iter_mut() {
        let x = match item {
            formulary::Item::Glyph { x, .. }
            | formulary::Item::Rule { x, .. }
            | formulary::Item::Background { x, .. } => x,
            _ => continue,
        };
        if *x >= corte {
            *x += dx;
        }
    }
}
