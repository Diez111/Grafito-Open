#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(deprecated)]
use grafito_core::{
    implicit_curve::{
        marching_squares_from_grid as cpu_marching_squares, MAX_IMPLICIT_GRID_SIZE,
        MAX_MARCHING_SQUARES_SEGMENTS, MAX_MARCHING_SQUARES_WORK_UNITS,
    },
    Cube3DObj, Document, GeoObject, ImplicitCurveObj, ParametricCurve2DObj, ParametricCurve3DObj,
    PolarCurveObj, RelationOperator, Surface3DObj, VectorField2DObj,
};
use grafito_geometry::{Camera3D, Color, Point3D};
use grafito_render::{
    complex_compute::ComplexComputePipeline,
    domain_coloring_compute::DomainColoringComputePipeline,
    fill_compute::FillComputePipeline,
    function_compute::FunctionComputePipeline,
    implicit_compute::{marching_squares_from_grid, ImplicitComputePipeline},
    parametric_compute::{
        maybe_compute_curve_2d_on_gpu, maybe_compute_curve_3d_on_gpu,
        maybe_compute_curves_2d_on_gpu_batched, maybe_compute_curves_3d_on_gpu_batched,
        maybe_compute_polar_on_gpu, maybe_compute_polars_on_gpu_batched,
        maybe_compute_surface_on_gpu, ParametricComputePipeline,
    },
    vector_compute::VectorComputePipeline,
    Renderer,
};
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use wgpu::util::DeviceExt;

const EPSILON: f64 = 1e-5;

struct GpuContext {
    // Serialize device creation and readback because software Vulkan drivers are
    // commonly shared by parallel test threads.
    _guard: MutexGuard<'static, ()>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu_context_or_skip() -> Option<GpuContext> {
    static GPU_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = GPU_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // Required coverage must prove the Vulkan path rather than accepting
            // another backend that happens to be available on the test host.
            backends: if gpu_tests_are_required() {
                wgpu::Backends::VULKAN
            } else {
                wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all())
            },
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .ok_or_else(|| "no compatible adapter was found".to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .map_err(|error| format!("could not create a device: {error}"))?;
        Ok::<_, String>((device, queue))
    });

    match result {
        Ok((device, queue)) => Some(GpuContext {
            _guard: guard,
            device,
            queue,
        }),
        Err(reason) if gpu_tests_are_required() => {
            panic!("GRAFITO_REQUIRE_GPU_TESTS is set, but GPU compute coverage could not run: {reason}");
        }
        Err(reason) => {
            eprintln!(
                "GPU compute coverage skipped: {reason}. Set GRAFITO_REQUIRE_GPU_TESTS=1 to require a working adapter."
            );
            None
        }
    }
}

fn gpu_tests_are_required() -> bool {
    std::env::var_os("GRAFITO_REQUIRE_GPU_TESTS")
        .is_some_and(|value| value != "0" && value != "false")
}

fn cpu_scalar(expr: &str, x: f64, y: f64) -> f64 {
    grafito_geometry::expr::prepare_function_ast(expr, &HashMap::new(), &["x", "y"])
        .expect("test expression must parse")
        .eval_2d("x", x, "y", y)
}

fn assert_gpu_matches_cpu(actual: f64, expr: &str, x: f64, y: f64, context: &str) {
    let expected = cpu_scalar(expr, x, y);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{context}: expected NaN, got {actual}");
        return;
    }

    let tolerance = 1e-5_f64.max(expected.abs() * 1e-6);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context}: expected {expected}, got {actual}"
    );
}

fn assert_gpu_matches_cpu_parametric(actual: f64, expr: &str, t: f64, context: &str) {
    let expected = grafito_geometry::expr::prepare_function_ast(expr, &HashMap::new(), &["t"])
        .expect("test expression must parse")
        .eval_at("t", t);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{context}: expected NaN, got {actual}");
        return;
    }

    let tolerance = 1e-5_f64.max(expected.abs() * 1e-6);
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context}: expected {expected}, got {actual}"
    );
}

#[test]
fn required_vulkan_function_evaluator_matches_cpu_edge_semantics() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = FunctionComputePipeline::new(&gpu.device, &gpu.queue, 1);

    for expr in [
        "mod(x - 5.5, 2)",
        "asin(x + 2)",
        "acos(x + 2)",
        "acosh(x)",
        "atanh(x + 2)",
        "0.0000001 / (x + 0.000000000001)",
        "round(x - 0.5)",
        "clamp(x, 1, -1) + 2",
        "clamp(x, 1.00000001, 1) + 2",
    ] {
        let values = compute
            .evaluate_expr(
                &gpu.device,
                &gpu.queue,
                expr,
                (0.0, 1.0),
                1,
                &HashMap::new(),
            )
            .expect("supported function expression must execute on the GPU");
        assert_eq!(values.len(), 2);
        for (index, value) in values.into_iter().enumerate() {
            assert_gpu_matches_cpu(value, expr, index as f64, 0.0, "function evaluator");
        }
    }
}

#[test]
fn required_vulkan_function_evaluator_supports_exactly_32_stack_slots() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = FunctionComputePipeline::new(&gpu.device, &gpu.queue, 1);
    let expression = (1..32).fold("x".to_string(), |expression, _| {
        format!("min(x, {expression})")
    });
    grafito_geometry::expr::prepare_function_ast(&expression, &HashMap::new(), &["x"])
        .expect("the exact-stack test expression must parse");

    let values = compute
        .evaluate_expr(
            &gpu.device,
            &gpu.queue,
            &expression,
            (0.25, 0.5),
            1,
            &HashMap::new(),
        )
        .expect("an expression with exactly 32 stack slots must run on the GPU");

    assert!(values.iter().all(|value| value.is_finite()));
    for (index, value) in values.into_iter().enumerate() {
        assert!((value - (0.25 + index as f64 * 0.25)).abs() <= EPSILON);
    }

    let function = grafito_core::FunctionObj::new(expression);
    assert!(
        grafito_render::function_compute::maybe_compute_function_on_gpu(
            &compute,
            &gpu.device,
            &gpu.queue,
            &function,
            (0.25, 0.5),
            1,
            &HashMap::new(),
        )
    );
    assert!(function
        .cached_samples
        .read()
        .expect("function cache lock")
        .iter()
        .all(|(_, value)| value.is_some_and(f64::is_finite)));
}

fn right_nested_complex_sum(terms: usize) -> String {
    (1..terms).fold("z".to_owned(), |expression, _| {
        format!("z + ({expression})")
    })
}

#[test]
fn required_vulkan_complex_pipelines_accept_the_stack_limit_and_reject_overflow() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let valid = grafito_complex::math::complex_expr::parse(&right_nested_complex_sum(32))
        .expect("the exact-stack complex expression must parse");
    let overflow = grafito_complex::math::complex_expr::parse(&right_nested_complex_sum(33))
        .expect("the overflow complex expression must parse");

    let transform = ComplexComputePipeline::new(&gpu.device, &gpu.queue);
    let mapped = transform
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &valid,
            &[grafito_geometry::Point2::new(1.0, 0.0)],
            &HashMap::new(),
        )
        .expect("a 32-slot complex program must execute without a NaN mapping");
    assert!(mapped[0].x.is_finite() && mapped[0].y.is_finite());
    assert_close(mapped[0].x, 32.0, "exact-stack complex transform");
    assert!(transform
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &overflow,
            &[grafito_geometry::Point2::new(1.0, 0.0)],
            &HashMap::new(),
        )
        .is_none());

    let domain = DomainColoringComputePipeline::new(&gpu.device, &gpu.queue);
    let colors = domain
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &valid,
            &[(1.0, 0.0)],
            &HashMap::new(),
            0,
        )
        .expect("a 32-slot domain program must not black-map as a stack failure");
    assert!(colors[0][..3].iter().any(|component| *component > 0.0));
    assert!(domain
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &overflow,
            &[(1.0, 0.0)],
            &HashMap::new(),
            0,
        )
        .is_none());
}

#[test]
fn required_vulkan_implicit_evaluator_matches_cpu_edge_semantics() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ImplicitComputePipeline::new(&gpu.device, &gpu.queue, 1);

    for expr in [
        "mod(x - 5.5, 2)",
        "asin(x + 2)",
        "acos(x + 2)",
        "acosh(x)",
        "atanh(x + 2)",
        "0.0000001 / (x + 0.000000000001)",
        "round(x - 0.5)",
        "clamp(x, 1, -1) + 2",
        "clamp(x, 1.00000001, 1) + 2",
    ] {
        let curve = ImplicitCurveObj::new(expr, "0", RelationOperator::Eq);
        let rows = compute
            .evaluate(
                &gpu.device,
                &gpu.queue,
                &curve,
                (0.0, 1.0, 0.0, 1.0),
                1,
                &HashMap::new(),
            )
            .expect("supported implicit expression must execute on the GPU");
        for (j, row) in rows.into_iter().enumerate() {
            for (i, value) in row.into_iter().enumerate() {
                assert_gpu_matches_cpu(value, expr, i as f64, j as f64, "implicit evaluator");
            }
        }
    }
}

#[test]
fn required_vulkan_parametric_evaluator_matches_cpu_edge_semantics() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 1, 1);

    let modulo_and_round = ParametricCurve2DObj::new("mod(t - 5.5, 2)", "round(t - 0.5)", 0.0, 1.0);
    let samples = compute
        .evaluate_curve_2d(
            &gpu.device,
            &gpu.queue,
            &modulo_and_round,
            1,
            &HashMap::new(),
        )
        .expect("supported parametric curve must execute on the GPU");
    for (index, (x, y)) in samples.into_iter().enumerate() {
        let t = index as f64;
        assert_gpu_matches_cpu_parametric(x, "mod(t - 5.5, 2)", t, "parametric modulo");
        assert_gpu_matches_cpu_parametric(y, "round(t - 0.5)", t, "parametric round");
    }

    let invalid_clamp = ParametricCurve2DObj::new("clamp(t, 1.00000001, 1) + 2", "0", 0.0, 1.0);
    let samples = compute
        .evaluate_curve_2d(&gpu.device, &gpu.queue, &invalid_clamp, 1, &HashMap::new())
        .expect("supported parametric curve must execute on the GPU");
    for (index, (x, _)) in samples.into_iter().enumerate() {
        assert_gpu_matches_cpu_parametric(
            x,
            "clamp(t, 1.00000001, 1) + 2",
            index as f64,
            "parametric invalid clamp",
        );
    }

    let division_and_domain =
        ParametricCurve2DObj::new("0.0000001 / (t + 0.000000000001)", "asin(t + 2)", 0.0, 1.0);
    let samples = compute
        .evaluate_curve_2d(
            &gpu.device,
            &gpu.queue,
            &division_and_domain,
            1,
            &HashMap::new(),
        )
        .expect("supported parametric curve must execute on the GPU");
    for (index, (x, y)) in samples.into_iter().enumerate() {
        let t = index as f64;
        assert_gpu_matches_cpu_parametric(
            x,
            "0.0000001 / (t + 0.000000000001)",
            t,
            "parametric division",
        );
        assert_gpu_matches_cpu_parametric(y, "asin(t + 2)", t, "parametric asin domain");
    }

    let inverse_domains = ParametricCurve2DObj::new("acos(t + 2)", "acosh(t)", 0.0, 1.0);
    let samples = compute
        .evaluate_curve_2d(
            &gpu.device,
            &gpu.queue,
            &inverse_domains,
            1,
            &HashMap::new(),
        )
        .expect("supported parametric curve must execute on the GPU");
    for (index, (x, y)) in samples.into_iter().enumerate() {
        let t = index as f64;
        assert_gpu_matches_cpu_parametric(x, "acos(t + 2)", t, "parametric acos domain");
        assert_gpu_matches_cpu_parametric(y, "acosh(t)", t, "parametric acosh domain");
    }

    let atanh_domain = ParametricCurve2DObj::new("atanh(t + 2)", "0", 0.0, 1.0);
    let samples = compute
        .evaluate_curve_2d(&gpu.device, &gpu.queue, &atanh_domain, 1, &HashMap::new())
        .expect("supported parametric curve must execute on the GPU");
    for (index, (x, _)) in samples.into_iter().enumerate() {
        assert_gpu_matches_cpu_parametric(
            x,
            "atanh(t + 2)",
            index as f64,
            "parametric atanh domain",
        );
    }

    let curve_3d = ParametricCurve3DObj::new(
        "mod(t - 5.5, 2)",
        "0.0000001 / (t + 0.000000000001)",
        "round(t - 0.5)",
        0.0,
        1.0,
    );
    let samples = compute
        .evaluate_curve_3d(&gpu.device, &gpu.queue, &curve_3d, 1, &HashMap::new())
        .expect("supported parametric Curve3D must execute on the GPU");
    for (index, (x, y, z)) in samples.into_iter().enumerate() {
        let t = index as f64;
        assert_gpu_matches_cpu_parametric(x, "mod(t - 5.5, 2)", t, "Curve3D modulo");
        assert_gpu_matches_cpu_parametric(
            y,
            "0.0000001 / (t + 0.000000000001)",
            t,
            "Curve3D division",
        );
        assert_gpu_matches_cpu_parametric(z, "round(t - 0.5)", t, "Curve3D round");
    }
}

#[test]
fn gpu_explicit_surface_samples_remain_in_document_xyz_order() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 2, 2);
    let surface = Surface3DObj::new("10*x + y", (1.0, 2.0), (3.0, 4.0));

    let grid = compute
        .evaluate_surface(&gpu.device, &gpu.queue, &surface, 1, &HashMap::new())
        .expect("supported explicit surface must execute on the GPU");

    assert_eq!(grid[0][0], Point3D::new(1.0, 3.0, 13.0));
}

#[test]
fn gpu_surface_nan_in_evaluated_z_falls_back_without_replacing_cpu_grid() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 2, 2);
    let surface = Surface3DObj::new(
        "(1e20*x * 1e20*x) / (1e20*x * 1e20*x)",
        (1.0, 2.0),
        (1.0, 2.0),
    );
    let cpu_grid =
        grafito_core::parametric_sampling::evaluate_surface_3d(&surface, 1, &HashMap::new());
    assert!(cpu_grid.iter().flatten().all(Point3D::is_finite));
    *surface.cached_grid.write().unwrap() = cpu_grid.clone();

    assert!(!maybe_compute_surface_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &surface,
        1,
        &HashMap::new(),
    ));
    assert_eq!(*surface.cached_grid.read().unwrap(), cpu_grid);
}

#[test]
fn required_vulkan_vector_evaluator_matches_cpu_edge_semantics() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = VectorComputePipeline::new(&gpu.device, &gpu.queue, 1);

    for (u_expr, v_expr) in [
        ("mod(x - 5.5, 2)", "round(y - 0.5)"),
        ("0.0000001 / (x + 0.000000000001)", "asin(y + 2)"),
        ("acos(x + 2)", "acosh(y)"),
        ("atanh(x + 2)", "0"),
        ("clamp(x, 1, -1) + 2", "0"),
        ("clamp(x, 1.00000001, 1) + 2", "0"),
    ] {
        let field = VectorField2DObj::new(u_expr, v_expr);
        let samples = compute
            .evaluate(
                &gpu.device,
                &gpu.queue,
                &field,
                (0.0, 1.0, 0.0, 1.0),
                1,
                &HashMap::new(),
            )
            .expect("supported vector field must execute on the GPU");
        for (x, y, u, v) in samples {
            assert_gpu_matches_cpu(u, u_expr, x, y, "vector u component");
            assert_gpu_matches_cpu(v, v_expr, x, y, "vector v component");
        }
    }
}

#[test]
fn required_vulkan_fill_evaluator_matches_cpu_edge_semantics() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = FillComputePipeline::new(&gpu.device, &gpu.queue);

    for (expr, operator) in [
        ("mod(x - 5.5, 2)", RelationOperator::Less),
        ("asin(x + 2)", RelationOperator::Greater),
        ("acos(x + 2)", RelationOperator::Greater),
        ("acosh(x)", RelationOperator::GreaterEq),
        ("atanh(x + 2)", RelationOperator::Greater),
        (
            "0.0000001 / (x + 0.000000000001)",
            RelationOperator::Greater,
        ),
        ("round(x - 0.5)", RelationOperator::GreaterEq),
        ("clamp(x, 1, -1)", RelationOperator::Less),
        ("clamp(x, 1.00000001, 1)", RelationOperator::Greater),
    ] {
        let lhs = grafito_geometry::expr::prepare_function_ast(expr, &HashMap::new(), &["x", "y"])
            .expect("test expression must parse");
        let rhs = grafito_geometry::ast::Expr::Const(0.0);
        let pixels = compute
            .evaluate_fill(
                &gpu.device,
                &gpu.queue,
                &lhs,
                &rhs,
                operator,
                (0.0, 1.0, 0.0, 1.0),
                (1, 1),
                &HashMap::new(),
            )
            .expect("supported fill expression must execute on the GPU");
        let expected = cpu_scalar(expr, 0.0, 1.0);
        let cpu_inside = match operator {
            RelationOperator::Less | RelationOperator::LessEq => expected <= 0.0,
            RelationOperator::Greater | RelationOperator::GreaterEq => expected >= 0.0,
            RelationOperator::Eq => false,
        };
        assert_eq!(
            pixels[3] != 0,
            cpu_inside,
            "fill evaluator disagrees with CPU for {expr}"
        );
    }
}

#[test]
fn required_vulkan_scalar_evaluators_reject_nonfinite_clamp_bounds() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let variables = HashMap::from([("upper".to_string(), f64::INFINITY)]);

    let function = FunctionComputePipeline::new(&gpu.device, &gpu.queue, 1);
    let values = function
        .evaluate_expr(
            &gpu.device,
            &gpu.queue,
            "clamp(x, -1, upper) + 2",
            (0.0, 1.0),
            1,
            &variables,
        )
        .expect("nonfinite clamp function should execute on the GPU");
    assert!(values.iter().all(|value| value.is_nan()));

    let implicit = ImplicitComputePipeline::new(&gpu.device, &gpu.queue, 1);
    let curve = ImplicitCurveObj::new("clamp(x, -1, upper)", "0", RelationOperator::Eq);
    let rows = implicit
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &curve,
            (0.0, 1.0, 0.0, 1.0),
            1,
            &variables,
        )
        .expect("nonfinite clamp implicit curve should execute on the GPU");
    assert!(rows.iter().flatten().all(|value| value.is_nan()));

    let parametric = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 1, 1);
    let curve = ParametricCurve2DObj::new("clamp(t, -1, upper)", "0", 0.0, 1.0);
    let samples = parametric
        .evaluate_curve_2d(&gpu.device, &gpu.queue, &curve, 1, &variables)
        .expect("nonfinite clamp parametric curve should execute on the GPU");
    assert!(samples.iter().all(|(x, _)| x.is_nan()));

    let vector = VectorComputePipeline::new(&gpu.device, &gpu.queue, 1);
    let field = VectorField2DObj::new("clamp(x, -1, upper)", "0");
    let samples = vector
        .evaluate(
            &gpu.device,
            &gpu.queue,
            &field,
            (0.0, 1.0, 0.0, 1.0),
            1,
            &variables,
        )
        .expect("nonfinite clamp vector field should execute on the GPU");
    assert!(samples.iter().all(|(_, _, u, _)| u.is_nan()));

    let fill = FillComputePipeline::new(&gpu.device, &gpu.queue);
    let lhs = grafito_geometry::expr::prepare_function_ast(
        "clamp(x, -1, upper)",
        &variables,
        &["x", "y"],
    )
    .expect("nonfinite clamp fill expression must parse");
    let pixels = fill
        .evaluate_fill(
            &gpu.device,
            &gpu.queue,
            &lhs,
            &grafito_geometry::ast::Expr::Const(0.0),
            RelationOperator::Less,
            (0.0, 1.0, 0.0, 1.0),
            (1, 1),
            &variables,
        )
        .expect("nonfinite clamp fill should execute on the GPU");
    assert_eq!(pixels[3], 0);
}

#[test]
fn parametric_gpu_rejects_resolved_reversed_and_degenerate_domains_without_replacing_cpu_cache() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 8, 8);
    let variables = HashMap::from([
        ("low".to_string(), 0.0),
        ("high".to_string(), 1.0),
        ("same".to_string(), 0.5),
    ]);

    let mut curve_2d = ParametricCurve2DObj::new("t", "t", 0.0, 1.0);
    curve_2d.t_min_expr = Some("high".to_string());
    curve_2d.t_max_expr = Some("low".to_string());
    let cpu_curve_2d = vec![(7.0, 8.0)];
    *curve_2d.cached_samples.write().unwrap() = cpu_curve_2d.clone();
    assert!(compute
        .evaluate_curve_2d(&gpu.device, &gpu.queue, &curve_2d, 4, &variables)
        .is_none());
    assert!(!maybe_compute_curve_2d_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &curve_2d,
        4,
        &variables,
    ));
    assert_eq!(*curve_2d.cached_samples.read().unwrap(), cpu_curve_2d);

    let mut curve_3d = ParametricCurve3DObj::new("t", "t", "t", 0.0, 1.0);
    curve_3d.t_min_expr = Some("same".to_string());
    curve_3d.t_max_expr = Some("same".to_string());
    let cpu_curve_3d = vec![(7.0, 8.0, 9.0)];
    *curve_3d.cached_samples.write().unwrap() = cpu_curve_3d.clone();
    assert!(compute
        .evaluate_curve_3d(&gpu.device, &gpu.queue, &curve_3d, 4, &variables)
        .is_none());
    assert!(!maybe_compute_curve_3d_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &curve_3d,
        4,
        &variables,
    ));
    assert_eq!(*curve_3d.cached_samples.read().unwrap(), cpu_curve_3d);

    let mut polar = PolarCurveObj::new("1", 0.0, 1.0);
    polar.t_min_expr = Some("high".to_string());
    polar.t_max_expr = Some("low".to_string());
    assert!(compute
        .evaluate_polar(&gpu.device, &gpu.queue, &polar, 4, &variables)
        .is_none());
    let cpu_polar = vec![(3.0, 4.0)];
    *polar.cached_samples.write().unwrap() = cpu_polar.clone();
    assert!(!maybe_compute_polar_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &polar,
        4,
        &variables,
    ));
    assert_eq!(*polar.cached_samples.read().unwrap(), cpu_polar);

    let mut surface = Surface3DObj::new("x + y", (0.0, 1.0), (0.0, 1.0));
    surface.x_min_expr = Some("high".to_string());
    surface.x_max_expr = Some("low".to_string());
    assert!(compute
        .evaluate_surface(&gpu.device, &gpu.queue, &surface, 4, &variables)
        .is_none());
    let cpu_surface = vec![vec![Point3D::new(3.0, 4.0, 5.0)]];
    *surface.cached_grid.write().unwrap() = cpu_surface.clone();
    assert!(!maybe_compute_surface_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &surface,
        4,
        &variables,
    ));
    assert_eq!(*surface.cached_grid.read().unwrap(), cpu_surface);
}

#[test]
fn parametric_gpu_overflow_falls_back_without_erasing_cpu_samples_or_real_gaps() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 8, 8);

    let overflowing = ParametricCurve2DObj::new("exp(100) / exp(100)", "0", 0.0, 1.0);
    let cpu_samples = vec![(7.0, 8.0), (9.0, 10.0)];
    *overflowing.cached_samples.write().unwrap() = cpu_samples.clone();
    assert!(!maybe_compute_curve_2d_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &overflowing,
        4,
        &HashMap::new(),
    ));
    assert_eq!(*overflowing.cached_samples.read().unwrap(), cpu_samples);

    let discontinuous = ParametricCurve2DObj::new("1 / t", "0", -1.0, 1.0);
    assert!(maybe_compute_curve_2d_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &discontinuous,
        2,
        &HashMap::new(),
    ));
    let samples = discontinuous.cached_samples.read().unwrap();
    assert!(samples[0].0.is_finite() && samples[2].0.is_finite());
    assert!(samples[1].0.is_nan() && samples[1].1.is_finite());
}

#[test]
fn curve_3d_gpu_uses_declared_parameter_despite_document_variable() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 8, 8);
    let curve = ParametricCurve3DObj::new("s", "s + 1", "s^2", 0.0, 1.0).with_parameter("s");
    let variables = HashMap::from([("s".to_string(), 99.0)]);

    let samples = compute
        .evaluate_curve_3d(&gpu.device, &gpu.queue, &curve, 4, &variables)
        .expect("supported Curve3D expression must execute on the GPU");

    assert_eq!(samples.len(), 5);
    for (index, (x, y, z)) in samples.into_iter().enumerate() {
        let s = index as f64 / 4.0;
        assert_close(x, s, "Curve3D x sample");
        assert_close(y, s + 1.0, "Curve3D y sample");
        assert_close(z, s * s, "Curve3D z sample");
    }
}

#[test]
fn curve_3d_gpu_falls_back_without_replacing_cpu_samples_when_f32_loses_resolution() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 8, 8);
    let curve = ParametricCurve3DObj::new("999999+s", "0", "0", 0.0, 0.01).with_parameter("s");
    let cpu_samples = vec![(999999.0, 0.0, 0.0), (999999.0025, 0.0, 0.0)];
    *curve.cached_samples.write().unwrap() = cpu_samples.clone();

    let used_gpu = maybe_compute_curve_3d_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &curve,
        4,
        &HashMap::new(),
    );

    assert!(!used_gpu, "the GPU must decline imprecise Curve3D samples");
    assert_eq!(
        *curve.cached_samples.read().unwrap(),
        cpu_samples,
        "a declined GPU evaluation must preserve usable CPU samples"
    );
}

#[test]
fn required_gpu_depth_and_composite_pipeline_renders_a_world_mesh() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
    );
    let mut document = Document::new();
    let mut rear_cube = Cube3DObj::new(Point3D::new(0.0, 0.0, -1.0), 3.0);
    rear_cube.fill_color = Some(Color::BLUE);
    document.add_object(GeoObject::Cube3D(rear_cube));
    let mut front_cube = Cube3DObj::new(Point3D::new(0.0, 0.0, 2.0), 3.0);
    front_cube.fill_color = Some(Color::new(1.0, 0.0, 0.0, 0.5));
    document.add_object(GeoObject::Cube3D(front_cube));
    let camera = Camera3D {
        theta: std::f32::consts::FRAC_PI_2,
        phi: 0.0,
        distance: 10.0,
        target: glam::Vec3::ZERO,
        fov: 60.0,
        near: 0.1,
        far: 100.0,
        aspect: 1.0,
    };
    let mesh = Renderer::build_3d_world_mesh(&document, &camera, 64.0, 64.0);
    assert!(!mesh.opaque_indices.is_empty());
    assert!(!mesh.wire_indices.is_empty());
    renderer.update_mvp(&gpu.queue, camera.mvp());

    let opaque_vertices = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU pipeline test opaque vertices"),
            contents: bytemuck::cast_slice(&mesh.opaque_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let opaque_indices = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU pipeline test opaque indices"),
            contents: bytemuck::cast_slice(&mesh.opaque_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
    let wire_vertices = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU pipeline test wire vertices"),
            contents: bytemuck::cast_slice(&mesh.wire_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let wire_indices = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GPU pipeline test wire indices"),
            contents: bytemuck::cast_slice(&mesh.wire_indices),
            usage: wgpu::BufferUsages::INDEX,
        });
    let depth_target = renderer.create_depth_render_target(&gpu.device, 64, 64);
    let output = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("GPU pipeline test composite output"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("GPU pipeline test readback"),
        size: 64 * 64 * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GPU pipeline test depth pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &depth_target.render_color_view,
                resolve_target: depth_target.resolve_target(),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_target.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&renderer.pipeline_3d);
        pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
        pass.set_vertex_buffer(0, opaque_vertices.slice(..));
        pass.set_index_buffer(opaque_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.opaque_indices.len() as u32, 0, 0..1);
        pass.set_pipeline(&renderer.pipeline_3d_wire);
        pass.set_bind_group(0, &renderer.mvp_bind_group, &[]);
        pass.set_vertex_buffer(0, wire_vertices.slice(..));
        pass.set_index_buffer(wire_indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.wire_indices.len() as u32, 0, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GPU pipeline test composite pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.composite_depth_render_target(&mut pass, &depth_target);
    }
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(64 * 4),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(std::iter::once(encoder.finish()));

    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    gpu.device.poll(wgpu::Maintain::Wait);
    receiver.recv().unwrap().expect("GPU readback must succeed");
    let pixels = readback.slice(..).get_mapped_range();
    let center = &pixels[(32 * 64 + 32) * 4..(32 * 64 + 33) * 4];
    assert!(
        center[0] > 0 && center[2] > 0 && center[3] > 0,
        "the translucent front cube must blend over, not depth-occlude, the opaque rear cube: {center:?}"
    );
    drop(pixels);
    readback.unmap();
    let validation_error = pollster::block_on(gpu.device.pop_error_scope());
    assert!(
        validation_error.is_none(),
        "the depth and composite passes must not produce validation errors: {validation_error:?}"
    );
}

#[test]
fn implicit_gpu_greater_relations_match_cpu_field_and_nonzero_contour() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ImplicitComputePipeline::new(&gpu.device, &gpu.queue, 4);
    let bounds = (0.0, 1.0, 1.0, 3.0);
    let levels = [1.0];

    for operator in [RelationOperator::Greater, RelationOperator::GreaterEq] {
        let mut curve = ImplicitCurveObj::new("y", "3", operator);
        curve.contour_levels = Some(levels.to_vec());

        let rows = compute
            .evaluate(&gpu.device, &gpu.queue, &curve, bounds, 2, &HashMap::new())
            .expect("supported implicit expression must execute on the GPU");

        for (row_index, row) in rows.iter().enumerate() {
            let expected = 2.0 - row_index as f64;
            for value in row {
                assert_close(*value, expected, "Greater relation GPU field");
            }
        }

        let gpu_segments =
            marching_squares_from_grid(&rows, &levels, bounds.0, bounds.2, bounds.1, bounds.3);
        let cpu_segments = grafito_core::implicit_curve::evaluate_implicit_curve(
            &curve,
            bounds,
            2,
            &HashMap::new(),
        );

        assert_eq!(gpu_segments.len(), 1);
        assert_eq!(gpu_segments[0].0, 1.0);
        assert_eq!(gpu_segments[0].1.len(), 2);
        assert_eq!(gpu_segments.len(), cpu_segments.len());
        assert_eq!(gpu_segments[0].1.len(), cpu_segments[0].1.len());
        for ((gpu_a, gpu_b), (cpu_a, cpu_b)) in gpu_segments[0].1.iter().zip(&cpu_segments[0].1) {
            assert_close(gpu_a.x, cpu_a.x, "GPU/CPU contour start x");
            assert_close(gpu_a.y, cpu_a.y, "GPU/CPU contour start y");
            assert_close(gpu_b.x, cpu_b.x, "GPU/CPU contour end x");
            assert_close(gpu_b.y, cpu_b.y, "GPU/CPU contour end y");
            assert_close(gpu_a.y, 2.0, "nonzero contour start y");
            assert_close(gpu_b.y, 2.0, "nonzero contour end y");
        }
    }
}

#[test]
fn implicit_gpu_rejects_a_grid_above_the_shared_per_object_limit() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ImplicitComputePipeline::new(&gpu.device, &gpu.queue, MAX_IMPLICIT_GRID_SIZE);
    let curve = ImplicitCurveObj::new("x", "y", RelationOperator::Eq);

    let rows = compute.evaluate(
        &gpu.device,
        &gpu.queue,
        &curve,
        (-1.0, 1.0, -1.0, 1.0),
        MAX_IMPLICIT_GRID_SIZE + 1,
        &HashMap::new(),
    );

    assert!(rows.is_none());
}

#[test]
fn marching_squares_caps_segments_across_all_contour_levels() {
    let grid_size = 256;
    let rows: Vec<Vec<f64>> = (0..=grid_size)
        .map(|y| {
            (0..=grid_size)
                .map(|x| if (x + y) % 2 == 0 { -1.0 } else { 1.0 })
                .collect()
        })
        .collect();
    let levels = [0.0];

    let segments =
        marching_squares_from_grid(&rows, &levels, 0.0, 0.0, grid_size as f64, grid_size as f64);
    let total = segments.iter().map(|(_, level)| level.len()).sum::<usize>();

    assert_eq!(
        total, MAX_MARCHING_SQUARES_SEGMENTS,
        "GPU contour extraction must use the CPU cap"
    );
}

#[test]
fn gpu_and_cpu_contours_share_the_no_crossing_work_budget() {
    let grid_size = MAX_IMPLICIT_GRID_SIZE;
    let rows = vec![vec![1.0; grid_size + 1]; grid_size + 1];
    let levels: Vec<f64> = (2..=10).map(f64::from).collect();

    let gpu_segments =
        marching_squares_from_grid(&rows, &levels, 0.0, 0.0, grid_size as f64, grid_size as f64);
    let cpu_segments =
        cpu_marching_squares(&rows, &levels, 0.0, 0.0, grid_size as f64, grid_size as f64);

    let cells_per_level = grid_size * grid_size;
    assert_eq!(gpu_segments, cpu_segments);
    assert_eq!(
        gpu_segments.len(),
        MAX_MARCHING_SQUARES_WORK_UNITS / cells_per_level
    );
    assert!(gpu_segments.iter().all(|(_, segments)| segments.is_empty()));
}

#[test]
fn renderer_fill_compute_stays_none_without_fillable_implicits_and_activates_on_demand() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let mut renderer = Renderer::new(
        &gpu.device,
        &gpu.queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
    );
    assert!(
        renderer.fill_compute.is_none(),
        "Renderer::new no debe reservar los ~128 MiB de buffers de fill"
    );

    let mut plain = Document::new();
    plain.add_object(GeoObject::ParametricCurve2D(ParametricCurve2DObj::new(
        "cos(t)",
        "sin(t)",
        0.0,
        std::f64::consts::TAU,
    )));
    assert!(
        renderer
            .ensure_fill_compute_for_document(&gpu.device, &gpu.queue, &plain)
            .is_none(),
        "sin implícitas rellenables el pipeline de fill no se crea"
    );
    assert!(
        renderer.fill_compute.is_none(),
        "el campo fill_compute sigue None tras un documento sin fill (128 MiB ahorrados)"
    );

    let mut fillable = Document::new();
    fillable.add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
        "x",
        "y",
        RelationOperator::Less,
    )));
    assert!(
        renderer
            .ensure_fill_compute_for_document(&gpu.device, &gpu.queue, &fillable)
            .is_some(),
        "una implícita con op != Eq activa el pipeline de fill"
    );
    assert!(renderer.fill_compute.is_some());
}

#[test]
fn parametric_batch_matches_individual_dispatches() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 4000, 128);

    let circle = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU);
    let line = ParametricCurve2DObj::new("2*t - 1", "t*t", 0.0, 1.0);
    let polar = PolarCurveObj::new("1 + cos(t)", 0.0, std::f64::consts::TAU);

    let individual_2d = compute
        .evaluate_curve_2d(&gpu.device, &gpu.queue, &circle, 64, &HashMap::new())
        .expect("single 2D dispatch must execute");
    let individual_polar = compute
        .evaluate_polar(&gpu.device, &gpu.queue, &polar, 64, &HashMap::new())
        .expect("single polar dispatch must execute");

    let batched_2d = compute.evaluate_curves_2d_batched(
        &gpu.device,
        &gpu.queue,
        &[(&circle, 64), (&line, 64)],
        &HashMap::new(),
    );
    let batched_polar =
        compute.evaluate_polars_batched(&gpu.device, &gpu.queue, &[(&polar, 64)], &HashMap::new());

    assert_eq!(batched_2d.len(), 2);
    let batched_circle = batched_2d[0].as_ref().expect("circle must batch");
    assert_eq!(batched_circle.len(), individual_2d.len());
    for ((bx, by), (ix, iy)) in batched_circle.iter().zip(&individual_2d) {
        assert_close(*bx, *ix, "batched 2D x");
        assert_close(*by, *iy, "batched 2D y");
    }
    let batched_line = batched_2d[1].as_ref().expect("line must batch");
    assert_eq!(batched_line.len(), 65);

    let batched_polar_samples = batched_polar[0].as_ref().expect("polar must batch");
    assert_eq!(batched_polar_samples.len(), individual_polar.len());
    for ((bx, by), (ix, iy)) in batched_polar_samples.iter().zip(&individual_polar) {
        assert_close(*bx, *ix, "batched polar x");
        assert_close(*by, *iy, "batched polar y");
    }
}

#[test]
fn parametric_3d_batch_matches_individual_dispatch() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 4000, 128);
    let helix = ParametricCurve3DObj::new("cos(t)", "sin(t)", "t", 0.0, std::f64::consts::TAU);

    let individual = compute
        .evaluate_curve_3d(&gpu.device, &gpu.queue, &helix, 64, &HashMap::new())
        .expect("single 3D dispatch must execute");
    let batched = compute.evaluate_curves_3d_batched(
        &gpu.device,
        &gpu.queue,
        &[(&helix, 64)],
        &HashMap::new(),
    );
    let batched = batched[0].as_ref().expect("helix must batch");

    assert_eq!(batched.len(), individual.len());
    for ((bx, by, bz), (ix, iy, iz)) in batched.iter().zip(&individual) {
        assert_close(*bx, *ix, "batched 3D x");
        assert_close(*by, *iy, "batched 3D y");
        assert_close(*bz, *iz, "batched 3D z");
    }
}

#[test]
fn maybe_compute_batched_populates_caches_in_one_submit() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 4000, 128);
    let circle = ParametricCurve2DObj::new("cos(t)", "sin(t)", 0.0, std::f64::consts::TAU);
    let line = ParametricCurve2DObj::new("t", "t", 0.0, 1.0);

    let results = maybe_compute_curves_2d_on_gpu_batched(
        &compute,
        &gpu.device,
        &gpu.queue,
        &[(&circle, 64), (&line, 64)],
        &HashMap::new(),
    );
    assert_eq!(results, vec![true, true]);
    assert_eq!(circle.cached_samples.read().unwrap().len(), 65);
    assert_eq!(line.cached_samples.read().unwrap().len(), 65);

    // Segunda llamada: cachés ya pobladas → true sin re-despachar.
    let results2 = maybe_compute_curves_2d_on_gpu_batched(
        &compute,
        &gpu.device,
        &gpu.queue,
        &[(&circle, 64), (&line, 64)],
        &HashMap::new(),
    );
    assert_eq!(results2, vec![true, true]);
}

#[test]
fn maybe_compute_batched_3d_and_polar_populate_caches_in_one_submit() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 4000, 128);
    let helix = ParametricCurve3DObj::new("cos(t)", "sin(t)", "t", 0.0, std::f64::consts::TAU);
    let polar = PolarCurveObj::new("1 + cos(t)", 0.0, std::f64::consts::TAU);

    let results_3d = maybe_compute_curves_3d_on_gpu_batched(
        &compute,
        &gpu.device,
        &gpu.queue,
        &[(&helix, 64)],
        &HashMap::new(),
    );
    assert_eq!(results_3d, vec![true]);
    assert_eq!(helix.cached_samples.read().unwrap().len(), 65);

    let results_polar = maybe_compute_polars_on_gpu_batched(
        &compute,
        &gpu.device,
        &gpu.queue,
        &[(&polar, 64)],
        &HashMap::new(),
    );
    assert_eq!(results_polar, vec![true]);
    assert_eq!(polar.cached_samples.read().unwrap().len(), 65);
}

#[test]
fn domain_coloring_rejects_over_250k_cells_before_dispatch() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = DomainColoringComputePipeline::new(&gpu.device, &gpu.queue);
    let expr = grafito_complex::math::complex_expr::parse("z").expect("test expression must parse");
    let points: Vec<(f64, f64)> = (0..250_001).map(|i| (i as f64 * 1e-6, 0.0)).collect();

    let result = compute.evaluate(&gpu.device, &gpu.queue, &expr, &points, &HashMap::new(), 0);
    assert!(
        result.is_none(),
        "MAX_CELLS 250k es un presupuesto duro: 250_001 celdas se rechazan"
    );
}

#[test]
fn surface_gpu_rejects_over_128_resolution_without_clamping() {
    let Some(gpu) = gpu_context_or_skip() else {
        return;
    };
    let compute = ParametricComputePipeline::new(&gpu.device, &gpu.queue, 4000, 128);
    let surface = Surface3DObj::new("x + y", (0.0, 1.0), (0.0, 1.0));

    assert!(
        compute
            .evaluate_surface(&gpu.device, &gpu.queue, &surface, 129, &HashMap::new())
            .is_none(),
        "MAX_SURFACE_RES 128 es un presupuesto duro: res 129 se rechaza"
    );

    let cpu_grid = vec![vec![Point3D::new(3.0, 4.0, 5.0)]];
    *surface.cached_grid.write().unwrap() = cpu_grid.clone();
    assert!(!maybe_compute_surface_on_gpu(
        &compute,
        &gpu.device,
        &gpu.queue,
        &surface,
        129,
        &HashMap::new(),
    ));
    assert_eq!(*surface.cached_grid.read().unwrap(), cpu_grid);
}

fn assert_close(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= EPSILON,
        "{context}: expected {expected}, got {actual}"
    );
}
