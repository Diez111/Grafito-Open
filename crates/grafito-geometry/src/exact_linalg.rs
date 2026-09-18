//! Álgebra lineal exacta (Ola 2.5) sobre [`ExactRational`] (`i128`).
//!
//! Determinante, inversa y RREF sin errores de redondeo para matrices con
//! entradas decimales exactas simples, hasta [`MAX_EXACT_DIM`] por lado.
//! Cualquier operación que no cabe en `i128` devuelve
//! [`ExactLinalgError::Overflow`]; nunca se degrada a `f64` en silencio (el
//! llamador decide el fallback).

use crate::exact::ExactRational;

/// Lado máximo de una matriz exacta (mismo espíritu que las cotas del motor:
/// acota el costo cúbico y el desborde de `i128`).
pub const MAX_EXACT_DIM: usize = 32;

/// Error tipado del álgebra exacta.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactLinalgError {
    /// La dimensión excede [`MAX_EXACT_DIM`].
    Dimension { rows: usize, cols: usize },
    /// La operación exige matriz cuadrada.
    NotSquare { rows: usize, cols: usize },
    /// La matriz no es invertible.
    NotInvertible,
    /// El resultado exacto no cabe en los racionales `i128`.
    Overflow,
}

impl std::fmt::Display for ExactLinalgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dimension { rows, cols } => write!(
                f,
                "matriz {rows}x{cols} excede el máximo exacto {MAX_EXACT_DIM}x{MAX_EXACT_DIM}"
            ),
            Self::NotSquare { rows, cols } => {
                write!(f, "se requiere matriz cuadrada ({rows}x{cols})")
            }
            Self::NotInvertible => f.write_str("la matriz no es invertible"),
            Self::Overflow => f.write_str("el resultado exacto excede los enteros i128"),
        }
    }
}

impl std::error::Error for ExactLinalgError {}

/// Matriz exacta densa (fila mayor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactMatrix {
    rows: usize,
    cols: usize,
    data: Vec<ExactRational>,
}

impl ExactMatrix {
    /// Construye validando dimensiones y cota.
    pub fn new(
        rows: usize,
        cols: usize,
        data: Vec<ExactRational>,
    ) -> Result<Self, ExactLinalgError> {
        if rows > MAX_EXACT_DIM || cols > MAX_EXACT_DIM {
            return Err(ExactLinalgError::Dimension { rows, cols });
        }
        if data.len() != rows.saturating_mul(cols) {
            return Err(ExactLinalgError::Dimension { rows, cols });
        }
        Ok(Self { rows, cols, data })
    }

    /// Convierte desde `f64` solo si cada entrada es un decimal exacto simple;
    /// `None` no es error: el llamador usa la vía numérica.
    pub fn from_f64_rows(rows: &[Vec<f64>]) -> Option<Self> {
        let row_count = rows.len();
        if row_count == 0 || row_count > MAX_EXACT_DIM {
            return None;
        }
        let col_count = rows[0].len();
        if col_count == 0 || col_count > MAX_EXACT_DIM {
            return None;
        }
        if rows.iter().any(|row| row.len() != col_count) {
            return None;
        }
        let mut data = Vec::with_capacity(row_count * col_count);
        for row in rows {
            for value in row {
                data.push(ExactRational::from_f64_decimal(*value)?);
            }
        }
        Self::new(row_count, col_count, data).ok()
    }

    /// Dimensión `(filas, columnas)`.
    pub const fn dims(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    /// Elemento `(r, c)`; fuera de rango → cero (la eliminación no lo usa).
    pub fn get(&self, r: usize, c: usize) -> ExactRational {
        if r >= self.rows || c >= self.cols {
            return ExactRational::zero();
        }
        self.data[r * self.cols + c]
    }

    fn set(&mut self, r: usize, c: usize, value: ExactRational) {
        self.data[r * self.cols + c] = value;
    }

    fn swap_rows(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        for c in 0..self.cols {
            let left = self.get(a, c);
            let right = self.get(b, c);
            self.set(a, c, right);
            self.set(b, c, left);
        }
    }

    fn identity(n: usize) -> Result<Self, ExactLinalgError> {
        let mut data = vec![ExactRational::zero(); n * n];
        for i in 0..n {
            data[i * n + i] = ExactRational::one();
        }
        Self::new(n, n, data)
    }

    /// Determinante por eliminación exacta (filas). `-0` no existe acá.
    pub fn determinant(&self) -> Result<ExactRational, ExactLinalgError> {
        if self.rows != self.cols {
            return Err(ExactLinalgError::NotSquare {
                rows: self.rows,
                cols: self.cols,
            });
        }
        let n = self.rows;
        let mut work = self.clone();
        let mut det = ExactRational::one();
        for pivot in 0..n {
            let mut chosen = None;
            for row in pivot..n {
                if !work.get(row, pivot).is_zero() {
                    chosen = Some(row);
                    break;
                }
            }
            let Some(row) = chosen else {
                return Ok(ExactRational::zero());
            };
            if row != pivot {
                work.swap_rows(pivot, row);
                det = det.checked_neg().map_err(|_| ExactLinalgError::Overflow)?;
            }
            let pivot_value = work.get(pivot, pivot);
            det = det
                .checked_mul(pivot_value)
                .map_err(|_| ExactLinalgError::Overflow)?;
            for row in (pivot + 1)..n {
                let factor = work
                    .get(row, pivot)
                    .checked_div(pivot_value)
                    .map_err(|_| ExactLinalgError::Overflow)?;
                if factor.is_zero() {
                    continue;
                }
                for col in pivot..n {
                    let value = work
                        .get(row, col)
                        .checked_sub(
                            factor
                                .checked_mul(work.get(pivot, col))
                                .map_err(|_| ExactLinalgError::Overflow)?,
                        )
                        .map_err(|_| ExactLinalgError::Overflow)?;
                    work.set(row, col, value);
                }
            }
        }
        Ok(det)
    }

    /// Inversa por Gauss-Jordan exacta.
    pub fn inverse(&self) -> Result<Self, ExactLinalgError> {
        if self.rows != self.cols {
            return Err(ExactLinalgError::NotSquare {
                rows: self.rows,
                cols: self.cols,
            });
        }
        let n = self.rows;
        let mut work = self.clone();
        let mut inverse = Self::identity(n)?;
        for pivot in 0..n {
            let mut chosen = None;
            for row in pivot..n {
                if !work.get(row, pivot).is_zero() {
                    chosen = Some(row);
                    break;
                }
            }
            let Some(row) = chosen else {
                return Err(ExactLinalgError::NotInvertible);
            };
            if row != pivot {
                work.swap_rows(pivot, row);
                inverse.swap_rows(pivot, row);
            }
            let pivot_value = work.get(pivot, pivot);
            let inverse_pivot = ExactRational::one()
                .checked_div(pivot_value)
                .map_err(|_| ExactLinalgError::Overflow)?;
            for col in 0..n {
                let value = work
                    .get(pivot, col)
                    .checked_mul(inverse_pivot)
                    .map_err(|_| ExactLinalgError::Overflow)?;
                work.set(pivot, col, value);
                let value = inverse
                    .get(pivot, col)
                    .checked_mul(inverse_pivot)
                    .map_err(|_| ExactLinalgError::Overflow)?;
                inverse.set(pivot, col, value);
            }
            for row in 0..n {
                if row == pivot {
                    continue;
                }
                let factor = work.get(row, pivot);
                if factor.is_zero() {
                    continue;
                }
                for col in 0..n {
                    let value = work
                        .get(row, col)
                        .checked_sub(
                            factor
                                .checked_mul(work.get(pivot, col))
                                .map_err(|_| ExactLinalgError::Overflow)?,
                        )
                        .map_err(|_| ExactLinalgError::Overflow)?;
                    work.set(row, col, value);
                    let value = inverse
                        .get(row, col)
                        .checked_sub(
                            factor
                                .checked_mul(inverse.get(pivot, col))
                                .map_err(|_| ExactLinalgError::Overflow)?,
                        )
                        .map_err(|_| ExactLinalgError::Overflow)?;
                    inverse.set(row, col, value);
                }
            }
        }
        Ok(inverse)
    }

    /// Forma escalonada reducida exacta (RREF).
    pub fn rref(&self) -> Result<Self, ExactLinalgError> {
        let mut work = self.clone();
        let mut lead = 0usize;
        for row in 0..work.rows {
            if lead >= work.cols {
                break;
            }
            let mut pivot_row = row;
            while work.get(pivot_row, lead).is_zero() {
                pivot_row += 1;
                if pivot_row == work.rows {
                    pivot_row = row;
                    lead += 1;
                    if lead == work.cols {
                        return Ok(work);
                    }
                }
            }
            work.swap_rows(row, pivot_row);
            let pivot_value = work.get(row, lead);
            let inverse_pivot = ExactRational::one()
                .checked_div(pivot_value)
                .map_err(|_| ExactLinalgError::Overflow)?;
            for col in 0..work.cols {
                let value = work
                    .get(row, col)
                    .checked_mul(inverse_pivot)
                    .map_err(|_| ExactLinalgError::Overflow)?;
                work.set(row, col, value);
            }
            for other in 0..work.rows {
                if other == row {
                    continue;
                }
                let factor = work.get(other, lead);
                if factor.is_zero() {
                    continue;
                }
                for col in 0..work.cols {
                    let value = work
                        .get(other, col)
                        .checked_sub(
                            factor
                                .checked_mul(work.get(row, col))
                                .map_err(|_| ExactLinalgError::Overflow)?,
                        )
                        .map_err(|_| ExactLinalgError::Overflow)?;
                    work.set(other, col, value);
                }
            }
            lead += 1;
        }
        Ok(work)
    }

    /// Producto exacto `self · other` (validando dimensiones y overflow).
    pub fn multiply(&self, other: &Self) -> Result<Self, ExactLinalgError> {
        if self.cols != other.rows {
            return Err(ExactLinalgError::Dimension {
                rows: other.rows,
                cols: self.cols,
            });
        }
        let mut data = vec![ExactRational::zero(); self.rows * other.cols];
        for row in 0..self.rows {
            for col in 0..other.cols {
                let mut acc = ExactRational::zero();
                for k in 0..self.cols {
                    let product = self
                        .get(row, k)
                        .checked_mul(other.get(k, col))
                        .map_err(|_| ExactLinalgError::Overflow)?;
                    acc = acc
                        .checked_add(product)
                        .map_err(|_| ExactLinalgError::Overflow)?;
                }
                data[row * other.cols + col] = acc;
            }
        }
        Self::new(self.rows, other.cols, data)
    }

    /// Render multilínea estilo matriz `{{a, b}, {c, d}}` con racionales.
    pub fn to_display_string(&self) -> String {
        let mut rows = Vec::with_capacity(self.rows);
        for r in 0..self.rows {
            let mut entries = Vec::with_capacity(self.cols);
            for c in 0..self.cols {
                entries.push(self.get(r, c).to_string());
            }
            rows.push(format!("{{{}}}", entries.join(", ")));
        }
        format!("{{{}}}", rows.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matrix(rows: &[&[i128]]) -> ExactMatrix {
        let row_count = rows.len();
        let col_count = rows[0].len();
        let data = rows
            .iter()
            .flat_map(|row| row.iter().map(|value| ExactRational::from(*value)))
            .collect();
        ExactMatrix::new(row_count, col_count, data).expect("matriz válida")
    }

    #[test]
    fn determinant_exacto_2x2_y_3x3() {
        let m = matrix(&[&[1, 2], &[3, 4]]);
        assert_eq!(m.determinant(), Ok(ExactRational::from(-2)));
        let n = matrix(&[&[2, 0, 1], &[1, 3, 2], &[1, 1, 1]]);
        // 2*(3-2) - 0 + 1*(1-3) = 2 - 2 = 0
        assert_eq!(n.determinant(), Ok(ExactRational::zero()));
        let i = matrix(&[&[2, 0, 0], &[0, 3, 0], &[0, 0, 5]]);
        assert_eq!(i.determinant(), Ok(ExactRational::from(30)));
    }

    #[test]
    fn inverse_exacta_con_fracciones() {
        let m = matrix(&[&[1, 2], &[3, 4]]);
        let inverse = m.inverse().expect("invertible");
        assert_eq!(
            inverse.to_display_string(),
            "{{-2, 1}, {3/2, -1/2}}",
            "inversa exacta"
        );
        // A · A⁻¹ = I (verificación exacta).
        let product = m.multiply(&inverse).expect("producto");
        assert_eq!(product.to_display_string(), "{{1, 0}, {0, 1}}");
    }

    #[test]
    fn singular_no_tiene_inversa_y_det_cero() {
        let m = matrix(&[&[1, 2], &[2, 4]]);
        assert_eq!(m.determinant(), Ok(ExactRational::zero()));
        assert_eq!(m.inverse(), Err(ExactLinalgError::NotInvertible));
    }

    #[test]
    fn rref_exacta() {
        let m = matrix(&[&[1, 2, 3], &[4, 5, 6]]);
        assert_eq!(
            m.rref().expect("rref").to_display_string(),
            "{{1, 0, -1}, {0, 1, 2}}"
        );
    }

    #[test]
    fn cota_de_dimension_y_conversion() {
        let big = vec![vec![0.0; MAX_EXACT_DIM + 1]; MAX_EXACT_DIM + 1];
        assert_eq!(
            ExactMatrix::from_f64_rows(&big),
            None,
            "33×33 fuera de cota"
        );
        let decimals = vec![vec![0.5, 0.25], vec![0.1, 1.0]];
        let converted = ExactMatrix::from_f64_rows(&decimals).expect("decimales exactos");
        assert_eq!(
            converted.get(0, 0),
            ExactRational::from_f64_decimal(0.5).unwrap()
        );
        // 1/3 no cierra en 12 decimales: cae a la vía numérica.
        assert!(ExactMatrix::from_f64_rows(&[vec![1.0 / 3.0]]).is_none());
    }
}
