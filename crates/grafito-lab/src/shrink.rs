//! `shrink`: achicado greedy preservando no-k-coloreabilidad.
//!
//! Port de `lab/shrink.py` (105L), `lab/symI_shrink2.py` (209L) y la parte
//! de achicado de `lab/hypL_minimize.py` (255L): loops que quitan vértices
//! (o lotes) y re-chequean no-k-coloreabilidad, conservando el cambio solo
//! si el invariante UNSAT se mantiene.
//!
//! Desvíos documentados respecto a los `.py`:
//! - Oráculo: `is_k_colorable_bruteforce` en vez de kissat vía CNF. Límite
//!   honesto: `n <= MAX_SHRINK_POINTS` (24) y `k <= MAX_SHRINK_K` (8); para
//!   más, exportar DIMACS con `export_dimacs_kcoloring` y usar SAT externo.
//! - `grafito-mcp::sat::run_solver` es invocable como lib pero spawnea
//!   procesos (kissat/cadical); acá no se usa: shrink es puro, sin I/O ni
//!   spawn, y los tests nunca tocan red ni procesos.
//! - `symI` usa CNF con symbreak solo para acelerar al solver; con
//!   backtracking exacto el symbreak no cambia el veredicto, así que
//!   `shrink_two_phase` porta solo la estructura en 2 fases (lotes + fina).
//! - Sin presupuestos de wall-clock (no deterministas): `max_tests` acota
//!   los checks del oráculo, `max_passes` las pasadas a punto fijo.
//! - Shuffle determinista propio (xorshift64 + Fisher-Yates): el orden
//!   difiere del `random.Random(seed)` de Python, así que la paridad se
//!   pinea solo en fixtures cuyo resultado no depende del orden
//!   (`tests/parity_shrink.rs`).
//!
//! Presupuestos:
//!
//! | Constante | Valor | Origen |
//! |---|---|---|
//! | `MAX_SHRINK_POINTS` | 24 | backtracking (`MAX_BRUTE_N`) |
//! | `MAX_SHRINK_K` | 8 | backtracking (`search.rs`) |
//! | `MAX_SHRINK_TESTS` | 200 | `--budget 200` (`shrink.py`) |
//! | `MAX_SHRINK_PASSES` | 32 | tope pasadas punto fijo (hypL fina) |
//! | `DEFAULT_SHRINK_BATCH` | 8 | lote grueso (symI/hypL) |
//! | `SHRINK_UNIT_TOL` | 1e-9 | tolerancia unit-distance |

use grafito_geometry::search::{is_k_colorable_bruteforce, unit_graph_edges, MAX_BRUTE_N};
use grafito_geometry::Point2;
use std::collections::BTreeSet;

/// Límite de vértices: el oráculo es backtracking exacto.
pub const MAX_SHRINK_POINTS: usize = MAX_BRUTE_N;
/// Límite de colores del backtracking (ver `search.rs`).
pub const MAX_SHRINK_K: usize = 8;
/// Presupuesto default de checks del oráculo.
pub const MAX_SHRINK_TESTS: usize = 200;
/// Tope de pasadas de la fase fina hasta punto fijo.
pub const MAX_SHRINK_PASSES: usize = 32;
/// Tamaño default del lote grueso (symI/hypL).
pub const DEFAULT_SHRINK_BATCH: usize = 8;
/// Tolerancia para derivar aristas unit-distance de puntos.
pub const SHRINK_UNIT_TOL: f64 = 1e-9;

/// Configuración del achicado (todos los campos con default sensato).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShrinkConfig {
    /// Semilla del shuffle determinista (los `.py` usan 1).
    pub seed: u64,
    /// Tope de checks del oráculo (base y validación final no cuentan).
    pub max_tests: usize,
    /// Tamaño del lote grueso (`shrink_two_phase`, `shrink_hypl`).
    pub batch: usize,
    /// Tope de pasadas de la fase fina (`shrink_hypl`).
    pub max_passes: usize,
}

impl Default for ShrinkConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            max_tests: MAX_SHRINK_TESTS,
            batch: DEFAULT_SHRINK_BATCH,
            max_passes: MAX_SHRINK_PASSES,
        }
    }
}

/// Reporte del achicado. `kept` son índices originales ordenados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShrinkReport {
    /// Vértices conservados (índices originales, ordenados).
    pub kept: Vec<usize>,
    /// Checks del oráculo (sin contar base ni validación final).
    pub tests: usize,
    /// La base ya era no-k-coloreable (si no, no hay nada que minimizar).
    pub base_unsat: bool,
    /// El resultado sigue no-k-coloreable (falso si la base era SAT).
    pub invariant: bool,
    /// Removidos en fases por lotes / inicial (gruesa symI, fases 1-2 hypL).
    pub removed_coarse: usize,
    /// Removidos en fase fina vértice por vértice.
    pub removed_fine: usize,
}

impl ShrinkReport {
    /// Cantidad de vértices removidos sobre `n` original.
    #[must_use]
    pub fn removed(&self, n: usize) -> usize {
        n.saturating_sub(self.kept.len())
    }
}

/// Error honesto del achicado (sin pánicos ni fallbacks silenciosos).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShrinkError {
    /// Sin vértices de entrada.
    #[error("shrink: sin vértices")]
    Empty,
    /// `k` debe ser >= 1.
    #[error("shrink: k debe ser >= 1")]
    BadK,
    /// Excede el backtracking: usar DIMACS + SAT externo.
    #[error("shrink: n={n} excede el máximo {MAX_SHRINK_POINTS}; exportá DIMACS y usá kissat")]
    TooLarge {
        /// Cantidad de vértices pedida.
        n: usize,
    },
    /// Excede el backtracking en colores.
    #[error("shrink: k={k} excede backtracking {MAX_SHRINK_K}; exportá DIMACS y usá kissat")]
    KTooLarge {
        /// Colores pedidos.
        k: usize,
    },
    /// El oráculo rechazó la entrada (aristas inválidas, puntos no finitos).
    #[error("shrink: oráculo: {0}")]
    Oracle(String),
}

fn validate(n: usize, k: usize) -> Result<(), ShrinkError> {
    if n == 0 {
        return Err(ShrinkError::Empty);
    }
    if k == 0 {
        return Err(ShrinkError::BadK);
    }
    if n > MAX_SHRINK_POINTS {
        return Err(ShrinkError::TooLarge { n });
    }
    if k > MAX_SHRINK_K {
        return Err(ShrinkError::KTooLarge { k });
    }
    Ok(())
}

/// `true` si el grafo inducido NO es k-coloreable (invariante a preservar).
fn non_colorable(n: usize, edges: &[(usize, usize)], k: usize) -> Result<bool, ShrinkError> {
    is_k_colorable_bruteforce(n, edges, k)
        .map(|colorable| !colorable)
        .map_err(|e| ShrinkError::Oracle(e.0))
}

/// Aristas del subgrafo inducido por `alive`, reindexadas a `[0, n')`.
fn induced(edges: &[(usize, usize)], alive: &BTreeSet<usize>) -> Vec<(usize, usize)> {
    let keep: Vec<usize> = alive.iter().copied().collect();
    let mut out = Vec::new();
    for (a, b) in edges {
        if let (Ok(ia), Ok(ib)) = (keep.binary_search(a), keep.binary_search(b)) {
            out.push((ia, ib));
        }
    }
    out
}

/// Fisher-Yates con xorshift64 (determinista, sin deps).
fn shuffle(order: &mut [usize], seed: u64) {
    let mut state = seed | 1;
    let mut i = order.len();
    while i > 1 {
        let mut x = state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        state = x;
        i -= 1;
        let j = (x % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
}

/// Estado de trabajo del achicado.
struct Work {
    alive: BTreeSet<usize>,
    tests: usize,
    removed_coarse: usize,
    removed_fine: usize,
}

/// Intenta quitar `block` de `alive`; conserva solo si sigue UNSAT.
/// Falso (conservador, como hypL ante TIMEOUT) si el oráculo falla o si el
/// trial queda vacío (CNF vacía = SAT) o `block` ya no está en `alive`.
/// Cuenta un test solo cuando realmente consulta al oráculo.
fn try_remove(
    block: &[usize],
    edges: &[(usize, usize)],
    k: usize,
    config: &ShrinkConfig,
    work: &mut Work,
    coarse: bool,
) -> bool {
    if work.tests >= config.max_tests {
        return false;
    }
    let victims: BTreeSet<usize> = block.iter().copied().collect();
    if victims.is_disjoint(&work.alive) {
        return false;
    }
    let trial: BTreeSet<usize> = work.alive.difference(&victims).copied().collect();
    if trial.is_empty() {
        return false;
    }
    let te = induced(edges, &trial);
    work.tests += 1;
    if !matches!(non_colorable(trial.len(), &te, k), Ok(true)) {
        return false;
    }
    let gone = work.alive.len() - trial.len();
    work.alive = trial;
    if coarse {
        work.removed_coarse += gone;
    } else {
        work.removed_fine += gone;
    }
    true
}

/// Reporte cuando la base ya es k-coloreable: nada que minimizar.
fn base_sat(n: usize) -> ShrinkReport {
    ShrinkReport {
        kept: (0..n).collect(),
        tests: 0,
        base_unsat: false,
        invariant: false,
        removed_coarse: 0,
        removed_fine: 0,
    }
}

/// Reporte final con validación del invariante (como symI).
fn finish(edges: &[(usize, usize)], k: usize, work: &Work) -> Result<ShrinkReport, ShrinkError> {
    let kept: Vec<usize> = work.alive.iter().copied().collect();
    let te = induced(edges, &work.alive);
    let invariant = non_colorable(kept.len(), &te, k)?;
    Ok(ShrinkReport {
        kept,
        tests: work.tests,
        base_unsat: true,
        invariant,
        removed_coarse: work.removed_coarse,
        removed_fine: work.removed_fine,
    })
}

/// Entrada por puntos (nombre pedido): deriva aristas unit-distance y achica
/// con una pasada greedy (`lab/shrink.py`). `kept` indexa `points`.
pub fn shrink_preserving_non_kcolorable(
    points: &[Point2],
    k: usize,
    config: ShrinkConfig,
) -> Result<ShrinkReport, ShrinkError> {
    validate(points.len(), k)?;
    let edges = unit_graph_edges(points, SHRINK_UNIT_TOL).map_err(|e| ShrinkError::Oracle(e.0))?;
    shrink_greedy(points.len(), &edges, k, config)
}

/// Una pasada greedy en orden aleatorio (`lab/shrink.py::shrink`).
pub fn shrink_greedy(
    n: usize,
    edges: &[(usize, usize)],
    k: usize,
    config: ShrinkConfig,
) -> Result<ShrinkReport, ShrinkError> {
    validate(n, k)?;
    if !non_colorable(n, edges, k)? {
        return Ok(base_sat(n));
    }
    let mut work = Work {
        alive: (0..n).collect(),
        tests: 0,
        removed_coarse: 0,
        removed_fine: 0,
    };
    let mut order: Vec<usize> = work.alive.iter().copied().collect();
    shuffle(&mut order, config.seed);
    for v in order {
        if work.tests >= config.max_tests {
            break;
        }
        try_remove(&[v], edges, k, &config, &mut work, false);
    }
    finish(edges, k, &work)
}

/// Dos fases: lotes de `batch` con reshuffle mientras haya progreso, luego
/// pasada fina vértice por vértice (`lab/symI_shrink2.py::shrink2`).
pub fn shrink_two_phase(
    n: usize,
    edges: &[(usize, usize)],
    k: usize,
    config: ShrinkConfig,
) -> Result<ShrinkReport, ShrinkError> {
    validate(n, k)?;
    if !non_colorable(n, edges, k)? {
        return Ok(base_sat(n));
    }
    let mut work = Work {
        alive: (0..n).collect(),
        tests: 0,
        removed_coarse: 0,
        removed_fine: 0,
    };
    // Fase gruesa: rondas de lotes; cada ronda con progreso quita >= 1,
    // así que termina en <= n rondas.
    let mut round: u64 = 0;
    loop {
        let mut order: Vec<usize> = work.alive.iter().copied().collect();
        shuffle(&mut order, config.seed.wrapping_add(round));
        round += 1;
        let mut progress = false;
        for lot in order.chunks(config.batch.max(1)) {
            if work.tests >= config.max_tests {
                break;
            }
            let set: BTreeSet<usize> = lot.iter().copied().collect();
            if set == work.alive {
                continue; // sin vaciado total (symI).
            }
            if try_remove(lot, edges, k, &config, &mut work, true) {
                progress = true;
            }
        }
        if !progress || work.tests >= config.max_tests {
            break;
        }
    }
    // Fase fina: una pasada vértice por vértice.
    let mut order: Vec<usize> = work.alive.iter().copied().collect();
    shuffle(&mut order, config.seed.wrapping_add(round));
    for v in order {
        if work.tests >= config.max_tests {
            break;
        }
        try_remove(&[v], edges, k, &config, &mut work, false);
    }
    finish(edges, k, &work)
}

/// Fase 1 greedy ascendente con quite inmediato, fase 2 por bloques con
/// subdivisión recursiva, fase fina hasta punto fijo (parte de shrink de
/// `lab/hypL_minimize.py`). Remociones de fases 1-2 cuentan como gruesas.
pub fn shrink_hypl(
    n: usize,
    edges: &[(usize, usize)],
    k: usize,
    config: ShrinkConfig,
) -> Result<ShrinkReport, ShrinkError> {
    validate(n, k)?;
    if !non_colorable(n, edges, k)? {
        return Ok(base_sat(n));
    }
    let mut work = Work {
        alive: (0..n).collect(),
        tests: 0,
        removed_coarse: 0,
        removed_fine: 0,
    };
    // Fase 1: vértice por vértice ascendente, quite en el acto.
    for v in 0..n {
        if work.tests >= config.max_tests {
            break;
        }
        try_remove(&[v], edges, k, &config, &mut work, true);
    }
    // Fase 2: bloques de `batch` con subdivisión 8-4-2-1 recursiva.
    let rest: Vec<usize> = work.alive.iter().copied().collect();
    for block in rest.chunks(config.batch.max(1)) {
        process_block(block, edges, k, &config, &mut work);
    }
    // Fase fina: pasadas hasta punto fijo o tope.
    let mut passes = 0;
    while passes < config.max_passes {
        let mut moved = 0;
        let order: Vec<usize> = work.alive.iter().copied().collect();
        for v in &order {
            if work.tests >= config.max_tests {
                break;
            }
            if try_remove(&[*v], edges, k, &config, &mut work, false) {
                moved += 1;
            }
        }
        passes += 1;
        if moved == 0 || work.tests >= config.max_tests {
            break;
        }
    }
    finish(edges, k, &work)
}

/// Bloque hypL: si sale entero van todos juntos; si no, se subdivide.
/// Singletons críticos se quedan.
fn process_block(
    block: &[usize],
    edges: &[(usize, usize)],
    k: usize,
    config: &ShrinkConfig,
    work: &mut Work,
) {
    if block.is_empty() || work.tests >= config.max_tests {
        return;
    }
    if try_remove(block, edges, k, config, work, true) {
        return;
    }
    if block.len() == 1 {
        return;
    }
    let half = block.len() / 2;
    process_block(&block[..half], edges, k, config, work);
    process_block(&block[half..], edges, k, config, work);
}
