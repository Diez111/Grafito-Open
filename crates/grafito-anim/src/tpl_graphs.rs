//! Grafos y redes animados nivel 3Blue1Brown (networks).
//!
//! Cerebro puro: sin egui, sin wgpu, sin I/O, sin red, sin dependencias
//! (solo `std`). Cada plantilla es una escena didáctica con setup +
//! animación + etiqueta, en la paleta oscura nocturna (fondo azul noche,
//! nodos ámbar/verde/azul/rojo, aristas tenues).
//!
//! ## Escenas (ver [`TEMPLATE_IDS`])
//!
//! | Id | Idea |
//! |---|---|
//! | `moser-spindle-coloreo` | el Moser spindle (7v/11a) se colorea en orden BFS: 3 colores no alcanzan, el 4to cae sobre el terminal |
//! | `bfs-animado` | la frontera BFS se expande nivel por nivel y el árbol queda resaltado |
//! | `force-directed` | Fruchterman-Reingold con iteraciones fijas converge desde el círculo |
//! | `unit-distance` | los puntos caen a la red triangular y las aristas aparecen cuando `dist ≈ 1` |
//! | `camino-minimo` | Dijkstra expande visitados y el camino más barato queda en verde |
//!
//! Fuentes: Fruchterman–Reingold 1991 (`Graph Drawing by Force-Directed
//! Placement`, repulsión `k²/d` + atracción `d²/k` + temperatura que decae);
//! Eades 1984 (spring-embedder); Moser & Moser 1961 (spindle, cota `χ ≥ 4`
//! del problema Hadwiger–Nelson); de Grey 2018 (cota `χ ≥ 5`).
//! Conexión con el hunt HN del repo: aristas abstractas del Moser idénticas
//! a `grafito-lab/tests/parity_shrink.rs:49` (2 diamantes sobre el 0 +
//! arista terminal `2-5`, 11 aristas, 4-crítico); tolerancia unit-distance
//! [`GRAPH_UNIT_TOL`] en paridad con `SHRINK_UNIT_TOL` (`grafito-lab`);
//! construcción geométrica (rombo 60° + rotación `2·r·sin(φ/2) = 1`) en
//! paridad con `grafito-lab/src/gadget.rs:moser_spindle`.
//!
//! ## Formato de frame
//!
//! [`RgbaFrame`] es RGBA 8 bits por canal, fila-mayor, origen arriba-izq
//! (igual que el `buf` que `anim_native` mete en
//! `egui::ColorImage::from_rgba_unmultiplied`). La app cablea copiando
//! `pixels` tal cual; acá no se depende de egui a propósito.
//!
//! ## Presupuestos (paridad con `protocol.rs`, sin importarlo: este módulo
//! es autocontenido y compila con `rustc --test` standalone)
//!
//! - Frames por set: 1..=64 en preview ([`GRAPH_MAX_FRAMES`], paridad
//!   `PREVIEW_SHORT_MAX_FRAMES`), hasta 1500 en largo
//!   ([`GRAPH_LONGFORM_MAX_FRAMES`], 50 s a 30 fps ≤ 60 s del timeline).
//! - Set en RAM ≤ 64 MiB ([`GRAPH_CHUNK_MAX_BYTES`]): si excede es `Err`
//!   ([`GraphError::SetDemasiadoGrande`]) y el frente compone por rangos
//!   con [`render_graphs_rango`] drenando a disco.
//! - Lienzo 64..=4096 por lado (paridad `Resolution`).
//! - Nodos por layout ≤ 256 ([`GRAPH_MAX_NODOS`], pineado; de más es `Err`
//!   honesto en [`force_layout_hasta`] y dato ignorado en los helpers).
//! - Iteraciones force-directed fijas: [`FORCE_ITERS_TOTAL`] por animación
//!   (1 por frame en el default de 48); el frame `g` aplica
//!   `round(t·48)` pasos desde la semilla determinista (sin estado entre
//!   frames: el corte por rangos es empalme exacto).
//! - Samplers sin allocs por frame: la matemática por frame devuelve
//!   `Copy` o escribe en buffers del llamador; [`Scratch`] se prepara una
//!   vez en el setup y se muestrea por índice. La única alloc por frame es
//!   el propio píxel-buffer de salida (inherente: cada frame es dueño).

// ── Ids estables ──────────────────────────────────────────────────────────

/// Ids estables (kebab-case) de las 5 plantillas de grafos y redes.
/// El frente los cablea al dispatcher nativo; acá ya los atiende
/// [`render_graphs_frames`].
pub const TEMPLATE_IDS: &[&str] = &[
    "moser-spindle-coloreo",
    "bfs-animado",
    "force-directed",
    "unit-distance",
    "camino-minimo",
];

// ── Presupuestos ──────────────────────────────────────────────────────────

/// Ancho default del set (paridad con el canónico del chat 480×360).
pub const GRAPH_DEFAULT_W: u32 = 480;
/// Alto default del set.
pub const GRAPH_DEFAULT_H: u32 = 360;
/// Frames default por escena (paridad `NATIVE_ANIM_FRAME_COUNT`).
pub const GRAPH_DEFAULT_FRAMES: usize = 48;
/// Frames mínimos por pedido.
pub const GRAPH_MIN_FRAMES: usize = 1;
/// Tope de frames en preview (paridad `PREVIEW_SHORT_MAX_FRAMES`).
pub const GRAPH_MAX_FRAMES: usize = 64;
/// Tope de frames en largo (paridad `VIDEO_LONGFORM_MAX_FRAMES`: 50 s a 30 fps).
pub const GRAPH_LONGFORM_MAX_FRAMES: usize = 1500;
/// Tope de bytes RGBA del set en RAM (paridad `LONGFORM_CHUNK_MAX_BYTES`).
pub const GRAPH_CHUNK_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Lado mínimo del lienzo (paridad `Resolution`).
pub const GRAPH_CANVAS_MIN: u32 = 64;
/// Lado máximo del lienzo (paridad `Resolution`).
pub const GRAPH_CANVAS_MAX: u32 = 4096;
/// Bytes por píxel RGBA.
pub const GRAPH_BYTES_POR_PIXEL: usize = 4;
/// Tope de nodos por layout (las fuerzas son O(n²) por iteración).
pub const GRAPH_MAX_NODOS: usize = 256;
/// Iteraciones totales del force-directed en una animación completa.
pub const FORCE_ITERS_TOTAL: usize = 48;
/// Semilado de la caja del force-directed (mundo `±3.2`, vista `±4.5`).
pub const FORCE_CAJA: f64 = 3.2;
/// Colores máximos del coloreo greedy (máscara de bits en `u16`).
pub const GRAPH_MAX_COLORES: usize = 8;
/// Nodos máximos de Dijkstra (O(n²) sin heap, de sobra para escenas).
pub const DIJKSTRA_MAX_NODOS: usize = 64;
/// Tolerancia unit-distance (paridad `SHRINK_UNIT_TOL` de `grafito-lab`).
pub const GRAPH_UNIT_TOL: f64 = 1e-9;

// ── Error ─────────────────────────────────────────────────────────────────

/// Error honesto de las escenas (todo en español, sin pánicos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    /// Lienzo fuera de 64..=4096 por lado (o cero).
    LienzoInvalido { w: u32, h: u32 },
    /// Conteo de frames fuera de 1..=1500.
    FramesFueraDeRango { got: usize },
    /// Plantilla que no es de este módulo.
    PlantillaDesconocida { got: String },
    /// El set excede 64 MiB: bajar frames/lienzo o componer por rangos.
    SetDemasiadoGrande { bytes: usize },
    /// Más de 256 nodos en el force-directed (O(n²) acotado a propósito).
    DemasiadosNodos { got: usize },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LienzoInvalido { w, h } => write!(
                f,
                "lienzo {w}x{h} inválido: usá {GRAPH_CANVAS_MIN}..={GRAPH_CANVAS_MAX} por lado"
            ),
            Self::FramesFueraDeRango { got } => write!(
                f,
                "{got} frames fuera de {GRAPH_MIN_FRAMES}..={GRAPH_LONGFORM_MAX_FRAMES}: \
                 preview hasta {GRAPH_MAX_FRAMES}, largo por rangos"
            ),
            Self::PlantillaDesconocida { got } => write!(
                f,
                "plantilla {got:?} desconocida: elegí una de {TEMPLATE_IDS:?}"
            ),
            Self::SetDemasiadoGrande { bytes } => write!(
                f,
                "set de {bytes} bytes excede {GRAPH_CHUNK_MAX_BYTES}: \
                 bajá frames/lienzo o componé por rangos con render_graphs_rango"
            ),
            Self::DemasiadosNodos { got } => write!(
                f,
                "{got} nodos exceden {GRAPH_MAX_NODOS}: achicá el grafo o partilo"
            ),
        }
    }
}

impl std::error::Error for GraphError {}

// ── Frame RGBA ────────────────────────────────────────────────────────────

/// Un frame RGBA dueño de sus píxeles (fila-mayor, origen arriba-izq).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaFrame {
    /// Ancho en px.
    pub width: u32,
    /// Alto en px.
    pub height: u32,
    /// `width*height*4` bytes RGBA.
    pub pixels: Vec<u8>,
}

impl RgbaFrame {
    /// Constructor validado (lienzo 64..=4096; `Err` honesto si no).
    pub fn nuevo(width: u32, height: u32) -> Result<Self, GraphError> {
        valida_lienzo(width, height)?;
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(GRAPH_BYTES_POR_PIXEL));
        match len {
            Some(n) => Ok(Self {
                width,
                height,
                pixels: vec![0u8; n],
            }),
            None => Err(GraphError::SetDemasiadoGrande { bytes: usize::MAX }),
        }
    }

    /// Píxel en `(x, y)` (`None` honesto si está fuera).
    pub fn pixel_en(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?
            .checked_mul(GRAPH_BYTES_POR_PIXEL)?;
        let p = self.pixels.get(i..i + GRAPH_BYTES_POR_PIXEL)?;
        Some([p[0], p[1], p[2], p[3]])
    }
}

/// Lienzo válido 64..=4096 por lado. Puro.
fn valida_lienzo(w: u32, h: u32) -> Result<(), GraphError> {
    if !(GRAPH_CANVAS_MIN..=GRAPH_CANVAS_MAX).contains(&w)
        || !(GRAPH_CANVAS_MIN..=GRAPH_CANVAS_MAX).contains(&h)
    {
        return Err(GraphError::LienzoInvalido { w, h });
    }
    Ok(())
}

// ── Matemática pura (samplers: Copy o buffers del llamador, sin allocs) ───

/// Finito o cero (guarda de los samplers). Puro.
fn finito_o_cero(v: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// Smoothstep `3t²−2t³`. Clamp 0..1 + guardia finita. Puro.
pub fn suave(t: f64) -> f64 {
    let u = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    u * u * (3.0 - 2.0 * u)
}

/// Tramo de timeline: 0 antes de `a`, 1 después de `b`, rampa lineal
/// entremedio (`a == b` → escalón honesto, sin división por cero). Puro.
pub fn tramo(t: f64, a: f64, b: f64) -> f64 {
    if !t.is_finite() {
        return 0.0;
    }
    if b <= a {
        return if t < a { 0.0 } else { 1.0 };
    }
    ((t - a) / (b - a)).clamp(0.0, 1.0)
}

/// Progreso global 0..1 del frame `indice` en un set de `total`
/// (`total <= 1` → 0; índice pasado → clamp, sin pánicos). Puro.
pub fn progreso_en(indice: usize, total: usize) -> f64 {
    if total <= 1 {
        return 0.0;
    }
    let i = indice.min(total - 1) as f64;
    let n = (total - 1) as f64;
    if n > 0.0 {
        (i / n).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Distancia euclídea (no finita → +inf honesto, sin pánico). Pura.
pub fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let d = (dx * dx + dy * dy).sqrt();
    if d.is_finite() {
        d
    } else {
        f64::INFINITY
    }
}

/// ¿La distancia `d` cuenta como arista unidad con tolerancia `tol`
/// (`|d − 1| ≤ tol`; `tol` no finita/negativa → `false`)? Pura.
/// Espejo de `grafito-geometry::search::unit_graph_edges` y
/// `SHRINK_UNIT_TOL` (`grafito-lab`).
pub fn arista_unitaria(d: f64, tol: f64) -> bool {
    if !d.is_finite() || !tol.is_finite() || tol < 0.0 {
        return false;
    }
    (d - 1.0).abs() <= tol
}

/// Cuenta las aristas unidad entre los `n` primeros puntos
/// (`n` mayor que el buffer → 0 honesto). Pura, sin allocs.
pub fn cuenta_aristas_unidad(pos: &[[f64; 2]], n: usize, tol: f64) -> usize {
    if pos.len() < n || !tol.is_finite() || tol < 0.0 {
        return 0;
    }
    let mut cuenta = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            if arista_unitaria(dist(pos[i], pos[j]), tol) {
                cuenta += 1;
            }
        }
    }
    cuenta
}

/// Hash determinista `i → [0, 1)` (splitmix64, sin estado ni RNG). Puro.
/// Da el jitter reproducible de las semillas.
fn hash01(i: usize, sal: u64) -> f64 {
    let mut z = (i as u64)
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(sal);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // 53 bits de mantisa → [0, 1).
    ((z >> 11) as f64) / 9007199254740992.0
}

/// Layout circular de radio `r` (`n` puntos, ángulo desde arriba).
/// `out` corto o `n > 256` → no escribe nada (sin pánicos). Puro.
pub fn circular_layout(n: usize, r: f64, out: &mut [[f64; 2]]) {
    if n > GRAPH_MAX_NODOS || out.len() < n || n == 0 {
        return;
    }
    let r = if r.is_finite() { r } else { 1.0 };
    for (i, celda) in out.iter_mut().take(n).enumerate() {
        let ang =
            2.0 * std::f64::consts::PI * (i as f64) / (n as f64) - std::f64::consts::FRAC_PI_2;
        *celda = [r * ang.cos(), r * ang.sin()];
    }
}

/// Layout en grilla (`cols` columnas, espaciado `paso`, centrado).
/// Guarda como [`circular_layout`]. Puro.
pub fn grilla_layout(n: usize, cols: usize, paso: f64, out: &mut [[f64; 2]]) {
    if n > GRAPH_MAX_NODOS || out.len() < n || n == 0 {
        return;
    }
    let cols = cols.clamp(1, 256).max(1);
    let paso = if paso.is_finite() && paso > 0.0 {
        paso
    } else {
        1.0
    };
    let filas = (n + cols - 1) / cols.max(1);
    for (i, celda) in out.iter_mut().take(n).enumerate() {
        let c = (i % cols) as f64;
        let f = (i / cols) as f64;
        *celda = [
            (c - (cols as f64 - 1.0) / 2.0) * paso,
            ((filas as f64 - 1.0) / 2.0 - f) * paso,
        ];
    }
}

/// Layout por capas: `nivel[v]` es la fila (`0` arriba); dentro de cada
/// capa los nodos se centran con espaciado `paso_x` y las capas se
/// separan `paso_y`. Guarda como [`circular_layout`]. Puro, sin allocs
/// (dos pasadas sobre buffers del llamador).
pub fn capas_layout(n: usize, nivel: &[u8], paso_x: f64, paso_y: f64, out: &mut [[f64; 2]]) {
    if n > GRAPH_MAX_NODOS || out.len() < n || nivel.len() < n || n == 0 {
        return;
    }
    let paso_x = if paso_x.is_finite() && paso_x > 0.0 {
        paso_x
    } else {
        1.0
    };
    let paso_y = if paso_y.is_finite() && paso_y > 0.0 {
        paso_y
    } else {
        1.0
    };
    let capa_max = nivel.iter().take(n).copied().max().unwrap_or(0);
    // Cuenta por capa (tope 256 capas, índice por valor u8).
    let mut cuentas = [0usize; 256];
    for v in nivel.iter().take(n) {
        cuentas[*v as usize] += 1;
    }
    let mut vistos = [0usize; 256];
    for (i, celda) in out.iter_mut().take(n).enumerate() {
        let c = nivel[i] as usize;
        let total = cuentas[c].max(1);
        let k = vistos[c];
        vistos[c] += 1;
        *celda = [
            (k as f64 - (total as f64 - 1.0) / 2.0) * paso_x,
            ((capa_max as f64) / 2.0 - nivel[i] as f64) * paso_y,
        ];
    }
}

/// Morph entre layouts: `out = a + (b − a)·suave(t)`.
/// Longitudes cortas → no escribe nada. Puro, sin allocs.
pub fn morph_layout(a: &[[f64; 2]], b: &[[f64; 2]], t: f64, out: &mut [[f64; 2]]) {
    let n = a.len().min(b.len()).min(out.len());
    if n == 0 || n > GRAPH_MAX_NODOS {
        return;
    }
    let u = suave(t);
    for i in 0..n {
        out[i] = [
            a[i][0] + (b[i][0] - a[i][0]) * u,
            a[i][1] + (b[i][1] - a[i][1]) * u,
        ];
        out[i] = [finito_o_cero(out[i][0]), finito_o_cero(out[i][1])];
    }
}

/// Encuadra los `n` puntos a la caja `±3` (centra por bbox y escala el
/// extent máximo a 6; degenerado → todo al origen). Puro, sin allocs.
pub fn encuadra(pos: &mut [[f64; 2]], n: usize) {
    if pos.len() < n || n == 0 || n > GRAPH_MAX_NODOS {
        return;
    }
    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y0 = f64::INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for p in pos.iter().take(n) {
        if p[0].is_finite() && p[1].is_finite() {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            y0 = y0.min(p[1]);
            y1 = y1.max(p[1]);
        }
    }
    if !x0.is_finite() {
        for p in pos.iter_mut().take(n) {
            *p = [0.0, 0.0];
        }
        return;
    }
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let ext = (x1 - x0).max(y1 - y0);
    if !ext.is_finite() || ext < 1e-9 {
        for p in pos.iter_mut().take(n) {
            *p = [0.0, 0.0];
        }
        return;
    }
    let s = 6.0 / ext;
    for p in pos.iter_mut().take(n) {
        *p = [(p[0] - cx) * s, (p[1] - cy) * s];
    }
}

/// BFS desde `origen`: llena `orden` (visita) y `nivel` (distancia en
/// aristas, `255` = inalanzable). Vecinos en índice creciente (padre
/// determinista = el de menor índice). Devuelve visitados; `0` honesto si
/// la entrada es inválida (`n` fuera de `1..=256`, buffers cortos u origen
/// fuera). Puro, sin allocs (cola en `orden` mismo).
pub fn bfs_orden(
    n: usize,
    aristas: &[(u8, u8)],
    origen: usize,
    orden: &mut [u8],
    nivel: &mut [u8],
) -> usize {
    if n == 0 || n > GRAPH_MAX_NODOS || orden.len() < n || nivel.len() < n || origen >= n {
        return 0;
    }
    for v in nivel.iter_mut().take(n) {
        *v = 255;
    }
    nivel[origen] = 0;
    orden[0] = origen as u8;
    let mut cabeza = 0usize;
    let mut cola = 1usize;
    while cabeza < cola {
        let v = orden[cabeza] as usize;
        cabeza += 1;
        let nv = nivel[v].saturating_add(1);
        for (a, b) in aristas {
            let (x, y) = (*a as usize, *b as usize);
            if x >= n || y >= n {
                continue;
            }
            let w = if x == v {
                y
            } else if y == v {
                x
            } else {
                continue;
            };
            if nivel[w] == 255 {
                nivel[w] = nv;
                if cola < n {
                    orden[cola] = w as u8;
                    cola += 1;
                }
            }
        }
    }
    cola
}

/// Coloreo greedy en el orden dado con `k ≤ 8` colores: el menor color
/// libre por vértice; sin color libre queda en `255` y se sigue (honesto:
/// Moser con `k = 3` deja el terminal sin color). Devuelve coloreados;
/// `0` honesto si la entrada es inválida. Puro, sin allocs.
pub fn greedy_colorea(
    n: usize,
    aristas: &[(u8, u8)],
    orden: &[u8],
    k: u8,
    color: &mut [u8],
) -> usize {
    if n == 0
        || n > GRAPH_MAX_NODOS
        || k == 0
        || (k as usize) > GRAPH_MAX_COLORES
        || orden.len() < n
        || color.len() < n
    {
        return 0;
    }
    for c in color.iter_mut().take(n) {
        *c = 255;
    }
    let mut hechos = 0usize;
    for v in orden.iter().take(n).map(|v| *v as usize) {
        if v >= n {
            continue;
        }
        let mut usados: u16 = 0;
        for (a, b) in aristas {
            let (x, y) = (*a as usize, *b as usize);
            if x >= n || y >= n {
                continue;
            }
            let w = if x == v {
                y
            } else if y == v {
                x
            } else {
                continue;
            };
            let cw = color[w];
            if cw != 255 && (cw as usize) < GRAPH_MAX_COLORES {
                usados |= 1 << cw;
            }
        }
        let mut elegido = 255u8;
        for c in 0..k {
            if usados & (1 << c) == 0 {
                elegido = c;
                break;
            }
        }
        color[v] = elegido;
        if elegido != 255 {
            hechos += 1;
        }
    }
    hechos
}

/// ¿El coloreo es propio (vecinos con distinto color, nadie en 255)?
/// Pura, sin allocs.
pub fn coloreo_propio(n: usize, aristas: &[(u8, u8)], color: &[u8]) -> bool {
    if color.len() < n {
        return false;
    }
    for v in color.iter().take(n) {
        if *v == 255 {
            return false;
        }
    }
    for (a, b) in aristas {
        let (x, y) = (*a as usize, *b as usize);
        if x >= n || y >= n {
            continue;
        }
        if color[x] == color[y] {
            return false;
        }
    }
    true
}

/// Dijkstra O(n²) sin heap: `peso[k]` es el peso de `aristas[k]`
/// (negativo/no-finito → arista ignorada). Llena `dist` (origen 0,
/// inalanzable +inf), `prev` (−1 = ninguno) y `visit` (orden de
/// extracción). Devuelve extraídos; `0` honesto si la entrada es inválida
/// (`n` fuera de `1..=64`, buffers cortos, largos de pesos distintos).
/// Puro, sin allocs.
#[allow(clippy::too_many_arguments)]
pub fn dijkstra(
    n: usize,
    aristas: &[(u8, u8)],
    peso: &[f64],
    origen: usize,
    dist: &mut [f64],
    prev: &mut [i8],
    visit: &mut [u8],
) -> usize {
    if n == 0
        || n > DIJKSTRA_MAX_NODOS
        || origen >= n
        || aristas.len() != peso.len()
        || dist.len() < n
        || prev.len() < n
        || visit.len() < n
    {
        return 0;
    }
    for (i, d) in dist.iter_mut().take(n).enumerate() {
        *d = if i == origen { 0.0 } else { f64::INFINITY };
    }
    for p in prev.iter_mut().take(n) {
        *p = -1;
    }
    let mut hecho = [false; DIJKSTRA_MAX_NODOS];
    let mut extraidos = 0usize;
    loop {
        let mut u = n;
        let mut mejor = f64::INFINITY;
        for v in 0..n {
            if !hecho[v] && dist[v] < mejor {
                mejor = dist[v];
                u = v;
            }
        }
        if u >= n || !mejor.is_finite() {
            break;
        }
        hecho[u] = true;
        if extraidos < n {
            visit[extraidos] = u as u8;
            extraidos += 1;
        }
        for (k, (a, b)) in aristas.iter().enumerate() {
            let w = peso[k];
            if !w.is_finite() || w < 0.0 {
                continue;
            }
            let (x, y) = (*a as usize, *b as usize);
            if x >= n || y >= n {
                continue;
            }
            let v = if x == u {
                y
            } else if y == u {
                x
            } else {
                continue;
            };
            if hecho[v] {
                continue;
            }
            let cand = dist[u] + w;
            if cand.is_finite() && cand < dist[v] {
                dist[v] = cand;
                prev[v] = u as i8;
            }
        }
    }
    extraidos
}

/// Reconstruye el camino `origen → destino` por `prev` en `salida`
/// (orden origen-primero). Devuelve el largo; `0` honesto si no hay camino
/// o la entrada es inválida. Puro, sin allocs.
pub fn reconstruye_camino(
    n: usize,
    prev: &[i8],
    origen: usize,
    destino: usize,
    salida: &mut [u8],
) -> usize {
    if n == 0 || n > DIJKSTRA_MAX_NODOS || prev.len() < n || origen >= n || destino >= n {
        return 0;
    }
    // Camina hacia atrás con tope `n` (ciclos en `prev` no cuelgan).
    let mut rev = [0u8; DIJKSTRA_MAX_NODOS];
    let mut largo = 0usize;
    let mut v = destino;
    for _ in 0..n {
        if largo >= rev.len() {
            return 0;
        }
        rev[largo] = v as u8;
        largo += 1;
        if v == origen {
            break;
        }
        let p = prev[v];
        if p < 0 || (p as usize) >= n {
            return 0;
        }
        v = p as usize;
    }
    if largo == 0 || rev[largo - 1] as usize != origen {
        return 0;
    }
    if salida.len() < largo {
        return 0;
    }
    for (i, celda) in salida.iter_mut().take(largo).enumerate() {
        *celda = rev[largo - 1 - i];
    }
    largo
}

/// Un paso Fruchterman–Reingold sobre los `n` puntos (`k` = longitud
/// ideal, `temp` = desplazamiento máximo). Devuelve el desplazamiento
/// máximo aplicado (medida de convergencia). Guarda de entrada inválida:
/// `0.0` sin tocar nada. Puro salvo el buffer del llamador, sin allocs
/// (desplazamientos en stack fijo de 256).
pub fn force_paso(pos: &mut [[f64; 2]], n: usize, aristas: &[(u8, u8)], k: f64, temp: f64) -> f64 {
    if pos.len() < n || n == 0 || n > GRAPH_MAX_NODOS {
        return 0.0;
    }
    let k = if k.is_finite() && k > 1e-6 { k } else { 1.0 };
    let temp = if temp.is_finite() {
        temp.clamp(0.0, FORCE_CAJA)
    } else {
        0.0
    };
    let mut desp = [[0.0f64; 2]; GRAPH_MAX_NODOS];
    // Repulsión entre todos los pares: `k²/d`.
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = pos[i][0] - pos[j][0];
            let dy = pos[i][1] - pos[j][1];
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            if !d.is_finite() {
                continue;
            }
            let f = k * k / d / d;
            let fx = dx * f;
            let fy = dy * f;
            desp[i][0] += fx;
            desp[i][1] += fy;
            desp[j][0] -= fx;
            desp[j][1] -= fy;
        }
    }
    // Atracción en aristas: `d²/k`.
    for (a, b) in aristas {
        let (x, y) = (*a as usize, *b as usize);
        if x >= n || y >= n || x == y {
            continue;
        }
        let dx = pos[x][0] - pos[y][0];
        let dy = pos[x][1] - pos[y][1];
        let d = (dx * dx + dy * dy).sqrt().max(0.01);
        if !d.is_finite() {
            continue;
        }
        let f = d * d / k / d;
        let fx = dx * f;
        let fy = dy * f;
        desp[x][0] -= fx;
        desp[x][1] -= fy;
        desp[y][0] += fx;
        desp[y][1] += fy;
    }
    // Aplica con tope `temp` + gravedad leve al centro + caja.
    let mut maximo: f64 = 0.0;
    for (i, p) in pos.iter_mut().take(n).enumerate() {
        let mx = desp[i][0];
        let my = desp[i][1];
        let m = (mx * mx + my * my).sqrt();
        if !m.is_finite() || m <= 0.0 {
            continue;
        }
        let paso = m.min(temp);
        let nx = (p[0] + mx / m * paso) * 0.999;
        let ny = (p[1] + my / m * paso) * 0.999;
        *p = [
            finito_o_cero(nx).clamp(-FORCE_CAJA, FORCE_CAJA),
            finito_o_cero(ny).clamp(-FORCE_CAJA, FORCE_CAJA),
        ];
        if paso.is_finite() {
            maximo = maximo.max(paso);
        }
    }
    maximo
}

/// Longitud ideal FR para `n` nodos en la caja (`sqrt(A/n)`).
/// `n = 0` o gigante → `1.0` honesto. Pura.
pub fn force_k_ideal(n: usize) -> f64 {
    if n == 0 || n > GRAPH_MAX_NODOS {
        return 1.0;
    }
    let a = (2.0 * FORCE_CAJA) * (2.0 * FORCE_CAJA);
    (a / (n as f64)).sqrt()
}

/// Semilla determinista del force: círculo `r = 2.2` + jitter por hash
/// (misma entrada → misma salida, sin RNG). Guarda como los layouts. Pura.
pub fn force_semilla(n: usize, out: &mut [[f64; 2]]) {
    if n > GRAPH_MAX_NODOS || out.len() < n {
        return;
    }
    circular_layout(n, 2.2, out);
    for (i, p) in out.iter_mut().take(n).enumerate() {
        p[0] += (hash01(i, 11) - 0.5) * 0.9;
        p[1] += (hash01(i, 77) - 0.5) * 0.9;
    }
}

/// Corre `iters` pasos FR desde la semilla (temperatura lineal
/// `0.35 → 0.02`). `n` fuera de `1..=256` o buffer corto → `Err`
/// honesto. Determinista: el frame `g` de la escena llama con
/// `round(t·48)` y siempre obtiene lo mismo. Sin allocs.
pub fn force_layout_hasta(
    n: usize,
    aristas: &[(u8, u8)],
    iters: usize,
    out: &mut [[f64; 2]],
) -> Result<(), GraphError> {
    if n == 0 || n > GRAPH_MAX_NODOS {
        return Err(GraphError::DemasiadosNodos { got: n });
    }
    if out.len() < n {
        return Err(GraphError::DemasiadosNodos { got: out.len() });
    }
    force_semilla(n, out);
    let k = force_k_ideal(n);
    let total = iters.min(FORCE_ITERS_TOTAL);
    for i in 0..total {
        let f = if total <= 1 {
            1.0
        } else {
            (i as f64) / ((total - 1) as f64)
        };
        let temp = 0.35 * (1.0 - f) + 0.02;
        force_paso(out, n, aristas, k, temp);
    }
    Ok(())
}

/// Energía media de aristas `Σ(d − k)²/m` (`m = 0` → +inf honesto).
/// Mide convergencia: el layout relajado la baja. Pura, sin allocs.
pub fn energia_aristas(pos: &[[f64; 2]], n: usize, aristas: &[(u8, u8)], k: f64) -> f64 {
    if pos.len() < n {
        return f64::INFINITY;
    }
    let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
    let mut suma = 0.0;
    let mut m = 0usize;
    for (a, b) in aristas {
        let (x, y) = (*a as usize, *b as usize);
        if x >= n || y >= n {
            continue;
        }
        let d = dist(pos[x], pos[y]);
        if d.is_finite() {
            suma += (d - k) * (d - k);
            m += 1;
        }
    }
    if m == 0 {
        return f64::INFINITY;
    }
    suma / (m as f64)
}

// ── Presupuestos puros ────────────────────────────────────────────────────

/// Bytes RGBA del set (`w·h·4·frames`); `None` si desborda. Puro, sin allocs.
pub fn estima_bytes(w: u32, h: u32, frames: usize) -> Option<usize> {
    (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(GRAPH_BYTES_POR_PIXEL))
        .and_then(|v| v.checked_mul(frames))
}

/// ¿Cuántos frames de `w`×`h` entran en 64 MiB? Lados 0 o desborde → 0
/// honesto. Puro. (Pineado: 1280×720→18, 640×480→54, como en el protocolo.)
pub fn frames_por_chunk(w: u32, h: u32) -> usize {
    let por_frame = (w as usize)
        .checked_mul(h as usize)
        .and_then(|v| v.checked_mul(GRAPH_BYTES_POR_PIXEL))
        .unwrap_or(0);
    if por_frame == 0 {
        return 0;
    }
    GRAPH_CHUNK_MAX_BYTES / por_frame
}

/// Frames para una duración a `fps` (`duracion_ms·fps/1000`, saturado;
/// `fps` 0 → 0 honesto). Puro. (Pineado: 50 s a 30 fps = 1500.)
pub fn frames_para_duracion(duracion_ms: u64, fps: u32) -> u64 {
    if fps == 0 {
        return 0;
    }
    duracion_ms.saturating_mul(u64::from(fps)) / 1000
}

/// Valida un pedido completo (lienzo + 1..=1500 frames + set ≤ 64 MiB).
pub fn valida_pedido(w: u32, h: u32, frames: usize) -> Result<(), GraphError> {
    valida_lienzo(w, h)?;
    if !(GRAPH_MIN_FRAMES..=GRAPH_LONGFORM_MAX_FRAMES).contains(&frames) {
        return Err(GraphError::FramesFueraDeRango { got: frames });
    }
    match estima_bytes(w, h, frames) {
        Some(n) if n <= GRAPH_CHUNK_MAX_BYTES => Ok(()),
        Some(n) => Err(GraphError::SetDemasiadoGrande { bytes: n }),
        None => Err(GraphError::SetDemasiadoGrande { bytes: usize::MAX }),
    }
}

// ── Datos de las escenas ──────────────────────────────────────────────────

/// Moser spindle abstracto: 2 diamantes sobre el 0 + arista terminal
/// `2-5`. Idéntico a `grafito-lab/tests/parity_shrink.rs:49` (7v/11a,
/// 4-crítico: ningún subgrafo propio inducido necesita el 4to color).
pub const MOSER_N: usize = 7;
/// Aristas del Moser (orden del hunt HN).
pub const MOSER_ARISTAS: [(u8, u8); 11] = [
    (0, 1),
    (1, 3),
    (0, 3),
    (1, 2),
    (2, 3),
    (0, 4),
    (4, 6),
    (0, 6),
    (4, 5),
    (5, 6),
    (2, 5),
];
/// Terminales del spindle (las que fuerzan el 4to color).
pub const MOSER_TERMINALES: (u8, u8) = (2, 5);
/// Orden BFS desde el 0 (revelado de la animación).
pub const MOSER_BFS: [u8; 7] = [0, 1, 3, 4, 6, 2, 5];
/// Coloreo greedy en [`MOSER_BFS`] con `k = 4` (el terminal 5 cae en 3).
pub const MOSER_COLORES: [u8; 7] = [0, 1, 0, 2, 1, 3, 2];

/// Grafo del BFS animado (10 nodos, 13 aristas, diámetro 3).
pub const BFS_N: usize = 10;
/// Aristas del BFS (índices crecientes = padre determinista menor).
pub const BFS_ARISTAS: [(u8, u8); 13] = [
    (0, 1),
    (0, 2),
    (0, 3),
    (1, 4),
    (1, 5),
    (2, 5),
    (2, 6),
    (3, 6),
    (3, 7),
    (4, 8),
    (5, 8),
    (6, 9),
    (7, 9),
];
/// Orden BFS esperado desde el 0.
pub const BFS_ORDEN: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
/// Niveles BFS esperados desde el 0.
pub const BFS_NIVELES: [u8; 10] = [0, 1, 1, 1, 2, 2, 2, 2, 3, 3];

/// Grafo del force-directed (12 nodos: 2 triángulos + puente + ramas).
pub const FORCE_N: usize = 12;
/// Aristas del force-directed (13).
pub const FORCE_ARISTAS: [(u8, u8); 13] = [
    (0, 1),
    (1, 2),
    (2, 0),
    (3, 4),
    (4, 5),
    (5, 3),
    (2, 3),
    (0, 6),
    (1, 7),
    (4, 8),
    (5, 9),
    (6, 10),
    (9, 11),
];

/// Red unit-distance de la escena (centro + hexágono: 12 aristas unidad).
pub const UNIT_N: usize = 7;
/// Aristas unidad esperadas con [`GRAPH_UNIT_TOL`] en la red exacta.
pub const UNIT_ARISTAS_ESPERADAS: usize = 12;

/// Grafo ponderado del camino mínimo (8 nodos, 13 aristas).
pub const CAMINO_N: usize = 8;
/// Aristas del camino mínimo.
pub const CAMINO_ARISTAS: [(u8, u8); 13] = [
    (0, 1),
    (0, 2),
    (1, 2),
    (1, 3),
    (2, 3),
    (2, 4),
    (3, 4),
    (3, 5),
    (4, 5),
    (4, 6),
    (5, 6),
    (5, 7),
    (6, 7),
];
/// Pesos paralelos a [`CAMINO_ARISTAS`].
pub const CAMINO_PESOS: [f64; 13] = [
    4.0, 2.0, 1.0, 5.0, 8.0, 10.0, 2.0, 6.0, 3.0, 7.0, 1.0, 4.0, 2.0,
];
/// Origen de Dijkstra.
pub const CAMINO_ORIGEN: usize = 0;
/// Destino del camino mínimo.
pub const CAMINO_DESTINO: usize = 7;
/// Distancia mínima `0 → 7` (= 2+1+5+2+3+1+2).
pub const CAMINO_DISTANCIA: f64 = 16.0;
/// Camino mínimo `0 → 7` por `prev`.
pub const CAMINO_ESPERADO: [u8; 8] = [0, 2, 1, 3, 4, 5, 6, 7];
/// Orden de extracción Dijkstra desde el 0.
pub const CAMINO_VISITA: [u8; 8] = [0, 2, 1, 3, 4, 5, 6, 7];

/// Posición Moser del vértice `v` (rombo 60° + copia rotada por φ con
/// `2·r·sin(φ/2) = 1`, paridad `gadget.rs:moser_spindle`; `cos φ = 5/6`,
/// `sin φ = √11/6`). Índices del hunt: diamante 1 `{0,1,3}→W=2`,
/// diamante 2 `{0,4,6}→W=5`. Fuera de rango → origen. Pura.
pub fn moser_pos(v: usize) -> [f64; 2] {
    let s3 = 0.75f64.sqrt() * 2.0 / 2.0; // √3/2 sin literal mágico suelto
    let s3 = if s3.is_finite() { s3 } else { 0.866 };
    let s11 = 11.0f64.sqrt();
    let s11 = if s11.is_finite() { s11 } else { 3.317 };
    let coseno = 5.0 / 6.0;
    let seno = s11 / 6.0;
    // Rota `[x, y]` por φ.
    let rota = |x: f64, y: f64| [coseno * x - seno * y, seno * x + coseno * y];
    match v {
        0 => [0.0, 0.0],
        1 => [1.0, 0.0],
        3 => [0.5, s3],
        2 => [1.5, s3],
        4 => rota(1.0, 0.0),
        6 => rota(0.5, s3),
        5 => rota(1.5, s3),
        _ => [0.0, 0.0],
    }
}

/// Red triangular exacta del `unit-distance` (centro + hexágono radio 1).
/// `i = 0` centro, `1..=6` vértices. Fuera de rango → origen. Pura.
pub fn unit_red(i: usize) -> [f64; 2] {
    if i == 0 {
        return [0.0, 0.0];
    }
    if i > 6 {
        return [0.0, 0.0];
    }
    let ang = 2.0 * std::f64::consts::PI * ((i - 1) as f64) / 6.0;
    [ang.cos(), ang.sin()]
}

/// Jitter inicial del `unit-distance` (red + desplazamiento por hash
/// `±0.45`, determinista). Pura.
pub fn unit_jitter(i: usize) -> [f64; 2] {
    let base = unit_red(i);
    [
        base[0] + (hash01(i, 5) - 0.5) * 0.9,
        base[1] + (hash01(i, 9) - 0.5) * 0.9,
    ]
}

// ── Scratch reutilizable (setup una vez, muestreo sin allocs) ─────────────

/// Buffer reutilizable del sampler: layouts A/B, orden de revelado,
/// colores y niveles. Se prepara una vez en el setup; cada frame se
/// muestrea por índice. Cero allocs por frame una vez crecido.
#[derive(Debug, Clone, Default)]
pub struct Scratch {
    /// Layout inicial (semilla / jitter / circular).
    pub pa: Vec<[f64; 2]>,
    /// Layout final (relajado / red / capas).
    pub pb: Vec<[f64; 2]>,
    /// Orden de revelado (BFS / Dijkstra / coloreo).
    pub orden: Vec<u8>,
    /// Color por vértice (`255` = sin color).
    pub color: Vec<u8>,
    /// Nivel/capa por vértice.
    pub nivel: Vec<u8>,
    /// Nodos del grafo activo.
    pub n: usize,
}

impl Scratch {
    /// Nuevo (reserva 256: el caso común sin regrow).
    pub fn nuevo() -> Self {
        Self {
            pa: Vec::with_capacity(GRAPH_MAX_NODOS),
            pb: Vec::with_capacity(GRAPH_MAX_NODOS),
            orden: Vec::with_capacity(GRAPH_MAX_NODOS),
            color: Vec::with_capacity(GRAPH_MAX_NODOS),
            nivel: Vec::with_capacity(GRAPH_MAX_NODOS),
            n: 0,
        }
    }

    /// Prepara el Moser: A = posiciones exactas (encuadradas), B = idem,
    /// orden BFS + colores greedy `k = 4`. La única alloc vive acá.
    pub fn prepara_moser(&mut self) {
        self.n = MOSER_N;
        self.pa.clear();
        self.pb.clear();
        self.orden.clear();
        self.color.clear();
        self.nivel.clear();
        let mut pos = [[0.0f64; 2]; MOSER_N];
        for (i, p) in pos.iter_mut().enumerate() {
            *p = moser_pos(i);
        }
        encuadra(&mut pos, MOSER_N);
        self.pa.extend_from_slice(&pos);
        self.pb.extend_from_slice(&pos);
        self.orden.extend_from_slice(&MOSER_BFS);
        self.color.extend_from_slice(&MOSER_COLORES);
        let mut niv = [0u8; MOSER_N];
        let mut ord = [0u8; MOSER_N];
        let got = bfs_orden(MOSER_N, &MOSER_ARISTAS, 0, &mut ord, &mut niv);
        if got == MOSER_N {
            self.nivel.extend_from_slice(&niv);
        } else {
            self.nivel.extend_from_slice(&[0, 1, 1, 1, 1, 2, 2]);
        }
    }

    /// Prepara el BFS: A = circular, B = capas, orden + niveles reales.
    pub fn prepara_bfs(&mut self) {
        self.n = BFS_N;
        self.pa.clear();
        self.pb.clear();
        self.orden.clear();
        self.color.clear();
        self.nivel.clear();
        let mut a = [[0.0f64; 2]; BFS_N];
        let mut niv = [0u8; BFS_N];
        let mut ord = [0u8; BFS_N];
        let got = bfs_orden(BFS_N, &BFS_ARISTAS, 0, &mut ord, &mut niv);
        circular_layout(BFS_N, 2.6, &mut a);
        let mut b = [[0.0f64; 2]; BFS_N];
        if got == BFS_N {
            capas_layout(BFS_N, &niv, 1.7, 1.7, &mut b);
        } else {
            b = a;
        }
        self.pa.extend_from_slice(&a);
        self.pb.extend_from_slice(&b);
        self.orden.extend_from_slice(&BFS_ORDEN);
        self.nivel.extend_from_slice(&BFS_NIVELES);
        self.color.extend_from_slice(&[255u8; BFS_N]);
    }

    /// Prepara el force: A = semilla, B = relajado 48 iters.
    pub fn prepara_force(&mut self) {
        self.n = FORCE_N;
        self.pa.clear();
        self.pb.clear();
        self.orden.clear();
        self.color.clear();
        self.nivel.clear();
        let mut a = [[0.0f64; 2]; FORCE_N];
        let mut b = [[0.0f64; 2]; FORCE_N];
        force_semilla(FORCE_N, &mut a);
        if force_layout_hasta(FORCE_N, &FORCE_ARISTAS, FORCE_ITERS_TOTAL, &mut b).is_err() {
            b = a;
        }
        self.pa.extend_from_slice(&a);
        self.pb.extend_from_slice(&b);
        self.color.extend_from_slice(&[255u8; FORCE_N]);
    }

    /// Prepara el unit-distance: A = jitter, B = red ×2 (encuadre fijo).
    pub fn prepara_unit(&mut self) {
        self.n = UNIT_N;
        self.pa.clear();
        self.pb.clear();
        self.orden.clear();
        self.color.clear();
        self.nivel.clear();
        let mut a = [[0.0f64; 2]; UNIT_N];
        let mut b = [[0.0f64; 2]; UNIT_N];
        for i in 0..UNIT_N {
            a[i] = unit_jitter(i);
            let r = unit_red(i);
            b[i] = [r[0] * 2.0, r[1] * 2.0];
        }
        self.pa.extend_from_slice(&a);
        self.pb.extend_from_slice(&b);
        self.color.extend_from_slice(&[255u8; UNIT_N]);
    }

    /// Prepara el camino mínimo: A = circular, B = capas por distancia
    /// Dijkstra, orden de visita + camino. Si Dijkstra falla (no debería:
    /// datos pineados), deja circular y orden identidad.
    pub fn prepara_camino(&mut self) {
        self.n = CAMINO_N;
        self.pa.clear();
        self.pb.clear();
        self.orden.clear();
        self.color.clear();
        self.nivel.clear();
        let mut a = [[0.0f64; 2]; CAMINO_N];
        circular_layout(CAMINO_N, 2.6, &mut a);
        let mut dist = [0.0f64; CAMINO_N];
        let mut prev = [0i8; CAMINO_N];
        let mut visit = [0u8; CAMINO_N];
        let got = dijkstra(
            CAMINO_N,
            &CAMINO_ARISTAS,
            &CAMINO_PESOS,
            CAMINO_ORIGEN,
            &mut dist,
            &mut prev,
            &mut visit,
        );
        let mut camino = [0u8; CAMINO_N];
        let largo = reconstruye_camino(CAMINO_N, &prev, CAMINO_ORIGEN, CAMINO_DESTINO, &mut camino);
        let mut b = a;
        if got == CAMINO_N && largo == CAMINO_ESPERADO.len() {
            // Capas por distancia: nivel = posición en la visita.
            let mut niv = [0u8; CAMINO_N];
            for (k, v) in visit.iter().enumerate().take(got) {
                niv[*v as usize] = k.min(255) as u8;
            }
            capas_layout(CAMINO_N, &niv, 1.4, 1.1, &mut b);
            self.orden.extend_from_slice(&visit[..got]);
            self.color.extend_from_slice(&camino[..largo]);
            self.nivel.extend_from_slice(&niv);
        } else {
            let mut ident = [0u8; CAMINO_N];
            for (i, c) in ident.iter_mut().enumerate() {
                *c = i as u8;
            }
            self.orden.extend_from_slice(&ident);
            self.nivel.extend_from_slice(&[0u8; CAMINO_N]);
        }
        self.pa.extend_from_slice(&a);
        self.pb.extend_from_slice(&b);
    }

    /// Posición del vértice `v` con morph `pa → pb` en `u` (`None` si
    /// está fuera). Sin allocs.
    pub fn pos_en(&self, v: usize, u: f64) -> Option<[f64; 2]> {
        let a = *self.pa.get(v)?;
        let b = *self.pb.get(v)?;
        let t = suave(u);
        Some([
            finito_o_cero(a[0] + (b[0] - a[0]) * t),
            finito_o_cero(a[1] + (b[1] - a[1]) * t),
        ])
    }
}

// ── Paleta ────────────────────────────────────────────────────────────────

const FONDO: [u8; 4] = [15, 19, 28, 255];
const BARRA: [u8; 4] = [10, 14, 22, 255];
const GRILLA: [u8; 4] = [36, 46, 64, 255];
const EJE: [u8; 4] = [110, 130, 165, 255];
const BLANCO: [u8; 4] = [235, 238, 245, 255];
const AMARILLO: [u8; 4] = [255, 205, 70, 255];
const VERDE: [u8; 4] = [88, 196, 130, 255];
const ROJO: [u8; 4] = [250, 110, 110, 255];
const AZUL: [u8; 4] = [110, 170, 255, 255];
const NARANJA: [u8; 4] = [255, 160, 70, 255];
const GRIS: [u8; 4] = [140, 150, 170, 255];
const GRIS_OSCURO: [u8; 4] = [52, 62, 82, 255];
const ARISTA: [u8; 4] = [74, 88, 112, 255];

/// Color de vértice por índice de color 0..=7 (más allá → gris).
/// Puro, `Copy`.
pub fn color_de(c: u8) -> [u8; 4] {
    match c {
        0 => AMARILLO,
        1 => VERDE,
        2 => AZUL,
        3 => ROJO,
        4 => NARANJA,
        5 => BLANCO,
        6 => EJE,
        7 => GRIS,
        _ => GRIS_OSCURO,
    }
}

// ── Raster CPU (puro, con chequeo de bordes, sin pánicos) ─────────────────

/// Vista mundo→píxel (`y` hacia arriba en mundo, abajo en píxel).
struct Vista {
    w: usize,
    h: usize,
    escala: f64,
}

/// Vista con `±4.5` unidades en la dimensión menor. Pura.
fn vista_de(w: u32, h: u32) -> Vista {
    let menor = (w.min(h) as f64).max(1.0);
    Vista {
        w: w as usize,
        h: h as usize,
        escala: menor / 9.0,
    }
}

/// Mundo → píxel (`None` si no finito). El llamador recorta a bordes.
fn mundo_a_px(v: &Vista, x: f64, y: f64) -> Option<(i32, i32)> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let px = v.w as f64 / 2.0 + x * v.escala;
    let py = v.h as f64 / 2.0 - y * v.escala;
    if !px.is_finite() || !py.is_finite() {
        return None;
    }
    Some((px.round() as i32, py.round() as i32))
}

/// Píxel opaco (silencioso si está fuera).
fn pinta(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, c: [u8; 4]) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = (y as usize) * w + (x as usize);
    let i = i.saturating_mul(GRAPH_BYTES_POR_PIXEL);
    if let Some(p) = buf.get_mut(i..i + GRAPH_BYTES_POR_PIXEL) {
        p.copy_from_slice(&c);
    }
}

/// Disco relleno de radio `r`. Puro.
fn disco(buf: &mut [u8], w: usize, h: usize, cx: i32, cy: i32, r: i32, c: [u8; 4]) {
    if r <= 0 {
        pinta(buf, w, h, cx, cy, c);
        return;
    }
    let r = r.min(64);
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                pinta(buf, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

/// Segmento en píxeles con grosor (DDA determinista). Puro.
#[allow(clippy::too_many_arguments)]
fn segmento_px(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    grosor: i32,
    c: [u8; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let pasos = dx.abs().max(dy.abs()).max(0);
    if pasos == 0 {
        disco(buf, w, h, x0, y0, grosor / 2, c);
        return;
    }
    let radio = (grosor / 2).clamp(0, 64);
    for s in 0..=pasos {
        let x = x0 + dx.saturating_mul(s) / pasos;
        let y = y0 + dy.saturating_mul(s) / pasos;
        if radio == 0 {
            pinta(buf, w, h, x, y, c);
        } else {
            disco(buf, w, h, x, y, radio, c);
        }
    }
}

/// Segmento mundo→píxel (silencioso si un extremo no es finito).
#[allow(clippy::too_many_arguments)]
fn segmento(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    grosor: i32,
    c: [u8; 4],
) {
    if let (Some((ax, ay)), Some((bx, by))) = (mundo_a_px(v, x0, y0), mundo_a_px(v, x1, y1)) {
        segmento_px(buf, w, h, ax, ay, bx, by, grosor, c);
    }
}

/// Nodo: disco de color con borde (borde `None` = sin borde). Puro.
#[allow(clippy::too_many_arguments)]
fn nodo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    p: [f64; 2],
    radio: i32,
    relleno: [u8; 4],
    borde: Option<[u8; 4]>,
) {
    if let Some((x, y)) = mundo_a_px(v, p[0], p[1]) {
        if let Some(b) = borde {
            disco(buf, w, h, x, y, radio + 2, b);
        }
        disco(buf, w, h, x, y, radio, relleno);
    }
}

/// Rectángulo lleno en píxeles (barra de título). Puro.
#[allow(clippy::too_many_arguments)]
fn rect_lleno(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32, c: [u8; 4]) {
    for dy in 0..rh.max(0) {
        for dx in 0..rw.max(0) {
            pinta(buf, w, h, x + dx, y + dy, c);
        }
    }
}

/// Fondo + puntos de grilla cada 1.0 unidad. El setup de las escenas.
fn fondo(buf: &mut [u8], w: usize, h: usize, v: &Vista) {
    rect_lleno(buf, w, h, 0, 0, w as i32, h as i32, FONDO);
    let mut gy = -4.0f64;
    while gy <= 4.0 {
        let mut gx = -4.0f64;
        while gx <= 4.0 {
            if let Some((x, y)) = mundo_a_px(v, gx, gy) {
                pinta(buf, w, h, x, y, GRILLA);
            }
            gx += 1.0;
        }
        gy += 1.0;
    }
}

// ── Fuente bitmap 5×7 (mayúsculas; la etiqueta se normaliza acá) ──────────

/// Glifo 5×7 como 7 filas de 5 bits (bit 4 = izq.). `None` si no existe.
fn glifo(b: u8) -> Option<[u8; 7]> {
    match b {
        b'A' => Some([0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        b'B' => Some([0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E]),
        b'C' => Some([0x0F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0F]),
        b'D' => Some([0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E]),
        b'E' => Some([0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F]),
        b'F' => Some([0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10]),
        b'G' => Some([0x0F, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0E]),
        b'H' => Some([0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        b'I' => Some([0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1F]),
        b'J' => Some([0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        b'K' => Some([0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]),
        b'L' => Some([0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F]),
        b'M' => Some([0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11]),
        b'N' => Some([0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11]),
        b'O' => Some([0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        b'P' => Some([0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10]),
        b'Q' => Some([0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D]),
        b'R' => Some([0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11]),
        b'S' => Some([0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E]),
        b'T' => Some([0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        b'U' => Some([0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        b'V' => Some([0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04]),
        b'W' => Some([0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11]),
        b'X' => Some([0x11, 0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11]),
        b'Y' => Some([0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04]),
        b'Z' => Some([0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F]),
        b'0' => Some([0x0E, 0x13, 0x15, 0x15, 0x19, 0x11, 0x0E]),
        b'1' => Some([0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        b'2' => Some([0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F]),
        b'3' => Some([0x1E, 0x01, 0x01, 0x0E, 0x01, 0x01, 0x1E]),
        b'4' => Some([0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
        b'5' => Some([0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
        b'6' => Some([0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
        b'7' => Some([0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        b'8' => Some([0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
        b'9' => Some([0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E]),
        b' ' => Some([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        b'.' => Some([0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x06]),
        b',' => Some([0x00, 0x00, 0x00, 0x00, 0x06, 0x06, 0x08]),
        b':' => Some([0x00, 0x06, 0x06, 0x00, 0x06, 0x06, 0x00]),
        b'=' => Some([0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00]),
        b'+' => Some([0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00]),
        b'-' => Some([0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00]),
        b'/' => Some([0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10]),
        b'|' => Some([0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        b'(' => Some([0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02]),
        b')' => Some([0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08]),
        b'?' => Some([0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04]),
        _ => None,
    }
}

/// Minúsculas, acentos y `ñ` → ASCII mayús (`None` = se salta el char).
fn mayus_ascii(ch: char) -> Option<u8> {
    match ch {
        'a'..='z' => Some((u32::from(ch) - u32::from('a') + u32::from('A')) as u8),
        'A'..='Z' => Some(ch as u8),
        '0'..='9' => Some(ch as u8),
        ' ' | '.' | ',' | ':' | '=' | '+' | '-' | '/' | '|' | '(' | ')' | '?' => Some(ch as u8),
        'á' | 'à' | 'ä' | 'â' => Some(b'A'),
        'é' | 'è' | 'ë' | 'ê' => Some(b'E'),
        'í' | 'ì' | 'ï' | 'î' => Some(b'I'),
        'ó' | 'ò' | 'ö' | 'ô' => Some(b'O'),
        'ú' | 'ù' | 'ü' | 'û' => Some(b'U'),
        'ñ' | 'Ñ' => Some(b'N'),
        'ç' | 'Ç' => Some(b'C'),
        _ => None,
    }
}

/// Pinta un glifo en `(x, y)` con escala entera. Devuelve el avance en px.
#[allow(clippy::too_many_arguments)]
fn pinta_glifo(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    g: [u8; 7],
) -> i32 {
    let esc = (esc.clamp(1, 4)) as i32;
    for (fila, bits) in g.iter().enumerate() {
        for col in 0..5 {
            if bits & (1 << (4 - col)) != 0 {
                for dy in 0..esc {
                    for dx in 0..esc {
                        pinta(buf, w, h, x + col * esc + dx, y + fila as i32 * esc + dy, c);
                    }
                }
            }
        }
    }
    6 * esc
}

/// Texto ASCII (normalizado a mayúsculas) en píxeles. Devuelve la `x` final.
#[allow(clippy::too_many_arguments)]
fn texto(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    s: &str,
) -> i32 {
    let mut cursor = x;
    for ch in s.chars() {
        if let Some(b) = mayus_ascii(ch) {
            if let Some(g) = glifo(b) {
                cursor += pinta_glifo(buf, w, h, cursor, y, esc, c, g);
            } else {
                cursor += 6 * (esc.clamp(1, 4) as i32);
            }
        }
    }
    cursor
}

/// Bytes ya ASCII (números formateados en stack). Devuelve la `x` final.
#[allow(clippy::too_many_arguments)]
fn texto_bytes(
    buf: &mut [u8],
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    esc: usize,
    c: [u8; 4],
    s: &[u8],
) -> i32 {
    let mut cursor = x;
    for b in s {
        if let Some(g) = glifo(*b) {
            cursor += pinta_glifo(buf, w, h, cursor, y, esc, c, g);
        } else {
            cursor += 6 * (esc.clamp(1, 4) as i32);
        }
    }
    cursor
}

/// Entero `0..=999` en stack, sin heap. Devuelve el largo usado.
fn formatea_entero(v: usize, salida: &mut [u8; 4]) -> usize {
    let v = v.min(999);
    let c = (v / 100) as u8;
    let d = ((v / 10) % 10) as u8;
    let u = (v % 10) as u8;
    let mut n = 0usize;
    if c > 0 && n < salida.len() {
        salida[n] = b'0' + c;
        n += 1;
    }
    if (c > 0 || d > 0) && n < salida.len() {
        salida[n] = b'0' + d;
        n += 1;
    }
    if n < salida.len() {
        salida[n] = b'0' + u;
        n += 1;
    }
    n
}

/// Un decimal con redondeo (`2.5`, `-0.4`, `?` si no finito) en stack,
/// sin heap. Devuelve el largo usado.
fn formatea_1_decimal(v: f64, salida: &mut [u8; 10]) -> usize {
    if !v.is_finite() {
        salida[0] = b'?';
        return 1;
    }
    let mut n = 0usize;
    let mut q = (v * 10.0).round();
    if q < 0.0 {
        if n < salida.len() {
            salida[n] = b'-';
            n += 1;
        }
        q = -q;
    }
    if q > 999_999.0 {
        q = 999_999.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let qi = q as i64;
    let ent = qi / 10;
    let dec = (qi % 10) as u8;
    // Dígitos del entero (invertidos en stack, sin heap).
    let mut tmp = [0u8; 8];
    let mut m = 0usize;
    let mut e = ent.max(0);
    loop {
        if m < tmp.len() {
            tmp[m] = b'0' + (e % 10) as u8;
            m += 1;
        }
        e /= 10;
        if e == 0 {
            break;
        }
    }
    while m > 0 {
        m -= 1;
        if n < salida.len() {
            salida[n] = tmp[m];
            n += 1;
        }
    }
    if n < salida.len() {
        salida[n] = b'.';
        n += 1;
    }
    if n < salida.len() {
        salida[n] = b'0' + dec;
        n += 1;
    }
    n
}

/// Contador `k/n` en stack, sin heap. Devuelve el largo usado.
fn formatea_progreso(k: usize, n: usize, salida: &mut [u8; 9]) -> usize {
    let mut num = [0u8; 4];
    let lk = formatea_entero(k, &mut num);
    let mut m = 0usize;
    for b in num.iter().take(lk) {
        if m < salida.len() {
            salida[m] = *b;
            m += 1;
        }
    }
    if m < salida.len() {
        salida[m] = b'/';
        m += 1;
    }
    let ln = formatea_entero(n, &mut num);
    for b in num.iter().take(ln) {
        if m < salida.len() {
            salida[m] = *b;
            m += 1;
        }
    }
    m
}

/// Escala de texto según lienzo (1 en chicos, 2 desde 300 px).
fn escala_texto(w: u32, h: u32) -> usize {
    if w.min(h) >= 300 {
        2
    } else {
        1
    }
}

/// Radio de nodo según lienzo (3..=9 px).
fn radio_nodo(w: u32, h: u32) -> i32 {
    ((w.min(h) / 48) as i32).clamp(3, 9)
}

/// Barra de título + etiqueta. Devuelve el alto de la barra en px.
fn rotulo(buf: &mut [u8], w: usize, h: usize, esc: usize, etiqueta: &str) -> i32 {
    let alto = 7 * esc.clamp(1, 4) as i32 + 8;
    rect_lleno(buf, w, h, 0, 0, w as i32, alto, BARRA);
    texto(buf, w, h, 6, 4, esc, BLANCO, etiqueta);
    for x in 0..w as i32 {
        pinta(buf, w, h, x, alto, GRILLA);
    }
    alto + 1
}

// ── Registro público ──────────────────────────────────────────────────────

/// ¿Atiende este módulo la plantilla (case-insensitive, con trim)?
pub fn es_plantilla_graphs(template: &str) -> bool {
    etiqueta_de(template).is_some()
}

/// Etiqueta en español de la escena (`None` si no es de este módulo).
pub fn etiqueta_de(template: &str) -> Option<&'static str> {
    match template.trim().to_lowercase().as_str() {
        "moser-spindle-coloreo" => Some("MOSER SPINDLE: 4 COLORES"),
        "bfs-animado" => Some("BFS POR CAPAS"),
        "force-directed" => Some("FORCE-DIRECTED: CONVERGE"),
        "unit-distance" => Some("UNIT DISTANCE: DIST=1"),
        "camino-minimo" => Some("CAMINO MINIMO: DIJKSTRA"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escena {
    Moser,
    Bfs,
    Force,
    Unit,
    Camino,
}

/// Resuelve el id a escena (`Err` honesto si no es de este módulo).
fn escena_de(template: &str) -> Result<Escena, GraphError> {
    match template.trim().to_lowercase().as_str() {
        "moser-spindle-coloreo" => Ok(Escena::Moser),
        "bfs-animado" => Ok(Escena::Bfs),
        "force-directed" => Ok(Escena::Force),
        "unit-distance" => Ok(Escena::Unit),
        "camino-minimo" => Ok(Escena::Camino),
        _ => Err(GraphError::PlantillaDesconocida {
            got: template.trim().to_string(),
        }),
    }
}

// ── Escenas (setup + animación + etiqueta) ────────────────────────────────

/// ¿La arista `(a, b)` está en la lista (cualquiera de los dos órdenes)?
fn tiene_arista(aristas: &[(u8, u8)], a: usize, b: usize) -> bool {
    for (x, y) in aristas {
        let (x, y) = (*x as usize, *y as usize);
        if (x == a && y == b) || (x == b && y == a) {
            return true;
        }
    }
    false
}

/// Resaltado por índice de arista: color y grosor, o `None` si no resalta.
type ResaltaArista = Option<fn(usize) -> Option<([u8; 4], i32)>>;

/// Dibuja las aristas entre posiciones actuales (`resalta` decide grosor
/// y color por índice de arista; `None` = tenue).
#[allow(clippy::too_many_arguments)]
fn dibuja_aristas(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    pos: &[[f64; 2]],
    n: usize,
    aristas: &[(u8, u8)],
    resalta: ResaltaArista,
) {
    for (k, (a, b)) in aristas.iter().enumerate() {
        let (x, y) = (*a as usize, *b as usize);
        if x >= n || y >= n || x >= pos.len() || y >= pos.len() {
            continue;
        }
        let (c, g) = resalta.and_then(|f| f(k)).unwrap_or((ARISTA, 1));
        segmento(
            buf, w, h, v, pos[x][0], pos[x][1], pos[y][0], pos[y][1], g, c,
        );
    }
}

/// Moser spindle: las aristas se trazan (`t < 0.3`), los vértices se
/// colorean en orden BFS (`0.2..0.85`, uno por uno con el color greedy
/// `k = 4`) y al final la arista terminal `2-5` queda en rojo con los
/// terminales en borde blanco (los que fuerzan el 4to color).
#[allow(clippy::too_many_arguments)]
fn dibuja_moser(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    esc_texto: usize,
    radio: i32,
) {
    // 1. Setup: fondo + puntos.
    fondo(buf, w, h, v);
    // 2. Animación.
    let n = MOSER_N;
    let mut pos = [[0.0f64; 2]; MOSER_N];
    for (i, celda) in pos.iter_mut().enumerate().take(n) {
        *celda = scratch.pos_en(i, 1.0).unwrap_or([0.0, 0.0]);
    }
    let trazo = tramo(t, 0.0, 0.30);
    let cierre = tramo(t, 0.85, 1.0);
    // Aristas: aparecen en orden de lista (terminal al final).
    let total = MOSER_ARISTAS.len();
    let hasta = (trazo * (total as f64)).round() as usize;
    for (k, (a, b)) in MOSER_ARISTAS.iter().enumerate() {
        if k >= hasta.min(total) {
            break;
        }
        let (x, y) = (*a as usize, *b as usize);
        let es_terminal = (x == 2 && y == 5) || (x == 5 && y == 2);
        let (c, g) = if es_terminal && cierre > 0.0 {
            (ROJO, 3)
        } else if es_terminal {
            (ARISTA, 1)
        } else {
            (ARISTA, 2)
        };
        segmento(
            buf, w, h, v, pos[x][0], pos[x][1], pos[y][0], pos[y][1], g, c,
        );
    }
    // Vértices: uno por uno en orden BFS.
    let revela = tramo(t, 0.20, 0.85);
    let hechos = (revela * (n as f64)).round() as usize;
    let mut nk = [0u8; 9];
    let mut hechos_reales = 0usize;
    for (rango, vtx) in scratch.orden.iter().take(n).enumerate() {
        let vi = *vtx as usize;
        if vi >= n {
            continue;
        }
        if rango < hechos.min(n) {
            let c = scratch.color.get(vi).copied().unwrap_or(255);
            let es_terminal =
                vi == MOSER_TERMINALES.0 as usize || vi == MOSER_TERMINALES.1 as usize;
            let borde = if es_terminal && cierre > 0.0 {
                Some(BLANCO)
            } else {
                None
            };
            nodo(buf, w, h, v, pos[vi], radio, color_de(c), borde);
            hechos_reales += 1;
        } else {
            nodo(buf, w, h, v, pos[vi], radio, GRIS_OSCURO, None);
        }
    }
    // 3. Valor vivo: coloreados + K.
    let y = 7 * esc_texto.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "N=");
    let ln = formatea_progreso(hechos_reales, n, &mut nk);
    let x = texto_bytes(buf, w, h, x, y, esc_texto, AMARILLO, &nk[..ln]);
    texto(buf, w, h, x, y, esc_texto, BLANCO, " K=4 (3 NO ALCANZA)");
}

/// BFS animado: morph circular→capas en la primera mitad mientras la
/// frontera se expande nivel por nivel; el árbol BFS (padre = menor
/// índice) queda en verde y la frontera actual lleva borde blanco.
#[allow(clippy::too_many_arguments)]
fn dibuja_bfs(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    esc_texto: usize,
    radio: i32,
) {
    // 1. Setup: fondo + puntos.
    fondo(buf, w, h, v);
    // 2. Animación.
    let n = BFS_N;
    let u_morph = suave(tramo(t, 0.0, 0.45));
    let mut pos = [[0.0f64; 2]; BFS_N];
    for (i, celda) in pos.iter_mut().enumerate().take(n) {
        let a = scratch.pa.get(i).copied().unwrap_or([0.0, 0.0]);
        let b = scratch.pb.get(i).copied().unwrap_or([0.0, 0.0]);
        *celda = [
            a[0] + (b[0] - a[0]) * u_morph,
            a[1] + (b[1] - a[1]) * u_morph,
        ];
    }
    let revela = tramo(t, 0.15, 0.95);
    let hechos = (revela * (n as f64)).round() as usize;
    let hechos = hechos.min(n);
    // Visitados por rango de revelado.
    let mut visitado = [false; BFS_N];
    for vtx in scratch.orden.iter().take(hechos) {
        let vi = *vtx as usize;
        if vi < n {
            visitado[vi] = true;
        }
    }
    let frontera = if hechos < n {
        scratch.orden.get(hechos).copied().unwrap_or(255)
    } else {
        255
    };
    // Aristas: árbol en verde (hijo visitado + padre visitado), resto tenue.
    for (a, b) in BFS_ARISTAS {
        let (x, y) = (a as usize, b as usize);
        if !visitado[x] || !visitado[y] {
            continue;
        }
        // ¿Es arista de árbol? el de nivel mayor cuelga del menor índice.
        let nx = scratch.nivel.get(x).copied().unwrap_or(255);
        let ny = scratch.nivel.get(y).copied().unwrap_or(255);
        let es_arbol = nx != ny
            && tiene_arista(&BFS_ARISTAS, x, y)
            && (nx.saturating_add(1) == ny || ny.saturating_add(1) == nx);
        let (c, g) = if es_arbol { (VERDE, 2) } else { (ARISTA, 1) };
        segmento(
            buf, w, h, v, pos[x][0], pos[x][1], pos[y][0], pos[y][1], g, c,
        );
    }
    // Nodos por nivel; la frontera con borde.
    let mut nk = [0u8; 9];
    let mut cuenta = 0usize;
    for i in 0..n {
        if visitado[i] {
            let nv = scratch.nivel.get(i).copied().unwrap_or(0);
            let borde = if i as u8 == frontera {
                Some(BLANCO)
            } else {
                None
            };
            nodo(buf, w, h, v, pos[i], radio, color_de(nv % 4), borde);
            cuenta += 1;
        } else {
            nodo(buf, w, h, v, pos[i], radio, GRIS_OSCURO, None);
        }
    }
    // 3. Valor vivo: visitados.
    let y = 7 * esc_texto.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "VISITADOS ");
    let ln = formatea_progreso(cuenta, n, &mut nk);
    texto_bytes(buf, w, h, x, y, esc_texto, VERDE, &nk[..ln]);
}

/// Force-directed: `round(t·48)` pasos FR desde la semilla determinista;
/// la energía media de aristas baja y queda como valor vivo junto al
/// contador de iteraciones.
fn dibuja_force(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    esc_texto: usize,
    radio: i32,
) {
    // 1. Setup: fondo + puntos.
    fondo(buf, w, h, v);
    // 2. Animación (recomputa desde la semilla: sin estado entre frames).
    let iters = (t * (FORCE_ITERS_TOTAL as f64)).round() as usize;
    let mut pos = [[0.0f64; 2]; FORCE_N];
    if force_layout_hasta(
        FORCE_N,
        &FORCE_ARISTAS,
        iters.min(FORCE_ITERS_TOTAL),
        &mut pos,
    )
    .is_err()
    {
        force_semilla(FORCE_N, &mut pos);
    }
    dibuja_aristas(buf, w, h, v, &pos, FORCE_N, &FORCE_ARISTAS, None);
    for p in pos.iter().take(FORCE_N) {
        nodo(buf, w, h, v, *p, radio, AMARILLO, None);
    }
    // 3. Valor vivo: IT + energía.
    let k = force_k_ideal(FORCE_N);
    let e = energia_aristas(&pos, FORCE_N, &FORCE_ARISTAS, k);
    let y = 7 * esc_texto.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "IT=");
    let mut nk = [0u8; 9];
    let ln = formatea_progreso(iters.min(FORCE_ITERS_TOTAL), FORCE_ITERS_TOTAL, &mut nk);
    let x = texto_bytes(buf, w, h, x, y, esc_texto, AMARILLO, &nk[..ln]);
    let x = texto(buf, w, h, x, y, esc_texto, BLANCO, " E=");
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(e, &mut num);
    texto_bytes(buf, w, h, x, y, esc_texto, VERDE, &num[..ln]);
}

/// Unit-distance: morph jitter→red con la tolerancia cerrando de `0.35`
/// a [`GRAPH_UNIT_TOL`]; las aristas aparecen cuando `|d − 1| ≤ tol` y al
/// final quedan exactamente las 12 de la red (centro + hexágono).
#[allow(clippy::too_many_arguments)]
fn dibuja_unit(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    esc_texto: usize,
    radio: i32,
) {
    // 1. Setup: fondo + puntos.
    fondo(buf, w, h, v);
    // 2. Animación.
    let n = UNIT_N;
    let u = suave(t);
    let mut pos = [[0.0f64; 2]; UNIT_N];
    for (i, celda) in pos.iter_mut().enumerate().take(n) {
        let a = scratch.pa.get(i).copied().unwrap_or([0.0, 0.0]);
        let b = scratch.pb.get(i).copied().unwrap_or([0.0, 0.0]);
        *celda = [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u];
    }
    let tol = 0.35 * (1.0 - u) + GRAPH_UNIT_TOL;
    // Pares a distancia unidad con la tolerancia actual.
    let mut cuantas = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            if arista_unitaria(dist(pos[i], pos[j]), tol) {
                segmento(
                    buf, w, h, v, pos[i][0], pos[i][1], pos[j][0], pos[j][1], 2, AZUL,
                );
                cuantas += 1;
            }
        }
    }
    for (i, p) in pos.iter().take(n).enumerate() {
        // ¿Toca alguna arista? ámbar, si no gris.
        let mut toca = false;
        for j in 0..n {
            if j != i && arista_unitaria(dist(pos[i], pos[j]), tol) {
                toca = true;
                break;
            }
        }
        let _ = i;
        nodo(
            buf,
            w,
            h,
            v,
            *p,
            radio,
            if toca { AMARILLO } else { GRIS_OSCURO },
            None,
        );
    }
    // 3. Valor vivo: aristas visibles.
    let y = 7 * esc_texto.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "ARISTAS ");
    let mut nk = [0u8; 9];
    let ln = formatea_progreso(cuantas, UNIT_ARISTAS_ESPERADAS, &mut nk);
    texto_bytes(buf, w, h, x, y, esc_texto, AZUL, &nk[..ln]);
}

/// Camino mínimo: morph circular→capas(Dijkstra) en la primera mitad,
/// los visitados se encienden en orden de extracción y al final el camino
/// `0→7` (costo 16) se traza en verde progresivo.
#[allow(clippy::too_many_arguments)]
fn dibuja_camino(
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    esc_texto: usize,
    radio: i32,
) {
    // 1. Setup: fondo + puntos.
    fondo(buf, w, h, v);
    // 2. Animación.
    let n = CAMINO_N;
    let u_morph = suave(tramo(t, 0.0, 0.40));
    let mut pos = [[0.0f64; 2]; CAMINO_N];
    for (i, celda) in pos.iter_mut().enumerate().take(n) {
        let a = scratch.pa.get(i).copied().unwrap_or([0.0, 0.0]);
        let b = scratch.pb.get(i).copied().unwrap_or([0.0, 0.0]);
        *celda = [
            a[0] + (b[0] - a[0]) * u_morph,
            a[1] + (b[1] - a[1]) * u_morph,
        ];
    }
    let revela = tramo(t, 0.10, 0.60);
    let hechos = ((revela * (n as f64)).round() as usize).min(n);
    let mut visitado = [false; CAMINO_N];
    for vtx in scratch.orden.iter().take(hechos) {
        let vi = *vtx as usize;
        if vi < n {
            visitado[vi] = true;
        }
    }
    // Aristas tenues entre visitados.
    for (a, b) in CAMINO_ARISTAS {
        let (x, y) = (a as usize, b as usize);
        if visitado[x] && visitado[y] {
            segmento(
                buf, w, h, v, pos[x][0], pos[x][1], pos[y][0], pos[y][1], 1, ARISTA,
            );
        }
    }
    // Camino en verde progresivo (tramos entre vértices consecutivos).
    let trazo = tramo(t, 0.60, 1.0);
    let tramos = scratch.color.len().saturating_sub(1);
    let pintados = (trazo * (tramos as f64)).round() as usize;
    for k in 0..pintados.min(tramos) {
        let x = scratch.color.get(k).copied().unwrap_or(255) as usize;
        let y = scratch.color.get(k + 1).copied().unwrap_or(255) as usize;
        if x < n && y < n {
            segmento(
                buf, w, h, v, pos[x][0], pos[x][1], pos[y][0], pos[y][1], 3, VERDE,
            );
        }
    }
    // Nodos: camino en verde, visitados en azul, resto gris.
    for i in 0..n {
        let en_camino = scratch.color.iter().any(|c| *c as usize == i);
        let es_punta = i == CAMINO_ORIGEN || i == CAMINO_DESTINO;
        if en_camino && trazo > 0.0 {
            nodo(
                buf,
                w,
                h,
                v,
                pos[i],
                radio,
                VERDE,
                if es_punta { Some(BLANCO) } else { None },
            );
        } else if visitado[i] {
            nodo(
                buf,
                w,
                h,
                v,
                pos[i],
                radio,
                AZUL,
                if es_punta { Some(BLANCO) } else { None },
            );
        } else {
            nodo(buf, w, h, v, pos[i], radio, GRIS_OSCURO, None);
        }
    }
    // 3. Valor vivo: distancia mínima.
    let y = 7 * esc_texto.clamp(1, 4) as i32 + 14;
    let x = texto(buf, w, h, 8, y, esc_texto, BLANCO, "D(0,7)=");
    let mut num = [0u8; 10];
    let ln = formatea_1_decimal(CAMINO_DISTANCIA, &mut num);
    texto_bytes(buf, w, h, x, y, esc_texto, VERDE, &num[..ln]);
}

/// Un frame de la escena en `t` global (setup del scratch ya hecho).
#[allow(clippy::too_many_arguments)]
fn dibuja_frame(
    esc: Escena,
    buf: &mut [u8],
    w: usize,
    h: usize,
    v: &Vista,
    t: f64,
    scratch: &Scratch,
    etiqueta: &str,
    esc_texto: usize,
    radio: i32,
) {
    match esc {
        Escena::Moser => dibuja_moser(buf, w, h, v, t, scratch, esc_texto, radio),
        Escena::Bfs => dibuja_bfs(buf, w, h, v, t, scratch, esc_texto, radio),
        Escena::Force => dibuja_force(buf, w, h, v, t, esc_texto, radio),
        Escena::Unit => dibuja_unit(buf, w, h, v, t, scratch, esc_texto, radio),
        Escena::Camino => dibuja_camino(buf, w, h, v, t, scratch, esc_texto, radio),
    }
    // 3. Etiqueta (siempre, arriba).
    rotulo(buf, w, h, esc_texto, etiqueta);
}

// ── Renderers públicos ────────────────────────────────────────────────────

/// Núcleo: rango `[desde, desde+cantidad)` de un set de `total` frames
/// (el `t` global sale del índice global: el corte es empalme exacto).
/// Valida lienzo, rango y que el chunk entre en 64 MiB.
pub fn render_graphs_rango(
    template: &str,
    width: u32,
    height: u32,
    total: usize,
    desde: usize,
    cantidad: usize,
) -> Result<Vec<RgbaFrame>, GraphError> {
    let esc = escena_de(template)?;
    let etiqueta = etiqueta_de(template).unwrap_or("GRAFOS Y REDES");
    valida_lienzo(width, height)?;
    if !(GRAPH_MIN_FRAMES..=GRAPH_LONGFORM_MAX_FRAMES).contains(&total) {
        return Err(GraphError::FramesFueraDeRango { got: total });
    }
    if cantidad == 0 || desde.checked_add(cantidad).is_none_or(|fin| fin > total) {
        return Err(GraphError::FramesFueraDeRango { got: cantidad });
    }
    match estima_bytes(width, height, cantidad) {
        Some(n) if n <= GRAPH_CHUNK_MAX_BYTES => {}
        Some(n) => return Err(GraphError::SetDemasiadoGrande { bytes: n }),
        None => return Err(GraphError::SetDemasiadoGrande { bytes: usize::MAX }),
    }
    let vista = vista_de(width, height);
    let esc_texto = escala_texto(width, height);
    let radio = radio_nodo(width, height);
    // Setup del sampler (una vez, no por frame).
    let mut scratch = Scratch::nuevo();
    match esc {
        Escena::Moser => scratch.prepara_moser(),
        Escena::Bfs => scratch.prepara_bfs(),
        Escena::Force => scratch.prepara_force(),
        Escena::Unit => scratch.prepara_unit(),
        Escena::Camino => scratch.prepara_camino(),
    }
    let mut frames = Vec::new();
    if frames.try_reserve_exact(cantidad).is_err() {
        return Err(GraphError::SetDemasiadoGrande { bytes: usize::MAX });
    }
    for g in desde..desde.saturating_add(cantidad) {
        let t = progreso_en(g, total);
        let mut frame = RgbaFrame::nuevo(width, height)?;
        dibuja_frame(
            esc,
            &mut frame.pixels,
            vista.w,
            vista.h,
            &vista,
            t,
            &scratch,
            etiqueta,
            esc_texto,
            radio,
        );
        frames.push(frame);
    }
    Ok(frames)
}

/// Set completo de `frames` para la plantilla (valida el presupuesto de
/// 64 MiB: lo que no entra se pide por rangos con [`render_graphs_rango`]).
pub fn render_graphs_frames(
    template: &str,
    width: u32,
    height: u32,
    frames: usize,
) -> Result<Vec<RgbaFrame>, GraphError> {
    valida_pedido(width, height, frames)?;
    render_graphs_rango(template, width, height, frames, 0, frames)
}

/// Moser spindle coloreado (48 frames default).
pub fn render_moser_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, GraphError> {
    render_graphs_frames("moser-spindle-coloreo", width, height, GRAPH_DEFAULT_FRAMES)
}

/// BFS animado (48 frames default).
pub fn render_bfs_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, GraphError> {
    render_graphs_frames("bfs-animado", width, height, GRAPH_DEFAULT_FRAMES)
}

/// Force-directed convergiendo (48 frames default).
pub fn render_force_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, GraphError> {
    render_graphs_frames("force-directed", width, height, GRAPH_DEFAULT_FRAMES)
}

/// Unit-distance apareciendo (48 frames default).
pub fn render_unit_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, GraphError> {
    render_graphs_frames("unit-distance", width, height, GRAPH_DEFAULT_FRAMES)
}

/// Camino mínimo (48 frames default).
pub fn render_camino_frames(width: u32, height: u32) -> Result<Vec<RgbaFrame>, GraphError> {
    render_graphs_frames("camino-minimo", width, height, GRAPH_DEFAULT_FRAMES)
}

// ── Tests inline ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn set_ok(id: &str, w: u32, h: u32, n: usize) -> Vec<RgbaFrame> {
        let r = render_graphs_frames(id, w, h, n);
        assert!(r.is_ok(), "{id} falló: {:?}", r.err());
        if let Ok(set) = r {
            assert_eq!(set.len(), n, "{id}: len");
            for f in &set {
                assert_eq!((f.width, f.height), (w, h), "{id}: dims");
                assert_eq!(f.pixels.len(), w as usize * h as usize * 4, "{id}: px");
            }
            set
        } else {
            Vec::new()
        }
    }

    #[test]
    fn registro_cinco_ids_kebab_unicos() {
        assert_eq!(TEMPLATE_IDS.len(), 5);
        let mut vistos = Vec::new();
        for id in TEMPLATE_IDS {
            assert!(!id.is_empty(), "id vacío");
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "no kebab: {id}"
            );
            assert!(!vistos.contains(id), "duplicado: {id}");
            vistos.push(*id);
            assert!(es_plantilla_graphs(id), "{id} no reconocido");
            assert!(
                etiqueta_de(id).is_some_and(|e| !e.is_empty()),
                "{id} sin etiqueta"
            );
            assert!(escena_de(id).is_ok(), "{id} sin escena");
        }
        assert!(!es_plantilla_graphs("taylor-series"));
        assert!(!es_plantilla_graphs(""));
        assert!(etiqueta_de("taylor-series").is_none());
        assert!(escena_de("taylor-series").is_err());
        // Case-insensitive con trim (como el dispatcher nativo).
        assert!(es_plantilla_graphs("  Moser-Spindle-Coloreo "));
        assert!(es_plantilla_graphs("FORCE-DIRECTED"));
    }

    #[test]
    fn presupuestos_pineados() {
        assert_eq!(GRAPH_DEFAULT_FRAMES, 48);
        assert_eq!((GRAPH_MIN_FRAMES, GRAPH_MAX_FRAMES), (1, 64));
        assert_eq!(GRAPH_LONGFORM_MAX_FRAMES, 1500);
        assert_eq!(GRAPH_CHUNK_MAX_BYTES, 64 * 1024 * 1024);
        assert_eq!((GRAPH_CANVAS_MIN, GRAPH_CANVAS_MAX), (64, 4096));
        assert_eq!(GRAPH_MAX_NODOS, 256);
        assert_eq!(FORCE_ITERS_TOTAL, 48);
        assert_eq!(estima_bytes(480, 360, 48), Some(33_177_600));
        assert!(estima_bytes(480, 360, 48).is_some_and(|n| n <= GRAPH_CHUNK_MAX_BYTES));
        assert_eq!(frames_por_chunk(1280, 720), 18);
        assert_eq!(frames_por_chunk(640, 480), 54);
        assert_eq!(frames_por_chunk(0, 480), 0);
        assert_eq!(frames_para_duracion(50_000, 30), 1500);
        assert_eq!(frames_para_duracion(2000, 12), 24);
        assert_eq!(frames_para_duracion(1000, 0), 0);
        assert!(valida_pedido(480, 360, 48).is_ok());
        assert!(valida_pedido(64, 64, 1500).is_ok());
        assert!(valida_pedido(480, 360, 0).is_err());
        assert!(valida_pedido(480, 360, 1501).is_err());
        assert!(valida_pedido(63, 360, 48).is_err());
        assert!(valida_pedido(480, 4097, 48).is_err());
        assert!(valida_pedido(4096, 4096, 48).is_err());
        // N > 256 pineado.
        assert!(force_layout_hasta(257, &FORCE_ARISTAS, 4, &mut [[0.0; 2]; 257]).is_err());
        assert!(force_layout_hasta(0, &FORCE_ARISTAS, 4, &mut [[0.0; 2]; 4]).is_err());
    }

    #[test]
    fn moser_geometria_y_coloreo_del_hunt() {
        // Aristas del hunt: 11, terminales 2-5.
        assert_eq!(MOSER_ARISTAS.len(), 11);
        assert!(MOSER_ARISTAS.contains(&MOSER_TERMINALES));
        // Geometría: las 11 aristas miden 1 (tol del hunt 1e-9).
        let mut pos = [[0.0f64; 2]; MOSER_N];
        for (i, p) in pos.iter_mut().enumerate() {
            *p = moser_pos(i);
        }
        for (a, b) in MOSER_ARISTAS {
            let d = dist(pos[a as usize], pos[b as usize]);
            assert!((d - 1.0).abs() <= GRAPH_UNIT_TOL, "arista {a}-{b}: d={d}");
        }
        // Estricto: ningún par no-adyacente cae a distancia 1.
        for i in 0..MOSER_N {
            for j in (i + 1)..MOSER_N {
                if !tiene_arista(&MOSER_ARISTAS, i, j) {
                    let d = dist(pos[i], pos[j]);
                    assert!((d - 1.0).abs() > 1e-6, "falsa arista {i}-{j}: d={d}");
                }
            }
        }
        // BFS desde el 0 cubre los 7.
        let mut ord = [0u8; MOSER_N];
        let mut niv = [0u8; MOSER_N];
        assert_eq!(bfs_orden(MOSER_N, &MOSER_ARISTAS, 0, &mut ord, &mut niv), 7);
        // Greedy k=3: honesto, deja 1 sin color (el terminal).
        let mut col3 = [255u8; MOSER_N];
        let hechos3 = greedy_colorea(MOSER_N, &MOSER_ARISTAS, &MOSER_BFS, 3, &mut col3);
        assert_eq!(hechos3, 6, "k=3 debería fallar en 1: {col3:?}");
        assert!(!coloreo_propio(MOSER_N, &MOSER_ARISTAS, &col3));
        // Greedy k=4 en orden BFS: propio y pineado.
        let mut col4 = [255u8; MOSER_N];
        assert_eq!(
            greedy_colorea(MOSER_N, &MOSER_ARISTAS, &MOSER_BFS, 4, &mut col4),
            7
        );
        assert_eq!(col4, MOSER_COLORES);
        assert!(coloreo_propio(MOSER_N, &MOSER_ARISTAS, &col4));
        // Abierto (sin arista terminal): k=3 propio (mono-pair del hunt).
        let abiertas: [(u8, u8); 10] = [
            (0, 1),
            (1, 3),
            (0, 3),
            (1, 2),
            (2, 3),
            (0, 4),
            (4, 6),
            (0, 6),
            (4, 5),
            (5, 6),
        ];
        let mut col3b = [255u8; MOSER_N];
        assert_eq!(
            greedy_colorea(MOSER_N, &abiertas, &MOSER_BFS, 3, &mut col3b),
            7
        );
        assert!(coloreo_propio(MOSER_N, &abiertas, &col3b));
    }

    #[test]
    fn bfs_orden_niveles_y_layouts() {
        let mut ord = [0u8; BFS_N];
        let mut niv = [0u8; BFS_N];
        assert_eq!(bfs_orden(BFS_N, &BFS_ARISTAS, 0, &mut ord, &mut niv), BFS_N);
        assert_eq!(ord, BFS_ORDEN);
        assert_eq!(niv, BFS_NIVELES);
        // Origen inválido / buffers cortos → 0 honesto.
        let mut corto = [0u8; 4];
        assert_eq!(bfs_orden(BFS_N, &BFS_ARISTAS, 0, &mut corto, &mut niv), 0);
        assert_eq!(bfs_orden(BFS_N, &BFS_ARISTAS, BFS_N, &mut ord, &mut niv), 0);
        // Circular: radio exacto y centrada.
        let mut c = [[0.0f64; 2]; BFS_N];
        circular_layout(BFS_N, 2.6, &mut c);
        for p in c.iter().take(BFS_N) {
            assert!((dist(*p, [0.0, 0.0]) - 2.6).abs() < 1e-9);
        }
        // Capas: misma fila por nivel, raíz arriba.
        let mut l = [[0.0f64; 2]; BFS_N];
        capas_layout(BFS_N, &BFS_NIVELES, 1.7, 1.7, &mut l);
        assert!(l[0][1] > l[1][1] && l[1][1] > l[4][1] && l[4][1] > l[8][1]);
        for (i, p) in l.iter().enumerate().take(BFS_N) {
            for (j, q) in l.iter().enumerate().take(BFS_N) {
                if BFS_NIVELES[i] == BFS_NIVELES[j] && i != j {
                    assert!((p[1] - q[1]).abs() < 1e-9, "capa {i}/{j}");
                }
            }
        }
        // Grilla 3x3-ish centrada.
        let mut g = [[0.0f64; 2]; 9];
        grilla_layout(9, 3, 1.5, &mut g);
        assert_eq!(g[4], [0.0, 0.0]);
        // Morph: extremos exactos y punto medio.
        let mut m = [[0.0f64; 2]; BFS_N];
        morph_layout(&c, &l, 0.0, &mut m);
        assert_eq!(m, c);
        morph_layout(&c, &l, 1.0, &mut m);
        // `t = 1` no es bit-exacto (`a + (b − a)` redondea): tolerancia.
        for i in 0..BFS_N {
            assert!((m[i][0] - l[i][0]).abs() < 1e-12, "morph t=1 x{i}");
            assert!((m[i][1] - l[i][1]).abs() < 1e-12, "morph t=1 y{i}");
        }
        morph_layout(&c, &l, 0.5, &mut m);
        for i in 0..BFS_N {
            assert!(((m[i][0] - (c[i][0] + l[i][0]) / 2.0).abs() < 1e-9));
        }
        // Encuadre: extent máximo 6 y centrado.
        let mut e = c;
        encuadra(&mut e, BFS_N);
        let (mut x0, mut x1) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in e.iter().take(BFS_N) {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
        }
        assert!((x1 - x0 - 6.0).abs() < 1e-9 || (x1 - x0) <= 6.0);
    }

    #[test]
    fn force_converge_determinista_y_acotado() {
        let k = force_k_ideal(FORCE_N);
        assert!((k - (40.96f64 / 12.0).sqrt()).abs() < 1e-9);
        let mut p0 = [[0.0f64; 2]; FORCE_N];
        let r0 = force_layout_hasta(FORCE_N, &FORCE_ARISTAS, 0, &mut p0);
        assert!(r0.is_ok(), "0 iters: {:?}", r0.err());
        let e0 = energia_aristas(&p0, FORCE_N, &FORCE_ARISTAS, k);
        let mut p1 = [[0.0f64; 2]; FORCE_N];
        let r1 = force_layout_hasta(FORCE_N, &FORCE_ARISTAS, FORCE_ITERS_TOTAL, &mut p1);
        assert!(r1.is_ok(), "48 iters: {:?}", r1.err());
        let e1 = energia_aristas(&p1, FORCE_N, &FORCE_ARISTAS, k);
        assert!(e1 < e0, "no converge: {e0} → {e1}");
        // Determinista: dos corridas iguales.
        let mut p2 = [[0.0f64; 2]; FORCE_N];
        let r2 = force_layout_hasta(FORCE_N, &FORCE_ARISTAS, FORCE_ITERS_TOTAL, &mut p2);
        assert!(r2.is_ok(), "repite: {:?}", r2.err());
        assert_eq!(p1, p2);
        // Enfriamiento: el último paso mueve menos que el primero.
        let mut pa = [[0.0f64; 2]; FORCE_N];
        force_semilla(FORCE_N, &mut pa);
        let mov0 = force_paso(&mut pa, FORCE_N, &FORCE_ARISTAS, k, 0.35);
        let mut pb = p1;
        let movf = force_paso(&mut pb, FORCE_N, &FORCE_ARISTAS, k, 0.02);
        assert!(movf < mov0, "sin enfriar: {mov0} → {movf}");
        // En caja y con entradas inválidas sin tocar.
        for p in p1.iter().take(FORCE_N) {
            assert!(p[0].abs() <= FORCE_CAJA && p[1].abs() <= FORCE_CAJA);
        }
        let mut mala = [[9.0f64; 2]; 4];
        assert_eq!(force_paso(&mut mala, 257, &FORCE_ARISTAS, k, 0.3), 0.0);
        assert_eq!(mala, [[9.0; 2]; 4]);
    }

    #[test]
    fn unit_distance_doce_aristas() {
        // Red exacta: 12 aristas con la tolerancia del hunt.
        let mut red = [[0.0f64; 2]; UNIT_N];
        for (i, p) in red.iter_mut().enumerate() {
            *p = unit_red(i);
        }
        assert_eq!(cuenta_aristas_unidad(&red, UNIT_N, GRAPH_UNIT_TOL), 12);
        assert_eq!(cuenta_aristas_unidad(&red, UNIT_N, 0.02), 12);
        // Jitter: alguna arista se rompe (hay algo que animar).
        let mut jit = [[0.0f64; 2]; UNIT_N];
        for (i, p) in jit.iter_mut().enumerate() {
            *p = unit_jitter(i);
        }
        assert!(cuenta_aristas_unidad(&jit, UNIT_N, GRAPH_UNIT_TOL) < 12);
        // `arista_unitaria` honesta en bordes.
        assert!(arista_unitaria(1.0, GRAPH_UNIT_TOL));
        assert!(!arista_unitaria(f64::NAN, 0.1));
        assert!(!arista_unitaria(1.0, -0.1));
        assert!(!arista_unitaria(1.5, 0.35));
        assert!(arista_unitaria(1.2, 0.35));
        assert_eq!(cuenta_aristas_unidad(&red, 99, GRAPH_UNIT_TOL), 0);
    }

    #[test]
    fn dijkstra_distancia_y_camino_pineados() {
        let mut dist = [0.0f64; CAMINO_N];
        let mut prev = [0i8; CAMINO_N];
        let mut visit = [0u8; CAMINO_N];
        let got = dijkstra(
            CAMINO_N,
            &CAMINO_ARISTAS,
            &CAMINO_PESOS,
            CAMINO_ORIGEN,
            &mut dist,
            &mut prev,
            &mut visit,
        );
        assert_eq!(got, CAMINO_N);
        assert_eq!(visit, CAMINO_VISITA);
        assert!((dist[CAMINO_DESTINO] - CAMINO_DISTANCIA).abs() < 1e-9);
        let mut camino = [0u8; CAMINO_N];
        let largo = reconstruye_camino(CAMINO_N, &prev, CAMINO_ORIGEN, CAMINO_DESTINO, &mut camino);
        assert_eq!(largo, CAMINO_ESPERADO.len());
        assert_eq!(&camino[..largo], &CAMINO_ESPERADO);
        // Costo del camino == distancia.
        let mut costo = 0.0;
        for par in camino[..largo].windows(2) {
            let (a, b) = (par[0] as usize, par[1] as usize);
            let mut w = f64::NAN;
            for (k, (x, y)) in CAMINO_ARISTAS.iter().enumerate() {
                if (*x as usize == a && *y as usize == b) || (*x as usize == b && *y as usize == a)
                {
                    w = CAMINO_PESOS[k];
                }
            }
            assert!(w.is_finite(), "tramo {a}-{b} sin peso");
            costo += w;
        }
        assert!((costo - CAMINO_DISTANCIA).abs() < 1e-9);
        // Entradas inválidas → 0 honesto.
        let mut d2 = [0.0f64; 4];
        let mut p2 = [0i8; 4];
        let mut v2 = [0u8; 4];
        assert_eq!(
            dijkstra(
                CAMINO_N,
                &CAMINO_ARISTAS,
                &CAMINO_PESOS,
                0,
                &mut d2,
                &mut p2,
                &mut v2
            ),
            0
        );
        assert_eq!(
            reconstruye_camino(CAMINO_N, &prev, CAMINO_ORIGEN, CAMINO_N, &mut camino),
            0
        );
    }

    #[test]
    fn scratch_setup_una_vez_muestreo_sin_allocs() {
        let mut s = Scratch::nuevo();
        s.prepara_moser();
        assert_eq!(s.n, MOSER_N);
        assert_eq!(s.pa.len(), MOSER_N);
        assert_eq!(s.orden, MOSER_BFS);
        assert_eq!(s.color, MOSER_COLORES);
        let p0 = s.pos_en(0, 0.0);
        let p1 = s.pos_en(0, 1.0);
        assert_eq!(p0, p1, "moser estático");
        assert!(s.pos_en(99, 0.5).is_none());
        let mut u = Scratch::nuevo();
        u.prepara_unit();
        let a = u.pos_en(1, 0.0);
        let b = u.pos_en(1, 1.0);
        assert!(a.is_some() && b.is_some());
        if let (Some(a), Some(b)) = (a, b) {
            assert!(dist(a, b) > 0.1, "morph unit quieto");
            assert_eq!(b, [unit_red(1)[0] * 2.0, unit_red(1)[1] * 2.0]);
        }
        let m = u.pos_en(1, 0.5);
        if let (Some(a), Some(b), Some(m)) = (a, b, m) {
            assert!(((m[0] - (a[0] + b[0]) / 2.0).abs() < 1e-9));
        }
        let mut f = Scratch::nuevo();
        f.prepara_force();
        assert_eq!(f.n, FORCE_N);
        let mut c = Scratch::nuevo();
        c.prepara_camino();
        assert_eq!(c.n, CAMINO_N);
        assert_eq!(c.orden, CAMINO_VISITA);
        assert_eq!(c.color, CAMINO_ESPERADO);
        let mut g = Scratch::nuevo();
        g.prepara_bfs();
        assert_eq!(g.nivel, BFS_NIVELES);
    }

    #[test]
    fn fuente_cubre_todas_las_etiquetas_y_valores() {
        for id in TEMPLATE_IDS {
            if let Some(e) = etiqueta_de(id) {
                for ch in e.chars() {
                    let b = mayus_ascii(ch);
                    assert!(b.is_some(), "{id}: char sin mapa {ch:?}");
                    if let Some(byte) = b {
                        assert!(glifo(byte).is_some(), "{id}: sin glifo {ch:?}");
                    }
                }
            }
        }
        for ch in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,:;=+-/()? ".chars() {
            if let Some(b) = mayus_ascii(ch) {
                assert!(glifo(b).is_some(), "sin glifo {ch:?}");
            }
        }
        let vivos = [
            "N=",
            " K=4 (3 NO ALCANZA)",
            "VISITADOS ",
            "IT=",
            " E=",
            "ARISTAS ",
            "D(0,7)=",
        ];
        for v in vivos {
            for ch in v.chars() {
                let b = mayus_ascii(ch);
                assert!(b.is_some(), "vivo sin mapa {ch:?}");
                if let Some(byte) = b {
                    assert!(glifo(byte).is_some(), "vivo sin glifo {ch:?}");
                }
            }
        }
        let mut num = [0u8; 10];
        assert_eq!(formatea_1_decimal(CAMINO_DISTANCIA, &mut num), 4);
        assert_eq!(&num[..4], b"16.0");
        let mut nk = [0u8; 9];
        assert_eq!(formatea_progreso(48, 48, &mut nk), 5);
        assert_eq!(&nk[..5], b"48/48");
        let mut ne = [0u8; 4];
        assert_eq!(formatea_entero(7, &mut ne), 1);
    }

    #[test]
    fn defaults_48_frames_en_80x64() {
        let r = render_moser_frames(80, 64);
        assert!(r.is_ok());
        if let Ok(set) = r {
            assert_eq!(set.len(), 48);
        }
        assert!(render_bfs_frames(80, 64).is_ok());
        assert!(render_force_frames(80, 64).is_ok());
        assert!(render_unit_frames(80, 64).is_ok());
        assert!(render_camino_frames(80, 64).is_ok());
    }

    #[test]
    fn contenido_determinista_anima_y_rotula() {
        for id in TEMPLATE_IDS {
            let a = set_ok(id, 96, 64, 6);
            let b = set_ok(id, 96, 64, 6);
            assert_eq!(a, b, "{id} no determinista");
            let difieren = match (a.first(), a.last()) {
                (Some(primero), Some(ultimo)) => primero.pixels != ultimo.pixels,
                _ => false,
            };
            assert!(difieren, "{id} no anima");
            // Etiqueta: la barra superior tiene píxeles no-fondo.
            if let Some(primero) = a.first() {
                let mut tinta = 0usize;
                for y in 0..20 {
                    for x in 0..96 {
                        if let Some(p) = primero.pixel_en(x, y) {
                            if [p[0], p[1], p[2]] != [FONDO[0], FONDO[1], FONDO[2]]
                                && [p[0], p[1], p[2]] != [BARRA[0], BARRA[1], BARRA[2]]
                            {
                                tinta += 1;
                            }
                        }
                    }
                }
                assert!(tinta > 20, "{id} sin etiqueta visible ({tinta})");
            }
            // Un frame tiene fondo real (no todo negro).
            if let Some(medio) = a.get(3) {
                let fondo_px = medio
                    .pixels
                    .chunks(4)
                    .filter(|p| p[0] == FONDO[0] && p[1] == FONDO[1] && p[2] == FONDO[2]);
                assert!(fondo_px.count() > 0, "{id} sin fondo");
            }
        }
    }

    #[test]
    fn rango_empalma_exact() {
        let total = 8usize;
        for id in TEMPLATE_IDS {
            let completo = set_ok(id, 96, 64, total);
            let r1 = render_graphs_rango(id, 96, 64, total, 0, 3);
            let r2 = render_graphs_rango(id, 96, 64, total, 3, 5);
            assert!(r1.is_ok() && r2.is_ok());
            if let (Ok(a), Ok(b)) = (r1, r2) {
                let mut pegado = a;
                pegado.extend(b);
                assert_eq!(pegado, completo, "{id} rango != completo");
            }
        }
    }

    #[test]
    fn errores_honestos() {
        assert!(render_graphs_frames("taylor-series", 96, 64, 4).is_err());
        assert!(render_graphs_frames(TEMPLATE_IDS[0], 0, 64, 4).is_err());
        assert!(render_graphs_frames(TEMPLATE_IDS[0], 96, 64, 0).is_err());
        assert!(render_graphs_frames(TEMPLATE_IDS[0], 96, 64, 1501).is_err());
        assert!(render_graphs_frames(TEMPLATE_IDS[0], 4096, 4096, 64).is_err());
        assert!(render_graphs_rango(TEMPLATE_IDS[0], 96, 64, 8, 5, 4).is_err());
        assert!(render_graphs_rango(TEMPLATE_IDS[0], 96, 64, 8, 0, 0).is_err());
        let r = RgbaFrame::nuevo(63, 64);
        assert!(r.is_err());
        let r = RgbaFrame::nuevo(96, 64);
        assert!(r.is_ok());
        if let Ok(f) = r {
            assert!(f.pixel_en(96, 0).is_none());
            assert!(f.pixel_en(0, 64).is_none());
            assert!(f.pixel_en(0, 0).is_some());
        }
    }
}
