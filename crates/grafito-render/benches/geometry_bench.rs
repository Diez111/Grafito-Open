use criterion::{black_box, criterion_group, criterion_main, Criterion};
use grafito_core::{Document, GeoObject};
use grafito_geometry::{Point2, ViewTransform};
use grafito_render::Renderer;

fn build_document_with_50_objects() -> Document {
    let mut doc = Document::new();
    let mut points = Vec::new();

    // 25 points arranged in a grid.
    for i in 0..5 {
        for j in 0..5 {
            let p = doc.add_object(GeoObject::Point(grafito_core::PointObj::new(Point2::new(
                i as f64 * 2.0,
                j as f64 * 2.0,
            ))));
            points.push(p);
        }
    }

    // 15 line segments connecting neighbouring grid points.
    for i in 0..5 {
        for j in 0..3 {
            let a = points[i * 5 + j];
            let b = points[i * 5 + j + 1];
            let pa = doc.point_position(a).unwrap();
            let pb = doc.point_position(b).unwrap();
            doc.add_object(GeoObject::Line(grafito_core::LineObj::new(pa, pb)));
        }
    }

    // 10 circles at alternating grid positions.
    for i in 0..5 {
        for j in 0..2 {
            let center = Point2::new(i as f64 * 2.0 + 0.5, j as f64 * 4.0 + 0.5);
            doc.add_object(GeoObject::Circle(grafito_core::CircleObj::new(
                center, 0.75,
            )));
        }
    }

    assert_eq!(doc.object_count(), 50);
    doc
}

fn bench_geometry_building(c: &mut Criterion) {
    let doc = build_document_with_50_objects();
    let view = ViewTransform::default();

    c.bench_function("geometry_build_50_objects", |b| {
        b.iter(|| {
            let (vertices, indices) =
                Renderer::build_geometry_static(black_box(&doc), black_box(&view), false);
            black_box((vertices.len(), indices.len()))
        })
    });
}

criterion_group!(benches, bench_geometry_building);
criterion_main!(benches);
