#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unusual_byte_groupings,
    clippy::unnecessary_sort_by
)]
//! Property test: BTreeMap determinism — insertion order must not affect serialization.
//!
//! Document currently uses HashMap but `ValidatedDocument` + `semantic_document_baseline`
//! guarantee stable snapshots via explicit ordering in persistence. This test documents
//! the invariant that 500 objects inserted in random order (fixed seed) serialise
//! deterministically. After migrating `Document {objects,variables}` to BTreeMap the
//! canonical path can be simplified to direct `serde_json::to_string` equality.

use grafito_core::{Document, GeoObject, ObjectId, PointObj};
use grafito_geometry::Point2;
use std::collections::BTreeMap;

fn next_u64(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

fn shuffled_indices(seed: u64, n: usize) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..n).collect();
    let mut state = seed;
    for i in (1..n).rev() {
        let j = (next_u64(&mut state) as usize) % (i + 1);
        indices.swap(i, j);
    }
    indices
}

fn position_for_index(index: usize) -> Point2 {
    // Deterministic finite positions inside renderable bounds, not colinear degenerate.
    let x = (index as f64 * 1.7).rem_euclid(200.0) - 100.0;
    let y = (index as f64 * 3.1).rem_euclid(200.0) - 100.0;
    Point2::new(x, y)
}

fn deterministic_id(index: usize) -> ObjectId {
    // Deterministic UUID from index — avoids random v4 so same logical set yields same ids
    // regardless of build order. High bits fixed to keep UUID version 4 shape but stable.
    let u = 0x1234_5678_0000_4000u128
        | ((index as u128) << 48)
        | (index as u128).wrapping_mul(0x9E3779B97F4A7C15);
    // Set variant bits (RFC 4122 variant 10xxxxxx)
    let u =
        (u & 0xFFFFFFFF_FFFF_3FFF_FFFF_FFFF_FFFFu128) | 0x0000_0000_0000_8000_0000_0000_0000u128;
    ObjectId(uuid::Uuid::from_u128(u))
}

fn build_document_with_order(order: &[usize]) -> Document {
    let mut doc = Document::new();
    for &idx in order {
        let pos = position_for_index(idx);
        // Explicit label + deterministic id ensures HashMap insertion order does not affect identity.
        let mut point = PointObj::new(pos).with_label(format!("P{idx:04}"));
        point.id = deterministic_id(idx);
        let obj = GeoObject::Point(point);
        doc.try_add_object(obj)
            .expect("object insertion must succeed");
    }
    doc
}

/// Canonical JSON value where HashMap order is neutralised via BTreeMap sorting.
/// Once Document migrates to BTreeMap this helper becomes identity.
/// Also sorts `free_objects` arrays (HashSet-derived) for deterministic comparison.
fn canonical_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for (k, v) in map {
                let canon = canonical_value(v);
                // Sort free_objects arrays lexicographically for determinism
                let canon = if k == "free_objects" {
                    if let serde_json::Value::Array(mut arr) = canon {
                        arr.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
                        serde_json::Value::Array(arr)
                    } else {
                        canon
                    }
                } else {
                    canon
                };
                sorted.insert(k, canon);
            }
            let mut out = serde_json::Map::new();
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(canonical_value).collect())
        }
        other => other,
    }
}

fn canonical_json(doc: &Document) -> serde_json::Value {
    let v = serde_json::to_value(doc).expect("Document serialises");
    canonical_value(v)
}

#[test]
fn btreemap_determinism() {
    const N: usize = 500;
    const SEED: u64 = 0xC0FFEE_1234_5678;

    let order = shuffled_indices(SEED, N);
    let doc = build_document_with_order(&order);

    // Serializa 2× el mismo documento — debe ser idéntico (stable snapshot).
    let json_a = serde_json::to_string(&doc).expect("serialize 1");
    let json_b = serde_json::to_string(&doc).expect("serialize 2");
    assert_eq!(
        json_a, json_b,
        "same document must serialise identically twice"
    );

    // Determinismo cruzado: mismo conjunto insertado en orden natural vs aleatorio
    // debe producir snapshots canónicos idénticos (BTreeMap).
    let natural: Vec<usize> = (0..N).collect();
    let doc_natural = build_document_with_order(&natural);
    let doc_shuffled = doc;

    let canon_natural = canonical_json(&doc_natural);
    let canon_shuffled = canonical_json(&doc_shuffled);
    assert_eq!(
        canon_natural, canon_shuffled,
        "canonical JSON must be identical regardless of insertion order (BTreeMap invariant)"
    );

    // Also verify via serde_json::Value direct equality after canonicalisation
    let str_natural = serde_json::to_string(&canon_natural).unwrap();
    let str_shuffled = serde_json::to_string(&canon_shuffled).unwrap();
    assert_eq!(str_natural, str_shuffled);

    // Idempotencia: deserializar y volver a serializar preserva determinismo
    let decoded: Document =
        serde_json::from_str(&json_a).expect("Document deserialises from its own JSON");
    let re_encoded = serde_json::to_string(&decoded).expect("re-serialize");
    let canon_re = canonical_value(serde_json::from_str(&re_encoded).unwrap());
    assert_eq!(
        canonical_value(serde_json::from_str(&json_a).unwrap()),
        canon_re,
        "round-trip must preserve canonical form"
    );
}

#[test]
fn btreemap_determinism_small_seed_variants() {
    // Sanity: different seeds but same logical set still canonicalise equally
    let order_a = shuffled_indices(1, 50);
    let order_b = shuffled_indices(2, 50);
    let doc_a = build_document_with_order(&order_a);
    let doc_b = build_document_with_order(&order_b);
    assert_eq!(
        canonical_json(&doc_a),
        canonical_json(&doc_b),
        "50-object sets must canonicalise identically across seeds"
    );
}
