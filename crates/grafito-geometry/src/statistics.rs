use std::collections::HashMap;

pub fn mean(data: &[f64]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    Some(data.iter().sum::<f64>() / data.len() as f64)
}

pub fn median(data: &[f64]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 0 {
        Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2.0)
    } else {
        Some(sorted[n / 2])
    }
}

pub fn mode(data: &[f64]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let mut counts: HashMap<String, (f64, usize)> = HashMap::new();
    for &v in data {
        let key = format!("{:.10}", v);
        let entry = counts.entry(key).or_insert((v, 0));
        entry.1 += 1;
    }
    counts.values().max_by_key(|(_, c)| *c).map(|(v, _)| *v)
}

pub fn variance(data: &[f64]) -> Option<f64> {
    let m = mean(data)?;
    let n = data.len() as f64;
    if n < 2.0 {
        return None;
    }
    Some(data.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0))
}

pub fn std_dev(data: &[f64]) -> Option<f64> {
    variance(data).map(|v| v.sqrt())
}

pub fn min(data: &[f64]) -> Option<f64> {
    data.iter().cloned().reduce(f64::min)
}

pub fn max(data: &[f64]) -> Option<f64> {
    data.iter().cloned().reduce(f64::max)
}

pub fn range(data: &[f64]) -> Option<f64> {
    Some(max(data)? - min(data)?)
}

pub fn quantile(data: &[f64], q: f64) -> Option<f64> {
    if data.is_empty() || !(0.0..=1.0).contains(&q) {
        return None;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f64;
    Some(sorted[lo] * (1.0 - frac) + sorted[hi.min(sorted.len() - 1)] * frac)
}

pub fn q1(data: &[f64]) -> Option<f64> {
    quantile(data, 0.25)
}
pub fn q3(data: &[f64]) -> Option<f64> {
    quantile(data, 0.75)
}
pub fn iqr(data: &[f64]) -> Option<f64> {
    Some(q3(data)? - q1(data)?)
}

pub fn covariance(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let mx = mean(xs)?;
    let my = mean(ys)?;
    let n = xs.len() as f64;
    Some(
        xs.iter()
            .zip(ys.iter())
            .map(|(x, y)| (x - mx) * (y - my))
            .sum::<f64>()
            / (n - 1.0),
    )
}

pub fn pearson_correlation(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let cov = covariance(xs, ys)?;
    let sx = std_dev(xs)?;
    let sy = std_dev(ys)?;
    if sx == 0.0 || sy == 0.0 {
        return None;
    }
    Some(cov / (sx * sy))
}

pub fn linear_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let mx = mean(xs)?;
    let my = mean(ys)?;
    let ss_xx: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    let ss_xy: f64 = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| (x - mx) * (y - my))
        .sum();
    if ss_xx.abs() < 1e-15 {
        return None;
    }
    let slope = ss_xy / ss_xx;
    let intercept = my - slope * mx;
    let ss_yy: f64 = ys.iter().map(|y| (y - my).powi(2)).sum();
    let r_squared = if ss_yy.abs() < 1e-15 {
        1.0
    } else {
        (ss_xy * ss_xy) / (ss_xx * ss_yy)
    };
    Some((slope, intercept, r_squared))
}

pub fn polynomial_regression(xs: &[f64], ys: &[f64], degree: usize) -> Option<Vec<f64>> {
    if xs.len() != ys.len() || xs.len() <= degree {
        return None;
    }
    let m = degree + 1;
    let mut a = vec![vec![0.0f64; m + 1]; m];
    #[allow(clippy::needless_range_loop)]
    for (i, row) in a.iter_mut().enumerate() {
        for j in 0..m {
            row[j] = xs.iter().map(|x| x.powi((i + j) as i32)).sum();
        }
        row[m] = xs
            .iter()
            .zip(ys.iter())
            .map(|(x, y)| y * x.powi(i as i32))
            .sum();
    }
    gauss_elimination(&mut a, m)
}

fn gauss_elimination(a: &mut [Vec<f64>], n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        let mut max_row = col;
        for row in (col + 1)..n {
            if a[row][col].abs() > a[max_row][col].abs() {
                max_row = row;
            }
        }
        a.swap(col, max_row);
        if a[col][col].abs() < 1e-15 {
            return None;
        }
        #[allow(clippy::needless_range_loop)]
        for row in (col + 1)..n {
            let factor = a[row][col] / a[col][col];
            for j in col..=n {
                a[row][j] -= factor * a[col][j];
            }
        }
    }
    let mut result = vec![0.0; n];
    for i in (0..n).rev() {
        result[i] = a[i][n];
        for j in (i + 1)..n {
            result[i] -= a[i][j] * result[j];
        }
        result[i] /= a[i][i];
    }
    Some(result)
}

pub fn exponential_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_ys: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
    if log_ys.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    let (slope, intercept, r2) = linear_regression(xs, &log_ys)?;
    Some((intercept.exp(), slope, r2))
}

pub fn logarithmic_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_xs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
    if log_xs.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    linear_regression(&log_xs, ys)
}

pub fn power_regression(xs: &[f64], ys: &[f64]) -> Option<(f64, f64, f64)> {
    let log_xs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
    let log_ys: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
    if log_xs.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    if log_ys.iter().any(|v| v.is_nan() || v.is_infinite()) {
        return None;
    }
    let (slope, intercept, r2) = linear_regression(&log_xs, &log_ys)?;
    Some((intercept.exp(), slope, r2))
}

pub fn histogram(data: &[f64], bins: usize) -> Vec<(f64, f64, f64)> {
    if data.is_empty() || bins == 0 {
        return vec![];
    }
    let lo = min(data).unwrap();
    let hi = max(data).unwrap();
    let width = if (hi - lo).abs() < 1e-15 {
        1.0
    } else {
        (hi - lo) / bins as f64
    };
    let mut counts = vec![0usize; bins];
    for &v in data {
        let idx = ((v - lo) / width).floor() as usize;
        let idx = idx.min(bins - 1);
        counts[idx] += 1;
    }
    counts
        .iter()
        .enumerate()
        .map(|(i, &c)| {
            let left = lo + i as f64 * width;
            (left, left + width, c as f64)
        })
        .collect()
}

pub fn frequency_table(data: &[f64]) -> Vec<(f64, usize, f64, f64)> {
    let mut counts: HashMap<String, (f64, usize)> = HashMap::new();
    for &v in data {
        let key = format!("{:.10}", v);
        let entry = counts.entry(key).or_insert((v, 0));
        entry.1 += 1;
    }
    let n = data.len() as f64;
    let mut table: Vec<_> = counts
        .values()
        .map(|(v, c)| (*v, *c, *c as f64 / n, *c as f64 / n))
        .collect();
    table.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut cum = 0.0;
    for entry in &mut table {
        cum += entry.3;
        entry.3 = cum;
    }
    table
}

pub fn boxplot_stats(data: &[f64]) -> Option<(f64, f64, f64, f64, f64, Vec<f64>)> {
    let q1 = q1(data)?;
    let med = median(data)?;
    let q3 = q3(data)?;
    let iqr = q3 - q1;
    let lower_fence = q1 - 1.5 * iqr;
    let upper_fence = q3 + 1.5 * iqr;
    let outliers: Vec<f64> = data
        .iter()
        .cloned()
        .filter(|&v| v < lower_fence || v > upper_fence)
        .collect();
    let whisker_lo = data
        .iter()
        .cloned()
        .filter(|&v| v >= lower_fence)
        .reduce(f64::min)
        .unwrap_or(q1);
    let whisker_hi = data
        .iter()
        .cloned()
        .filter(|&v| v <= upper_fence)
        .reduce(f64::max)
        .unwrap_or(q3);
    Some((whisker_lo, q1, med, q3, whisker_hi, outliers))
}

fn erf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y
}

fn gamma_ln(x: f64) -> f64 {
    let c = [
        76.18009172947146,
        -86.50532032941677,
        24.01409824083091,
        -1.231739572450155,
        0.1208650973866179e-2,
        -0.5395239384953e-5,
    ];
    let mut y = x;
    let mut tmp = x + 5.5;
    tmp -= (x + 0.5) * tmp.ln();
    let mut ser = 1.000000000190015;
    for &cj in &c {
        y += 1.0;
        ser += cj / y;
    }
    -tmp + (2.5066282746310005 * ser / x).ln()
}

pub fn normal_pdf(x: f64, mu: f64, sigma: f64) -> f64 {
    let z = (x - mu) / sigma;
    (-0.5 * z * z).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt())
}

pub fn normal_cdf(x: f64, mu: f64, sigma: f64) -> f64 {
    0.5 * (1.0 + erf((x - mu) / (sigma * std::f64::consts::SQRT_2)))
}

pub fn normal_quantile(p: f64, mu: f64, sigma: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let a = [
        -3.969_683_028_665_376e+01,
        2.209_460_984_245_205e+02,
        -2.759_285_104_469_687e+02,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e+01,
        2.506_628_277_459_239e+00,
    ];
    let b = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];
    let c = [
        -7.784894002430293e-03,
        -3.223964580411365e-01,
        -2.400758277161838e+00,
        -2.549732539343734e+00,
        4.374664141464968e+00,
        2.938163982698783e+00,
    ];
    let d = [
        7.784695709041462e-03,
        3.224671290700398e-01,
        2.445134137142996e+00,
        3.754408661907416e+00,
    ];
    let p_low = 0.02425;
    let p_high = 1.0 - p_low;
    let q;
    let r;
    if p < p_low {
        q = (-2.0 * p.ln()).sqrt();
        let z = (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
        mu + sigma * z
    } else if p <= p_high {
        q = p - 0.5;
        r = q * q;
        let z = (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0);
        mu + sigma * z
    } else {
        q = (-2.0 * (1.0 - p).ln()).sqrt();
        let z = -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0);
        mu + sigma * z
    }
}

pub fn binomial_pmf(n: u32, p: f64, k: u32) -> f64 {
    if k > n {
        return 0.0;
    }
    let ln_coeff =
        gamma_ln(n as f64 + 1.0) - gamma_ln(k as f64 + 1.0) - gamma_ln((n - k) as f64 + 1.0);
    (ln_coeff + k as f64 * p.ln() + (n - k) as f64 * (1.0 - p).ln()).exp()
}

pub fn binomial_cdf(n: u32, p: f64, k: u32) -> f64 {
    (0..=k).map(|i| binomial_pmf(n, p, i)).sum()
}

pub fn poisson_pmf(lambda: f64, k: u32) -> f64 {
    (k as f64 * lambda.ln() - lambda - gamma_ln(k as f64 + 1.0)).exp()
}

pub fn poisson_cdf(lambda: f64, k: u32) -> f64 {
    (0..=k).map(|i| poisson_pmf(lambda, i)).sum()
}

pub fn student_t_pdf(x: f64, nu: f64) -> f64 {
    let coeff =
        (gamma_ln((nu + 1.0) / 2.0) - gamma_ln(nu / 2.0) - 0.5 * (nu * std::f64::consts::PI).ln())
            .exp();
    coeff * (1.0 + x * x / nu).powf(-(nu + 1.0) / 2.0)
}

pub fn student_t_cdf(x: f64, nu: f64) -> f64 {
    let t = nu / (nu + x * x);
    let i = regularized_incomplete_beta(nu / 2.0, 0.5, t);
    if x >= 0.0 {
        1.0 - 0.5 * i
    } else {
        0.5 * i
    }
}

fn regularized_incomplete_beta(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let ln_beta = gamma_ln(a) + gamma_ln(b) - gamma_ln(a + b);
    let front = (a * x.ln() + b * (1.0 - x).ln() - ln_beta).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * continued_fraction_beta(a, b, x) / a
    } else {
        1.0 - front * continued_fraction_beta(b, a, 1.0 - x) / b
    }
}

fn continued_fraction_beta(a: f64, b: f64, x: f64) -> f64 {
    let max_iter = 200;
    let eps = 1e-14;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < 1e-30 {
        d = 1e-30;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=max_iter {
        let m = m as f64;
        let m2 = 2.0 * m;
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = 1.0 + aa / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < eps {
            break;
        }
    }
    h
}

pub fn chi_squared_pdf(x: f64, k: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let half_k = k / 2.0;
    ((half_k - 1.0) * x.ln() - x / 2.0 - half_k * (2.0_f64.ln()) - gamma_ln(half_k)).exp()
}

pub fn chi_squared_cdf(x: f64, k: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    regularized_gamma_lower(k / 2.0, x / 2.0)
}

fn regularized_gamma_lower(a: f64, x: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x < a + 1.0 {
        let mut sum = 1.0 / a;
        let mut term = 1.0 / a;
        for n in 1..200 {
            term *= x / (a + n as f64);
            sum += term;
            if term.abs() < 1e-14 * sum.abs() {
                break;
            }
        }
        sum * (-x + a * x.ln() - gamma_ln(a)).exp()
    } else {
        1.0 - regularized_gamma_upper(a, x)
    }
}

fn regularized_gamma_upper(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1e30;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..200 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < 1e-30 {
            d = 1e-30;
        }
        c = b + an / c;
        if c.abs() < 1e-30 {
            c = 1e-30;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-14 {
            break;
        }
    }
    h * (-x + a * x.ln() - gamma_ln(a)).exp()
}

pub fn f_distribution_pdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let half_d1 = d1 / 2.0;
    let half_d2 = d2 / 2.0;
    let ln_coeff = half_d1 * d1.ln() + half_d2 * d2.ln() - gamma_ln(half_d1) - gamma_ln(half_d2)
        + gamma_ln(half_d1 + half_d2);
    let ln_val = ln_coeff + (half_d1 - 1.0) * x.ln() - (half_d1 + half_d2) * (d1 * x + d2).ln();
    ln_val.exp()
}

pub fn exponential_pdf(x: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    lambda * (-lambda * x).exp()
}

pub fn exponential_cdf(x: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-lambda * x).exp()
}

pub fn geometric_pmf(p: f64, k: u32) -> f64 {
    (1.0 - p).powi(k as i32) * p
}

pub fn geometric_cdf(p: f64, k: u32) -> f64 {
    1.0 - (1.0 - p).powi(k as i32 + 1)
}

pub fn hypergeometric_pmf(n_pop: u32, k_success: u32, n_draw: u32, k_observed: u32) -> f64 {
    if k_observed > k_success || k_observed > n_draw {
        return 0.0;
    }
    if n_draw - k_observed > n_pop - k_success {
        return 0.0;
    }
    let ln_num = gamma_ln(k_success as f64 + 1.0)
        - gamma_ln(k_observed as f64 + 1.0)
        - gamma_ln((k_success - k_observed) as f64 + 1.0)
        + gamma_ln((n_pop - k_success) as f64 + 1.0)
        - gamma_ln((n_draw - k_observed) as f64 + 1.0)
        - gamma_ln((n_pop - k_success - n_draw + k_observed) as f64 + 1.0);
    let ln_den = gamma_ln(n_pop as f64 + 1.0)
        - gamma_ln(n_draw as f64 + 1.0)
        - gamma_ln((n_pop - n_draw) as f64 + 1.0);
    (ln_num - ln_den).exp()
}

pub fn logistic_pdf(x: f64, mu: f64, s: f64) -> f64 {
    let z = (x - mu) / s;
    let ez = (-z).exp();
    ez / (s * (1.0 + ez).powi(2))
}

pub fn logistic_cdf(x: f64, mu: f64, s: f64) -> f64 {
    1.0 / (1.0 + (-(x - mu) / s).exp())
}

pub fn weibull_pdf(x: f64, k: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    (k / lambda) * (x / lambda).powf(k - 1.0) * (-(x / lambda).powf(k)).exp()
}

pub fn weibull_cdf(x: f64, k: f64, lambda: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-(x / lambda).powf(k)).exp()
}

pub fn uniform_pdf(x: f64, a: f64, b: f64) -> f64 {
    if x < a || x > b {
        return 0.0;
    }
    1.0 / (b - a)
}

pub fn uniform_cdf(x: f64, a: f64, b: f64) -> f64 {
    if x < a {
        return 0.0;
    }
    if x > b {
        return 1.0;
    }
    (x - a) / (b - a)
}

pub fn gamma_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    let coef = beta.powf(alpha) / super::special_functions::gamma(alpha);
    coef * x.powf(alpha - 1.0) * (-beta * x).exp()
}

pub fn beta_pdf(x: f64, alpha: f64, beta: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) {
        return 0.0;
    }
    let coef = super::special_functions::gamma(alpha + beta)
        / (super::special_functions::gamma(alpha) * super::special_functions::gamma(beta));
    coef * x.powf(alpha - 1.0) * (1.0 - x).powf(beta - 1.0)
}

pub fn cauchy_pdf(x: f64, x0: f64, gamma: f64) -> f64 {
    1.0 / (std::f64::consts::PI * gamma * (1.0 + ((x - x0) / gamma).powi(2)))
}

pub fn cauchy_cdf(x: f64, x0: f64, gamma: f64) -> f64 {
    0.5 + (x - x0).atan() / (std::f64::consts::PI * gamma)
}

pub fn pareto_pdf(x: f64, xm: f64, alpha: f64) -> f64 {
    if x < xm {
        return 0.0;
    }
    alpha * xm.powf(alpha) / x.powf(alpha + 1.0)
}

pub fn pareto_cdf(x: f64, xm: f64, alpha: f64) -> f64 {
    if x < xm {
        return 0.0;
    }
    1.0 - (xm / x).powf(alpha)
}

pub fn rayleigh_pdf(x: f64, sigma: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    (x / sigma.powi(2)) * (-x.powi(2) / (2.0 * sigma.powi(2))).exp()
}

pub fn rayleigh_cdf(x: f64, sigma: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }
    1.0 - (-x.powi(2) / (2.0 * sigma.powi(2))).exp()
}

pub fn laplace_pdf(x: f64, mu: f64, b: f64) -> f64 {
    (1.0 / (2.0 * b)) * (-(x - mu).abs() / b).exp()
}

pub fn laplace_cdf(x: f64, mu: f64, b: f64) -> f64 {
    if x < mu {
        0.5 * ((x - mu) / b).exp()
    } else {
        1.0 - 0.5 * (-(x - mu) / b).exp()
    }
}

pub fn negative_binomial_pmf(r: u32, p: f64, k: u32) -> f64 {
    let coef = super::special_functions::gamma((k + r) as f64)
        / (super::special_functions::gamma(r as f64)
            * super::special_functions::gamma((k + 1) as f64));
    coef * p.powf(r as f64) * (1.0 - p).powf(k as f64)
}

pub fn negative_binomial_cdf(r: u32, p: f64, k: u32) -> f64 {
    let mut sum = 0.0;
    for i in 0..=k {
        sum += negative_binomial_pmf(r, p, i);
    }
    sum
}

pub fn t_test_one_sample(data: &[f64], mu0: f64) -> Option<(f64, f64)> {
    let n = data.len();
    if n < 2 {
        return None;
    }

    let sample_mean = mean(data)?;
    let sample_std = std_dev(data)?;

    let t_stat = (sample_mean - mu0) / (sample_std / (n as f64).sqrt());
    let df = (n - 1) as f64;

    let p_value = 2.0 * (1.0 - student_t_cdf(t_stat.abs(), df));

    Some((t_stat, p_value))
}

pub fn t_test_two_sample(data1: &[f64], data2: &[f64]) -> Option<(f64, f64)> {
    let n1 = data1.len();
    let n2 = data2.len();
    if n1 < 2 || n2 < 2 {
        return None;
    }

    let mean1 = mean(data1)?;
    let mean2 = mean(data2)?;
    let var1 = variance(data1)?;
    let var2 = variance(data2)?;

    let se = (var1 / n1 as f64 + var2 / n2 as f64).sqrt();
    let t_stat = (mean1 - mean2) / se;

    let df_num = (var1 / n1 as f64 + var2 / n2 as f64).powi(2);
    let df_den =
        (var1 / n1 as f64).powi(2) / (n1 - 1) as f64 + (var2 / n2 as f64).powi(2) / (n2 - 1) as f64;
    let df = df_num / df_den;

    let p_value = 2.0 * (1.0 - student_t_cdf(t_stat.abs(), df));

    Some((t_stat, p_value))
}

pub fn z_test_one_sample(data: &[f64], mu0: f64, sigma: f64) -> Option<(f64, f64)> {
    let n = data.len();
    if n < 1 {
        return None;
    }

    let sample_mean = mean(data)?;
    let z_stat = (sample_mean - mu0) / (sigma / (n as f64).sqrt());
    let p_value = 2.0 * (1.0 - normal_cdf(z_stat.abs(), 0.0, 1.0));

    Some((z_stat, p_value))
}

pub fn chi_squared_test(observed: &[f64], expected: &[f64]) -> Option<(f64, f64)> {
    if observed.len() != expected.len() || observed.len() < 2 {
        return None;
    }

    let mut chi2 = 0.0;
    for (o, e) in observed.iter().zip(expected.iter()) {
        if *e <= 0.0 {
            return None;
        }
        chi2 += (o - e).powi(2) / e;
    }

    let df = (observed.len() - 1) as f64;
    let p_value = 1.0 - chi_squared_cdf(chi2, df);

    Some((chi2, p_value))
}

pub fn anova_one_way(groups: &[&[f64]]) -> Option<(f64, f64)> {
    if groups.len() < 2 {
        return None;
    }

    let k = groups.len();
    let mut n_total = 0;
    let mut grand_sum = 0.0;

    for group in groups {
        n_total += group.len();
        grand_sum += group.iter().sum::<f64>();
    }

    if n_total < k + 1 {
        return None;
    }

    let grand_mean = grand_sum / n_total as f64;

    let mut ss_between = 0.0;
    let mut ss_within = 0.0;

    for group in groups {
        let group_mean = mean(group)?;
        let n_i = group.len() as f64;
        ss_between += n_i * (group_mean - grand_mean).powi(2);

        for &x in *group {
            ss_within += (x - group_mean).powi(2);
        }
    }

    let df_between = (k - 1) as f64;
    let df_within = (n_total - k) as f64;

    let ms_between = ss_between / df_between;
    let ms_within = ss_within / df_within;

    let f_stat = ms_between / ms_within;
    let p_value = 1.0 - f_distribution_cdf(f_stat, df_between, df_within);

    Some((f_stat, p_value))
}

pub fn f_distribution_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if x < 0.0 {
        return 0.0;
    }

    let z = d1 * x / (d1 * x + d2);
    regularized_incomplete_beta(d1 / 2.0, d2 / 2.0, z)
}

pub fn confidence_interval_mean(data: &[f64], confidence: f64) -> Option<(f64, f64, f64)> {
    let n = data.len();
    if n < 2 {
        return None;
    }

    let sample_mean = mean(data)?;
    let sample_std = std_dev(data)?;
    let se = sample_std / (n as f64).sqrt();

    let alpha = 1.0 - confidence;
    let t_crit = normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0);

    let margin = t_crit * se;
    let lower = sample_mean - margin;
    let upper = sample_mean + margin;

    Some((lower, sample_mean, upper))
}

pub fn confidence_interval_proportion(
    successes: u32,
    n: u32,
    confidence: f64,
) -> Option<(f64, f64, f64)> {
    if n == 0 || successes > n {
        return None;
    }

    let p_hat = successes as f64 / n as f64;
    let se = (p_hat * (1.0 - p_hat) / n as f64).sqrt();

    let alpha = 1.0 - confidence;
    let z_crit = normal_quantile(1.0 - alpha / 2.0, 0.0, 1.0);

    let margin = z_crit * se;
    let lower = (p_hat - margin).max(0.0);
    let upper = (p_hat + margin).min(1.0);

    Some((lower, p_hat, upper))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mean() {
        assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0, 5.0]), Some(3.0));
        assert_eq!(mean(&[]), None);
    }

    #[test]
    fn test_median() {
        assert_eq!(median(&[1.0, 3.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
    }

    #[test]
    fn test_std_dev() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let sd = std_dev(&data).unwrap();
        assert!((sd - 2.138).abs() < 0.01);
    }

    #[test]
    fn test_linear_regression() {
        let xs = [1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = [2.0, 4.0, 6.0, 8.0, 10.0];
        let (slope, intercept, r2) = linear_regression(&xs, &ys).unwrap();
        assert!((slope - 2.0).abs() < 1e-10);
        assert!((intercept - 0.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_normal_cdf() {
        assert!((normal_cdf(0.0, 0.0, 1.0) - 0.5).abs() < 1e-7);
        assert!((normal_cdf(1.96, 0.0, 1.0) - 0.975).abs() < 0.001);
    }

    #[test]
    fn test_binomial() {
        assert!((binomial_pmf(10, 0.5, 5) - 0.2461).abs() < 0.001);
    }

    #[test]
    fn test_histogram() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let h = histogram(&data, 5);
        assert_eq!(h.len(), 5);
        let total: f64 = h.iter().map(|(_, _, c)| c).sum();
        assert_eq!(total, 10.0);
    }

    #[test]
    fn test_boxplot() {
        let data = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 100.0];
        let (_wl, _q1, med, _q3, _wh, outliers) = boxplot_stats(&data).unwrap();
        assert!((med - 5.5).abs() < 0.1);
        assert!(!outliers.is_empty());
    }
}
