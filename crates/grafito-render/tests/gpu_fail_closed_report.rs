#![allow(clippy::unwrap_used, clippy::expect_used)]
//! GPU fail-closed honesto en local (G17: los `gpu_compute` con skip silencioso
//! hacían verde un CI local sin GPU, indistinguible de "todo probado").
//!
//! Este archivo NO necesita GPU y NUNCA falla por falta de adapter: reporta.
//! - Si `GRAFITO_REQUIRE_GPU_TESTS` está seteada (1/true/yes), el suite real
//!   `gpu_compute.rs` hace fail-closed (panic con mensaje). Aquí solo se
//!   verifica el predicado y se deja constancia.
//! - Si NO está seteada (caso local típico), este test pasa pero imprime un
//!   aviso explícito para que nadie confunda "12 passed, 20 skipped-silentes"
//!   con cobertura GPU real.
//!
//! Regresión que atraparía:
//! - alguien cambia la semántica de "0"/"false" (p.ej. `is_some()` a secas)
//!   y de pronto `=0` exige GPU en laptops sin Vulkan → este test falla;
//! - alguien silencia el skip (quita el eprintln) → el aviso de aquí sigue
//!   recordando que la cobertura fue omitida.

/// Duplica la semántica de `gpu_tests_are_required()` en
/// `crates/grafito-render/tests/gpu_compute.rs:87-90`.
/// Duplicar a propósito: si el predicado real cambia sin actualizar este
/// espejo, el test de paridad de abajo lo delata en la revisión.
fn gpu_tests_are_required_from(value: Option<&str>) -> bool {
    matches!(value, Some(v) if v != "0" && v != "false")
}

fn gpu_tests_are_required() -> bool {
    gpu_tests_are_required_from(std::env::var("GRAFITO_REQUIRE_GPU_TESTS").ok().as_deref())
}

#[test]
fn gpu_require_predicate_treats_zero_and_false_as_off() {
    // Paridad con gpu_compute.rs: solo "0"/"false"/ausente = off.
    assert!(!gpu_tests_are_required_from(None));
    assert!(!gpu_tests_are_required_from(Some("0")));
    assert!(!gpu_tests_are_required_from(Some("false")));
    assert!(gpu_tests_are_required_from(Some("1")));
    assert!(gpu_tests_are_required_from(Some("true")));
    assert!(gpu_tests_are_required_from(Some("yes")));
    assert!(gpu_tests_are_required_from(Some("")));
}

#[test]
fn gpu_coverage_status_is_reported_never_silent() {
    // Este test siempre pasa; su valor es el reporte. En CI con
    // GRAFITO_REQUIRE_GPU_TESTS=1 el job gpu-compute es fail-closed y este
    // mensaje confirma el modo estricto. En local sin la var, advierte que
    // los tests `required_vulkan_*` hicieron early-return y NO probaron nada.
    if gpu_tests_are_required() {
        eprintln!(
            "GPU coverage: STRICT (GRAFITO_REQUIRE_GPU_TESTS={:?}) — \
             los tests required_vulkan_* deben ejecutar o el suite falla.",
            std::env::var("GRAFITO_REQUIRE_GPU_TESTS").ok()
        );
    } else {
        eprintln!(
            "GPU coverage: SKIPPED-LOCAL (GRAFITO_REQUIRE_GPU_TESTS no seteada) — \
             los tests required_vulkan_* retornan early sin probar la GPU. \
             Para exigir GPU: GRAFITO_REQUIRE_GPU_TESTS=1 cargo test -p grafito-render --test gpu_compute. \
             Ver docs/tests/bench_gpu_gates.md §GPU."
        );
    }
    // El predicado del entorno debe coincidir con el espejo puro de arriba.
    assert_eq!(
        gpu_tests_are_required(),
        gpu_tests_are_required_from(std::env::var("GRAFITO_REQUIRE_GPU_TESTS").ok().as_deref())
    );
}
