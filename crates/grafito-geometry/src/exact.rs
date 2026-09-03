//! Aritmética racional exacta con enteros `i128` normalizados.
//!
//! [`ExactRational`] es exacto dentro del rango finito de `i128`; no pretende
//! ser una implementación de precisión arbitraria. Las operaciones que no
//! caben en ese rango devuelven [`ExactRationalError::Overflow`] en lugar de
//! degradarse silenciosamente a `f64`.

use std::cmp::Ordering;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

/// Errores de construcción y aritmética de [`ExactRational`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactRationalError {
    /// El denominador de una fracción no puede ser cero.
    ZeroDenominator,
    /// Dividir por el racional cero no está definido.
    DivisionByZero,
    /// El resultado exacto no cabe en los enteros `i128` acotados.
    Overflow,
    /// La entrada no tiene la forma de entero o fracción `numerador/denominador`.
    InvalidFormat,
}

impl fmt::Display for ExactRationalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroDenominator => "el denominador no puede ser cero",
            Self::DivisionByZero => "división por el racional cero",
            Self::Overflow => "el resultado no cabe en un racional i128",
            Self::InvalidFormat => "se esperaba un entero o una fracción n/d",
        };
        f.write_str(message)
    }
}

impl Error for ExactRationalError {}

/// Racional exacto reducido con denominador estrictamente positivo.
///
/// La representación usa `i128`, por lo que los resultados fuera de ese rango
/// se rechazan con [`ExactRationalError::Overflow`]. Use esta clase cuando el
/// rango acotado sea aceptable; no sustituye una biblioteca de enteros grandes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExactRational {
    numerator: i128,
    denominator: i128,
}

impl ExactRational {
    /// Construye y normaliza `numerator / denominator`.
    pub fn new(numerator: i128, denominator: i128) -> Result<Self, ExactRationalError> {
        if denominator == 0 {
            return Err(ExactRationalError::ZeroDenominator);
        }
        if numerator == 0 {
            return Ok(Self::zero());
        }

        let divisor = gcd(numerator.unsigned_abs(), denominator.unsigned_abs());
        let (mut numerator, mut denominator) = if divisor == (1_u128 << 127) {
            // The only non-representable magnitude is |i128::MIN|. Reaching
            // this branch means both values are i128::MIN, so both reduce to -1.
            debug_assert_eq!(numerator, i128::MIN);
            debug_assert_eq!(denominator, i128::MIN);
            (-1, -1)
        } else {
            let divisor = divisor as i128;
            (numerator / divisor, denominator / divisor)
        };

        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?;
            denominator = denominator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?;
        }

        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// El racional cero.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    /// El racional uno.
    #[must_use]
    pub const fn one() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }

    /// Numerador reducido.
    #[must_use]
    pub const fn numerator(self) -> i128 {
        self.numerator
    }

    /// Denominador reducido y siempre positivo.
    #[must_use]
    pub const fn denominator(self) -> i128 {
        self.denominator
    }

    /// Indica si el racional es exactamente cero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    /// Suma dos racionales verificando cada operación intermedia.
    pub fn checked_add(self, other: Self) -> Result<Self, ExactRationalError> {
        self.checked_add_or_sub(other, false)
    }

    /// Resta dos racionales verificando cada operación intermedia.
    pub fn checked_sub(self, other: Self) -> Result<Self, ExactRationalError> {
        self.checked_add_or_sub(other, true)
    }

    /// Multiplica dos racionales tras cancelar factores cruzados.
    pub fn checked_mul(self, other: Self) -> Result<Self, ExactRationalError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Self::zero());
        }

        let left_factor = gcd(self.numerator.unsigned_abs(), other.denominator as u128) as i128;
        let right_factor = gcd(other.numerator.unsigned_abs(), self.denominator as u128) as i128;
        let numerator = (self.numerator / left_factor)
            .checked_mul(other.numerator / right_factor)
            .ok_or(ExactRationalError::Overflow)?;
        let denominator = (self.denominator / right_factor)
            .checked_mul(other.denominator / left_factor)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    /// Divide dos racionales tras cancelar factores cruzados.
    pub fn checked_div(self, other: Self) -> Result<Self, ExactRationalError> {
        if other.is_zero() {
            return Err(ExactRationalError::DivisionByZero);
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }

        let numerator_factor = gcd(
            self.numerator.unsigned_abs(),
            other.numerator.unsigned_abs(),
        );
        let denominator_factor = gcd(other.denominator as u128, self.denominator as u128) as i128;
        let numerator = divide_by_gcd(self.numerator, numerator_factor)?
            .checked_mul(other.denominator / denominator_factor)
            .ok_or(ExactRationalError::Overflow)?;
        let denominator = (self.denominator / denominator_factor)
            .checked_mul(divide_by_gcd(other.numerator, numerator_factor)?)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    /// Niega el racional sin convertirlo a punto flotante.
    pub fn checked_neg(self) -> Result<Self, ExactRationalError> {
        Self::new(
            self.numerator
                .checked_neg()
                .ok_or(ExactRationalError::Overflow)?,
            self.denominator,
        )
    }

    fn checked_add_or_sub(self, other: Self, subtract: bool) -> Result<Self, ExactRationalError> {
        let divisor = gcd(self.denominator as u128, other.denominator as u128) as i128;
        let left_multiplier = other.denominator / divisor;
        let right_multiplier = self.denominator / divisor;
        let left = self
            .numerator
            .checked_mul(left_multiplier)
            .ok_or(ExactRationalError::Overflow)?;
        let right = other
            .numerator
            .checked_mul(right_multiplier)
            .ok_or(ExactRationalError::Overflow)?;
        let numerator = if subtract {
            left.checked_sub(right)
        } else {
            left.checked_add(right)
        }
        .ok_or(ExactRationalError::Overflow)?;
        let denominator = right_multiplier
            .checked_mul(other.denominator)
            .ok_or(ExactRationalError::Overflow)?;
        Self::new(numerator, denominator)
    }

    fn cmp_positive(
        mut left_numerator: u128,
        mut left_denominator: u128,
        mut right_numerator: u128,
        mut right_denominator: u128,
    ) -> Ordering {
        // Continued fractions compare fractions exactly without multiplying two
        // potentially large i128 values.
        let mut reversed = false;
        loop {
            let left_integer = left_numerator / left_denominator;
            let right_integer = right_numerator / right_denominator;
            if left_integer != right_integer {
                let ordering = left_integer.cmp(&right_integer);
                return if reversed {
                    ordering.reverse()
                } else {
                    ordering
                };
            }

            let left_remainder = left_numerator % left_denominator;
            let right_remainder = right_numerator % right_denominator;
            if left_remainder == 0 || right_remainder == 0 {
                let ordering = match (left_remainder == 0, right_remainder == 0) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => unreachable!("covered by the condition above"),
                };
                return if reversed {
                    ordering.reverse()
                } else {
                    ordering
                };
            }

            left_numerator = left_denominator;
            left_denominator = left_remainder;
            right_numerator = right_denominator;
            right_denominator = right_remainder;
            reversed = !reversed;
        }
    }
}

impl From<i128> for ExactRational {
    fn from(value: i128) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }
}

impl FromStr for ExactRational {
    type Err = ExactRationalError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ExactRationalError::InvalidFormat);
        }

        match input.split_once('/') {
            Some((numerator, denominator)) if !denominator.contains('/') => {
                let numerator = numerator
                    .trim()
                    .parse::<i128>()
                    .map_err(|_| ExactRationalError::InvalidFormat)?;
                let denominator = denominator
                    .trim()
                    .parse::<i128>()
                    .map_err(|_| ExactRationalError::InvalidFormat)?;
                Self::new(numerator, denominator)
            }
            Some(_) => Err(ExactRationalError::InvalidFormat),
            None => input
                .parse::<i128>()
                .map(Self::from)
                .map_err(|_| ExactRationalError::InvalidFormat),
        }
    }
}

impl fmt::Display for ExactRational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(f, "{}", self.numerator)
        } else {
            write!(f, "{}/{}", self.numerator, self.denominator)
        }
    }
}

impl Ord for ExactRational {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.numerator.is_negative(), other.numerator.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => Self::cmp_positive(
                self.numerator as u128,
                self.denominator as u128,
                other.numerator as u128,
                other.denominator as u128,
            ),
            (true, true) => Self::cmp_positive(
                other.numerator.unsigned_abs(),
                other.denominator as u128,
                self.numerator.unsigned_abs(),
                self.denominator as u128,
            ),
        }
    }
}

impl PartialOrd for ExactRational {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn divide_by_gcd(value: i128, divisor: u128) -> Result<i128, ExactRationalError> {
    if divisor <= i128::MAX as u128 {
        Ok(value / divisor as i128)
    } else if value == i128::MIN && divisor == (1_u128 << 127) {
        Ok(-1)
    } else {
        Err(ExactRationalError::Overflow)
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

// ── Cónicas helpers — fórmulas exactas ──────────────────────────────────────
// Estas funciones encapsulan la geometría exacta de las construcciones
// EllipseByFoci / HyperbolaByFoci / ParabolaByFocusDirectrix para que
// `grafito-command` pueda exponer getters puros (Focus, Center, ...)
// sin duplicar fórmulas. Todas las funciones son puras, validan finitud y
// usan `f64` con tolerancia `1e-12`; el sufijo `exact` indica que no hay
// muestreo ni aproximación iterativa más allá de `sqrt`/`hypot`.

use crate::Point2;

/// Tolerancia geométrica mínima para considerar una cónica degenerada.
const CONIC_EPS: f64 = 1e-12;

/// Centro de una elipse definida por sus dos focos (punto medio exacto).
#[must_use]
pub fn ellipse_center(focus1: Point2, focus2: Point2) -> Point2 {
    Point2::new(
        focus1.x * 0.5 + focus2.x * 0.5,
        focus1.y * 0.5 + focus2.y * 0.5,
    )
}

/// Semiejes `(a, b)` de una elipse definida por focos y un punto de la cónica.
/// `a = (|P-F1|+|P-F2|)/2`, `c = |F1-F2|/2`, `b = sqrt(a²-c²)`. Devuelve `None` si es degenerada.
#[must_use]
pub fn ellipse_axes(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<(f64, f64)> {
    let dist_f = focus1.distance(&focus2);
    let d1 = point_on.distance(&focus1);
    let d2 = point_on.distance(&focus2);
    if !dist_f.is_finite() || !d1.is_finite() || !d2.is_finite() {
        return None;
    }
    let a = (d1 + d2) * 0.5;
    let c = dist_f * 0.5;
    if !a.is_finite() || !c.is_finite() || a <= c + CONIC_EPS {
        return None;
    }
    let b2 = (a - c) * (a + c);
    if b2 <= CONIC_EPS * CONIC_EPS || !b2.is_finite() {
        return None;
    }
    let b = b2.sqrt();
    if !b.is_finite() || b <= CONIC_EPS {
        return None;
    }
    Some((a, b))
}

/// Excentricidad de elipse `e = c / a`.
#[must_use]
pub fn ellipse_eccentricity(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<f64> {
    let (a, _) = ellipse_axes(focus1, focus2, point_on)?;
    let c = focus1.distance(&focus2) * 0.5;
    if a <= CONIC_EPS || !c.is_finite() {
        return None;
    }
    let e = c / a;
    if !e.is_finite() || !(0.0..1.0).contains(&e) {
        return None;
    }
    Some(e)
}

/// Focos de una elipse (identidad trivial, expuesta como helper puro).
#[must_use]
pub fn ellipse_foci(focus1: Point2, focus2: Point2) -> (Point2, Point2) {
    (focus1, focus2)
}

/// Focos derivados de un `EllipseObj` geométrico (centro, rx, ry, ángulo).
/// Usa la dirección del eje mayor para colocar los focos a distancia `c`.
#[must_use]
pub fn ellipse_obj_foci(center: Point2, rx: f64, ry: f64, angle: f64) -> Option<(Point2, Point2)> {
    if !center.x.is_finite()
        || !center.y.is_finite()
        || !rx.is_finite()
        || !ry.is_finite()
        || !angle.is_finite()
        || rx <= CONIC_EPS
        || ry <= CONIC_EPS
    {
        return None;
    }
    let (a, b, dir_x, dir_y) = if rx >= ry {
        (rx, ry, angle.cos(), angle.sin())
    } else {
        (ry, rx, (-angle).sin(), angle.cos())
    };
    let c2 = (a - b) * (a + b);
    if c2 <= CONIC_EPS * CONIC_EPS || !c2.is_finite() {
        // Círculo: focos coincidentes con el centro.
        return Some((center, center));
    }
    let c = c2.sqrt();
    if !c.is_finite() {
        return None;
    }
    let f1 = Point2::new(center.x + c * dir_x, center.y + c * dir_y);
    let f2 = Point2::new(center.x - c * dir_x, center.y - c * dir_y);
    if !f1.x.is_finite() || !f1.y.is_finite() || !f2.x.is_finite() || !f2.y.is_finite() {
        return None;
    }
    Some((f1, f2))
}

/// Centro de hipérbola definida por focos (punto medio).
#[must_use]
pub fn hyperbola_center(focus1: Point2, focus2: Point2) -> Point2 {
    ellipse_center(focus1, focus2)
}

/// Semiejes `(a, b)` de hipérbola `a = |d1-d2|/2`, `c = |F1-F2|/2`, `b = sqrt(c²-a²)`.
#[must_use]
pub fn hyperbola_axes(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<(f64, f64)> {
    let dist_f = focus1.distance(&focus2);
    let d1 = point_on.distance(&focus1);
    let d2 = point_on.distance(&focus2);
    if !dist_f.is_finite() || !d1.is_finite() || !d2.is_finite() {
        return None;
    }
    let a = (d1 - d2).abs() * 0.5;
    let c = dist_f * 0.5;
    if !a.is_finite() || !c.is_finite() || a <= CONIC_EPS || a >= c - CONIC_EPS {
        return None;
    }
    let b2 = (c - a) * (c + a);
    if b2 <= CONIC_EPS * CONIC_EPS || !b2.is_finite() {
        return None;
    }
    let b = b2.sqrt();
    if !b.is_finite() || b <= CONIC_EPS {
        return None;
    }
    Some((a, b))
}

/// Excentricidad de hipérbola `e = c / a > 1`.
#[must_use]
pub fn hyperbola_eccentricity(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<f64> {
    let (a, _) = hyperbola_axes(focus1, focus2, point_on)?;
    let c = focus1.distance(&focus2) * 0.5;
    let e = c / a;
    if !e.is_finite() || e <= 1.0 {
        return None;
    }
    Some(e)
}

/// Focos de hipérbola (identidad).
#[must_use]
pub fn hyperbola_foci(focus1: Point2, focus2: Point2) -> (Point2, Point2) {
    (focus1, focus2)
}

/// Focos derivados de un `HyperbolaObj` (centro, a, b, ángulo, eje horizontal).
#[must_use]
pub fn hyperbola_obj_foci(center: Point2, a: f64, b: f64, angle: f64) -> Option<(Point2, Point2)> {
    if !center.x.is_finite()
        || !center.y.is_finite()
        || !a.is_finite()
        || !b.is_finite()
        || !angle.is_finite()
        || a <= CONIC_EPS
        || b <= CONIC_EPS
    {
        return None;
    }
    let c2 = a * a + b * b;
    if !c2.is_finite() {
        return None;
    }
    let c = c2.sqrt();
    if !c.is_finite() {
        return None;
    }
    let dir_x = angle.cos();
    let dir_y = angle.sin();
    let f1 = Point2::new(center.x + c * dir_x, center.y + c * dir_y);
    let f2 = Point2::new(center.x - c * dir_x, center.y - c * dir_y);
    if !f1.x.is_finite() || !f1.y.is_finite() || !f2.x.is_finite() || !f2.y.is_finite() {
        return None;
    }
    Some((f1, f2))
}

// ── Parábola ────────────────────────────────────────────────────────────────

fn project_point_to_line_exact(point: Point2, line_a: Point2, line_b: Point2) -> Option<Point2> {
    let dx = line_b.x - line_a.x;
    let dy = line_b.y - line_a.y;
    let len2 = dx * dx + dy * dy;
    if !dx.is_finite() || !dy.is_finite() || !len2.is_finite() || len2 <= CONIC_EPS * CONIC_EPS {
        return None;
    }
    let t = ((point.x - line_a.x) * dx + (point.y - line_a.y) * dy) / len2;
    if !t.is_finite() {
        return None;
    }
    let proj = Point2::new(line_a.x + t * dx, line_a.y + t * dy);
    if !proj.x.is_finite() || !proj.y.is_finite() {
        return None;
    }
    Some(proj)
}

/// Foco de parábola (identidad exacta).
#[must_use]
pub fn parabola_focus(focus: Point2) -> Point2 {
    focus
}

/// Directriz de parábola como segmento que define la recta infinita.
#[must_use]
pub fn parabola_directrix(line_a: Point2, line_b: Point2) -> Option<(Point2, Point2)> {
    if !line_a.x.is_finite()
        || !line_a.y.is_finite()
        || !line_b.x.is_finite()
        || !line_b.y.is_finite()
    {
        return None;
    }
    let dx = line_b.x - line_a.x;
    let dy = line_b.y - line_a.y;
    if !dx.is_finite() || !dy.is_finite() || dx.hypot(dy) <= CONIC_EPS {
        return None;
    }
    Some((line_a, line_b))
}

/// Vértice (centro) de parábola: punto medio entre foco y su proyección en la directriz.
#[must_use]
pub fn parabola_vertex(focus: Point2, line_a: Point2, line_b: Point2) -> Option<Point2> {
    let proj = project_point_to_line_exact(focus, line_a, line_b)?;
    Some(Point2::new(
        focus.x * 0.5 + proj.x * 0.5,
        focus.y * 0.5 + proj.y * 0.5,
    ))
}

/// Alias `center` para parábola: su vértice.
#[must_use]
pub fn parabola_center(focus: Point2, line_a: Point2, line_b: Point2) -> Option<Point2> {
    parabola_vertex(focus, line_a, line_b)
}

/// Parámetro `p = distancia(foco, directriz)/2` (distancia focal exacta).
#[must_use]
pub fn parabola_parameter(focus: Point2, line_a: Point2, line_b: Point2) -> Option<f64> {
    let proj = project_point_to_line_exact(focus, line_a, line_b)?;
    let dist = focus.distance(&proj);
    if !dist.is_finite() || dist <= CONIC_EPS {
        return None;
    }
    let p = dist * 0.5;
    if !p.is_finite() || p <= CONIC_EPS {
        return None;
    }
    Some(p)
}

/// Excentricidad de parábola: exactamente 1.
#[must_use]
pub const fn parabola_eccentricity() -> f64 {
    1.0
}

/// Foco derivado de un `ParabolaObj` (vértice, p, ángulo).
/// Eje local +y → dirección mundial `(-sin angle, cos angle)`.
#[must_use]
pub fn parabola_obj_focus(vertex: Point2, p: f64, angle: f64) -> Option<Point2> {
    if !vertex.x.is_finite()
        || !vertex.y.is_finite()
        || !p.is_finite()
        || !angle.is_finite()
        || p.abs() <= CONIC_EPS
    {
        return None;
    }
    let axis_x = -angle.sin();
    let axis_y = angle.cos();
    let f = Point2::new(vertex.x + p * axis_x, vertex.y + p * axis_y);
    if !f.x.is_finite() || !f.y.is_finite() {
        return None;
    }
    Some(f)
}

/// Directriz derivada de un `ParabolaObj` como recta infinita (dos puntos).
#[must_use]
pub fn parabola_obj_directrix(vertex: Point2, p: f64, angle: f64) -> Option<(Point2, Point2)> {
    if !vertex.x.is_finite()
        || !vertex.y.is_finite()
        || !p.is_finite()
        || !angle.is_finite()
        || p.abs() <= CONIC_EPS
    {
        return None;
    }
    let axis_x = -angle.sin();
    let axis_y = angle.cos();
    let directrix_point = Point2::new(vertex.x - p * axis_x, vertex.y - p * axis_y);
    // Dirección perpendicular al eje.
    let dir_x = angle.cos();
    let dir_y = angle.sin();
    let a = Point2::new(
        directrix_point.x - dir_x * 100.0,
        directrix_point.y - dir_y * 100.0,
    );
    let b = Point2::new(
        directrix_point.x + dir_x * 100.0,
        directrix_point.y + dir_y * 100.0,
    );
    if !a.x.is_finite() || !a.y.is_finite() || !b.x.is_finite() || !b.y.is_finite() {
        return None;
    }
    Some((a, b))
}

/// Determina si una recta es tangente a una elipse (discriminante exacto).
/// La recta se da como segmento `a-b` (recta infinita). Devuelve `Some(true/false)`
/// si la intersección es computable, `None` si es degenerada.
#[must_use]
pub fn is_tangent_to_ellipse(
    center: Point2,
    rx: f64,
    ry: f64,
    angle: f64,
    line_a: Point2,
    line_b: Point2,
) -> Option<bool> {
    if !center.x.is_finite()
        || !center.y.is_finite()
        || !rx.is_finite()
        || !ry.is_finite()
        || !angle.is_finite()
        || rx <= CONIC_EPS
        || ry <= CONIC_EPS
        || !line_a.x.is_finite()
        || !line_a.y.is_finite()
        || !line_b.x.is_finite()
        || !line_b.y.is_finite()
    {
        return None;
    }
    let dx = line_b.x - line_a.x;
    let dy = line_b.y - line_a.y;
    let len = dx.hypot(dy);
    if !dx.is_finite() || !dy.is_finite() || len <= CONIC_EPS {
        return None;
    }
    // Transforma la recta al sistema local de la elipse (traslación + rotación inversa).
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    let transform = |p: Point2| -> (f64, f64) {
        let tx = p.x - center.x;
        let ty = p.y - center.y;
        let lx = tx * cos_a + ty * sin_a;
        let ly = -tx * sin_a + ty * cos_a;
        (lx, ly)
    };
    let (ax, ay) = transform(line_a);
    let (bx, by) = transform(line_b);
    let ldx = bx - ax;
    let ldy = by - ay;
    // Ecuación paramétrica: (ax + t*ldx)^2/rx² + (ay + t*ldy)^2/ry² =1
    // → (ldx²/rx² + ldy²/ry²) t² + 2(ax*ldx/rx² + ay*ldy/ry²) t + (ax²/rx²+ay²/ry² -1)=0
    let rx2 = rx * rx;
    let ry2 = ry * ry;
    let a_coef = ldx * ldx / rx2 + ldy * ldy / ry2;
    let b_coef = 2.0 * (ax * ldx / rx2 + ay * ldy / ry2);
    let c_coef = ax * ax / rx2 + ay * ay / ry2 - 1.0;
    if !a_coef.is_finite() || !b_coef.is_finite() || !c_coef.is_finite() {
        return None;
    }
    if a_coef.abs() <= CONIC_EPS {
        // Recta degenerada en el sistema local (casi puntual): decide por distancia.
        return None;
    }
    let disc = b_coef * b_coef - 4.0 * a_coef * c_coef;
    if !disc.is_finite() {
        return None;
    }
    // Tangencia exacta cuando discriminante ≈0 (tolerancia relativa).
    let scale = (b_coef * b_coef + 4.0 * a_coef.abs() * c_coef.abs()).max(1.0);
    Some(disc.abs() <= CONIC_EPS * scale)
}

// ── Aliases genéricos exigidos por la tarea ────────────────────────────────

/// Alias genérico `focus` para elipse (dos focos).
#[must_use]
pub fn focus(focus1: Point2, focus2: Point2) -> (Point2, Point2) {
    ellipse_foci(focus1, focus2)
}

/// Alias genérico `directrix` para parábola (recta directriz).
#[must_use]
pub fn directrix(line_a: Point2, line_b: Point2) -> Option<(Point2, Point2)> {
    parabola_directrix(line_a, line_b)
}

/// Alias genérico `center` para elipse (punto medio de focos).
#[must_use]
pub fn center(focus1: Point2, focus2: Point2) -> Point2 {
    ellipse_center(focus1, focus2)
}

/// Alias genérico `eccentricity` para elipse.
#[must_use]
pub fn eccentricity(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<f64> {
    ellipse_eccentricity(focus1, focus2, point_on)
}

/// Alias genérico `axes` para elipse.
#[must_use]
pub fn axes(focus1: Point2, focus2: Point2, point_on: Point2) -> Option<(f64, f64)> {
    ellipse_axes(focus1, focus2, point_on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_large_fractions_without_cross_multiplication() {
        let left = ExactRational::new(i128::MAX, i128::MAX - 1).unwrap();
        let right = ExactRational::one();
        assert!(left > right);
    }

    #[test]
    fn division_handles_the_minimum_i128_factor() {
        let value = ExactRational::new(i128::MIN, 1).unwrap();
        let divisor = ExactRational::new(i128::MIN, 1).unwrap();
        assert_eq!(value.checked_div(divisor), Ok(ExactRational::one()));
    }
}
