//! Frente P0.2: teoría de números elemental sobre `i128` verificado.
//!
//! Wrappers honestos sin dependencias nuevas y sin MSRV nueva: Euclides
//! propio (no se usa `num-integer` porque no es dependencia directa de
//! `grafito-geometry`), Miller-Rabin determinista para primos `< 2^64`,
//! bases `2..=36` y fracciones continuas acotadas. Todo borde devuelve
//! `Err` en español; jamás pánico, jamás `unwrap` en producción.

use std::collections::BTreeMap;

/// Cota superior inclusiva de primalidad (igual que `IsPrime`/`PrimeFactors`).
pub const PRIME_UPPER_BOUND_I128: i128 = 1_000_000_000_000;

/// Máximo de términos de una fracción continua.
pub const MAX_CF_TERMS: usize = 64;

/// Máximo de elementos de una lista de divisores en salida.
pub const MAX_DIVISOR_LIST: usize = 10_000;

/// Máximo de caracteres Unicode reconstruidos por `unicode_to_text`.
pub const MAX_UNICODE_TEXT_CHARS: usize = 10_000;

/// Máximo de dígitos aceptados en `from_base` (muy por encima de `i128`).
pub const MAX_BASE_INPUT_LEN: usize = 256;

/// Convierte un `f64` finito e integral a `i128` sin el `as` saturante.
pub fn int_from_f64(value: f64, name: &str) -> Result<i128, String> {
    if !value.is_finite() {
        return Err(format!("{name}: se esperaba un entero finito"));
    }
    if value.fract() != 0.0 {
        return Err(format!("{name}: '{value}' no es entero"));
    }
    if value < i128::MIN as f64 || value >= 2f64.powi(127) {
        return Err(format!("{name}: '{value}' excede el rango i128"));
    }
    Ok(value as i128)
}

/// Parsea un entero desde texto (i128 exacto) o desde `f64` integral.
///
/// Acepta `_` como separador visual (`1_000`). No resuelve variables: el
/// llamador expande antes con `parse_numeric_arg` y pasa el `f64` por
/// `int_from_f64` cuando el texto no es un entero literal.
pub fn parse_int_text(text: &str, name: &str) -> Result<i128, String> {
    let cleaned: String = text.trim().chars().filter(|c| *c != '_').collect();
    if cleaned.is_empty() {
        return Err(format!("{name}: se esperaba un entero no vacío"));
    }
    if let Ok(direct) = cleaned.parse::<i128>() {
        return Ok(direct);
    }
    match cleaned.parse::<f64>() {
        Ok(value) => int_from_f64(value, name),
        Err(_) => Err(format!("{name}: '{text}' no es un entero")),
    }
}

/// Resuelve un entero desde texto literal (i128 exacto, sin pérdida) o
/// desde expresión/variable del documento (vía el evaluador, luego entero).
pub fn resolve_int_arg(
    raw: &str,
    vars: &BTreeMap<String, f64>,
    name: &str,
) -> Result<i128, String> {
    let texto = raw.trim();
    if texto.is_empty() {
        return Err(format!("{name}: se esperaba un entero no vacío"));
    }
    match parse_int_text(texto, name) {
        Ok(v) => Ok(v),
        Err(_) => {
            let lista: Vec<(String, f64)> = vars.iter().map(|(k, v)| (k.clone(), *v)).collect();
            match crate::expr::evaluate(texto, &lista) {
                Ok(valor) => int_from_f64(valor, name),
                Err(_) => Err(format!("{name}: '{texto}' no es un entero")),
            }
        }
    }
}

/// Máximo común divisor (Euclides). `gcd(0, 0) = 0` por convención GeoGebra.
#[must_use]
pub fn gcd(mut a: i128, mut b: i128) -> i128 {
    a = a.wrapping_abs();
    b = b.wrapping_abs();
    // `unsigned_abs` evita el borde `i128::MIN` del `wrapping_abs`.
    let mut x = a.unsigned_abs();
    let mut y = b.unsigned_abs();
    while y != 0 {
        let r = x % y;
        x = y;
        y = r;
    }
    if x > i128::MAX as u128 {
        i128::MAX
    } else {
        x as i128
    }
}

/// Mínimo común múltiplo con chequeo de overflow. `lcm(0, _) = 0`.
pub fn lcm(a: i128, b: i128) -> Result<i128, String> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    let divisor = gcd(a, b);
    if divisor <= 0 {
        return Err("el MCM excede el rango i128".to_string());
    }
    let cociente = a
        .checked_div(divisor)
        .ok_or_else(|| "el MCM excede el rango i128".to_string())?;
    let producto = cociente
        .checked_mul(b)
        .ok_or_else(|| "el MCM excede el rango i128".to_string())?;
    let resultado = producto.wrapping_abs();
    if resultado < 0 {
        return Err("el MCM excede el rango i128".to_string());
    }
    Ok(resultado)
}

/// Euclides extendido: `(g, x, y)` con `a·x + b·y = g = gcd(a, b)`.
pub fn extended_gcd(a: i128, b: i128) -> Result<(i128, i128, i128), String> {
    let err = || "Euclides extendido excede el rango i128".to_string();
    let (mut viejo_r, mut r) = (a, b);
    let (mut viejo_s, mut s) = (1i128, 0i128);
    let (mut viejo_t, mut t) = (0i128, 1i128);
    while r != 0 {
        let cociente = viejo_r.checked_div(r).ok_or_else(err)?;
        let resto = viejo_r.checked_rem(r).ok_or_else(err)?;
        viejo_r = r;
        r = resto;
        let qs = cociente.checked_mul(s).ok_or_else(err)?;
        let qt = cociente.checked_mul(t).ok_or_else(err)?;
        let ns = viejo_s.checked_sub(qs).ok_or_else(err)?;
        let nt = viejo_t.checked_sub(qt).ok_or_else(err)?;
        viejo_s = s;
        s = ns;
        viejo_t = t;
        t = nt;
    }
    Ok((viejo_r, viejo_s, viejo_t))
}

/// Lista ordenada de divisores positivos de `n` (`1 <= n <= 1e12`).
pub fn divisors(n: i128) -> Result<Vec<i128>, String> {
    if n < 1 {
        return Err("se esperaba un entero n >= 1".to_string());
    }
    if n > PRIME_UPPER_BOUND_I128 {
        return Err(format!(
            "n excede la cota 1e12 (llegó {n}); trial division honesto hasta √n"
        ));
    }
    let mut bajos = Vec::new();
    let mut altos = Vec::new();
    let mut d: i128 = 1;
    while let Some(cuadrado) = d.checked_mul(d) {
        if cuadrado > n {
            break;
        }
        if n % d == 0 {
            bajos.push(d);
            let otro = n / d;
            if otro != d {
                altos.push(otro);
            }
        }
        if bajos.len() + altos.len() > MAX_DIVISOR_LIST {
            return Err(format!("demasiados divisores (más de {MAX_DIVISOR_LIST})"));
        }
        d += 1;
    }
    altos.reverse();
    bajos.extend(altos);
    Ok(bajos)
}

/// Suma de divisores σ(n) con chequeo de overflow.
pub fn divisors_sigma(n: i128) -> Result<i128, String> {
    let lista = divisors(n)?;
    let mut total: i128 = 0;
    for d in lista {
        total = total
            .checked_add(d)
            .ok_or_else(|| "la suma de divisores excede i128".to_string())?;
    }
    Ok(total)
}

fn mod_mul(a: u64, b: u64, m: u64) -> u64 {
    // `a, b < m <= u64::MAX`: el producto cabe en `u128` sin overflow.
    ((u128::from(a) * u128::from(b)) % u128::from(m)) as u64
}

fn mod_pow(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut acc: u64 = 1;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mod_mul(acc, base, m);
        }
        base = mod_mul(base, base, m);
        exp >>= 1;
    }
    acc
}

/// Miller-Rabin determinista para `n < 2^64` (bases 2..17, exacto).
#[must_use]
pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for primo in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n == primo {
            return true;
        }
        if n.is_multiple_of(primo) {
            return false;
        }
    }
    let mut d = n - 1;
    let mut vueltas = 0u32;
    while d.is_multiple_of(2) {
        d /= 2;
        vueltas += 1;
    }
    // Determinista para 64 bits con estas 7 bases.
    for base in [2u64, 3, 5, 7, 11, 13, 17] {
        let mut x = mod_pow(base % n, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        let mut compuesto = true;
        for _ in 1..vueltas {
            x = mod_mul(x, x, n);
            if x == n - 1 {
                compuesto = false;
                break;
            }
        }
        if compuesto {
            return false;
        }
    }
    true
}

fn check_prime_arg(n: i128, name: &str) -> Result<u64, String> {
    if !(0..=PRIME_UPPER_BOUND_I128).contains(&n) {
        return Err(format!(
            "{name}: se esperaba un entero entre 0 y 1e12 (cota de criba honesta)"
        ));
    }
    Ok(n as u64)
}

/// Menor primo estrictamente mayor que `n` (`0 <= n < 1e12`).
pub fn next_prime(n: i128) -> Result<i128, String> {
    let base = check_prime_arg(n, "NextPrime")?;
    let mut candidato = base.saturating_add(1);
    while (candidato as i128) <= PRIME_UPPER_BOUND_I128 {
        if is_prime_u64(candidato) {
            return Ok(candidato as i128);
        }
        candidato = candidato.saturating_add(1);
        if candidato == u64::MAX {
            break;
        }
    }
    Err("no hay primo siguiente dentro de la cota 1e12".to_string())
}

/// Mayor primo estrictamente menor que `n` (`2 < n <= 1e12`).
pub fn previous_prime(n: i128) -> Result<i128, String> {
    let base = check_prime_arg(n, "PreviousPrime")?;
    if base <= 2 {
        return Err("no hay primo menor que 2".to_string());
    }
    let mut candidato = base - 1;
    while candidato >= 2 {
        if is_prime_u64(candidato) {
            return Ok(candidato as i128);
        }
        if candidato == 2 {
            break;
        }
        candidato -= 1;
    }
    Err("no hay primo menor dentro del rango".to_string())
}

/// Exponenciación modular binaria con `checked_*` (`exp >= 0`, `mod >= 2`;
/// `mod = 1` da 0 por convención).
pub fn modular_exponent(base: i128, exp: i128, modulus: i128) -> Result<i128, String> {
    if exp < 0 {
        return Err("el exponente debe ser >= 0 (sin inversos modulares)".to_string());
    }
    if modulus < 1 {
        return Err("el módulo debe ser >= 1".to_string());
    }
    if modulus == 1 {
        return Ok(0);
    }
    let m = modulus as u128;
    let mut b = base.rem_euclid(modulus) as u128;
    let mut acc: u128 = 1;
    let mut e = exp as u128;
    while e > 0 {
        if e & 1 == 1 {
            acc = acc
                .checked_mul(b)
                .and_then(|v| v.checked_rem(m))
                .ok_or_else(|| "overflow en exponenciación modular".to_string())?;
        }
        b = b
            .checked_mul(b)
            .and_then(|v| v.checked_rem(m))
            .ok_or_else(|| "overflow en exponenciación modular".to_string())?;
        e >>= 1;
    }
    i128::try_from(acc).map_err(|_| "overflow en exponenciación modular".to_string())
}

/// División euclídea: `(cociente, resto)` con `0 <= resto < |b|`.
pub fn euclid_divmod(a: i128, b: i128) -> Result<(i128, i128), String> {
    if b == 0 {
        return Err("división por cero".to_string());
    }
    let resto = a.rem_euclid(b);
    let cociente = a
        .checked_sub(resto)
        .and_then(|v| v.checked_div(b))
        .ok_or_else(|| "la división excede el rango i128".to_string())?;
    Ok((cociente, resto))
}

/// Convierte `n` a base `2..=36` (dígitos `0-9a-z`, `-` si negativo).
pub fn to_base(n: i128, base: u32) -> Result<String, String> {
    if !(2..=36).contains(&base) {
        return Err(format!("la base debe estar entre 2 y 36 (llegó {base})"));
    }
    if n == 0 {
        return Ok("0".to_string());
    }
    let negativo = n < 0;
    let mut resto_abs = n.unsigned_abs();
    let mut digitos = Vec::new();
    let b = u128::from(base);
    while resto_abs > 0 {
        let d = (resto_abs % b) as u32;
        let ch = char::from_digit(d, base).ok_or_else(|| "dígito fuera de rango".to_string())?;
        digitos.push(ch);
        resto_abs /= b;
    }
    if negativo {
        digitos.push('-');
    }
    Ok(digitos.iter().rev().collect())
}

/// Parsea `texto` en base `2..=36` a `i128` (signo opcional, sin prefijos).
pub fn from_base(text: &str, base: u32) -> Result<i128, String> {
    if !(2..=36).contains(&base) {
        return Err(format!("la base debe estar entre 2 y 36 (llegó {base})"));
    }
    let limpio = text.trim();
    if limpio.is_empty() {
        return Err("se esperaba un número no vacío en la base dada".to_string());
    }
    if limpio.len() > MAX_BASE_INPUT_LEN {
        return Err(format!("la entrada excede {MAX_BASE_INPUT_LEN} caracteres"));
    }
    let (negativo, cuerpo) = match limpio.strip_prefix('-') {
        Some(resto) => (true, resto),
        None => (false, limpio.strip_prefix('+').unwrap_or(limpio)),
    };
    if cuerpo.is_empty() {
        return Err("se esperaba un número no vacío en la base dada".to_string());
    }
    let mut acc: i128 = 0;
    for ch in cuerpo.chars() {
        let digito = ch
            .to_digit(base)
            .ok_or_else(|| format!("dígito '{ch}' inválido en base {base}"))?;
        acc = acc
            .checked_mul(i128::from(base))
            .and_then(|v| v.checked_add(i128::from(digito)))
            .ok_or_else(|| "el valor excede el rango i128".to_string())?;
    }
    if negativo {
        acc = acc
            .checked_neg()
            .ok_or_else(|| "el valor excede el rango i128".to_string())?;
    }
    Ok(acc)
}

/// Fracción continua simple de `x` con cota de términos (incluye el entero).
///
/// Corta cuando el denominador del convergente supera 1e12: más allá solo
/// hay ruido de representación binaria del `f64`, no información del número
/// (p. ej. π como `f64` daría una cola espuria tras `[3,7,15,1,25,...]`).
pub fn continued_fraction(x: f64, max_terms: usize) -> Result<Vec<i128>, String> {
    if !x.is_finite() {
        return Err("se esperaba un número finito".to_string());
    }
    if max_terms == 0 || max_terms > MAX_CF_TERMS {
        return Err(format!(
            "la cantidad de términos debe estar entre 1 y {MAX_CF_TERMS}"
        ));
    }
    const MAX_CONVERGENT_DENOM: f64 = 1e12;
    let mut terminos = Vec::with_capacity(max_terms);
    let (mut p_prev, mut p_curr) = (1i128, 0i128);
    let (mut q_prev, mut q_curr) = (0i128, 1i128);
    let mut resto = x;
    for _ in 0..max_terms {
        if !resto.is_finite() {
            break;
        }
        let parte = resto.floor();
        if parte < i128::MIN as f64 || parte >= 2f64.powi(127) {
            return Err("término de la fracción continua fuera de rango".to_string());
        }
        let a = parte as i128;
        terminos.push(a);
        // Recurrencia de convergentes pₙ/qₙ; corta ante denominador enorme.
        let (p_next, q_next) = (
            a.checked_mul(p_curr).and_then(|v| v.checked_add(p_prev)),
            a.checked_mul(q_curr).and_then(|v| v.checked_add(q_prev)),
        );
        match (p_next, q_next) {
            (Some(p), Some(q)) => {
                p_prev = p_curr;
                p_curr = p;
                q_prev = q_curr;
                q_curr = q;
                if (q_curr as f64).abs() > MAX_CONVERGENT_DENOM {
                    break;
                }
            }
            _ => break,
        }
        let frac = resto - parte;
        if frac.abs() < 1e-15 {
            break;
        }
        resto = 1.0 / frac;
    }
    if terminos.is_empty() {
        return Err("no se pudo expandir la fracción continua".to_string());
    }
    Ok(terminos)
}

/// `LetterToUnicode["a"]` → `U+0061 (97)`. Exactamente un carácter.
pub fn letter_to_unicode(text: &str) -> Result<String, String> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(unico), None) => Ok(format!("U+{:04X} ({})", unico as u32, unico as u32)),
        _ => Err("se esperaba exactamente un carácter".to_string()),
    }
}

/// `UnicodeToLetter[97]` / `["U+0061"]` / `["0x61"]` → carácter.
pub fn unicode_to_letter(text: &str) -> Result<char, String> {
    let limpio = text.trim();
    let numero = if let Some(hex) = limpio
        .strip_prefix("U+")
        .or_else(|| limpio.strip_prefix("u+"))
    {
        u32::from_str_radix(hex.trim(), 16)
            .map_err(|_| format!("'{text}' no es un punto de código válido (U+HHHH o decimal)"))?
    } else if let Some(hex) = limpio
        .strip_prefix("0x")
        .or_else(|| limpio.strip_prefix("0X"))
    {
        u32::from_str_radix(hex.trim(), 16)
            .map_err(|_| format!("'{text}' no es un punto de código válido (0xHH o decimal)"))?
    } else {
        limpio
            .parse::<u32>()
            .map_err(|_| format!("'{text}' no es un punto de código válido (U+HHHH o decimal)"))?
    };
    char::from_u32(numero).ok_or_else(|| format!("U+{numero:04X} no es un carácter Unicode válido"))
}

/// `TextToUnicode["hola"]` → `{104, 111, 108, 97}` (UTF-8 seguro).
pub fn text_to_unicode(text: &str) -> String {
    text.chars()
        .map(|c| (c as u32).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `UnicodeToText["{104, 111}"]` → `"hola"`. Acepta llaves, comas,
/// espacios y formas `U+HHHH`/`0xHH` mezcladas.
pub fn unicode_to_text(text: &str) -> Result<String, String> {
    let sin_llaves = text.trim().trim_start_matches('{').trim_end_matches('}');
    if sin_llaves.trim().is_empty() {
        return Err("se esperaba al menos un punto de código".to_string());
    }
    // El `;` se rechaza para no ambiguar listas futuras; `,` y espacios separan.
    if sin_llaves.contains(';') {
        return Err("separador ';' no soportado (usa ',' o espacios)".to_string());
    }
    let mut salida = String::new();
    let mut cantidad = 0usize;
    for token in sin_llaves.split([',', ' ', '\t', '\n']) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let ch = unicode_to_letter(token)?;
        salida.push(ch);
        cantidad += 1;
        if cantidad > MAX_UNICODE_TEXT_CHARS {
            return Err(format!(
                "el texto excede {MAX_UNICODE_TEXT_CHARS} caracteres"
            ));
        }
    }
    if salida.is_empty() {
        return Err("se esperaba al menos un punto de código".to_string());
    }
    Ok(salida)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gcd_basico_y_ceros() {
        assert_eq!(gcd(12, 18), 6);
        assert_eq!(gcd(-12, 18), 6);
        assert_eq!(gcd(0, 5), 5);
        assert_eq!(gcd(0, 0), 0);
    }

    #[test]
    fn lcm_basico_y_cero() {
        assert_eq!(lcm(4, 6), Ok(3 * 4));
        assert_eq!(lcm(0, 7), Ok(0));
        assert_eq!(lcm(-4, 6), Ok(12));
    }

    #[test]
    fn lcm_overflow_honesto() {
        assert!(lcm(i128::MAX, 2).is_err());
    }

    #[test]
    fn extended_gcd_bezout() {
        let (g, x, y) = extended_gcd(30, 21).expect("bezout");
        assert_eq!(g, 3);
        assert_eq!(30 * x + 21 * y, 3);
    }

    #[test]
    fn extended_gcd_ceros() {
        let (g, _, _) = extended_gcd(0, 5).expect("bezout cero");
        assert_eq!(g, 5);
    }

    #[test]
    fn int_from_f64_rechaza_fraccion_e_inf() {
        assert!(int_from_f64(2.5, "n").is_err());
        assert!(int_from_f64(f64::INFINITY, "n").is_err());
        assert_eq!(int_from_f64(-7.0, "n"), Ok(-7));
    }

    #[test]
    fn parse_int_text_exactitud_i128() {
        assert_eq!(
            parse_int_text("170141183460469231731687303715884105727", "n"),
            Ok(i128::MAX)
        );
        assert!(parse_int_text("170141183460469231731687303715884105728", "n").is_err());
        assert_eq!(parse_int_text("1_000", "n"), Ok(1000));
        assert!(parse_int_text("abc", "n").is_err());
        assert!(parse_int_text("", "n").is_err());
    }

    #[test]
    fn divisores_de_28() {
        assert_eq!(divisors(28), Ok(vec![1, 2, 4, 7, 14, 28]));
    }

    #[test]
    fn divisores_rechaza_cero_y_cota() {
        assert!(divisors(0).is_err());
        assert!(divisors(-4).is_err());
        assert!(divisors(1_000_000_000_001).is_err());
    }

    #[test]
    fn sigma_de_12() {
        assert_eq!(divisors_sigma(12), Ok(1 + 2 + 3 + 4 + 6 + 12));
    }

    #[test]
    fn miller_rabin_primos_y_compuestos() {
        for primo in [2u64, 3, 97, 1_000_003, 999_999_999_989] {
            assert!(is_prime_u64(primo), "{primo} debe ser primo");
        }
        for compuesto in [0u64, 1, 4, 100, 1_000_000_000_000] {
            assert!(!is_prime_u64(compuesto), "{compuesto} no debe ser primo");
        }
    }

    #[test]
    fn next_y_previous_prime() {
        assert_eq!(next_prime(7), Ok(11));
        assert_eq!(next_prime(2), Ok(3));
        assert_eq!(previous_prime(10), Ok(7));
        assert_eq!(previous_prime(3), Ok(2));
        assert!(previous_prime(2).is_err());
        assert!(next_prime(1_000_000_000_000).is_err());
        assert!(next_prime(-1).is_err());
    }

    #[test]
    fn modo_exponencial() {
        assert_eq!(modular_exponent(2, 10, 1000), Ok(24));
        assert_eq!(modular_exponent(5, 0, 7), Ok(1));
        assert_eq!(modular_exponent(9, 3, 1), Ok(0));
        assert!(modular_exponent(2, -1, 7).is_err());
        assert!(modular_exponent(2, 3, 0).is_err());
    }

    #[test]
    fn division_euclidea_signos() {
        assert_eq!(euclid_divmod(7, 3), Ok((2, 1)));
        assert_eq!(euclid_divmod(-7, 3), Ok((-3, 2)));
        assert_eq!(euclid_divmod(7, -3), Ok((-2, 1)));
        assert!(euclid_divmod(7, 0).is_err());
    }

    #[test]
    fn cambio_de_base_ida_y_vuelta() {
        assert_eq!(to_base(255, 16), Ok("ff".to_string()));
        assert_eq!(to_base(-10, 2), Ok("-1010".to_string()));
        assert_eq!(to_base(0, 36), Ok("0".to_string()));
        assert!(to_base(5, 1).is_err());
        assert!(to_base(5, 37).is_err());
        assert_eq!(from_base("ff", 16), Ok(255));
        assert_eq!(from_base("-1010", 2), Ok(-10));
        assert!(from_base("ff", 10).is_err());
        assert!(from_base("", 10).is_err());
        assert_eq!(from_base("zz", 36), Ok(35 * 36 + 35));
    }

    #[test]
    fn fraccion_continua_basica_y_cotas() {
        assert_eq!(continued_fraction(4.0, 8), Ok(vec![4]));
        let cf = continued_fraction(3.245, 5).expect("cf");
        assert!(cf.len() >= 2 && cf[0] == 3);
        assert!(continued_fraction(f64::NAN, 8).is_err());
        assert!(continued_fraction(1.5, 0).is_err());
        assert!(continued_fraction(1.5, 65).is_err());
    }

    #[test]
    fn unicode_letra_ida_y_vuelta() {
        assert_eq!(letter_to_unicode("a"), Ok("U+0061 (97)".to_string()));
        assert!(letter_to_unicode("ab").is_err());
        assert!(letter_to_unicode("").is_err());
        assert_eq!(unicode_to_letter("97"), Ok('a'));
        assert_eq!(unicode_to_letter("U+0061"), Ok('a'));
        assert_eq!(unicode_to_letter("0x61"), Ok('a'));
        assert!(unicode_to_letter("U+D800").is_err());
        assert!(unicode_to_letter("xyz").is_err());
    }

    #[test]
    fn unicode_texto_ida_y_vuelta() {
        assert_eq!(text_to_unicode("AB"), "65, 66".to_string());
        assert_eq!(unicode_to_text("{65, 66}"), Ok("AB".to_string()));
        assert_eq!(unicode_to_text("65 66"), Ok("AB".to_string()));
        assert_eq!(unicode_to_text("U+0041, U+0042"), Ok("AB".to_string()));
        assert!(unicode_to_text("").is_err());
        assert!(unicode_to_text("{}").is_err());
    }

    #[test]
    fn unicode_multibyte_seguro() {
        assert_eq!(text_to_unicode("ñ"), "241".to_string());
        assert_eq!(unicode_to_text("241"), Ok("ñ".to_string()));
        assert_eq!(unicode_to_letter("241"), Ok('ñ'));
        assert_eq!(letter_to_unicode("ñ"), Ok("U+00F1 (241)".to_string()));
    }

    #[test]
    fn resolve_int_arg_literal_y_variable() {
        let vacio = BTreeMap::new();
        assert_eq!(resolve_int_arg("42", &vacio, "t"), Ok(42));
        assert_eq!(resolve_int_arg("2^10", &vacio, "t"), Ok(1024));
        assert!(resolve_int_arg("2.5", &vacio, "t").is_err());
        assert!(resolve_int_arg("", &vacio, "t").is_err());
        let mut vars = BTreeMap::new();
        vars.insert("n".to_string(), 5.0);
        assert_eq!(resolve_int_arg("n", &vars, "t"), Ok(5));
        assert_eq!(resolve_int_arg("2*n", &vars, "t"), Ok(10));
        assert!(resolve_int_arg("zzz", &vars, "t").is_err());
    }

    #[test]
    fn primos_borde_y_fermat() {
        assert_eq!(divisors(1), Ok(vec![1]));
        assert_eq!(divisors_sigma(1), Ok(1));
        assert_eq!(next_prime(0), Ok(2));
        assert_eq!(previous_prime(4), Ok(3));
        assert_eq!(previous_prime(100), Ok(97));
        assert_eq!(modular_exponent(2, 100, 101), Ok(1));
        assert!(is_prime_u64(2_147_483_647));
    }

    #[test]
    fn euclides_casos_borde() {
        assert_eq!(gcd(1, 1), 1);
        assert_eq!(gcd(-7, -7), 7);
        assert_eq!(lcm(7, 7), Ok(7));
        assert_eq!(extended_gcd(7, 7).expect("e"), (7, 0, 1));
        assert_eq!(euclid_divmod(0, 5), Ok((0, 0)));
    }

    #[test]
    fn bases_borde_y_overflow() {
        assert_eq!(
            to_base(i128::MIN + 1, 16).map(|s| s.starts_with('-')),
            Ok(true)
        );
        assert!(from_base("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", 36).is_err());
        assert_eq!(from_base("10", 2), Ok(2));
        assert!(unicode_to_text("65;66").is_err());
    }
}
