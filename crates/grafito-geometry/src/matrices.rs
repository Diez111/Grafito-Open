use std::fmt;

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
}
