//! Parsing CAS — extraído de `commands.rs` para reducir el god file.
//!
//! Contiene `CasCmd`, `sanitize_unicode_input`, `extract_cas_command`,
//! `expand_all_cas`, `parse_cas_command`, `split_args` y
//! `looks_like_bracketed_command`. Todos son puros o de bajo acoplamiento
//! y testeables sin UI. Fase 1 de split seguro.

use grafito_core::{Document, GeoObject, ObjectId};
use grafito_geometry::expr::evaluate;
use grafito_geometry::symbolic;
use std::collections::HashMap;

pub(crate) fn sanitize_unicode_input(raw_text: &str) -> String {
    raw_text
        .replace("F(x)", "f(x)")
        .replace("G(x)", "g(x)")
        .replace("x²", "x^2")
        .replace("y²", "y^2")
        .replace("z²", "z^2")
        .replace("t²", "t^2")
        .replace("r²", "r^2")
        .replace("a²", "a^2")
        .replace("b²", "b^2")
        .replace("c²", "c^2")
        .replace("n²", "n^2")
        .replace("θ²", "θ^2")
        .replace("φ²", "φ^2")
        .replace("√", "sqrt")
        .replace("|x|", "abs(x)")
        .replace("π", "pi")
        .replace("τ", "tau")
        .replace("÷", "/")
        .replace("×", "*")
        .replace("−", "-")
        .replace("≤", "<=")
        .replace("≥", ">=")
        .replace("x³", "x^3")
        .replace("y³", "y^3")
        .replace("z³", "z^3")
}

pub struct CasCmd {
    pub command: String,
    pub args: Vec<String>,
}

pub fn extract_cas_command(text: &str) -> Option<(String, String, std::ops::Range<usize>)> {
    let keywords = [
        "Derivative",
        "Integral",
        "Solve",
        "Limit",
        "LimitAbove",
        "LimitBelow",
        "ParametricDerivative",
        "Asymptote",
        "GroebnerDegRevLex",
        "GroebnerBasis",
        "Groebner",
        "Factor",
        "Expand",
        "Simplify",
        "Taylor",
        "CompleteSquare",
        "PrimeFactors",
        "IFactor",
        "Assume",
        "deriv",
        "diff",
        "int",
        "nsolve",
        "lim",
        "derivada",
        "integrar",
        "resolver",
        "limite",
        "factorizar",
        "expandir",
        "simplificar",
        "completarCuadrado",
        "prime_factors",
        "ifactor",
        "assume",
    ];

    for &kw in &keywords {
        let mut start_idx = 0;
        while let Some(idx) = text[start_idx..].find(kw) {
            let actual_idx = start_idx + idx;
            let after_kw = &text[actual_idx + kw.len()..];
            let trimmed = after_kw.trim_start();
            if trimmed.starts_with('[') {
                let bracket_start = actual_idx + kw.len() + (after_kw.len() - trimmed.len());
                let mut depth = 0;
                let mut bracket_end = None;
                for (i, c) in text[bracket_start..].char_indices() {
                    if c == '[' {
                        depth += 1;
                    } else if c == ']' {
                        depth -= 1;
                        if depth == 0 {
                            bracket_end = Some(bracket_start + i);
                            break;
                        }
                    }
                }

                if let Some(end) = bracket_end {
                    let cmd_name = kw.to_string();
                    let inner = text[bracket_start + 1..end].to_string();
                    return Some((cmd_name, inner, actual_idx..end + 1));
                }
            }
            start_idx = actual_idx + kw.len();
        }
    }
    None
}

pub fn expand_all_cas(text: &str, document: &Document) -> String {
    let mut current = text.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 50;
    while let Some((cmd, inner, range)) = extract_cas_command(&current) {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            break;
        }
        let expanded_inner = expand_all_cas(&inner, document);
        let args: Vec<String> = split_args(&expanded_inner)
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect();
        let mut resolved_expr = String::new();

        let normalized = match cmd.to_lowercase().as_str() {
            "derivative" | "derivada" | "deriv" | "diff" => "Derivative",
            "integral" | "integrar" | "int" => "Integral",
            "solve" | "nsolve" | "resolver" => "Solve",
            "limit" | "limite" | "lim" => "Limit",
            "limitabove" | "limite_superior" | "limite_derecho" => "LimitAbove",
            "limitbelow" | "limite_inferior" | "limite_izquierdo" => "LimitBelow",
            "parametricderivative" | "derivada_parametrica" | "derivadaParametrica" => {
                "ParametricDerivative"
            }
            "asymptote" | "asintota" | "asíntota" => "Asymptote",
            "groebner" | "groebnerbasis" | "groebner_basis" | "groebnerdegrevlex"
            | "groebnerlex" => "GroebnerDegRevLex",
            "factor" | "factorizar" => "Factor",
            "expand" | "expandir" => "Expand",
            "simplify" | "simplificar" => "Simplify",
            "taylor" => "Taylor",
            "completesquare" | "complete_square" | "completarcuadrado" | "completar_cuadrado" => {
                "CompleteSquare"
            }
            "primefactors" | "prime_factors" | "factoresprimos" | "factores_primos" => {
                "PrimeFactors"
            }
            "ifactor" | "ifactorizar" | "factorentero" | "factor_entero" => "IFactor",
            "assume" | "asumir" | "suponer" | "supone" => "Assume",
            "tangentat" | "tangenteen" => "TangentAt",
            "normalat" | "normalen" => "NormalAt",
            "arclength" | "longitudarco" => "ArcLength",
            "curvatureat" | "curvaturaen" => "CurvatureAt",
            "volumeofrevolution" | "volumenrevolucion" => "VolumeOfRevolution",
            "surfaceofrevolution" | "superficierevolucion" => "SurfaceOfRevolution",
            _ => "Unknown",
        };

        let mut expr_arg = args.first().cloned().unwrap_or_default();

        // Try full expr_arg first (e.g. "f(x)")
        let mut found_func = false;
        if let Some(id) = find_object_by_label(document, &expr_arg) {
            if let Some(GeoObject::Function(f)) = document.get_object(id) {
                expr_arg = format!("({})", f.expr.clone());
                found_func = true;
            }
        }
        // If not found, try stripping (x)
        if !found_func {
            if let Some(pos) = expr_arg.find('(') {
                let fname = &expr_arg[..pos];
                if let Some(id) = find_object_by_label(document, fname) {
                    if let Some(GeoObject::Function(f)) = document.get_object(id) {
                        expr_arg = format!("({})", f.expr.clone());
                    }
                }
            }
        }

        match normalized {
            "Derivative" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = symbolic::derivative(&expr_arg, var)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Integral" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                if args.len() == 4 || args.len() == 3 {
                    let a_str = if args.len() == 4 {
                        args.get(2)
                    } else {
                        args.get(1)
                    };
                    let b_str = if args.len() == 4 {
                        args.get(3)
                    } else {
                        args.get(2)
                    };
                    if let (Some(a), Some(b)) = (a_str, b_str) {
                        if let (Ok(a_val), Ok(b_val)) = (
                            require_finite(parse_numeric_arg(a, &document.variables)),
                            require_finite(parse_numeric_arg(b, &document.variables)),
                        ) {
                            resolved_expr =
                                symbolic::integrate_definite(&expr_arg, var, a_val, b_val)
                                    .unwrap_or_else(|_| current[range.clone()].to_string());
                        } else {
                            resolved_expr = current[range.clone()].to_string();
                        }
                    }
                } else {
                    resolved_expr = symbolic::integrate(&expr_arg, var)
                        .unwrap_or_else(|_| current[range.clone()].to_string());
                }
            }
            "Taylor" => {
                if let (Some(var), Some(center), Some(order)) =
                    (args.get(1), args.get(2), args.get(3))
                {
                    match (
                        is_math_identifier(var),
                        require_finite(parse_numeric_arg(center, &document.variables)),
                        parse_taylor_order_local(Some(order)),
                    ) {
                        (true, Ok(center), Ok(order)) => {
                            resolved_expr = symbolic::taylor_series(&expr_arg, var, center, order)
                                .unwrap_or_else(|_| current[range.clone()].to_string());
                        }
                        _ => resolved_expr = current[range.clone()].to_string(),
                    }
                } else {
                    resolved_expr = current[range.clone()].to_string();
                }
            }
            "Expand" => {
                resolved_expr = symbolic::expand(&expr_arg)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Factor" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = symbolic::factor(&expr_arg, var)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "Simplify" => {
                resolved_expr = symbolic::simplify(&expr_arg)
                    .unwrap_or_else(|_| current[range.clone()].to_string());
            }
            "CompleteSquare" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = match symbolic::complete_square_typed(&expr_arg, var) {
                    grafito_geometry::outcome::MathResult::Exact(v) => v,
                    grafito_geometry::outcome::MathResult::Approximate { value, .. } => {
                        value.to_string()
                    }
                    _ => current[range.clone()].to_string(),
                };
            }
            "PrimeFactors" => {
                resolved_expr = match symbolic::prime_factors_typed(&expr_arg) {
                    grafito_geometry::outcome::MathResult::Exact(v) => v,
                    _ => current[range.clone()].to_string(),
                };
            }
            "IFactor" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                resolved_expr = match symbolic::ifactor_typed(&expr_arg, var) {
                    grafito_geometry::outcome::MathResult::Exact(v) => v,
                    _ => current[range.clone()].to_string(),
                };
            }
            "Assume" => {
                // No expansión dentro de expresiones: mantiene texto
                resolved_expr = current[range.clone()].to_string();
            }
            "Limit" => {
                resolved_expr = match (args.get(1), args.get(2)) {
                    (Some(var), Some(at)) if is_math_identifier(var) => {
                        match require_finite(parse_numeric_arg(at, &document.variables)) {
                            Ok(at) => match symbolic::limit_typed(&expr_arg, var, at) {
                                grafito_geometry::outcome::MathResult::Exact(value)
                                    if value.is_finite() =>
                                {
                                    value.to_string()
                                }
                                grafito_geometry::outcome::MathResult::Approximate {
                                    value,
                                    error_estimate,
                                } if value.is_finite() && error_estimate.is_finite() => {
                                    value.to_string()
                                }
                                _ => current[range.clone()].to_string(),
                            },
                            Err(_) => current[range.clone()].to_string(),
                        }
                    }
                    _ => current[range.clone()].to_string(),
                };
            }
            "LimitAbove" => {
                resolved_expr = match (args.get(1), args.get(2)) {
                    (Some(var), Some(at)) if is_math_identifier(var) => {
                        match require_finite(parse_numeric_arg(at, &document.variables)) {
                            Ok(at) => match symbolic::limit_above_typed(&expr_arg, var, at) {
                                grafito_geometry::outcome::MathResult::Exact(value)
                                    if value.is_finite() =>
                                {
                                    value.to_string()
                                }
                                grafito_geometry::outcome::MathResult::Approximate {
                                    value,
                                    error_estimate,
                                } if value.is_finite() && error_estimate.is_finite() => {
                                    value.to_string()
                                }
                                _ => current[range.clone()].to_string(),
                            },
                            Err(_) => current[range.clone()].to_string(),
                        }
                    }
                    _ => current[range.clone()].to_string(),
                };
            }
            "LimitBelow" => {
                resolved_expr = match (args.get(1), args.get(2)) {
                    (Some(var), Some(at)) if is_math_identifier(var) => {
                        match require_finite(parse_numeric_arg(at, &document.variables)) {
                            Ok(at) => match symbolic::limit_below_typed(&expr_arg, var, at) {
                                grafito_geometry::outcome::MathResult::Exact(value)
                                    if value.is_finite() =>
                                {
                                    value.to_string()
                                }
                                grafito_geometry::outcome::MathResult::Approximate {
                                    value,
                                    error_estimate,
                                } if value.is_finite() && error_estimate.is_finite() => {
                                    value.to_string()
                                }
                                _ => current[range.clone()].to_string(),
                            },
                            Err(_) => current[range.clone()].to_string(),
                        }
                    }
                    _ => current[range.clone()].to_string(),
                };
            }
            "ParametricDerivative" => {
                let var = args.get(2).map(|s| s.as_str()).unwrap_or("t");
                if args.len() >= 2 {
                    let x_arg = args[0].clone();
                    let y_arg = args[1].clone();
                    match symbolic::parametric_derivative_typed(&x_arg, &y_arg, var) {
                        grafito_geometry::outcome::MathResult::Exact(value) => {
                            resolved_expr = value;
                        }
                        grafito_geometry::outcome::MathResult::Approximate { value, .. } => {
                            resolved_expr = value;
                        }
                        _ => resolved_expr = current[range.clone()].to_string(),
                    }
                } else {
                    resolved_expr = current[range.clone()].to_string();
                }
            }
            "Asymptote" => {
                let var = args.get(1).map(|s| s.as_str()).unwrap_or("x");
                match symbolic::asymptote_typed(&expr_arg, var) {
                    grafito_geometry::outcome::MathResult::Exact(value) => {
                        resolved_expr = value;
                    }
                    _ => resolved_expr = current[range.clone()].to_string(),
                }
            }
            "GroebnerDegRevLex" => {
                // Stub: no expande, mantiene el texto original para que el handler
                // principal devuelva el mensaje informativo sin pánico.
                resolved_expr = "Groebner no implementado, use Eliminate".to_string();
            }
            _ => {
                resolved_expr = current[range.clone()].to_string();
            }
        }

        if resolved_expr == current[range.clone()] {
            break;
        }
        current.replace_range(range, &format!("({})", resolved_expr));
    }
    current
}

pub fn parse_cas_command(text: &str) -> Option<CasCmd> {
    let text = text.trim();
    if let Some(open) = text.find('[') {
        let mut depth = 0usize;
        let mut close = None;
        for (offset, ch) in text[open..].char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        if !text[close + 1..].trim().is_empty() {
            return None;
        }
        let command = text[..open].trim().to_string();
        let inside = &text[open + 1..close];
        let args: Vec<String> = if inside.trim().is_empty() {
            Vec::new()
        } else {
            split_args(inside)
                .into_iter()
                .map(|s| s.trim().to_string())
                .collect()
        };
        if command.is_empty() {
            return None;
        }
        let normalized = if let Some(normalized) = crate::command_registry::canonicalize(&command) {
            normalized
        } else {
            match command.to_lowercase().as_str() {
                "derivative" | "derivada" | "deriv" | "diff" => "Derivative",
                "integral" | "integrar" | "int" => "Integral",
                "solve" | "nsolve" | "resolver" => "Solve",
                "limit" | "limite" | "lim" => "Limit",
                "limitabove" | "limite_superior" | "limite_derecho" => "LimitAbove",
                "limitbelow" | "limite_inferior" | "limite_izquierdo" => "LimitBelow",
                "parametricderivative" | "derivada_parametrica" | "derivadaParametrica" => {
                    "ParametricDerivative"
                }
                "asymptote" | "asintota" | "asíntota" => "Asymptote",
                "groebner" | "groebnerbasis" | "groebner_basis" | "groebnerdegrevlex"
                | "groebnerlex" => "GroebnerDegRevLex",
                "factor" | "factorizar" => "Factor",
                "expand" | "expandir" => "Expand",
                "simplify" | "simplificar" => "Simplify",
                "completesquare" | "complete_square" | "completarcuadrado"
                | "completar_cuadrado" => "CompleteSquare",
                "primefactors" | "prime_factors" | "factoresprimos" | "factores_primos" => {
                    "PrimeFactors"
                }
                "ifactor" | "ifactorizar" | "factorentero" | "factor_entero" => "IFactor",
                "assume" | "asumir" | "suponer" | "supone" => "Assume",
                "tangentat" | "tangenteen" => "TangentAt",
                "normalat" | "normalen" => "NormalAt",
                "arclength" | "longitudarco" => "ArcLength",
                "curvatureat" | "curvaturaen" => "CurvatureAt",
                "volumeofrevolution" | "volumenrevolucion" => "VolumeOfRevolution",
                "surfaceofrevolution" | "superficierevolucion" => "SurfaceOfRevolution",
                "lorenz" => "Lorenz",
                "rossler" | "rössler" => "Rossler",
                "thomas" | "butterfly" => "Thomas",
                "aizawa" => "Aizawa",
                "chen" => "Chen",
                "halvorsen" => "Halvorsen",
                "dadras" => "Dadras",
                "chua" => "Chua",
                "mandelbrot" => "Mandelbrot",
                "julia" => "Julia",
                "burningship" | "burning_ship" => "BurningShip",
                "hypercube" | "tesseract" => "Hypercube",
                "hypersphere" => "Hypersphere",
                "vectorfield3d" | "vectorfield" => "VectorField3D",
                "histogram" | "histograma" => "Histogram",
                "scatterplot" | "scatter" => "ScatterPlot",
                "boxplot" => "BoxPlot",
                "linearregression" | "regression" | "regresion" => "LinearRegression",
                "mean" | "media" => "Mean",
                "median" | "mediana" => "Median",
                "stddev" | "desviacion" => "StdDev",
                "correlation" | "correlacion" => "Correlation",
                "determinant" | "det" => "Determinant",
                "inverse" | "inversa" => "Inverse",
                "transpose" | "transpuesta" => "Transpose",
                "trace" | "traza" => "Trace",
                "rank" | "rango" | "matrixrank" => "Rank",
                "nullspace" | "null_space" | "kernel" | "nucleo" | "núcleo" => "NullSpace",
                "linearsolve" | "linsolve" | "solvesystem" | "sistema" | "resolver_sistema" => {
                    "LinearSolve"
                }
                "eigenvalues" | "autovalores" => "Eigenvalues",
                "eigenvectors" | "autovectores" => "Eigenvectors",
                "lu" | "ludecomposition" | "lu_decomposition" => "LU",
                "qr" | "qrdecomposition" | "qr_decomposition" => "QR",
                "cholesky" => "Cholesky",
                "svd" | "singularvalues" | "valores_singulares" => "SVD",
                "conditionnumber" | "condition_number" | "condicion" => "ConditionNumber",
                "gaussjordan" | "gauss_jordan" | "jordan" | "reduccionescalonada" => "GaussJordan",
                "gaussjordansolve" | "gauss_jordan_solve" | "resolvergaussjordan" => {
                    "GaussJordanSolve"
                }
                "cramer" | "reglacramer" | "regla_cramer" => "Cramer",
                "cofactor" | "cofactorial" => "Cofactor",
                "adjugate" | "adjunta" => "Adjugate",
                "laplaceexpansion" | "laplace_expansion" | "desarrollolaplace" => {
                    "LaplaceExpansion"
                }
                "changeofbasis" | "change_of_basis" | "cambiobase" | "cambio_base" => {
                    "ChangeOfBasis"
                }
                "lineartransformationmatrix"
                | "linear_transformation_matrix"
                | "matriztransformacion" => "LinearTransformationMatrix",
                "diagonalization" | "diagonalizacion" | "diagonalización" => "Diagonalization",
                "gradient" | "gradiente" | "grad" => "Gradient",
                "jacobianmatrix" | "jacobian" | "jacobiana" | "matrizjacobiana" => "JacobianMatrix",
                "hessian" | "hessiana" => "Hessian",
                "criticalpoints" | "critical_points" | "puntoscriticos" | "puntos_críticos" => {
                    "CriticalPoints"
                }
                "lagrangemultipliers" | "lagrange_multipliers" | "multiplicadoreslagrange" => {
                    "LagrangeMultipliers"
                }
                "directionalderivative" | "directional_derivative" | "derivadadireccional" => {
                    "DirectionalDerivative"
                }
                "tangentplane" | "tangent_plane" | "planotangente" => "TangentPlane",
                "divergence" | "divergencia" => "Divergence",
                "curl" | "rotor" | "rotacional" => "Curl",
                "doubleintegral" | "double_integral" | "integraldoble" => "DoubleIntegral",
                "surfacearea" | "surface_area" | "areasuperficie" => "SurfaceArea",
                "lineintegralscalar" | "line_integral_scalar" | "integrallinealescalar" => {
                    "LineIntegralScalar"
                }
                "lineintegralvector" | "line_integral_vector" | "integrallinealvectorial" => {
                    "LineIntegralVector"
                }
                "tripleintegral" | "triple_integral" | "integraltriple" => "TripleIntegral",
                "surfaceintegralscalar" | "surface_integral_scalar" | "integralsuperficie" => {
                    "SurfaceIntegralScalar"
                }
                "flux" | "flujo" => "Flux",
                "isconservative" | "is_conservative" | "campoconservativo" | "conservativo" => {
                    "IsConservative"
                }
                "potentialfunction" | "potential_function" | "funcionpotencial" | "potencial" => {
                    "PotentialFunction"
                }
                "greentheorem" | "green_theorem" | "teoremagreen" => "GreenTheorem",
                "stokestheorem" | "stokes_theorem" | "teoremastokes" => "StokesTheorem",
                "gaussostrogradski" | "gauss_ostrogradski" | "divergencetheorem" => {
                    "GaussOstrogradski"
                }
                "changeofvariables" | "change_of_variables" | "cambiovariables" => {
                    "ChangeOfVariables"
                }
                "riemannsum" | "riemann_sum" | "sumariemann" => "RiemannSum",
                "improperintegral" | "improper_integral" | "integralimpropia" => "ImproperIntegral",
                "bolzanocheck" | "bolzano" | "teoremabolzano" => "BolzanoCheck",
                "rollecheck" | "rolle" | "teoremarolle" => "RolleCheck",
                "meanvaluecheck" | "mean_value_check" | "lagrangecheck" | "teoremalagrange" => {
                    "MeanValueCheck"
                }
                "cauchymeanvaluecheck" | "cauchy_mean_value_check" | "teoremacauchy" => {
                    "CauchyMeanValueCheck"
                }
                "lhopital" | "l'hopital" | "hopital" => "LHopital",
                "alternatingseriestest" | "alternating_series_test" | "criterioalternada" => {
                    "AlternatingSeriesTest"
                }
                "integraltest" | "integral_test" | "criteriointegral" => "IntegralTest",
                "absoluteconvergence" | "absolute_convergence" | "convergenciaabsoluta" => {
                    "AbsoluteConvergence"
                }
                "sequencelimit" | "sequence_limit" | "limitesucesion" | "limite_sucesion" => {
                    "SequenceLimit"
                }
                "seriessum" | "series_sum" | "sumaserie" | "suma_serie" => "SeriesSum",
                "ratiotest" | "ratio_test" | "cociente" | "criteriocociente" => "RatioTest",
                "roottest" | "root_test" | "criterioraiz" => "RootTest",
                "taylor" => "Taylor",
                "ode" | "edo" => "ODE",
                "odesystem" | "ode_system" | "sistemaedo" | "sistema_edo" => "ODESystem",
                "complexgrid" | "complex_grid" | "cgrid" => "ComplexGrid",
                "complexsurface" | "complex_surface" | "csurface" => "ComplexSurface",
                "quadrants" | "cuadrantes" => "Quadrants",
                "complexmapping"
                | "complex_mapping"
                | "mapeocomplejo"
                | "mapeo_complejo"
                | "transformadacompleja" => "ComplexMapping",
                "integralcompleja" | "contourintegral" | "complexintegral" => "ComplexIntegral",
                "gauss" | "residuos" | "residue" => "Gauss",
                "complexsymbol" | "complex_symbol" | "simbolocomplejo" => "ComplexSymbol",
                "domaincoloring" | "domain_coloring" | "dcolor" => "DomainColoring",
                "heatmap" | "heat_map" | "hmap" => "HeatMap",
                "polarcurve" | "polar_curve" | "polar" => "PolarCurve",
                "parametriccurve2d" | "parametric_curve_2d" | "param2d" => "ParametricCurve2D",
                "vectorfield2d" | "vector_field_2d" | "vf2d" => "VectorField2D",
                "phaseportrait" | "phase_portrait" | "phase" => "PhasePortrait",
                "contour" | "contourlines" | "contour_lines" => "Contour",
                "function" | "func" => "Function",
                "piecewise" | "pw" => "Piecewise",
                "distance" | "dist" => "Distance",
                "root" | "raices" | "raiz" => "Root",
                "extremum" | "extremos" | "max" | "min" => "Extremum",
                "intersect" | "interseccion" => "Intersect",
                "yintercept" | "interceptoy" | "intercepto_y" => "YIntercept",
                "xintercept" | "interceptox" | "intercepto_x" => "XIntercept",
                "analyze" | "analizar" | "analisis" => "Analyze",
                "angle" => "Angle",
                "tangent" => "Tangent",
                "coincident" => "Coincident",
                "horizontal" => "Horizontal",
                "vertical" => "Vertical",
                "equallength" | "equal_length" | "eqlength" => "EqualLength",
                "symmetry" => "Symmetry",
                "ellipsebyfoci" | "ellipse_by_foci" => "EllipseByFoci",
                "parabolabyfocusdirectrix" | "parabola_by_focus_directrix" => {
                    "ParabolaByFocusDirectrix"
                }
                "hyperbolabyfoci" | "hyperbola_by_foci" => "HyperbolaByFoci",
                "conicbyfivepoints" | "conic_by_five_points" => "ConicByFivePoints",
                "polygonunion" | "polyunion" => "PolygonUnion",
                "polygonintersection" | "polyintersection" => "PolygonIntersection",
                "polygondifference" | "polydifference" => "PolygonDifference",
                "polygonxor" | "polyxor" => "PolygonXor",
                "segment" => "Segment",
                "ray" => "Ray",
                "vector" => "Vector",
                "regularpolygon" | "regular_polygon" => "RegularPolygon",
                "plane3d" | "plane" | "plano" | "plano3d" => "Plane3D",
                "line3d" | "line3" | "recta3d" | "recta" => "Line3D",
                "equidistantfrom" | "equidistant" | "equidistante" => "EquidistantFrom",
                "solve3dgeometry" | "solve3d" | "resolver3d" => "Solve3DGeometry",
                "intersection3d" | "intersect3d" | "interseccion3d" | "intersección3d" => {
                    "Intersection3D"
                }
                "projection3d" | "project3d" | "proyeccion3d" | "proyección3d" => "Projection3D",
                "planethroughlines" | "planebylines" | "planoporrectas" | "plano_por_rectas" => {
                    "PlaneThroughLines"
                }
                "planethroughlinepoint" | "planoporrectapunto" => "PlaneThroughLinePoint",
                "linerelation3d" | "relacionrectas3d" | "relaciónrectas3d" => "LineRelation3D",
                "solveline3dparameters" | "resolverparametrosrecta3d" | "parametrosrecta3d" => {
                    "SolveLine3DParameters"
                }
                "matrixparamsolve" | "solveparammatrix" | "matrizparametrica" => "MatrixParamSolve",
                "p2dependence" | "p2dep" | "dependenciap2" => "P2Dependence",
                "p2basis" | "basep2" => "P2Basis",
                "p2equations" | "ecuacionesp2" => "P2Equations",
                "subspacedimension" | "subspacedim" | "dimsubspace" | "dimensionsubespacio" => {
                    "SubspaceDimension"
                }
                "subspacebasis" | "basissubspace" | "basesubespacio" => "SubspaceBasis",
                "subspacesum" | "sumsubspaces" | "sumasubespacios" => "SubspaceSum",
                "subspaceintersection" | "intersectionsubspaces" | "interseccionsubespacios" => {
                    "SubspaceIntersection"
                }
                "orthogonalcomplement" | "orthogonal" | "complementoortogonal" | "ortogonal" => {
                    "OrthogonalComplement"
                }
                _ => {
                    if args.is_empty()
                        || command.contains(' ')
                        || command.contains('=')
                        || command.contains('(')
                    {
                        return None;
                    }
                    return Some(CasCmd { command, args });
                }
            }
        };
        Some(CasCmd {
            command: normalized.to_string(),
            args,
        })
    } else {
        let cmd_lower = text.to_lowercase();
        let bare_commands = [
            "lorenz",
            "rossler",
            "thomas",
            "butterfly",
            "aizawa",
            "chen",
            "halvorsen",
            "dadras",
            "chua",
            "mandelbrot",
            "burningship",
            "hypercube",
            "hypersphere",
        ];
        for &cmd in &bare_commands {
            if cmd_lower == cmd {
                let normalized = match cmd {
                    "burningship" => "BurningShip".to_string(),
                    "butterfly" => "Thomas".to_string(),
                    "lorenz" => "Lorenz".to_string(),
                    "rossler" => "Rossler".to_string(),
                    "thomas" => "Thomas".to_string(),
                    "aizawa" => "Aizawa".to_string(),
                    "chen" => "Chen".to_string(),
                    "halvorsen" => "Halvorsen".to_string(),
                    "dadras" => "Dadras".to_string(),
                    "chua" => "Chua".to_string(),
                    "mandelbrot" => "Mandelbrot".to_string(),
                    "hypercube" => "Hypercube".to_string(),
                    "hypersphere" => "Hypersphere".to_string(),
                    _ => {
                        let mut c = cmd.to_string();
                        c[..1].make_ascii_uppercase();
                        c
                    }
                };
                return Some(CasCmd {
                    command: normalized,
                    args: vec![],
                });
            }
        }
        None
    }
}

pub(crate) fn looks_like_bracketed_command(text: &str) -> bool {
    let Some(open) = text.find('[') else {
        return false;
    };
    let command = text[..open].trim();
    !command.is_empty()
        && command
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch == '_' || ch == '\'')
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(s[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(s[start..].to_string());
    args
}

// ── helpers privados duplicados para evitar ciclo con commands.rs ──

fn is_math_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || ch == '_')
}

fn require_finite(value: Result<f64, String>) -> Result<f64, String> {
    let value = value?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("No es finito: {value}"))
    }
}

fn parse_numeric_arg(s: &str, variables: &HashMap<String, f64>) -> Result<f64, String> {
    let arg = s.trim();
    if let Ok(val) = arg.parse::<f64>() {
        return Ok(val);
    }
    let expanded = insert_implicit_multiplication(arg);
    if let Ok(val) = expanded.parse::<f64>() {
        return Ok(val);
    }
    match evaluate(
        &expanded,
        &variables
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>(),
    ) {
        Ok(val) if val.is_finite() => Ok(val),
        Ok(val) => Err(format!("No es finito: {}", val)),
        Err(e) => Err(format!(
            "No se pudo interpretar como número: '{}' ({})",
            arg, e
        )),
    }
}

fn insert_implicit_multiplication(text: &str) -> String {
    let mut res = String::new();
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        res.push(chars[i]);
        if i + 1 < chars.len() {
            let c1 = chars[i];
            let c2 = chars[i + 1];
            let exponent_start = i + 2;
            let scientific_exponent = matches!(c2, 'e' | 'E')
                && (chars
                    .get(exponent_start)
                    .is_some_and(|next| next.is_ascii_digit())
                    || (chars
                        .get(exponent_start)
                        .is_some_and(|next| matches!(next, '+' | '-'))
                        && chars
                            .get(exponent_start + 1)
                            .is_some_and(|next| next.is_ascii_digit())));
            if c1.is_ascii_digit() && c2.is_ascii_alphabetic() && !scientific_exponent {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_alphabetic() {
                res.push('*');
            }
            if c1 == ')' && c2.is_ascii_digit() {
                res.push('*');
            }
            if c1.is_ascii_digit() && c2 == '(' && (i == 0 || !chars[i - 1].is_ascii_alphabetic()) {
                res.push('*');
            }
            if c1 == ')' && c2 == '(' {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2 == '('
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
            if (c1 == 'x' || c1 == 'y')
                && c2.is_ascii_alphabetic()
                && (i == 0 || !chars[i - 1].is_ascii_alphabetic())
            {
                res.push('*');
            }
        }
    }
    res
}

fn find_object_by_label(document: &Document, label: &str) -> Option<ObjectId> {
    document.try_find_object_by_label(label).ok().flatten()
}

const MAX_TAYLOR_ORDER: usize = 64;

fn parse_taylor_order_local(value: Option<&str>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(5);
    };
    let order = value.trim().parse::<usize>().map_err(|_| {
        format!("Taylor: order must be an integer between 0 and {MAX_TAYLOR_ORDER}")
    })?;
    if order > MAX_TAYLOR_ORDER {
        return Err(format!(
            "Taylor: order {order} exceeds maximum {MAX_TAYLOR_ORDER}"
        ));
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use grafito_core::Document;

    #[test]
    fn sanitize_replaces_unicode() {
        assert_eq!(sanitize_unicode_input("x² + π"), "x^2 + pi");
        assert_eq!(sanitize_unicode_input("√(x)"), "sqrt(x)");
    }

    #[test]
    fn split_args_respects_nesting() {
        let args = split_args("a, b(c, d), e");
        assert_eq!(args, vec!["a", " b(c, d)", " e"]);
    }

    #[test]
    fn parse_cas_simple() {
        let cmd = parse_cas_command("Derivative[x^2, x]").unwrap();
        assert_eq!(cmd.command, "Derivative");
        assert_eq!(cmd.args.len(), 2);
    }

    #[test]
    fn expand_all_cas_noop() {
        let doc = Document::new();
        let out = expand_all_cas("x^2 + 1", &doc);
        assert_eq!(out, "x^2 + 1");
    }

    #[test]
    fn extract_cas_finds_bracketed() {
        let res = extract_cas_command("Solve[x^2-4, x]");
        assert!(res.is_some());
    }
}
