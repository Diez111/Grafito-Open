#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Regresión de las limpiezas: opcodes retirados, cota de constantes,
//! contorno cerrado en `sum_of_residues`, inversas trig/hiperbólicas y
//! topes del parser Möbius.
use grafito_complex::algebraic_mappings::ConformalMap;
use grafito_complex::math::complex_calculus::sum_of_residues;
use grafito_complex::math::complex_opcode::{
    compile_complex_expr, exec_cpu, CompileError, ComplexBytecodeProgram, ComplexOp,
};
use grafito_complex::parse_complex;
use num_complex::Complex64;
use std::collections::{BTreeMap, HashMap};

fn close(actual: Complex64, expected: Complex64, tol: f64, msg: &str) {
    let error = (actual - expected).norm();
    assert!(
        error <= tol,
        "{msg}: esperado {expected:?}, fue {actual:?} (error {error:e})"
    );
}

// ----- Opcodes retirados (16–19, 31–33): ni el validador ni exec los aceptan -----

#[test]
fn opcodes_retirados_devuelven_none_honesto() {
    for raw in [16u32, 17, 18, 19, 31, 32, 33] {
        let prog = ComplexBytecodeProgram {
            code: vec![
                ComplexOp::PushConst.encode(0),
                ComplexOp::PushConst.encode(2),
                raw,
            ],
            constants: vec![1.0, 0.0, 2.0, 0.0],
        };
        assert_eq!(
            exec_cpu(&prog, &[]),
            None,
            "opcode {raw} retirado debe dar None, no basura"
        );
    }
    // Sanidad: un programa válido con los mismos pushes sigue funcionando.
    let prog = ComplexBytecodeProgram {
        code: vec![
            ComplexOp::PushConst.encode(0),
            ComplexOp::PushConst.encode(2),
            ComplexOp::Add.encode(0),
        ],
        constants: vec![1.0, 0.0, 2.0, 0.0],
    };
    assert_eq!(exec_cpu(&prog, &[]), Some(Complex64::new(3.0, 0.0)));
}

// ----- Cota de constantes: 127 complejas (254 elementos) en compilación y validación -----

#[test]
fn mas_de_127_constantes_rechaza_compilacion() {
    let many = (1..=130)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let expr = parse_complex(&many).expect("parsea la suma larga");
    let mut prog = ComplexBytecodeProgram::default();
    match compile_complex_expr(&expr, &BTreeMap::new(), &[("z", 0)], &mut prog) {
        Err(CompileError::TooManyConstants) => {}
        other => panic!("130 consts debe dar TooManyConstants, fue {other:?}"),
    }
    // 100 constantes (200 elementos) compilan bien.
    let few = (1..=100)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let expr = parse_complex(&few).expect("parsea la suma corta");
    let mut prog = ComplexBytecodeProgram::default();
    compile_complex_expr(&expr, &BTreeMap::new(), &[("z", 0)], &mut prog)
        .expect("100 consts deben compilar");
}

// ----- sum_of_residues exige contorno cerrado -----

#[test]
fn residuo_en_contorno_abierto_da_error_honesto() {
    let expr = parse_complex("1/z").expect("parsea");
    let vars = HashMap::new();
    // Cuadrado sin cerrar: el último vértice no vuelve al primero.
    let open = vec![
        Complex64::new(-1.0, -1.0),
        Complex64::new(1.0, -1.0),
        Complex64::new(1.0, 1.0),
        Complex64::new(-1.0, 1.0),
    ];
    let result = sum_of_residues(&expr, &open, &vars, "z");
    let message = result.expect_err("contorno abierto debe fallar, no inventar residuo");
    assert!(message.contains("not closed"), "mensaje: {message}");
    // Cerrado sí integra: residuo de 1/z = 1.
    let mut closed = open.clone();
    closed.push(Complex64::new(-1.0, -1.0));
    let residue = sum_of_residues(&expr, &closed, &vars, "z").expect("cerrado integra");
    close(residue, Complex64::new(1.0, 0.0), 1e-6, "residuo de 1/z");
}

// ----- Inversas cerradas trig/hiperbólicas -----

#[test]
fn inversas_trig_hiperbolicas_hacen_roundtrip() {
    let maps = [
        ConformalMap::Sinh,
        ConformalMap::Cosh,
        ConformalMap::Sine,
        ConformalMap::Cosine,
        ConformalMap::Tangent,
    ];
    let w = Complex64::new(0.5, 0.25);
    for map in maps {
        let z = map
            .inverse_apply(w)
            .unwrap_or_else(|| panic!("{map:?} debe invertir {w:?}"));
        let back = map
            .apply(z)
            .unwrap_or_else(|| panic!("{map:?} debe re-aplicar en {z:?}"));
        close(back, w, 1e-12, &format!("roundtrip de {map:?}"));
    }
    // Rama principal: asin(sin(0.5)) = 0.5.
    let z = ConformalMap::Sine
        .inverse_apply(Complex64::new(0.5f64.sin(), 0.0))
        .expect("asin");
    close(z, Complex64::new(0.5, 0.0), 1e-12, "asin(sin(0.5))");
}

// ----- Topes del parser Möbius -----

#[test]
fn parser_mobius_rechaza_entradas_patologicas() {
    assert_eq!(ConformalMap::from_expr_string(&"z".repeat(5000)), None);
    let deep = format!("{}z+1)/(z+1{}", "(".repeat(200), ")".repeat(200));
    assert_eq!(ConformalMap::from_expr_string(&deep), None);
    // Una Möbius válida sigue parseando: (2z+1)/(z+3) en z=1 da 3/4.
    let map = ConformalMap::from_expr_string("(2*z+1)/(z+3)").expect("möbius válida");
    let w = map.apply(Complex64::new(1.0, 0.0)).expect("aplica");
    close(w, Complex64::new(0.75, 0.0), 1e-12, "(2+1)/(1+3)");
}
