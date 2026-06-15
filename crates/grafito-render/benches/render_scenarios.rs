use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{
    CircleObj, Document, FunctionObj, GeoObject, LineObj, ParametricCurve3DObj, PointObj,
    PolygonObj, Surface3DObj, VectorField2DObj,
};
use grafito_geometry::{Camera3D, Point2, ViewTransform};
use grafito_render::Renderer;

fn view_800x600() -> ViewTransform {
    ViewTransform::new(800.0, 600.0)
}

fn make_doc_with_mixed_2d_objects(count: usize) -> Document {
    let mut doc = Document::new();
    for i in 0..count {
        let obj = match i % 5 {
            0 => GeoObject::Point(PointObj::new(Point2::new(
                (i as f64) * 0.5,
                (i as f64).sin(),
            ))),
            1 => GeoObject::Line(LineObj::new(
                Point2::new(i as f64, 0.0),
                Point2::new(i as f64 + 1.0, 1.0),
            )),
            2 => GeoObject::Circle(CircleObj::new(
                Point2::new((i as f64) * 0.2, (i as f64) * 0.1),
                0.5,
            )),
            3 => GeoObject::Polygon(PolygonObj::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(1.0, 1.0),
                Point2::new(0.0, 1.0),
            ])),
            _ => GeoObject::Function(FunctionObj::new("sin(x)")),
        };
        doc.add_object(obj);
    }
    doc
}

fn bench_build_geometry_many_objects(c: &mut Criterion) {
    let doc = make_doc_with_mixed_2d_objects(100);
    let view = view_800x600();

    c.bench_function("build_geometry_many_objects", |b| {
        b.iter(|| {
            let (vertices, indices) =
                Renderer::build_geometry_static(black_box(&doc), black_box(&view), false);
            black_box((vertices.len(), indices.len()));
        })
    });
}

fn bench_build_geometry_with_functions(c: &mut Criterion) {
    let mut doc = Document::new();
    for i in 0..10 {
        doc.add_object(GeoObject::Function(FunctionObj::new(&format!(
            "sin({}*x)",
            i + 1
        ))));
    }
    let view = view_800x600();

    c.bench_function("build_geometry_with_functions", |b| {
        b.iter(|| {
            let (vertices, indices) =
                Renderer::build_geometry_static(black_box(&doc), black_box(&view), false);
            black_box((vertices.len(), indices.len()));
        })
    });
}

fn try_headless_renderer() -> Option<std::sync::Arc<Renderer>> {
    use std::sync::{Arc, OnceLock};
    static RENDERER: OnceLock<Option<Arc<Renderer>>> = OnceLock::new();
    RENDERER
        .get_or_init(|| {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
            let adapter = instance
                .enumerate_adapters(wgpu::Backends::all())
                .into_iter()
                .next()?;
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    label: None,
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            ))
            .ok()?;
            Some(Arc::new(Renderer::new(
                &device,
                &queue,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                1,
            )))
        })
        .clone()
}

fn bench_build_geometry_with_parametrics(c: &mut Criterion) {
    let mut doc = Document::new();
    for i in 0..10 {
        doc.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
            "cos(t)",
            "sin(t)",
            &format!("{}", (i as f64) * 0.1),
            0.0,
            std::f64::consts::TAU,
        )));
    }
    for i in 0..5 {
        doc.add_object(GeoObject::Surface3D(Surface3DObj::new(
            "x^2 + y^2",
            (-1.0 - i as f64 * 0.1, 1.0 + i as f64 * 0.1),
            (-1.0 - i as f64 * 0.1, 1.0 + i as f64 * 0.1),
        )));
    }

    let camera = Camera3D::new(800.0 / 600.0);

    if let Some(renderer) = try_headless_renderer() {
        c.bench_function("build_geometry_with_parametrics", |b| {
            b.iter(|| {
                let (vertices, indices) = renderer.build_3d_geometry(
                    black_box(&doc),
                    black_box(&camera),
                    false,
                    800.0,
                    600.0,
                );
                black_box((vertices.len(), indices.len()));
            })
        });
    } else {
        c.bench_function("build_geometry_with_parametrics", |b| {
            b.iter(|| {
                let (vertices, indices) = Renderer::build_3d_geometry_static(
                    black_box(&doc),
                    black_box(&camera),
                    false,
                    800.0,
                    600.0,
                );
                black_box((vertices.len(), indices.len()));
            })
        });
    }
}

fn bench_build_geometry_with_vector_fields(c: &mut Criterion) {
    let mut doc = Document::new();
    for i in 0..5 {
        doc.add_object(GeoObject::VectorField2D(VectorField2DObj::new(
            "x",
            &format!("y + {}", i),
        )));
    }
    let view = view_800x600();

    c.bench_function("build_geometry_with_vector_fields", |b| {
        b.iter(|| {
            let (vertices, indices) =
                Renderer::build_geometry_static(black_box(&doc), black_box(&view), false);
            black_box((vertices.len(), indices.len()));
        })
    });
}

criterion_group!(
    benches,
    bench_build_geometry_many_objects,
    bench_build_geometry_with_functions,
    bench_build_geometry_with_parametrics,
    bench_build_geometry_with_vector_fields
);
criterion_main!(benches);
