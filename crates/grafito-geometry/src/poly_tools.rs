//! Frente P0.2: herramientas polinómicas y matriciales sobre motores existentes.
//!
//! Reusa `symbolic::expand/factor/derivative`, `ast::parse_ast`,
//! `solve::{solve_all_real, solve_polynomial_complex}`,
//! `matrices::{eigenvalues, eigenvectors}` y `ExactRational`. Lo único
//! in-house es lo que el árbol no ofrece: extracción de coeficientes,
//! división euclídea polinómica, forma vértice, polinomio minimal por
//! eliminación exacta en base multicuadrática, Faddeeva-LeVerrier, RREF y
//! curvatura con signo. Sin dependencias nuevas, sin MSRV nueva.

use crate::ast::{parse_ast, Expr};
use crate::exact::ExactRational;
use crate::matrices::Matrix;
use std::collections::BTreeMap;

/// Grado máximo aceptado en extracción/división/formato.
pub const MAX_POLY_DEGREE: usize = 1024;

/// Grado máximo del polinomio minimal (2^radicales ≤ 8).
pub const MAX_MINPOLY_DEGREE: usize = 8;

/// Dimensión máxima de Faddeeva-LeVerrier (`O(n⁴)`, muy por debajo del
/// `MAX_MATRIX_DIMENSION` de 1000 por honestidad de costo).
pub const MAX_CHARPOLY_DIM: usize = 64;

/// Tolerancia de snap a cero para coeficientes `f64` expandidos.
const COEF_SNAP: f64 = 1e-12;

// ── Extracción polinómica ─────────────────────────────────────────────

/// Término `coef · ∏ var^exp` con potencias ordenadas.
#[derive(Clone, Debug)]
pub struct PolyTerm {
    /// Coeficiente `f64` del término.
    pub coef: f64,
    /// Exponentes por variable.
    pub powers: BTreeMap<String, u32>,
}

fn monomio(nodo: &Expr) -> Option<(f64, BTreeMap<String, u32>)> {
    match nodo {
        Expr::Const(c) => {
            if c.is_finite() {
                Some((*c, BTreeMap::new()))
            } else {
                None
            }
        }
        Expr::Var(v) => {
            let mut powers = BTreeMap::new();
            powers.insert(v.clone(), 1);
            Some((1.0, powers))
        }
        Expr::Neg(inner) => {
            let (c, p) = monomio(inner)?;
            Some((-c, p))
        }
        Expr::Mul(a, b) => {
            let (ca, mut pa) = monomio(a)?;
            let (cb, pb) = monomio(b)?;
            let coef = ca * cb;
            if !coef.is_finite() {
                return None;
            }
            for (var, exp) in pb {
                let entrada = pa.entry(var).or_insert(0);
                *entrada = entrada.checked_add(exp)?;
            }
            Some((coef, pa))
        }
        Expr::Div(a, b) => {
            let (ca, pa) = monomio(a)?;
            match b.as_ref() {
                Expr::Const(d) if *d != 0.0 && d.is_finite() => {
                    let coef = ca / d;
                    if coef.is_finite() {
                        Some((coef, pa))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        Expr::Pow(base, exp) => {
            let Expr::Const(n) = exp.as_ref() else {
                return None;
            };
            if !n.is_finite() || n.fract() != 0.0 || *n < 0.0 || *n > MAX_POLY_DEGREE as f64 {
                return None;
            }
            let grado = *n as u32;
            match base.as_ref() {
                Expr::Var(v) => {
                    let mut powers = BTreeMap::new();
                    powers.insert(v.clone(), grado);
                    Some((1.0, powers))
                }
                Expr::Const(c) => {
                    let coef = c.powf(*n);
                    if coef.is_finite() {
                        Some((coef, BTreeMap::new()))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn juntar(nodo: &Expr, signo: f64, salida: &mut Vec<PolyTerm>) -> Result<(), ()> {
    match nodo {
        Expr::Add(a, b) => {
            juntar(a, signo, salida)?;
            juntar(b, signo, salida)?;
            Ok(())
        }
        Expr::Sub(a, b) => {
            juntar(a, signo, salida)?;
            juntar(b, -signo, salida)?;
            Ok(())
        }
        _ => {
            let (coef, powers) = monomio(nodo).ok_or(())?;
            salida.push(PolyTerm {
                coef: signo * coef,
                powers,
            });
            Ok(())
        }
    }
}

/// Expande con el motor y extrae términos `coef · ∏ var^exp`.
pub fn poly_terms(expr: &str) -> Result<Vec<PolyTerm>, String> {
    let expandida =
        crate::symbolic::expand(expr).map_err(|e| format!("no se pudo expandir '{expr}': {e}"))?;
    let ast = parse_ast(&expandida).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let mut terminos = Vec::new();
    juntar(&ast, 1.0, &mut terminos).map_err(|()| format!("'{expr}' no es un polinomio"))?;
    if terminos.is_empty() {
        return Err(format!("'{expr}' no es un polinomio"));
    }
    for termino in &terminos {
        let total: u32 = termino.powers.values().sum();
        if total as usize > MAX_POLY_DEGREE {
            return Err(format!("grado excede el máximo {MAX_POLY_DEGREE}"));
        }
        if !termino.coef.is_finite() {
            return Err(format!("'{expr}' tiene coeficientes no finitos"));
        }
    }
    Ok(terminos)
}

/// Coeficientes ascendentes `[c0, c1, …]` en `var` (resto de variables → error).
pub fn poly_coeffs(expr: &str, var: &str) -> Result<Vec<f64>, String> {
    let terminos = poly_terms(expr)?;
    let mut max_exp = 0usize;
    for termino in &terminos {
        for nombre in termino.powers.keys() {
            if nombre != var {
                return Err(format!("'{expr}' no es polinomio univariado en '{var}'"));
            }
        }
        let exp = termino.powers.get(var).copied().unwrap_or(0) as usize;
        max_exp = max_exp.max(exp);
    }
    let mut coefs = vec![0.0f64; max_exp + 1];
    for termino in &terminos {
        let exp = termino.powers.get(var).copied().unwrap_or(0) as usize;
        coefs[exp] += termino.coef;
    }
    for c in &mut coefs {
        if c.abs() <= COEF_SNAP {
            *c = 0.0;
        }
    }
    while coefs.len() > 1 && coefs.last() == Some(&0.0) {
        coefs.pop();
    }
    Ok(coefs)
}

/// Grado total (suma máxima de exponentes entre términos).
pub fn poly_total_degree(expr: &str) -> Result<usize, String> {
    let terminos = poly_terms(expr)?;
    terminos
        .iter()
        .map(|t| t.powers.values().sum::<u32>() as usize)
        .max()
        .ok_or_else(|| format!("'{expr}' no es un polinomio"))
}

/// Grado en una variable (`0` si la variable no aparece).
pub fn poly_degree_in(expr: &str, var: &str) -> Result<usize, String> {
    let terminos = poly_terms(expr)?;
    Ok(terminos
        .iter()
        .map(|t| t.powers.get(var).copied().unwrap_or(0) as usize)
        .max()
        .unwrap_or(0))
}

fn fmt_num(c: f64) -> String {
    if c.fract() == 0.0 && c.abs() < 1e15 {
        format!("{:.0}", c)
    } else {
        format!("{c}")
    }
}

/// Forma canónica `a_n·x^n + …` desde coeficientes ascendentes.
#[must_use]
pub fn format_poly(coefs: &[f64], var: &str) -> String {
    let mut salida = String::new();
    let mut primero = true;
    for (grado, coef) in coefs.iter().enumerate().rev() {
        if *coef == 0.0 {
            continue;
        }
        let negativo = *coef < 0.0;
        let mag = coef.abs();
        let cuerpo = if grado == 0 {
            fmt_num(mag)
        } else {
            let potencia = if grado == 1 {
                var.to_string()
            } else {
                format!("{var}^{grado}")
            };
            if mag == 1.0 {
                potencia
            } else {
                format!("{}*{potencia}", fmt_num(mag))
            }
        };
        if primero {
            if negativo {
                salida.push('-');
            }
            salida.push_str(&cuerpo);
            primero = false;
        } else if negativo {
            salida.push_str(&format!(" - {cuerpo}"));
        } else {
            salida.push_str(&format!(" + {cuerpo}"));
        }
    }
    if salida.is_empty() {
        salida.push('0');
    }
    salida
}

/// Construye la forma canónica desde coeficientes ascendentes dados.
pub fn poly_from_coeffs(coefs: &[f64], var: &str) -> Result<String, String> {
    if coefs.is_empty() {
        return Err("se esperaba al menos un coeficiente".to_string());
    }
    if coefs.len() - 1 > MAX_POLY_DEGREE {
        return Err(format!("grado excede el máximo {MAX_POLY_DEGREE}"));
    }
    if coefs.iter().any(|c| !c.is_finite()) {
        return Err("los coeficientes deben ser finitos".to_string());
    }
    Ok(format_poly(coefs, var))
}

/// División euclídea polinómica `a = q·b + r` (coeficientes ascendentes).
pub fn poly_divmod(a: &[f64], b: &[f64]) -> Result<(Vec<f64>, Vec<f64>), String> {
    let mut resto: Vec<f64> = a.to_vec();
    while resto.len() > 1 && resto.last() == Some(&0.0) {
        resto.pop();
    }
    let mut divisor: Vec<f64> = b.to_vec();
    while divisor.len() > 1 && divisor.last() == Some(&0.0) {
        divisor.pop();
    }
    let Some(&lider) = divisor.last() else {
        return Err("divisor nulo".to_string());
    };
    if lider == 0.0 {
        return Err("divisor nulo".to_string());
    }
    if resto.len() < divisor.len() || (resto.len() == 1 && resto[0] == 0.0) {
        return Ok((vec![0.0], resto));
    }
    let mut cociente = vec![0.0f64; resto.len() - divisor.len() + 1];
    while resto.len() >= divisor.len() && !(resto.len() == 1 && resto[0] == 0.0) {
        let grado = resto.len() - divisor.len();
        let factor = resto[resto.len() - 1] / lider;
        if !factor.is_finite() {
            return Err("división polinómica no finita".to_string());
        }
        cociente[grado] += factor;
        for (i, d) in divisor.iter().enumerate() {
            resto[grado + i] -= factor * d;
        }
        while resto.len() > 1 && resto.last() == Some(&0.0) {
            resto.pop();
        }
        if resto.iter().all(|c| *c == 0.0) {
            resto = vec![0.0];
            break;
        }
    }
    for c in &mut cociente {
        if c.abs() <= COEF_SNAP {
            *c = 0.0;
        }
    }
    while cociente.len() > 1 && cociente.last() == Some(&0.0) {
        cociente.pop();
    }
    Ok((cociente, resto))
}

// ── Formas: factorizada y vértice ───────────────────────────────────

/// `true` si la forma ya es producto/potencia de factores no constantes.
///
/// Nota honesta: se compara estructura sintáctica (el motor `factor()`
/// normaliza hasta `"1 * (x + 1) * …"`, así que comparar su salida con la
/// entrada daría `false` hasta para formas ya factorizadas). `factor()` sí
/// se invoca para validar el dominio: si falla, el chequeo es `Err`.
pub fn is_factored_shape(expr: &str, var: &str) -> Result<bool, String> {
    let _ = crate::symbolic::factor(expr, var)
        .map_err(|e| format!("no se pudo factorizar '{expr}': {e}"))?;
    let sin_espacios = expr.replace(' ', "");
    let ast = parse_ast(&sin_espacios).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    Ok(es_producto(&ast))
}

fn tiene_variable(nodo: &Expr) -> bool {
    match nodo {
        Expr::Const(_) => false,
        Expr::Var(_) => true,
        Expr::Neg(inner)
        | Expr::Sin(inner)
        | Expr::Cos(inner)
        | Expr::Tan(inner)
        | Expr::Asin(inner)
        | Expr::Acos(inner)
        | Expr::Atan(inner)
        | Expr::Exp(inner)
        | Expr::Ln(inner)
        | Expr::Log(inner)
        | Expr::Sqrt(inner)
        | Expr::Abs(inner)
        | Expr::Sinh(inner)
        | Expr::Cosh(inner)
        | Expr::Tanh(inner)
        | Expr::Floor(inner)
        | Expr::Ceil(inner)
        | Expr::Round(inner)
        | Expr::Sec(inner)
        | Expr::Csc(inner)
        | Expr::Cot(inner)
        | Expr::Asinh(inner)
        | Expr::Acosh(inner)
        | Expr::Atanh(inner)
        | Expr::Sign(inner)
        | Expr::Heaviside(inner)
        | Expr::Cbrt(inner)
        | Expr::Re(inner)
        | Expr::Im(inner)
        | Expr::Arg(inner)
        | Expr::Conj(inner)
        | Expr::Erf(inner)
        | Expr::Erfc(inner)
        | Expr::Gamma(inner)
        | Expr::LnGamma(inner)
        | Expr::Digamma(inner)
        | Expr::Trigamma(inner) => tiene_variable(inner),
        Expr::Add(a, b)
        | Expr::Sub(a, b)
        | Expr::Mul(a, b)
        | Expr::Div(a, b)
        | Expr::Pow(a, b)
        | Expr::Atan2(a, b)
        | Expr::Modulo(a, b)
        | Expr::Min(a, b)
        | Expr::Max(a, b)
        | Expr::Beta(a, b)
        | Expr::BesselJ(a, b)
        | Expr::BesselY(a, b)
        | Expr::BesselI(a, b)
        | Expr::Lt(a, b)
        | Expr::Gt(a, b)
        | Expr::Le(a, b)
        | Expr::Ge(a, b)
        | Expr::Eq(a, b)
        | Expr::Ne(a, b) => tiene_variable(a) || tiene_variable(b),
        Expr::Clamp(x, lo, hi) => tiene_variable(x) || tiene_variable(lo) || tiene_variable(hi),
        Expr::Sum(cuerpo, _, inicio, fin) | Expr::Product(cuerpo, _, inicio, fin) => {
            tiene_variable(cuerpo) || tiene_variable(inicio) || tiene_variable(fin)
        }
        Expr::Piecewise(ramas, defecto) => {
            ramas
                .iter()
                .any(|(cond, val)| tiene_variable(cond) || tiene_variable(val))
                || tiene_variable(defecto)
        }
    }
}

fn es_producto(nodo: &Expr) -> bool {
    match nodo {
        Expr::Neg(inner) => es_producto(inner),
        Expr::Mul(a, b) => tiene_variable(a) || tiene_variable(b),
        Expr::Pow(base, exp) => {
            matches!(exp.as_ref(), Expr::Const(n) if *n >= 2.0 && n.fract() == 0.0)
                && tiene_variable(base)
        }
        _ => false,
    }
}

fn es_constante(nodo: &Expr) -> bool {
    match nodo {
        Expr::Const(c) => c.is_finite(),
        Expr::Neg(inner) => es_constante(inner),
        _ => false,
    }
}

fn base_vertice(nodo: &Expr) -> bool {
    match nodo {
        Expr::Var(_) => true,
        Expr::Add(a, b) | Expr::Sub(a, b) => {
            (matches!(a.as_ref(), Expr::Var(_)) && es_constante(b))
                || (matches!(b.as_ref(), Expr::Var(_)) && es_constante(a))
        }
        _ => false,
    }
}

fn potencia_vertice(nodo: &Expr) -> bool {
    match nodo {
        Expr::Pow(base, exp) => {
            matches!(exp.as_ref(), Expr::Const(e) if *e == 2.0) && base_vertice(base)
        }
        _ => false,
    }
}

/// `true` si la forma es `a·(x−h)²+k` (con `a`/`k` opcionales, `h = 0` válido).
pub fn is_vertex_form_shape(expr: &str) -> Result<bool, String> {
    let sin_espacios = expr.replace(' ', "");
    let ast = parse_ast(&sin_espacios).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    // Separa `cuerpo ± k` con `k` constante.
    let cuerpo = match &ast {
        Expr::Add(a, b) => {
            if es_constante(a) {
                b.as_ref()
            } else if es_constante(b) {
                a.as_ref()
            } else {
                &ast
            }
        }
        Expr::Sub(a, b) => {
            if es_constante(b) {
                a.as_ref()
            } else {
                &ast
            }
        }
        _ => &ast,
    };
    let es_forma = if potencia_vertice(cuerpo) {
        true
    } else {
        match cuerpo {
            Expr::Mul(a, b) => {
                let (coef, pot) = if es_constante(a) {
                    (Some(a), b.as_ref())
                } else if es_constante(b) {
                    (Some(b), a.as_ref())
                } else {
                    (None, cuerpo)
                };
                coef.is_some() && potencia_vertice(pot)
            }
            Expr::Neg(inner) => potencia_vertice(inner),
            _ => false,
        }
    };
    Ok(es_forma)
}

// ── Fracciones exactas ──────────────────────────────────────────────

/// `(numerador, denominador)` reducidos de una fracción constante.
pub fn fraction_parts(expr: &str) -> Result<(i128, i128), String> {
    let sin_espacios = expr.replace(' ', "");
    let ast = parse_ast(&sin_espacios).map_err(|e| format!("no se pudo parsear '{expr}': {e}"))?;
    let (num_ast, den_ast) = match &ast {
        Expr::Div(n, d) => (n.as_ref(), Some(d.as_ref())),
        _ => (&ast, None),
    };
    let evalua = |nodo: &Expr| {
        let texto = nodo.to_expr_string();
        crate::symbolic::evaluate_exact_rational(&texto)
            .map_err(|e| format!("'{expr}' no es fracción constante: {e}"))?
            .ok_or_else(|| format!("'{expr}' no es fracción constante"))
    };
    let num = evalua(num_ast)?;
    let racional = match den_ast {
        Some(d) => {
            let den = evalua(d)?;
            num.checked_div(den)
                .map_err(|_| format!("'{expr}': división por cero"))?
        }
        None => num,
    };
    Ok((racional.numerator(), racional.denominator()))
}

// ── Polinomio minimal (eliminación exacta multicuadrática) ──────────

/// Lados de una división como texto (`(x+1)/(x-1)` → `("x+1", "x-1")`).
///
/// A diferencia de [`fraction_parts`], no exige constantes: el numerador de
/// `a/b` es `a` por definición. Devuelve `None` si no hay división tópica.
pub fn fraction_sides(expr: &str) -> Option<(String, String)> {
    use crate::ast::{parse_ast, Expr};
    let ast = parse_ast(&expr.replace(' ', "")).ok()?;
    match &ast {
        Expr::Div(n, d) => Some((n.to_expr_string(), d.to_expr_string())),
        _ => None,
    }
}
/// Descompone `d >= 2` en `(núcleo_libre_de_cuadrados, cofactor)` con
/// `d = núcleo · cofactor²`.
fn nucleo_libre_cuadrados(mut d: u128) -> (u128, u128) {
    let mut nucleo = 1u128;
    let mut cofactor = 1u128;
    let mut p = 2u128;
    while p <= d / p {
        let mut exp = 0u32;
        while d.is_multiple_of(p) {
            d /= p;
            exp += 1;
        }
        for _ in 0..exp / 2 {
            cofactor *= p;
        }
        if exp % 2 == 1 {
            nucleo *= p;
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if d > 1 {
        nucleo *= d;
    }
    (nucleo, cofactor)
}

/// Factoriza `n` en primos (trial division; `n <= 1e12` es instantáneo).
fn factoriza_primos(mut n: u128) -> Vec<u128> {
    let mut factores = Vec::new();
    let mut p = 2u128;
    while p <= n / p {
        while n.is_multiple_of(p) {
            factores.push(p);
            n /= p;
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if n > 1 {
        factores.push(n);
    }
    factores
}

/// Par `(base_independiente, expresiones)` de [`base_independiente`].
type BaseExprs = (Vec<u128>, Vec<(u32, u128)>);

/// Base independiente (sobre GF(2)) de núcleos + expresión de cada
/// núcleo original como `(máscara_en_base, factor_racional)`.
fn base_independiente(nucleos: &[u128]) -> Result<BaseExprs, String> {
    // Primos distintos presentes, en orden.
    let mut primos: Vec<u128> = Vec::new();
    for n in nucleos {
        for p in factoriza_primos(*n) {
            if !primos.contains(&p) {
                primos.push(p);
            }
        }
    }
    primos.sort_unstable();
    // Filas = núcleos como vectores de paridad; eliminación que selecciona
    // un subconjunto independiente (solo importan las filas pivote).
    let mut trabajo: Vec<Vec<u8>> = nucleos
        .iter()
        .map(|n| {
            let fac = factoriza_primos(*n);
            primos.iter().map(|p| u8::from(fac.contains(p))).collect()
        })
        .collect();
    let mut perm: Vec<usize> = (0..trabajo.len()).collect();
    let mut pivotes: Vec<usize> = Vec::new();
    let mut col = 0usize;
    let mut fila = 0usize;
    while fila < trabajo.len() && col < primos.len() {
        if let Some(pos) = (fila..trabajo.len()).find(|i| trabajo[*i][col] == 1) {
            trabajo.swap(fila, pos);
            perm.swap(fila, pos);
            pivotes.push(perm[fila]);
            let fila_vals = trabajo[fila].clone();
            for (i, fila_i) in trabajo.iter_mut().enumerate() {
                if i != fila && fila_i[col] == 1 {
                    for (celda, piv) in fila_i.iter_mut().zip(fila_vals.iter()) {
                        *celda ^= *piv;
                    }
                }
            }
            fila += 1;
        }
        col += 1;
    }
    // Cada núcleo j = (Π base^e) · cuadrado: el cuadrado sale de dividir.
    let mut base: Vec<u128> = pivotes.iter().map(|r| nucleos[*r]).collect();
    base.sort_unstable();
    base.dedup();
    if base.len() > 3 {
        return Err(format!(
            "polinomio minimal: grado 2^{} excede el máximo {MAX_MINPOLY_DEGREE}",
            base.len()
        ));
    }
    // Cada núcleo se expresa por búsqueda directa de máscara (≤ 2^3 casos):
    // `núcleo_j / Π base^e` debe ser un cuadrado perfecto.
    let mut exprs = Vec::with_capacity(nucleos.len());
    for nj in nucleos {
        let mut hallada = None;
        for masc in 0..(1u32 << base.len()) {
            let mut prod = 1u128;
            for (i, b) in base.iter().enumerate() {
                if masc & (1 << i) != 0 {
                    prod = prod.saturating_mul(*b);
                }
            }
            if prod == 0 || *nj % prod != 0 {
                continue;
            }
            let coc = *nj / prod;
            let raiz = (coc as f64).sqrt() as u128;
            for cand in raiz.saturating_sub(2)..=raiz.saturating_add(2) {
                if cand.checked_mul(cand) == Some(coc) {
                    hallada = Some((masc, cand));
                    break;
                }
            }
            if hallada.is_some() {
                break;
            }
        }
        match hallada {
            Some(par) => exprs.push(par),
            None => return Err("no se pudo independizar el radicando".to_string()),
        }
    }
    Ok((base, exprs))
}

type Algebra = Vec<ExactRational>;

fn algebra_cero(dim: usize) -> Algebra {
    vec![ExactRational::zero(); dim]
}

fn algebra_mul(a: &Algebra, b: &Algebra, cuadrados: &[u128]) -> Result<Algebra, String> {
    let dim = a.len();
    let mut salida = algebra_cero(dim);
    for (i, ca) in a.iter().enumerate() {
        if ca.is_zero() {
            continue;
        }
        for (j, cb) in b.iter().enumerate() {
            if cb.is_zero() {
                continue;
            }
            let inter = (i & j) as u32;
            let mut factor = 1u128;
            for (k, d) in cuadrados.iter().enumerate() {
                if inter & (1 << k) != 0 {
                    factor = factor
                        .checked_mul(*d)
                        .ok_or_else(|| "polinomio minimal excede rango".to_string())?;
                }
            }
            let termino = ca
                .checked_mul(*cb)
                .map_err(|_| "polinomio minimal excede rango".to_string())?;
            let termino = ExactRational::new(i128::try_from(factor).unwrap_or(i128::MAX), 1)
                .map_err(|_| "polinomio minimal excede rango".to_string())?
                .checked_mul(termino)
                .map_err(|_| "polinomio minimal excede rango".to_string())?;
            let destino = (i ^ j) & (dim - 1);
            salida[destino] = salida[destino]
                .checked_add(termino)
                .map_err(|_| "polinomio minimal excede rango".to_string())?;
        }
    }
    Ok(salida)
}

fn algebra_neg(a: &Algebra) -> Result<Algebra, String> {
    a.iter()
        .map(|c| {
            c.checked_neg()
                .map_err(|_| "polinomio minimal excede rango".to_string())
        })
        .collect()
}

/// Evalúa la plantilla en el álgebra multicuadrática. Solo `+,-,*,/`
/// (divisor racional), potencias enteras `>= 0` y `sqrt(entero)`.
fn evalua_plantilla(
    nodo: &Expr,
    raices: &BTreeMap<u128, (u32, u128)>,
    base: &[u128],
    dim: usize,
) -> Result<Algebra, String> {
    let err = |detalle: &str| format!("polinomio minimal: {detalle}");
    match nodo {
        Expr::Const(c) => {
            if !c.is_finite() || c.fract() != 0.0 || c.abs() > 1e12 {
                return Err(err("solo coeficientes enteros |c| <= 1e12"));
            }
            let mut v = algebra_cero(dim);
            v[0] = ExactRational::new(*c as i128, 1).map_err(|_| err("rango"))?;
            Ok(v)
        }
        Expr::Var(v) => Err(err(&format!(
            "sin variables (llegó '{v}'): se espera un número"
        ))),
        Expr::Neg(inner) => Ok(algebra_neg(&evalua_plantilla(inner, raices, base, dim)?)?),
        Expr::Add(a, b) => {
            let x = evalua_plantilla(a, raices, base, dim)?;
            let y = evalua_plantilla(b, raices, base, dim)?;
            x.iter()
                .zip(y.iter())
                .map(|(p, q)| p.checked_add(*q).map_err(|_| err("rango")))
                .collect()
        }
        Expr::Sub(a, b) => {
            let x = evalua_plantilla(a, raices, base, dim)?;
            let y = evalua_plantilla(b, raices, base, dim)?;
            x.iter()
                .zip(y.iter())
                .map(|(p, q)| p.checked_sub(*q).map_err(|_| err("rango")))
                .collect()
        }
        Expr::Mul(a, b) => {
            let x = evalua_plantilla(a, raices, base, dim)?;
            let y = evalua_plantilla(b, raices, base, dim)?;
            algebra_mul(&x, &y, base)
        }
        Expr::Div(a, b) => {
            let x = evalua_plantilla(a, raices, base, dim)?;
            let y = evalua_plantilla(b, raices, base, dim)?;
            if y.iter().skip(1).any(|c| !c.is_zero()) || y[0].is_zero() {
                return Err(err("divisor debe ser racional no nulo"));
            }
            let inv = ExactRational::one()
                .checked_div(y[0])
                .map_err(|_| err("rango"))?;
            x.iter()
                .map(|c| c.checked_mul(inv).map_err(|_| err("rango")))
                .collect()
        }
        Expr::Pow(pot_base, pot_exp) => {
            let Expr::Const(n) = pot_exp.as_ref() else {
                return Err(err("exponente debe ser entero >= 0"));
            };
            if !n.is_finite() || n.fract() != 0.0 || *n < 0.0 || *n > 32.0 {
                return Err(err("exponente debe ser entero 0..=32"));
            }
            let base_v = evalua_plantilla(pot_base, raices, base, dim)?;
            let mut acc = algebra_cero(dim);
            acc[0] = ExactRational::one();
            for _ in 0..(*n as usize) {
                acc = algebra_mul(&acc, &base_v, base)?;
            }
            Ok(acc)
        }
        Expr::Sqrt(inner) => {
            let Expr::Const(d) = inner.as_ref() else {
                return Err(err("sqrt solo de entero positivo"));
            };
            if !d.is_finite() || d.fract() != 0.0 || *d < 2.0 || *d > 1e12 {
                return Err(err("sqrt solo de entero 2..=1e12"));
            }
            let clave = *d as u128;
            let (mascara, factor) = raices
                .get(&clave)
                .copied()
                .ok_or_else(|| err("radicando fuera de catálogo"))?;
            let mut v = algebra_cero(dim);
            let coef = ExactRational::new(i128::try_from(factor).unwrap_or(i128::MAX), 1)
                .map_err(|_| err("rango"))?;
            let idx = (mascara as usize) & (dim - 1);
            v[idx] = coef;
            Ok(v)
        }
        _ => Err(err("solo +,-,*,/,^ entera y sqrt de entero")),
    }
}

/// Polinomio minimal exacto de un número multicuadrático (`sqrt` de
/// enteros, grado `2^r <= 8`); si es racional da `den·x − num`.
pub fn minimal_polynomial(expr: &str) -> Result<String, String> {
    let sin_espacios = expr.replace(' ', "");
    if sin_espacios.is_empty() {
        return Err("polinomio minimal: expresión vacía".to_string());
    }
    let ast = parse_ast(&sin_espacios)
        .map_err(|e| format!("polinomio minimal: no se pudo parsear '{expr}': {e}"))?;
    // Catálogo de radicandos (enteros >= 2 distintos).
    let mut radicandos: Vec<u128> = Vec::new();
    let mut apila = vec![&ast];
    while let Some(nodo) = apila.pop() {
        match nodo {
            Expr::Sqrt(inner) => match inner.as_ref() {
                Expr::Const(d) if d.is_finite() && d.fract() == 0.0 && *d >= 2.0 && *d <= 1e12 => {
                    let clave = *d as u128;
                    if !radicandos.contains(&clave) {
                        radicandos.push(clave);
                    }
                }
                _ => {
                    return Err(
                        "polinomio minimal: sqrt solo de entero 2..=1e12 (sin anidar)".to_string(),
                    )
                }
            },
            Expr::Add(a, b)
            | Expr::Sub(a, b)
            | Expr::Mul(a, b)
            | Expr::Div(a, b)
            | Expr::Pow(a, b) => {
                apila.push(a);
                apila.push(b);
            }
            Expr::Neg(inner)
            | Expr::Sin(inner)
            | Expr::Cos(inner)
            | Expr::Tan(inner)
            | Expr::Exp(inner)
            | Expr::Ln(inner)
            | Expr::Abs(inner) => apila.push(inner),
            Expr::Const(_) | Expr::Var(_) => {}
            _ => {
                return Err("polinomio minimal: solo +,-,*,/,^ entera y sqrt de entero".to_string())
            }
        }
    }
    if radicandos.is_empty() {
        // Racional puro: verifica exactitud y da `den·x − num`.
        let racional = crate::symbolic::evaluate_exact_rational(&sin_espacios)
            .map_err(|e| format!("polinomio minimal: '{expr}' no es algebraico simple: {e}"))?
            .ok_or_else(|| format!("polinomio minimal: '{expr}' no es un número racional"))?;
        let (num, den) = (racional.numerator(), racional.denominator());
        if den == 1 {
            return Ok(format!("x - ({num})"));
        }
        return Ok(format!("{den}*x - ({num})"));
    }
    // Reduce a núcleos libres de cuadrados y halla base independiente.
    let mut nucleos: Vec<u128> = Vec::new();
    let mut factores: BTreeMap<u128, u128> = BTreeMap::new();
    for d in &radicandos {
        let (nucleo, cofactor) = nucleo_libre_cuadrados(*d);
        factores.insert(*d, cofactor);
        if nucleo > 1 && !nucleos.contains(&nucleo) {
            nucleos.push(nucleo);
        }
    }
    // Sustituye `sqrt(d) = cofactor · sqrt(núcleo)`; si el núcleo es 1 el
    // radical era un entero exacto.
    let (base, exprs_base) = if nucleos.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        base_independiente(&nucleos)?
    };
    if base.len() > 3 {
        return Err(format!(
            "polinomio minimal: grado 2^{} excede el máximo {MAX_MINPOLY_DEGREE}",
            base.len()
        ));
    }
    // Mapa radicando → (máscara en base, factor racional total).
    let nucleo_a_expr: BTreeMap<u128, (u32, u128)> = nucleos
        .iter()
        .zip(exprs_base.iter())
        .map(|(n, e)| (*n, *e))
        .collect();
    let mut raices: BTreeMap<u128, (u32, u128)> = BTreeMap::new();
    for d in &radicandos {
        let (nucleo, _) = nucleo_libre_cuadrados(*d);
        let cofactor = factores.get(d).copied().unwrap_or(1);
        if nucleo == 1 {
            // Cuadrado perfecto: `sqrt(d) = cofactor` racional.
            raices.insert(*d, (0, cofactor));
        } else if let Some((masc, f)) = nucleo_a_expr.get(&nucleo) {
            let total = cofactor
                .checked_mul(*f)
                .ok_or_else(|| "polinomio minimal excede rango".to_string())?;
            raices.insert(*d, (*masc, total));
        } else {
            return Err("polinomio minimal: núcleo fuera de base".to_string());
        }
    }
    // Racional puro tras reducir cuadrados perfectos.
    let dim = 1usize << base.len();
    let alfa = evalua_plantilla(&ast, &raices, &base, dim)?;
    if alfa.iter().skip(1).all(|c| c.is_zero()) {
        let (num, den) = (alfa[0].numerator(), alfa[0].denominator());
        if den == 1 {
            return Ok(format!("x - ({num})"));
        }
        return Ok(format!("{den}*x - ({num})"));
    }
    // Producto sobre conjugados de signos: coeficientes en el álgebra que
    // deben resultar racionales (invariantes por conjugación). Si α vive en
    // un subcuerpo propio (p. ej. √2·√3 = √6), varios conjugados coinciden
    // y el producto total sería m(x)^e: se toman representantes de las
    // clases laterales del estabilizador para obtener el minimal exacto.
    let estabilizador: Vec<u32> = (1..(dim as u32))
        .filter(|m| {
            alfa.iter()
                .enumerate()
                .all(|(j, c)| c.is_zero() || ((j as u32) & m).count_ones().is_multiple_of(2))
        })
        .collect();
    let mut representantes: Vec<u32> = Vec::new();
    for masc in 0..(dim as u32) {
        let conocida = representantes
            .iter()
            .any(|r| estabilizador.contains(&(masc ^ r)));
        if !conocida {
            representantes.push(masc);
        }
    }
    let mut poli: Vec<Algebra> = vec![{
        let mut uno = algebra_cero(dim);
        uno[0] = ExactRational::one();
        uno
    }];
    for masc in representantes {
        let mut conjugado = algebra_cero(dim);
        for (j, c) in alfa.iter().enumerate() {
            let cambios = ((j as u32) & masc).count_ones();
            conjugado[j] = if cambios % 2 == 1 {
                c.checked_neg()
                    .map_err(|_| "polinomio minimal excede rango".to_string())?
            } else {
                *c
            };
        }
        // Multiplica por `(x − conjugado)`.
        let mut nuevo = vec![algebra_cero(dim); poli.len() + 1];
        for (i, coef) in poli.iter().enumerate() {
            // término `· x`
            for (j, c) in coef.iter().enumerate() {
                nuevo[i + 1][j] = nuevo[i + 1][j]
                    .checked_add(*c)
                    .map_err(|_| "polinomio minimal excede rango".to_string())?;
            }
            // término `· (−conjugado)`
            let prod = algebra_mul(coef, &algebra_neg(&conjugado)?, &base)?;
            for (j, c) in prod.iter().enumerate() {
                nuevo[i][j] = nuevo[i][j]
                    .checked_add(*c)
                    .map_err(|_| "polinomio minimal excede rango".to_string())?;
            }
        }
        poli = nuevo;
    }
    // Extrae parte racional y verifica invarianza.
    let mut coefs = Vec::with_capacity(poli.len());
    for coef in &poli {
        if coef.iter().skip(1).any(|c| !c.is_zero()) {
            return Err(
                "polinomio minimal: no se pudo certificar invarianza por conjugación".to_string(),
            );
        }
        coefs.push(coef[0]);
    }
    // Limpia denominadores y vuelve primitivo con líder positivo.
    let mut mcm: i128 = 1;
    for c in &coefs {
        mcm = crate::number_theory::lcm(mcm, c.denominator())
            .map_err(|_| "polinomio minimal excede rango".to_string())?;
    }
    let factor = ExactRational::new(mcm, 1).map_err(|_| "rango".to_string())?;
    let mut enteros: Vec<i128> = Vec::with_capacity(coefs.len());
    for c in &coefs {
        let v = factor
            .checked_mul(*c)
            .map_err(|_| "polinomio minimal excede rango".to_string())?;
        if v.denominator() != 1 {
            return Err("polinomio minimal: denominador residual".to_string());
        }
        enteros.push(v.numerator());
    }
    let mut mcd: i128 = 0;
    for v in &enteros {
        mcd = crate::number_theory::gcd(mcd, *v);
    }
    if mcd > 1 {
        for v in &mut enteros {
            *v /= mcd;
        }
    }
    if let Some(ultimo) = enteros.last() {
        if *ultimo < 0 {
            for v in &mut enteros {
                *v = -*v;
            }
        }
    }
    while enteros.len() > 1 && enteros.last() == Some(&0) {
        enteros.pop();
    }
    if enteros.len() - 1 > MAX_MINPOLY_DEGREE {
        return Err(format!(
            "polinomio minimal: grado excede {MAX_MINPOLY_DEGREE}"
        ));
    }
    let flotantes: Vec<f64> = enteros.iter().map(|v| *v as f64).collect();
    Ok(format_poly(&flotantes, "x"))
}

// ── Matrices: Faddeeva, RREF, Jordan real ───────────────────────────

fn matriz_a_filas(m: &Matrix) -> Result<Vec<Vec<f64>>, String> {
    if m.rows == 0 || m.cols == 0 {
        return Err("matriz vacía".to_string());
    }
    let mut filas = Vec::with_capacity(m.rows);
    for r in 0..m.rows {
        let mut fila = Vec::with_capacity(m.cols);
        for c in 0..m.cols {
            let v = m.get(r, c);
            if !v.is_finite() {
                return Err("la matriz tiene entradas no finitas".to_string());
            }
            fila.push(v);
        }
        filas.push(fila);
    }
    Ok(filas)
}

fn filas_a_matriz(filas: Vec<Vec<f64>>) -> Result<Matrix, String> {
    Matrix::from_rows(filas).ok_or_else(|| "matriz inválida".to_string())
}

fn mat_mul(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let mut salida = vec![vec![0.0f64; n]; n];
    for i in 0..n {
        for k in 0..n {
            let aik = a[i][k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..n {
                salida[i][j] += aik * b[k][j];
            }
        }
    }
    salida
}

/// Polinomio característico `det(λI − A)` (Faddeeva-LeVerrier, numérico).
/// Devuelve coeficientes ascendentes `[c0, …, c_{n-1}, 1]`.
pub fn charpoly(matriz: &Matrix) -> Result<Vec<f64>, String> {
    if matriz.rows != matriz.cols || matriz.rows == 0 {
        return Err("la matriz debe ser cuadrada no vacía".to_string());
    }
    let n = matriz.rows;
    if n > MAX_CHARPOLY_DIM {
        return Err(format!(
            "dimensión {n} excede el máximo {MAX_CHARPOLY_DIM} (costo O(n⁴) honesto)"
        ));
    }
    let a = matriz_a_filas(matriz)?;
    let mut m = vec![vec![0.0f64; n]; n];
    // `c[0] = 1` implícito; `coefs[k]` = c_k con p(λ) = λ^n + Σ c_k λ^{n-k}.
    let mut altos = vec![0.0f64; n + 1];
    altos[0] = 1.0;
    for k in 1..=n {
        // M_k = A·M_{k-1} + c_{k-1}·I
        let mut nuevo = mat_mul(&a, &m);
        for (i, fila_n) in nuevo.iter_mut().enumerate().take(n) {
            fila_n[i] += altos[k - 1];
        }
        m = nuevo;
        let am = mat_mul(&a, &m);
        let traza: f64 = (0..n).map(|i| am[i][i]).sum();
        if !traza.is_finite() {
            return Err("polinomio característico no finito".to_string());
        }
        altos[k] = -traza / (k as f64);
    }
    // Ascendentes `[c_n, …, c_1, 1]`.
    let mut salida = Vec::with_capacity(n + 1);
    for k in (1..=n).rev() {
        salida.push(altos[k]);
    }
    salida.push(1.0);
    if salida.iter().any(|c| !c.is_finite()) {
        return Err("polinomio característico no finito".to_string());
    }
    Ok(salida)
}

/// Forma escalonada reducida por filas (Gauss-Jordan con pivoteo parcial).
pub fn rref(matriz: &Matrix) -> Result<Matrix, String> {
    let mut filas = matriz_a_filas(matriz)?;
    let (m, n) = (filas.len(), filas[0].len());
    let escala = filas
        .iter()
        .flat_map(|f| f.iter())
        .fold(1.0f64, |acc, v| acc.max(v.abs()));
    let tol = 1e-12 * escala;
    let mut fila = 0usize;
    for col in 0..n {
        if fila >= m {
            break;
        }
        let mut pivote = fila;
        for i in fila..m {
            if filas[i][col].abs() > filas[pivote][col].abs() {
                pivote = i;
            }
        }
        if filas[pivote][col].abs() <= tol {
            continue;
        }
        filas.swap(fila, pivote);
        let divisor = filas[fila][col];
        for v in &mut filas[fila] {
            *v /= divisor;
        }
        let piv = filas[fila].clone();
        for (i, fila_i) in filas.iter_mut().enumerate().take(m) {
            if i != fila && fila_i[col] != 0.0 {
                let factor = fila_i[col];
                for (dest, src) in fila_i.iter_mut().zip(piv.iter()) {
                    *dest -= factor * src;
                }
            }
        }
        fila += 1;
    }
    for fila_v in &mut filas {
        for v in fila_v {
            if v.abs() <= tol {
                *v = 0.0;
            }
            if !v.is_finite() {
                return Err("RREF no finita".to_string());
            }
        }
    }
    filas_a_matriz(filas)
}

/// Descomposición real `A = P·D·P⁻¹` cuando existe (numérica).
#[derive(Clone, Debug)]
pub struct JordanReal {
    /// Autovalores reales en el orden de las columnas de `p`.
    pub eigenvalues: Vec<f64>,
    /// Matriz `P` por columnas de autovectores.
    pub p: Matrix,
    /// Diagonal de `D`.
    pub diag: Vec<f64>,
}

/// Diagonalización real honesta: error si hay autovalores complejos o la
/// matriz es defectiva (Jordan no trivial no soportado).
pub fn jordan_real(matriz: &Matrix) -> Result<JordanReal, String> {
    use crate::matrices::{eigenvalues, eigenvectors};
    if matriz.rows != matriz.cols || matriz.rows == 0 {
        return Err("la matriz debe ser cuadrada no vacía".to_string());
    }
    let n = matriz.rows;
    let vals = eigenvalues(matriz).ok_or_else(|| "no se pudo diagonalizar".to_string())?;
    let escala = vals
        .iter()
        .fold(1.0f64, |acc, (re, im)| acc.max(re.abs()).max(im.abs()));
    let tol = 1e-9 * escala;
    for (re, im) in &vals {
        if im.abs() > tol {
            return Err(format!(
                "autovalor complejo {re:.6}±{im:.6}i: Jordan real no soportado (solo diagonalización real)"
            ));
        }
    }
    let vecs = eigenvectors(matriz).ok_or_else(|| "no se pudo diagonalizar".to_string())?;
    let reales: Vec<(Vec<f64>, f64)> = vecs
        .into_iter()
        .filter(|(_, _, im)| im.abs() <= tol)
        .map(|(v, re, _)| (v, re))
        .collect();
    if reales.len() < n {
        return Err(
            "matriz defectiva (faltan autovectores): forma de Jordan no soportada".to_string(),
        );
    }
    // Columnas candidatas → filas de P; rango por RREF debe ser n.
    let columnas: Vec<(Vec<f64>, f64)> = reales.into_iter().take(n).collect();
    let eigen: Vec<f64> = columnas.iter().map(|(_, re)| *re).collect();
    let filas_p: Vec<Vec<f64>> = (0..n)
        .map(|r| columnas.iter().map(|(v, _)| v[r]).collect())
        .collect();
    let p = filas_a_matriz(filas_p)?;
    let escalonada = rref(&p)?;
    let mut rango = 0usize;
    for r in 0..escalonada.rows {
        if (0..escalonada.cols).any(|c| escalonada.get(r, c).abs() > 0.5) {
            rango += 1;
        }
    }
    if rango < n {
        return Err(
            "matriz defectiva (autovectores dependientes): forma de Jordan no soportada"
                .to_string(),
        );
    }
    Ok(JordanReal {
        eigenvalues: eigen.clone(),
        p,
        diag: eigen,
    })
}

// ── Vectores y curvatura ────────────────────────────────────────────

/// Normaliza un vector (cualquier dimensión `1..=10000`).
pub fn normalize_vec(v: &[f64]) -> Result<Vec<f64>, String> {
    if v.is_empty() || v.len() > 10_000 {
        return Err("el vector debe tener entre 1 y 10000 componentes".to_string());
    }
    if v.iter().any(|c| !c.is_finite()) {
        return Err("el vector tiene componentes no finitas".to_string());
    }
    let norma: f64 = v.iter().map(|c| c * c).sum::<f64>().sqrt();
    if !norma.is_finite() || norma == 0.0 {
        return Err("el vector nulo no se puede normalizar".to_string());
    }
    Ok(v.iter().map(|c| c / norma).collect())
}

/// Perpendicular 2D `(-y, x)` (rotación +90°).
pub fn perp2(x: f64, y: f64) -> Result<(f64, f64), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("el vector tiene componentes no finitas".to_string());
    }
    if x == 0.0 && y == 0.0 {
        return Err("el vector nulo no tiene perpendicular definido".to_string());
    }
    Ok((-y, x))
}

/// Vector curvatura `(κ, Nx·κ, Ny·κ)` con signo, numérico honesto.
pub fn curvature_vector(expr: &str, var: &str, x0: f64) -> Result<(f64, f64, f64), String> {
    if !x0.is_finite() {
        return Err("el punto debe ser finito".to_string());
    }
    let primera = crate::symbolic::derivative(expr, var)
        .map_err(|e| format!("no se pudo derivar '{expr}': {e}"))?;
    let segunda = crate::symbolic::derivative(&primera, var)
        .map_err(|e| format!("no se pudo derivar dos veces '{expr}': {e}"))?;
    let f1 = crate::expr::eval_function_var(&primera, var, x0)
        .map_err(|e| format!("no se pudo evaluar en x={x0}: {e}"))?;
    let f2 = crate::expr::eval_function_var(&segunda, var, x0)
        .map_err(|e| format!("no se pudo evaluar en x={x0}: {e}"))?;
    if !f1.is_finite() || !f2.is_finite() {
        return Err(format!("curvatura no finita en x={x0}"));
    }
    let base = 1.0 + f1 * f1;
    let kappa = f2 / base.powf(1.5);
    if !kappa.is_finite() {
        return Err(format!("curvatura no finita en x={x0}"));
    }
    let h = base.sqrt();
    Ok((kappa, kappa * (-f1 / h), kappa / h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefs_cuadratica_expandida() {
        let c = poly_coeffs("(x+1)^2", "x").expect("coefs");
        assert_eq!(c.len(), 3);
        assert!((c[0] - 1.0).abs() < 1e-9);
        assert!((c[1] - 2.0).abs() < 1e-9);
        assert!((c[2] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn coefs_rechaza_otras_variables() {
        assert!(poly_coeffs("x*y + 1", "x").is_err());
    }

    #[test]
    fn coefs_rechaza_trascendente() {
        assert!(poly_coeffs("sin(x) + x", "x").is_err());
    }

    #[test]
    fn grado_total_y_en_variable() {
        assert_eq!(poly_total_degree("x^2*y + y^3").expect("grado"), 3);
        assert_eq!(poly_degree_in("x^2 + x^5", "x").expect("grado"), 5);
        assert_eq!(poly_degree_in("x^2 + 1", "y").expect("grado"), 0);
    }

    #[test]
    fn formato_canonico() {
        assert_eq!(format_poly(&[1.0, -2.0, 1.0], "x"), "x^2 - 2*x + 1");
        assert_eq!(format_poly(&[0.0], "x"), "0");
        assert_eq!(format_poly(&[-1.0, 0.0, -1.0], "t"), "-t^2 - 1");
    }

    #[test]
    fn construir_desde_coeficientes() {
        assert_eq!(
            poly_from_coeffs(&[1.0, 2.0, 1.0], "x").expect("poli"),
            "x^2 + 2*x + 1"
        );
        assert!(poly_from_coeffs(&[], "x").is_err());
    }

    #[test]
    fn division_exacta_e_inexacta() {
        let (q, r) = poly_divmod(&[-1.0, 0.0, 1.0], &[-1.0, 1.0]).expect("div");
        assert_eq!(q.len(), 2);
        assert!((q[0] - 1.0).abs() < 1e-9 && (q[1] - 1.0).abs() < 1e-9);
        assert!(r.iter().all(|c| c.abs() < 1e-9));
        let (_, r2) = poly_divmod(&[1.0, 0.0, 1.0], &[-1.0, 1.0]).expect("div2");
        assert!((r2[0] - 2.0).abs() < 1e-9);
        assert!(poly_divmod(&[1.0], &[0.0]).is_err());
    }

    #[test]
    fn forma_factorizada() {
        assert!(is_factored_shape("(x+1)^2", "x").expect("f"));
        assert!(is_factored_shape("(x+1)*(x+2)", "x").expect("f"));
        assert!(is_factored_shape("2*x", "x").expect("f"));
        assert!(!is_factored_shape("x^2 - 4", "x").expect("f"));
        assert!(!is_factored_shape("x + 1", "x").expect("f"));
    }

    #[test]
    fn forma_vertice() {
        assert!(is_vertex_form_shape("2*(x-3)^2+1").expect("v"));
        assert!(is_vertex_form_shape("(x+1)^2").expect("v"));
        assert!(is_vertex_form_shape("x^2").expect("v"));
        assert!(!is_vertex_form_shape("x^2+2*x+1").expect("v"));
        assert!(!is_vertex_form_shape("x^3").expect("v"));
    }

    #[test]
    fn fracciones_exactas() {
        assert_eq!(fraction_parts("6/8").expect("f"), (3, 4));
        assert_eq!(fraction_parts("-6/8").expect("f"), (-3, 4));
        assert_eq!(fraction_parts("5").expect("f"), (5, 1));
        assert!(fraction_parts("1/0").is_err());
        assert!(fraction_parts("x+1").is_err());
    }

    #[test]
    fn minimal_cuadratico_simple() {
        assert_eq!(minimal_polynomial("sqrt(2)").expect("m"), "x^2 - 2");
    }

    #[test]
    fn minimal_suma_y_racional() {
        assert_eq!(
            minimal_polynomial("sqrt(2)+sqrt(3)").expect("m"),
            "x^4 - 10*x^2 + 1"
        );
        assert_eq!(minimal_polynomial("3/4").expect("m"), "4*x - (3)");
        assert_eq!(minimal_polynomial("5").expect("m"), "x - (5)");
    }

    #[test]
    fn minimal_cuadrado_perfecto_y_dependientes() {
        assert_eq!(minimal_polynomial("sqrt(4)").expect("m"), "x - (2)");
        assert_eq!(
            minimal_polynomial("sqrt(2)+sqrt(8)").expect("m"),
            "x^2 - 18"
        );
    }

    #[test]
    fn minimal_rechaza_trascendente_y_grado() {
        assert!(minimal_polynomial("sin(1)").is_err());
        assert!(minimal_polynomial("x+1").is_err());
        assert!(minimal_polynomial("sqrt(2)+sqrt(3)+sqrt(5)+sqrt(7)").is_err());
    }

    #[test]
    fn charpoly_identidad_y_2x2() {
        let id = Matrix::from_rows(vec![vec![1.0, 0.0], vec![0.0, 1.0]]).expect("id");
        let c = charpoly(&id).expect("char");
        assert_eq!(c.len(), 3);
        assert!(
            (c[0] - 1.0).abs() < 1e-9 && (c[1] + 2.0).abs() < 1e-9 && (c[2] - 1.0).abs() < 1e-9
        );
        let a = Matrix::from_rows(vec![vec![2.0, 1.0], vec![1.0, 2.0]]).expect("a");
        let c2 = charpoly(&a).expect("char2");
        assert!((c2[0] - 3.0).abs() < 1e-9 && (c2[1] + 4.0).abs() < 1e-9);
        let mala = Matrix::from_rows(vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]).expect("rect");
        assert!(charpoly(&mala).is_err());
    }

    #[test]
    fn rref_identidad_y_rango() {
        let a = Matrix::from_rows(vec![vec![1.0, 2.0], vec![3.0, 4.0]]).expect("a");
        let r = rref(&a).expect("rref");
        assert!((r.get(0, 0) - 1.0).abs() < 1e-9);
        assert!((r.get(1, 1) - 1.0).abs() < 1e-9);
        assert!(r.get(0, 1).abs() < 1e-9 && r.get(1, 0).abs() < 1e-9);
        let sing = Matrix::from_rows(vec![vec![1.0, 2.0], vec![2.0, 4.0]]).expect("s");
        let rs = rref(&sing).expect("rref s");
        assert!(rs.get(1, 0).abs() < 1e-9 && rs.get(1, 1).abs() < 1e-9);
    }

    #[test]
    fn jordan_diagonalizable_y_defectiva() {
        let a = Matrix::from_rows(vec![vec![2.0, 1.0], vec![1.0, 2.0]]).expect("a");
        let j = jordan_real(&a).expect("jordan");
        assert_eq!(j.diag.len(), 2);
        let rot = Matrix::from_rows(vec![vec![0.0, -1.0], vec![1.0, 0.0]]).expect("rot");
        assert!(jordan_real(&rot).is_err());
        let def = Matrix::from_rows(vec![vec![1.0, 1.0], vec![0.0, 1.0]]).expect("def");
        assert!(jordan_real(&def).is_err());
    }

    #[test]
    fn vectores_normalizar_y_perp() {
        let n = normalize_vec(&[3.0, 4.0]).expect("n");
        assert!((n[0] - 0.6).abs() < 1e-12 && (n[1] - 0.8).abs() < 1e-12);
        assert!(normalize_vec(&[0.0, 0.0]).is_err());
        assert!(normalize_vec(&[]).is_err());
        assert_eq!(perp2(1.0, 0.0).expect("p"), (0.0, 1.0));
        assert!(perp2(0.0, 0.0).is_err());
    }

    #[test]
    fn curvatura_con_signo() {
        // y = x² en x=1: f'=2, f''=2, κ = 2/(1+4)^1.5 > 0.
        let (k, vx, vy) = curvature_vector("x^2", "x", 1.0).expect("k");
        assert!((k - 2.0 / 5.0f64.powf(1.5)).abs() < 1e-6);
        assert!(vx < 0.0 && vy > 0.0);
        // y = -x²: signo opuesto.
        let (k2, _, _) = curvature_vector("-x^2", "x", 1.0).expect("k2");
        assert!((k2 + k).abs() < 1e-9);
        assert!(curvature_vector("x^2", "x", f64::INFINITY).is_err());
    }

    #[test]
    fn nucleo_libre_cuadrados_casos() {
        assert_eq!(nucleo_libre_cuadrados(12), (3, 2));
        assert_eq!(nucleo_libre_cuadrados(7), (7, 1));
        assert_eq!(nucleo_libre_cuadrados(36), (1, 6));
    }

    #[test]
    fn cotas_y_casos_chicos() {
        assert!(poly_from_coeffs(&vec![0.0; 1026], "x").is_err());
        let (q, r) = poly_divmod(&[5.0], &[2.0]).expect("div const");
        assert!((q[0] - 2.5).abs() < 1e-12 && r[0].abs() < 1e-12);
        assert_eq!(poly_total_degree("7").expect("g"), 0);
        assert_eq!(format_poly(&[3.0, 2.0], "x"), "2*x + 3");
        assert!(is_vertex_form_shape("-(x-1)^2").expect("v"));
        assert_eq!(perp2(0.0, 1.0).expect("p"), (-1.0, 0.0));
    }

    #[test]
    fn minimal_desplazado_y_recta() {
        // 1 + √3: (x−1)² = 3 → x² − 2x − 2.
        assert_eq!(minimal_polynomial("sqrt(3)+1").expect("m"), "x^2 - 2*x - 2");
        let (k, _, _) = curvature_vector("2*x+1", "x", 0.0).expect("k");
        assert!(k.abs() < 1e-12);
        let n = normalize_vec(&[1.0, 2.0, 2.0]).expect("n");
        assert!((n[0] - 1.0 / 3.0).abs() < 1e-12);
        assert!((n[2] - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn matrices_chicas() {
        let uno = Matrix::from_rows(vec![vec![5.0]]).expect("1x1");
        let c = charpoly(&uno).expect("char");
        assert_eq!(c.len(), 2);
        assert!((c[0] + 5.0).abs() < 1e-9 && (c[1] - 1.0).abs() < 1e-9);
        let rect = Matrix::from_rows(vec![vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]]).expect("rect");
        let r = rref(&rect).expect("rref");
        assert!((r.get(0, 2) + 1.0).abs() < 1e-9);
        assert!((r.get(1, 2) - 2.0).abs() < 1e-9);
        let id3 = Matrix::from_rows(vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ])
        .expect("id3");
        let j = jordan_real(&id3).expect("jordan");
        assert_eq!(j.diag.len(), 3);
    }
}
