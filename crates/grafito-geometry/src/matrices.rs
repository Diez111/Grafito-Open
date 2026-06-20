use std::fmt;

use nalgebra::{linalg::SymmetricEigen, DMatrix};

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

impl Matrix {
    pub fn new(rows: usize, cols: usize, data: Vec<f64>) -> Option<Self> {
        if data.len() != rows * cols {
            return None;
        }
        Some(Self { rows, cols, data })
    }

    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            data: vec![0.0; rows * cols],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n, n);
        for i in 0..n {
            m.data[i * n + i] = 1.0;
        }
        m
    }

    pub fn from_rows(rows: Vec<Vec<f64>>) -> Option<Self> {
        if rows.is_empty() {
            return None;
        }
        let r = rows.len();
        let c = rows[0].len();
        if rows.iter().any(|row| row.len() != c) {
            return None;
        }
        let data: Vec<f64> = rows.into_iter().flatten().collect();
        Some(Self {
            rows: r,
            cols: c,
            data,
        })
    }

    pub fn get(&self, r: usize, c: usize) -> f64 {
        debug_assert!(
            r < self.rows && c < self.cols,
            "Matrix::get index ({}, {}) out of bounds for {}x{} matrix",
            r,
            c,
            self.rows,
            self.cols
        );
        self.data[r * self.cols + c]
    }

    pub fn set(&mut self, r: usize, c: usize, val: f64) {
        debug_assert!(
            r < self.rows && c < self.cols,
            "Matrix::set index ({}, {}) out of bounds for {}x{} matrix",
            r,
            c,
            self.rows,
            self.cols
        );
        self.data[r * self.cols + c] = val;
    }

    /// Versión segura de `get` que retorna `None` si los índices están fuera de rango.
    pub fn checked_get(&self, r: usize, c: usize) -> Option<f64> {
        if r < self.rows && c < self.cols {
            Some(self.data[r * self.cols + c])
        } else {
            None
        }
    }

    /// Versión segura de `set` que retorna `false` si los índices están fuera de rango.
    pub fn checked_set(&mut self, r: usize, c: usize, val: f64) -> bool {
        if r < self.rows && c < self.cols {
            self.data[r * self.cols + c] = val;
            true
        } else {
            false
        }
    }

    pub fn add(&self, other: &Matrix) -> Option<Matrix> {
        if self.rows != other.rows || self.cols != other.cols {
            return None;
        }
        let data: Vec<f64> = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(a, b)| a + b)
            .collect();
        Some(Matrix {
            rows: self.rows,
            cols: self.cols,
            data,
        })
    }

    pub fn sub(&self, other: &Matrix) -> Option<Matrix> {
        if self.rows != other.rows || self.cols != other.cols {
            return None;
        }
        let data: Vec<f64> = self
            .data
            .iter()
            .zip(&other.data)
            .map(|(a, b)| a - b)
            .collect();
        Some(Matrix {
            rows: self.rows,
            cols: self.cols,
            data,
        })
    }

    pub fn mul(&self, other: &Matrix) -> Option<Matrix> {
        if self.cols != other.rows {
            return None;
        }
        let mut result = Matrix::zeros(self.rows, other.cols);
        for i in 0..self.rows {
            for j in 0..other.cols {
                let mut sum = 0.0;
                for k in 0..self.cols {
                    sum += self.get(i, k) * other.get(k, j);
                }
                result.set(i, j, sum);
            }
        }
        Some(result)
    }

    pub fn scale(&self, s: f64) -> Matrix {
        let data: Vec<f64> = self.data.iter().map(|v| v * s).collect();
        Matrix {
            rows: self.rows,
            cols: self.cols,
            data,
        }
    }

    pub fn transpose(&self) -> Matrix {
        let mut result = Matrix::zeros(self.cols, self.rows);
        for i in 0..self.rows {
            for j in 0..self.cols {
                result.set(j, i, self.get(i, j));
            }
        }
        result
    }

    pub fn determinant(&self) -> Option<f64> {
        if self.rows != self.cols {
            return None;
        }
        let n = self.rows;
        if n == 1 {
            return Some(self.data[0]);
        }
        if n == 2 {
            return Some(self.get(0, 0) * self.get(1, 1) - self.get(0, 1) * self.get(1, 0));
        }
        if n == 3 {
            return Some(
                self.get(0, 0)
                    * (self.get(1, 1) * self.get(2, 2) - self.get(1, 2) * self.get(2, 1))
                    - self.get(0, 1)
                        * (self.get(1, 0) * self.get(2, 2) - self.get(1, 2) * self.get(2, 0))
                    + self.get(0, 2)
                        * (self.get(1, 0) * self.get(2, 1) - self.get(1, 1) * self.get(2, 0)),
            );
        }
        let mut lu = self.clone();
        let mut sign = 1.0;
        for col in 0..n {
            let mut max_row = col;
            for row in (col + 1)..n {
                if lu.get(row, col).abs() > lu.get(max_row, col).abs() {
                    max_row = row;
                }
            }
            if max_row != col {
                for j in 0..n {
                    let tmp = lu.get(col, j);
                    lu.set(col, j, lu.get(max_row, j));
                    lu.set(max_row, j, tmp);
                }
                sign *= -1.0;
            }
            if lu.get(col, col).abs() < 1e-15 {
                return Some(0.0);
            }
            for row in (col + 1)..n {
                let factor = lu.get(row, col) / lu.get(col, col);
                for j in col..n {
                    let v = lu.get(row, j) - factor * lu.get(col, j);
                    lu.set(row, j, v);
                }
            }
        }
        let mut det = sign;
        for i in 0..n {
            det *= lu.get(i, i);
        }
        Some(det)
    }

    pub fn inverse(&self) -> Option<Matrix> {
        if self.rows != self.cols {
            return None;
        }
        let n = self.rows;
        let mut aug = Matrix::zeros(n, 2 * n);
        for i in 0..n {
            for j in 0..n {
                aug.set(i, j, self.get(i, j));
            }
            aug.set(i, n + i, 1.0);
        }
        for col in 0..n {
            let mut max_row = col;
            for row in (col + 1)..n {
                if aug.get(row, col).abs() > aug.get(max_row, col).abs() {
                    max_row = row;
                }
            }
            aug.data.swap_ranges(
                col * 2 * n..(col + 1) * 2 * n,
                max_row * 2 * n..(max_row + 1) * 2 * n,
            );
            let pivot = aug.get(col, col);
            if pivot.abs() < 1e-15 {
                return None;
            }
            for j in 0..2 * n {
                aug.set(col, j, aug.get(col, j) / pivot);
            }
            for row in 0..n {
                if row == col {
                    continue;
                }
                let factor = aug.get(row, col);
                for j in 0..2 * n {
                    let v = aug.get(row, j) - factor * aug.get(col, j);
                    aug.set(row, j, v);
                }
            }
        }
        let mut result = Matrix::zeros(n, n);
        for i in 0..n {
            for j in 0..n {
                result.set(i, j, aug.get(i, n + j));
            }
        }
        Some(result)
    }

    pub fn trace(&self) -> Option<f64> {
        if self.rows != self.cols {
            return None;
        }
        Some((0..self.rows).map(|i| self.get(i, i)).sum())
    }

    pub fn row(&self, r: usize) -> Vec<f64> {
        (0..self.cols).map(|c| self.get(r, c)).collect()
    }

    pub fn col(&self, c: usize) -> Vec<f64> {
        (0..self.rows).map(|r| self.get(r, c)).collect()
    }
}

trait SwapRanges<T> {
    fn swap_ranges(&mut self, r1: std::ops::Range<usize>, r2: std::ops::Range<usize>);
}

impl<T: Copy> SwapRanges<T> for Vec<T> {
    fn swap_ranges(&mut self, r1: std::ops::Range<usize>, r2: std::ops::Range<usize>) {
        let len = r1.len().min(r2.len());
        for i in 0..len {
            self.swap(r1.start + i, r2.start + i);
        }
    }
}

pub fn solve_linear_system(a: &Matrix, b: &Matrix) -> Option<Matrix> {
    if a.rows != a.cols || b.rows != a.rows {
        return None;
    }
    let n = a.rows;
    let m = b.cols;
    let mut aug = Matrix::zeros(n, n + m);
    for i in 0..n {
        for j in 0..n {
            aug.set(i, j, a.get(i, j));
        }
        for j in 0..m {
            aug.set(i, n + j, b.get(i, j));
        }
    }
    for col in 0..n {
        let mut max_row = col;
        for row in (col + 1)..n {
            if aug.get(row, col).abs() > aug.get(max_row, col).abs() {
                max_row = row;
            }
        }
        aug.data.swap_ranges(
            col * (n + m)..(col + 1) * (n + m),
            max_row * (n + m)..(max_row + 1) * (n + m),
        );
        let pivot = aug.get(col, col);
        if pivot.abs() < 1e-15 {
            return None;
        }
        for j in 0..(n + m) {
            aug.set(col, j, aug.get(col, j) / pivot);
        }
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug.get(row, col);
            for j in 0..(n + m) {
                let v = aug.get(row, j) - factor * aug.get(col, j);
                aug.set(row, j, v);
            }
        }
    }
    let mut result = Matrix::zeros(n, m);
    for i in 0..n {
        for j in 0..m {
            result.set(i, j, aug.get(i, n + j));
        }
    }
    Some(result)
}

// ============================================================================
// Operaciones avanzadas de álgebra lineal (implementadas sobre nalgebra)
// ============================================================================

/// Convierte una `Matrix` (row-major) en una `nalgebra::DMatrix<f64>`.
fn to_nalgebra(m: &Matrix) -> DMatrix<f64> {
    DMatrix::from_row_slice(m.rows, m.cols, &m.data)
}

/// Convierte una `nalgebra::DMatrix<f64>` de vuelta a `Matrix` (row-major).
fn from_nalgebra(nmat: &DMatrix<f64>) -> Matrix {
    let rows = nmat.nrows();
    let cols = nmat.ncols();
    let mut data = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            data.push(nmat[(i, j)]);
        }
    }
    Matrix { rows, cols, data }
}

/// Comprueba si una matriz densa es simétrica (dentro de una tolerancia
/// relativa a su mayor valor absoluto).
fn is_symmetric_dense(m: &DMatrix<f64>) -> bool {
    let n = m.nrows();
    if m.ncols() != n || n == 0 {
        return false;
    }
    let scale = m.amax().max(1.0);
    let tol = 1e-12 * scale;
    for i in 0..n {
        for j in (i + 1)..n {
            if (m[(i, j)] - m[(j, i)]).abs() > tol {
                return false;
            }
        }
    }
    true
}

/// Base del espacio nulo de una matriz densa vía SVD: vectores fila de V^T
/// asociados a valores singulares cercanos a cero. Cada vector tiene longitud
/// igual al número de columnas de `m`.
fn null_space_basis_dense(m: &DMatrix<f64>, tol: f64) -> Vec<Vec<f64>> {
    let ncols = m.ncols();
    if ncols == 0 || m.nrows() == 0 {
        return Vec::new();
    }
    let svd = m.clone().svd(false, true);
    let v_t = match svd.v_t {
        Some(ref v) => v,
        None => return Vec::new(),
    };
    let svs = &svd.singular_values;
    let max_sv = svs.iter().copied().fold(0.0f64, f64::max).max(1e-300);
    let thr = tol.max(1e-12 * max_sv);
    let mut basis = Vec::new();
    for i in 0..svs.len() {
        if svs[i].abs() <= thr {
            let vec: Vec<f64> = (0..ncols).map(|j| v_t[(i, j)]).collect();
            basis.push(vec);
        }
    }
    basis
}

/// Autovalores de una matriz cuadrada como pares `(parte_real, parte_imag)`.
/// Usa la descomposición de Schur de nalgebra, que soporta matrices generales
/// (reales y complejas conjugadas). Devuelve `None` si la matriz no es cuadrada.
pub fn eigenvalues(m: &Matrix) -> Option<Vec<(f64, f64)>> {
    if m.rows != m.cols || m.rows == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let eig = nmat.complex_eigenvalues();
    Some(eig.iter().map(|c| (c.re, c.im)).collect())
}

/// Autovectores con sus autovalores asociados: `(vector, parte_real, parte_imag)`.
/// Para matrices simétricas usa `SymmetricEigen` (autovalores y autovectores
/// reales, normalizados). Para matrices generales, cada autovalor real obtiene
/// su autovector del espacio nulo de `(A - λI)`; para autovalores complejos
/// `λ = a + bi` se resuelve el sistema real de `2n × 2n`
/// `[A-aI, bI; -bI, A-aI]` y se devuelve la parte real `u` del autovector
/// complejo `u + w·i`.
pub fn eigenvectors(m: &Matrix) -> Option<Vec<(Vec<f64>, f64, f64)>> {
    if m.rows != m.cols || m.rows == 0 {
        return None;
    }
    let n = m.rows;
    let nmat = to_nalgebra(m);

    if is_symmetric_dense(&nmat) {
        let sym = SymmetricEigen::new(nmat);
        let mut result = Vec::with_capacity(n);
        for i in 0..n {
            let vec: Vec<f64> = (0..n).map(|r| sym.eigenvectors[(r, i)]).collect();
            let val = sym.eigenvalues[i];
            result.push((vec, val, 0.0));
        }
        return Some(result);
    }

    let eig = nmat.complex_eigenvalues();
    let tol = 1e-9;
    let mut result = Vec::with_capacity(n);
    for c in &eig {
        let re = c.re;
        let im = c.im;
        if im.abs() < tol {
            let mut a_shifted = nmat.clone();
            for i in 0..n {
                a_shifted[(i, i)] -= re;
            }
            let basis = null_space_basis_dense(&a_shifted, tol);
            let vec = basis.into_iter().next().unwrap_or_else(|| vec![0.0; n]);
            result.push((vec, re, 0.0));
        } else {
            // Sistema real 2n×2n para el par complejo conjugado.
            let mut big = DMatrix::zeros(2 * n, 2 * n);
            for i in 0..n {
                for j in 0..n {
                    let aij = nmat[(i, j)];
                    big[(i, j)] = aij - if i == j { re } else { 0.0 };
                    big[(i, n + j)] = if i == j { im } else { 0.0 };
                    big[(n + i, j)] = if i == j { -im } else { 0.0 };
                    big[(n + i, n + j)] = aij - if i == j { re } else { 0.0 };
                }
            }
            let basis = null_space_basis_dense(&big, tol);
            let vec = if let Some(full) = basis.into_iter().next() {
                full.into_iter().take(n).collect()
            } else {
                vec![0.0; n]
            };
            result.push((vec, re, im));
        }
    }
    Some(result)
}

/// Descomposición en valores singulares: `(U, Σ, V^T)`.
/// `Σ` se devuelve como vector de valores singulares (orden descendente).
pub fn svd(m: &Matrix) -> Option<(Matrix, Vec<f64>, Matrix)> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let svd = nmat.svd(true, true);
    let u = svd.u?;
    let v_t = svd.v_t?;
    let sigma: Vec<f64> = svd.singular_values.iter().copied().collect();
    Some((from_nalgebra(&u), sigma, from_nalgebra(&v_t)))
}

/// Descomposición LU (con pivoteo parcial de nalgebra): `(L, U)`.
/// `L` es triangular inferior con diagonal unidad y `U` triangular superior,
/// de modo que `P·A = L·U` para alguna permutación `P`. Devuelve `None` si la
/// matriz no es cuadrada.
pub fn lu_decomposition(m: &Matrix) -> Option<(Matrix, Matrix)> {
    if m.rows != m.cols || m.rows == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let lu = nmat.lu();
    Some((from_nalgebra(&lu.l()), from_nalgebra(&lu.u())))
}

/// Descomposición QR: `(Q, R)` con `Q` ortogonal y `R` triangular superior.
pub fn qr_decomposition(m: &Matrix) -> Option<(Matrix, Matrix)> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let qr = nmat.qr();
    Some((from_nalgebra(&qr.q()), from_nalgebra(&qr.r())))
}

/// Factorización de Cholesky: `L` triangular inferior tal que `A = L·L^T`.
/// Devuelve `None` si la matriz no es simétrica definida positiva.
pub fn cholesky(m: &Matrix) -> Option<Matrix> {
    if m.rows != m.cols || m.rows == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let chol = nmat.cholesky()?;
    Some(from_nalgebra(&chol.l()))
}

/// Rango numérico de la matriz (número de valores singulares significativos).
pub fn rank(m: &Matrix) -> Option<usize> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let svd = nmat.svd(false, false);
    let svs = &svd.singular_values;
    let max_sv = svs.iter().copied().fold(0.0f64, f64::max).max(1e-300);
    let thr = 1e-9 * max_sv;
    Some(svs.iter().filter(|s| s.abs() > thr).count())
}

/// Norma de Frobenius: `sqrt(Σ a_ij^2)`.
pub fn norm_frobenius(m: &Matrix) -> f64 {
    if m.rows == 0 || m.cols == 0 {
        return 0.0;
    }
    to_nalgebra(m).norm()
}

/// Norma espectral (mayor valor singular). Devuelve `None` para matrices vacías.
pub fn norm_2(m: &Matrix) -> Option<f64> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let svd = nmat.svd(false, false);
    let svs = &svd.singular_values;
    if svs.is_empty() {
        return None;
    }
    Some(svs[0].abs())
}

/// Número de condición `σ_max / σ_min`. Para matrices singulares
/// (`σ_min ≈ 0`) devuelve `f64::INFINITY`.
pub fn condition_number(m: &Matrix) -> Option<f64> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    let svd = nmat.svd(false, false);
    let svs = &svd.singular_values;
    if svs.is_empty() {
        return None;
    }
    let max_sv = svs[0].abs();
    let min_sv = svs.iter().map(|s| s.abs()).fold(f64::INFINITY, f64::min);
    let thr = 1e-12 * max_sv.max(1.0);
    if min_sv <= thr {
        Some(f64::INFINITY)
    } else {
        Some(max_sv / min_sv)
    }
}

/// Espacio nulo (kernel) de la matriz: base ortonormal de vectores `x` tales
/// que `A·x = 0`, obtenida de los vectores singulares derechos asociados a
/// valores singulares cercanos a cero.
pub fn null_space(m: &Matrix) -> Option<Vec<Vec<f64>>> {
    if m.rows == 0 || m.cols == 0 {
        return None;
    }
    let nmat = to_nalgebra(m);
    Some(null_space_basis_dense(&nmat, 1e-9))
}

pub fn taylor_series(expr: &str, var: &str, center: f64, order: usize) -> Option<String> {
    use crate::ast::parse_ast;
    let ast = parse_ast(expr).ok()?;
    let mut terms = Vec::new();
    let mut current = ast.clone();
    let mut factorial = 1.0f64;
    for n in 0..=order {
        if n > 0 {
            factorial *= n as f64;
        }
        let coeff = current.eval_at(var, center) / factorial;
        if coeff.abs() > 1e-12 {
            let term = if (center).abs() < 1e-12 {
                if n == 0 {
                    format_coeff(coeff)
                } else if n == 1 {
                    format!("{}*{}", format_coeff(coeff), var)
                } else {
                    format!("{}*{}^{}", format_coeff(coeff), var, n)
                }
            } else {
                if n == 0 {
                    format_coeff(coeff)
                } else if n == 1 {
                    format!("{}*({}-{})", format_coeff(coeff), var, format_f64(center))
                } else {
                    format!(
                        "{}*({}-{})^{}",
                        format_coeff(coeff),
                        var,
                        format_f64(center),
                        n
                    )
                }
            };
            terms.push(term);
        }
        if n < order {
            current = current.diff(var);
        }
    }
    if terms.is_empty() {
        return Some("0".to_string());
    }
    Some(terms.join(" + ").replace("+ -", "- "))
}

fn format_coeff(c: f64) -> String {
    if (c - c.round()).abs() < 1e-10 {
        format!("{}", c.round() as i64)
    } else {
        format!("{:.6}", c)
    }
}

fn format_f64(v: f64) -> String {
    if (v - v.round()).abs() < 1e-10 {
        format!("{}", v.round() as i64)
    } else {
        format!("{:.6}", v)
    }
}

impl fmt::Display for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.rows {
            write!(f, "[")?;
            for j in 0..self.cols {
                if j > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{:.4}", self.get(i, j))?;
            }
            writeln!(f, "]")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        let m = Matrix::identity(3);
        assert_eq!(m.get(0, 0), 1.0);
        assert_eq!(m.get(1, 1), 1.0);
        assert_eq!(m.get(0, 1), 0.0);
    }

    #[test]
    fn test_determinant_2x2() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        assert_eq!(m.determinant(), Some(-2.0));
    }

    #[test]
    fn test_determinant_3x3() {
        let m = Matrix::from_rows(vec![
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            vec![7.0, 8.0, 10.0],
        ])
        .unwrap();
        let det = m.determinant().unwrap();
        assert!((det - (-3.0)).abs() < 1e-10);
    }

    #[test]
    fn test_inverse() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        let inv = m.inverse().unwrap();
        let prod = m.mul(&inv).unwrap();
        assert!((prod.get(0, 0) - 1.0).abs() < 1e-10);
        assert!((prod.get(1, 1) - 1.0).abs() < 1e-10);
        assert!((prod.get(0, 1)).abs() < 1e-10);
    }

    #[test]
    fn test_solve_system() {
        let a = Matrix::from_rows(vec![vec![2.0, 1.0], vec![1.0, 3.0]]).unwrap();
        let b = Matrix::from_rows(vec![vec![5.0], vec![10.0]]).unwrap();
        let x = solve_linear_system(&a, &b).unwrap();
        assert!((x.get(0, 0) - 1.0).abs() < 1e-10);
        assert!((x.get(1, 0) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_matrix_mul() {
        let a = Matrix::from_rows(vec![vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        let b = Matrix::from_rows(vec![vec![5.0, 6.0], vec![7.0, 8.0]]).unwrap();
        let c = a.mul(&b).unwrap();
        assert_eq!(c.get(0, 0), 19.0);
        assert_eq!(c.get(0, 1), 22.0);
        assert_eq!(c.get(1, 0), 43.0);
        assert_eq!(c.get(1, 1), 50.0);
    }

    #[test]
    fn test_eigenvalues_diagonal() {
        let m = Matrix::from_rows(vec![vec![2.0, 0.0], vec![0.0, 3.0]]).unwrap();
        let eig = eigenvalues(&m).unwrap();
        assert_eq!(eig.len(), 2);
        let has_2 = eig
            .iter()
            .any(|(re, im)| (re - 2.0).abs() < 1e-8 && im.abs() < 1e-8);
        let has_3 = eig
            .iter()
            .any(|(re, im)| (re - 3.0).abs() < 1e-8 && im.abs() < 1e-8);
        assert!(
            has_2 && has_3,
            "esperaba autovalores 2 y 3, obtuvo {:?}",
            eig
        );
    }

    #[test]
    fn test_eigenvalues_nonsquare() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]).unwrap();
        assert_eq!(eigenvalues(&m), None);
    }

    #[test]
    fn test_eigenvectors_diagonal() {
        let m = Matrix::from_rows(vec![vec![2.0, 0.0], vec![0.0, 3.0]]).unwrap();
        let vecs = eigenvectors(&m).unwrap();
        assert_eq!(vecs.len(), 2);
        let for_2 = vecs
            .iter()
            .find(|(_, re, im)| (re - 2.0).abs() < 1e-8 && im.abs() < 1e-8)
            .unwrap();
        assert!(
            for_2.0[1].abs() < 1e-8 && for_2.0[0].abs() > 0.9,
            "autovector de 2 no paralelo a [1,0]: {:?}",
            for_2.0
        );
        let for_3 = vecs
            .iter()
            .find(|(_, re, im)| (re - 3.0).abs() < 1e-8 && im.abs() < 1e-8)
            .unwrap();
        assert!(
            for_3.0[0].abs() < 1e-8 && for_3.0[1].abs() > 0.9,
            "autovector de 3 no paralelo a [0,1]: {:?}",
            for_3.0
        );
    }

    #[test]
    fn test_svd() {
        let m = Matrix::from_rows(vec![vec![3.0, 0.0], vec![0.0, 4.0]]).unwrap();
        let (u, sigma, vt) = svd(&m).unwrap();
        assert_eq!(sigma.len(), 2);
        // σ descendentes
        assert!(sigma[0] >= sigma[1]);
        // Reconstrucción U·Σ·V^T ≈ A
        let k = sigma.len();
        let mut s = Matrix::zeros(u.cols, vt.rows);
        for (i, &val) in sigma.iter().enumerate().take(k) {
            s.set(i, i, val);
        }
        let recon = u.mul(&s).unwrap().mul(&vt).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (recon.get(i, j) - m.get(i, j)).abs() < 1e-8,
                    "reconstrucción SVD mismatch en ({},{}): {} vs {}",
                    i,
                    j,
                    recon.get(i, j),
                    m.get(i, j)
                );
            }
        }
    }

    #[test]
    fn test_lu_decomposition() {
        let m = Matrix::from_rows(vec![vec![4.0, 3.0], vec![6.0, 3.0]]).unwrap();
        let (l, u) = lu_decomposition(&m).unwrap();
        // L triangular inferior con diagonal unidad
        for i in 0..2 {
            assert!((l.get(i, i) - 1.0).abs() < 1e-10, "L diagonal no es 1");
            for j in (i + 1)..2 {
                assert!(l.get(i, j).abs() < 1e-10, "L no es triangular inferior");
            }
        }
        // U triangular superior
        for i in 0..2 {
            for j in 0..i {
                assert!(u.get(i, j).abs() < 1e-10, "U no es triangular superior");
            }
        }
        // |det(U)| == |det(A)| (det(L)=1, det(P)=±1)
        let det_u = u.get(0, 0) * u.get(1, 1) - u.get(0, 1) * u.get(1, 0);
        let det_a = m.determinant().unwrap();
        assert!(
            (det_u.abs() - det_a.abs()).abs() < 1e-8,
            "|det(U)|={} no coincide con |det(A)|={}",
            det_u.abs(),
            det_a.abs()
        );
    }

    #[test]
    fn test_qr_decomposition() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        let (q, r) = qr_decomposition(&m).unwrap();
        // Q·R = A
        let recon = q.mul(&r).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (recon.get(i, j) - m.get(i, j)).abs() < 1e-8,
                    "QR no reconstruye A"
                );
            }
        }
        // Q ortogonal: Q^T·Q = I
        let qtq = q.transpose().mul(&q).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((qtq.get(i, j) - expected).abs() < 1e-8, "Q no es ortogonal");
            }
        }
    }

    #[test]
    fn test_cholesky() {
        let m = Matrix::from_rows(vec![vec![4.0, 2.0], vec![2.0, 3.0]]).unwrap();
        let l = cholesky(&m).unwrap();
        // L triangular inferior
        for i in 0..2 {
            for j in (i + 1)..2 {
                assert!(l.get(i, j).abs() < 1e-10, "L no es triangular inferior");
            }
        }
        // L·L^T = A
        let recon = l.mul(&l.transpose()).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (recon.get(i, j) - m.get(i, j)).abs() < 1e-8,
                    "Cholesky L·L^T != A"
                );
            }
        }
    }

    #[test]
    fn test_cholesky_not_spd() {
        // Matriz no definida positiva (autovalor negativo)
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 1.0]]).unwrap();
        assert!(cholesky(&m).is_none());
    }

    #[test]
    fn test_rank() {
        let singular = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();
        assert_eq!(rank(&singular), Some(1));
        let full = Matrix::identity(3);
        assert_eq!(rank(&full), Some(3));
    }

    #[test]
    fn test_condition_number_singular() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();
        let cn = condition_number(&m).unwrap();
        assert!(
            cn.is_infinite(),
            "esperaba número de condición infinito, obtuvo {}",
            cn
        );
    }

    #[test]
    fn test_condition_number_well_conditioned() {
        let m = Matrix::identity(3);
        let cn = condition_number(&m).unwrap();
        assert!(
            (cn - 1.0).abs() < 1e-8,
            "identidad debe tener κ=1, obtuvo {}",
            cn
        );
    }

    #[test]
    fn test_null_space() {
        let m = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).unwrap();
        let ns = null_space(&m).unwrap();
        assert_eq!(ns.len(), 1, "esperaba 1 vector en el espacio nulo");
        let v = &ns[0];
        // A·v ≈ 0
        let mv0 = m.get(0, 0) * v[0] + m.get(0, 1) * v[1];
        let mv1 = m.get(1, 0) * v[0] + m.get(1, 1) * v[1];
        assert!(
            mv0.abs() < 1e-8 && mv1.abs() < 1e-8,
            "vector no está en el kernel: A·v = ({}, {})",
            mv0,
            mv1
        );
        // Paralelo a [2, -1]  =>  v[0] + 2·v[1] ≈ 0
        assert!(
            (v[0] + 2.0 * v[1]).abs() < 1e-8,
            "vector del kernel no paralelo a [2,-1]: {:?}",
            v
        );
    }

    #[test]
    fn test_null_space_full_rank() {
        let m = Matrix::identity(3);
        let ns = null_space(&m).unwrap();
        assert!(ns.is_empty(), "identidad debe tener kernel vacío");
    }

    #[test]
    fn test_norm_frobenius() {
        let m = Matrix::identity(3);
        let n = norm_frobenius(&m);
        assert!(
            (n - 3.0_f64.sqrt()).abs() < 1e-10,
            "Frobenius(I_3) != sqrt(3)"
        );
    }

    #[test]
    fn test_norm_2() {
        let m = Matrix::from_rows(vec![vec![3.0, 0.0], vec![0.0, 4.0]]).unwrap();
        let n = norm_2(&m).unwrap();
        assert!(
            (n - 4.0).abs() < 1e-8,
            "norma espectral esperaba 4, obtuvo {}",
            n
        );
    }
}
