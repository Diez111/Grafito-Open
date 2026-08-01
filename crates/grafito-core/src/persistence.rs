//! Versioned, validated persistence for Grafito documents.

use crate::validation::{parse_document_json, validate_document, MAX_DOCUMENT_SIZE_BYTES};
use crate::{Document, GeoObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Schema emitted by current Grafito document saves.
pub const CURRENT_DOCUMENT_SCHEMA_VERSION: u32 = 5;

const LEGACY_XZY_DOCUMENT_SCHEMA_VERSION: u32 = 1;
const LEGACY_PRE_CAS_WORKSHEET_SCHEMA_VERSION: u32 = 2;
const LEGACY_PRE_TETRAHEDRON_DOCUMENT_SCHEMA_VERSION: u32 = 3;
const LEGACY_PRE_REGULAR_POLYTOPE_DOCUMENT_SCHEMA_VERSION: u32 = 4;

static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// Versioned on-disk representation of a Grafito document.
#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentEnvelope {
    pub schema_version: u32,
    pub producer_version: String,
    pub document: Document,
}

/// Errors produced while serializing, loading, validating, or writing a document.
#[derive(Debug, Error)]
pub enum DocumentPersistenceError {
    #[error("invalid document JSON: {0}")]
    InvalidJson(String),
    #[error("document JSON conversion failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(
        "document schema version {schema_version} is newer than supported version {current_version}"
    )]
    UnsupportedFutureSchema {
        schema_version: u32,
        current_version: u32,
    },
    #[error("unsupported document schema version {0}")]
    UnsupportedSchema(u32),
    #[error("document semantic validation failed: {0}")]
    SemanticValidation(String),
    #[error("document file I/O failed: {0}")]
    Io(#[from] io::Error),
}

/// Serializes a document in the current versioned envelope format.
pub fn serialize_document(document: &Document) -> Result<String, DocumentPersistenceError> {
    validate_document(document).map_err(DocumentPersistenceError::SemanticValidation)?;
    let envelope = DocumentEnvelope {
        schema_version: CURRENT_DOCUMENT_SCHEMA_VERSION,
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        document: document.clone(),
    };
    let json = serde_json::to_string_pretty(&envelope)?;
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(DocumentPersistenceError::SemanticValidation(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        )));
    }
    Ok(json)
}

/// Deserializes either the current envelope or a legacy raw `Document` JSON.
pub fn deserialize_document(json: &str) -> Result<Document, DocumentPersistenceError> {
    // Validate untrusted text before any model deserialization.
    let value = parse_document_json(json).map_err(DocumentPersistenceError::InvalidJson)?;

    let mut document = match value {
        Value::Object(object) => {
            if let Some(schema_value) = object.get("schema_version") {
                let schema_version: u32 = serde_json::from_value(schema_value.clone())?;
                if schema_version > CURRENT_DOCUMENT_SCHEMA_VERSION {
                    return Err(DocumentPersistenceError::UnsupportedFutureSchema {
                        schema_version,
                        current_version: CURRENT_DOCUMENT_SCHEMA_VERSION,
                    });
                }
                let envelope: DocumentEnvelope = serde_json::from_value(Value::Object(object))?;
                match schema_version {
                    CURRENT_DOCUMENT_SCHEMA_VERSION => envelope.document,
                    LEGACY_PRE_REGULAR_POLYTOPE_DOCUMENT_SCHEMA_VERSION => envelope.document,
                    LEGACY_PRE_TETRAHEDRON_DOCUMENT_SCHEMA_VERSION => envelope.document,
                    LEGACY_PRE_CAS_WORKSHEET_SCHEMA_VERSION => envelope.document,
                    LEGACY_XZY_DOCUMENT_SCHEMA_VERSION => {
                        migrate_legacy_document(envelope.document, false, true)
                    }
                    _ => return Err(DocumentPersistenceError::UnsupportedSchema(schema_version)),
                }
            } else {
                let constraints_were_serialized = object.contains_key("constraints");
                migrate_legacy_document(
                    serde_json::from_value(Value::Object(object))?,
                    !constraints_were_serialized,
                    true,
                )
            }
        }
        value => migrate_legacy_document(serde_json::from_value(value)?, false, true),
    };

    document
        .recompute_spreadsheet_variables()
        .map_err(DocumentPersistenceError::SemanticValidation)?;
    document.prune_spreadsheet_coordinate_points();
    validate_document(&document).map_err(DocumentPersistenceError::SemanticValidation)?;
    document.spatial_dirty = true;
    Ok(document)
}

/// Writes a current-format document through a synced temporary file before renaming it.
///
/// On Unix, the destination directory is synced after the rename. On Windows,
/// an existing destination is atomically replaced with `ReplaceFileW`; a missing
/// destination uses `fs::rename`.
pub fn write_document_atomic(
    document: &Document,
    path: impl AsRef<Path>,
) -> Result<(), DocumentPersistenceError> {
    let json = serialize_document(document)?;
    write_atomic(path.as_ref(), json.as_bytes())?;
    Ok(())
}

/// Reads and validates a document file in either supported on-disk format.
pub fn read_document_file(path: impl AsRef<Path>) -> Result<Document, DocumentPersistenceError> {
    let path = path.as_ref();
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(DocumentPersistenceError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "document source must be a regular file",
        )));
    }
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_DOCUMENT_SIZE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(DocumentPersistenceError::InvalidJson(format!(
            "Document size exceeds maximum {}",
            MAX_DOCUMENT_SIZE_BYTES
        )));
    }
    let json = String::from_utf8(bytes)
        .map_err(|error| DocumentPersistenceError::InvalidJson(error.to_string()))?;
    deserialize_document(&json)
}

/// The raw Document format predates envelopes. Saves from before constraint
/// persistence treated every object as free, so restore that partition when
/// the field is absent while preserving serde defaults for other fields.
fn migrate_legacy_document(
    mut document: Document,
    missing_constraints: bool,
    migrate_axis_convention: bool,
) -> Document {
    if missing_constraints {
        let object_ids: Vec<_> = document.objects_iter().map(|(id, _)| *id).collect();
        for id in object_ids {
            document.constraints.add_free_object(id);
        }
    }
    if migrate_axis_convention {
        for (_, object) in document.objects_iter_mut() {
            migrate_legacy_3d_axis_convention(object);
        }
    }
    document
}

fn migrate_legacy_3d_axis_convention(object: &mut GeoObject) {
    match object {
        GeoObject::Surface3D(surface) if surface.is_parametric => {
            std::mem::swap(&mut surface.expr_y, &mut surface.expr_z);
        }
        GeoObject::Surface3D(surface) => {
            surface.legacy_axis_swap = true;
        }
        GeoObject::ParametricCurve3D(curve) => {
            std::mem::swap(&mut curve.expr_y, &mut curve.expr_z);
        }
        GeoObject::VectorField3D(field) => {
            let expr_u = swap_y_z_variables(&field.expr_u);
            let expr_v = swap_y_z_variables(&field.expr_w);
            let expr_w = swap_y_z_variables(&field.expr_v);
            field.expr_u = expr_u;
            field.expr_v = expr_v;
            field.expr_w = expr_w;
            std::mem::swap(&mut field.y_min, &mut field.z_min);
            std::mem::swap(&mut field.y_max, &mut field.z_max);
        }
        GeoObject::Transformed(transformed) => {
            migrate_legacy_3d_axis_convention(&mut transformed.inner);
        }
        _ => {}
    }
}

fn swap_y_z_variables(expression: &str) -> String {
    fn push_swapped(output: &mut String, identifier: &str) {
        match identifier {
            "y" => output.push('z'),
            "z" => output.push('y'),
            _ => output.push_str(identifier),
        }
    }

    let mut output = String::with_capacity(expression.len());
    let mut characters = expression.chars().peekable();
    while let Some(character) = characters.next() {
        if character.is_alphabetic() || character == '_' {
            let mut identifier = String::from(character);
            while characters
                .peek()
                .is_some_and(|next| next.is_alphanumeric() || *next == '_')
            {
                identifier.push(characters.next().expect("peeked identifier character"));
            }
            push_swapped(&mut output, &identifier);
        } else {
            output.push(character);
        }
    }
    output
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "document destination must include a file name",
        )
    })?;
    let (mut temporary_file, temporary_path) = create_temporary_file(parent, file_name)?;

    let write_result: io::Result<()> = (|| {
        temporary_file.write_all(contents)?;
        temporary_file.sync_all()?;
        #[cfg(unix)]
        {
            apply_destination_permissions(&temporary_file, path)?;
            temporary_file.sync_all()?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    drop(temporary_file);

    if let Err(error) = replace_temporary_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn apply_destination_permissions(temporary_file: &File, destination: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Keep temporary content private while it is being written, then retain the
    // destination's visible permission bits only at the atomic replacement point.
    let mode = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => metadata.permissions().mode() & 0o777,
        Ok(_) => 0o600,
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0o600,
        Err(error) => return Err(error),
    };
    temporary_file.set_permissions(fs::Permissions::from_mode(mode))
}

#[cfg(not(windows))]
fn replace_temporary_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary_path, path)
}

#[cfg(windows)]
fn replace_temporary_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
    if path.try_exists()? {
        replace_existing_file_windows(temporary_path, path)
    } else {
        fs::rename(temporary_path, path)
    }
}

#[cfg(windows)]
fn replace_existing_file_windows(temporary_path: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    fn path_as_wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows paths cannot contain an interior NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    let destination = path_as_wide(path)?;
    let replacement = path_as_wide(temporary_path)?;

    // Both vectors are validated, NUL-terminated UTF-16 paths that remain alive
    // for the duration of the synchronous Windows API call.
    let replaced = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn create_temporary_file(
    parent: &Path,
    file_name: &std::ffi::OsStr,
) -> io::Result<(File, PathBuf)> {
    for _ in 0..16 {
        let id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".{}-{}-{}.tmp",
            file_name.to_string_lossy(),
            std::process::id(),
            id
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;

            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a unique temporary document file",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BoxPlotObj, ComplexIntegralObj, Document, Fractal2DObj, GeoObject, HyperSurface4DObj,
        ImplicitCurveObj, ParametricCurve3DObj, Point3DObj, PolygonObj, RegularPolychoron4DObj,
        RegularPolytopeNDObj, Surface3DObj, Tetrahedron3DObj, TransformedObj, VectorField3DObj,
    };
    use grafito_geometry::{Color, Point2, Point3D, RegularPolychoron, RegularPolytopeFamily};
    use serde_json::Value;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn sample_document() -> Document {
        let mut document = Document::new();
        let a = document.add_point(Point2::new(0.0, 0.0));
        let b = document.add_point(Point2::new(4.0, 0.0));
        document.add_distance_constraint(a, b, 4.0);
        document
    }

    fn unchecked_document_with_object(object: GeoObject) -> Document {
        let id = object.id();
        let mut raw = serde_json::to_value(Document::new()).expect("serialize empty document");
        raw["objects"]
            .as_object_mut()
            .expect("objects are represented as a map")
            .insert(
                id.0.to_string(),
                serde_json::to_value(object).expect("serialize unchecked object"),
            );
        serde_json::from_value(raw).expect("deserialize unchecked test document")
    }

    fn temporary_path(name: &str) -> std::path::PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "grafito_persistence_{name}_{}_{}",
            std::process::id(),
            id
        ))
    }

    #[test]
    fn current_envelope_round_trip() {
        assert_eq!(CURRENT_DOCUMENT_SCHEMA_VERSION, 5);
        let document = sample_document();

        let json = serialize_document(&document).expect("serialize current document");
        let value: Value = serde_json::from_str(&json).expect("parse envelope JSON");
        let envelope = value.as_object().expect("envelope object");
        assert_eq!(
            envelope.get("schema_version"),
            Some(&Value::from(CURRENT_DOCUMENT_SCHEMA_VERSION))
        );
        assert!(envelope.contains_key("producer_version"));
        assert!(envelope.contains_key("document"));
        assert!(!envelope.contains_key("objects"));

        let loaded = deserialize_document(&json).expect("deserialize current envelope");
        assert_eq!(loaded.object_count(), document.object_count());
        assert_eq!(
            loaded.constraints.constraint_count(),
            document.constraints.constraint_count()
        );
    }

    #[test]
    fn tetrahedron_round_trips_in_the_current_document_schema() {
        let mut document = Document::new();
        let tetrahedron = document
            .try_add_object(GeoObject::Tetrahedron3D(Tetrahedron3DObj::new(
                Point3D::new(1.0, 2.0, 3.0),
                2.0,
            )))
            .expect("valid tetrahedron");

        let json = serialize_document(&document).expect("serialize tetrahedron");
        let loaded = deserialize_document(&json).expect("deserialize tetrahedron");

        assert!(matches!(
            loaded.get_object(tetrahedron),
            Some(GeoObject::Tetrahedron3D(object))
                if object.center == Point3D::new(1.0, 2.0, 3.0)
                    && object.edge_length == 2.0
                    && object.fill_color.is_some()
        ));
    }

    #[test]
    fn regular_polytope_objects_round_trip_with_their_presentation() {
        let mut document = Document::new();

        let mut polychoron =
            RegularPolychoron4DObj::new(RegularPolychoron::TwentyFourCell).with_label("24-cell");
        polychoron.scale = 2.5;
        polychoron.rotation_angles = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        polychoron.color = Color::new(0.1, 0.2, 0.3, 0.9);
        polychoron.visible = false;
        polychoron.width = 2.25;
        polychoron.fill_color = Some(Color::new(0.4, 0.5, 0.6, 0.7));
        let expected_polychoron = GeoObject::RegularPolychoron4D(polychoron.clone());
        let polychoron_id = document
            .try_add_object(GeoObject::RegularPolychoron4D(polychoron))
            .expect("valid regular polychoron");

        let mut polytope =
            RegularPolytopeNDObj::new(RegularPolytopeFamily::Hypercube, 5).with_label("5-cube");
        polytope.scale = 1.75;
        polytope.rotation_angles = (0..10).map(|index| index as f64 / 10.0).collect();
        polytope.color = Color::new(0.7, 0.6, 0.5, 0.4);
        polytope.visible = false;
        polytope.width = 3.0;
        polytope.fill_color = Some(Color::new(0.3, 0.2, 0.1, 0.8));
        let expected_polytope = GeoObject::RegularPolytopeND(polytope.clone());
        let polytope_id = document
            .try_add_object(GeoObject::RegularPolytopeND(polytope))
            .expect("valid regular N-D polytope");

        let json = serialize_document(&document).expect("serialize regular polytopes");
        let envelope: Value = serde_json::from_str(&json).expect("parse current envelope");
        assert_eq!(
            envelope["schema_version"],
            Value::from(CURRENT_DOCUMENT_SCHEMA_VERSION)
        );

        let loaded = deserialize_document(&json).expect("deserialize regular polytopes");
        assert_eq!(loaded.get_object(polychoron_id), Some(&expected_polychoron));
        assert_eq!(loaded.get_object(polytope_id), Some(&expected_polytope));
    }

    #[test]
    fn schemas_v2_through_v4_remain_readable_after_the_regular_polytope_schema_bump() {
        let document = sample_document();
        for schema_version in [
            LEGACY_PRE_CAS_WORKSHEET_SCHEMA_VERSION,
            LEGACY_PRE_TETRAHEDRON_DOCUMENT_SCHEMA_VERSION,
            LEGACY_PRE_REGULAR_POLYTOPE_DOCUMENT_SCHEMA_VERSION,
        ] {
            let legacy = serde_json::json!({
                "schema_version": schema_version,
                "producer_version": "1.2.20-beta",
                "document": document.clone(),
            });

            let loaded =
                deserialize_document(&legacy.to_string()).expect("legacy schema remains readable");
            assert_eq!(loaded.object_count(), 2);
        }
    }

    #[test]
    fn schema_v4_legacy_hypersurface_fixture_remains_readable() {
        let mut document = Document::new();
        let mut hypersurface = HyperSurface4DObj::hypercube().with_label("legacy-hypercube");
        hypersurface.rotation_angles = vec![0.1, 0.2, 0.3];
        let hypersurface_id = document
            .try_add_object(GeoObject::HyperSurface4D(hypersurface))
            .expect("valid legacy hypersurface");
        let fixture = serde_json::json!({
            "schema_version": LEGACY_PRE_REGULAR_POLYTOPE_DOCUMENT_SCHEMA_VERSION,
            "producer_version": "1.2.20-beta",
            "document": document,
        });

        let loaded = deserialize_document(&fixture.to_string())
            .expect("schema-v4 legacy hypersurface remains readable");

        assert!(matches!(
            loaded.get_object(hypersurface_id),
            Some(GeoObject::HyperSurface4D(object))
                if object.surface_type == "hypercube"
                    && object.rotation_angles == vec![0.1, 0.2, 0.3]
                    && object.label == "legacy-hypercube"
        ));
    }

    #[test]
    fn spreadsheet_coordinate_owners_round_trip_and_legacy_documents_default_to_none() {
        let mut document = Document::new();
        let point = document.add_object(GeoObject::Point(
            crate::PointObj::new(Point2::new(1.0, 2.0)).with_label("A1"),
        ));
        document.set_spreadsheet_coordinate_point("A1".to_string(), point);

        let json = serialize_document(&document).expect("serialize owned spreadsheet point");
        let mut value: Value = serde_json::from_str(&json).expect("parse envelope JSON");
        let mut loaded = deserialize_document(&json).expect("deserialize owned spreadsheet point");
        assert_eq!(loaded.spreadsheet_coordinate_point("A1"), Some(point));

        value["document"]
            .as_object_mut()
            .expect("document object")
            .remove("spreadsheet_coordinate_points");
        let legacy_json = serde_json::to_string(&value).expect("encode legacy document");
        let mut legacy = deserialize_document(&legacy_json).expect("deserialize legacy document");
        assert_eq!(legacy.spreadsheet_coordinate_point("A1"), None);
    }

    #[test]
    fn deserialized_document_rebuilds_its_spatial_index_before_picking() {
        let mut document = Document::new();
        let point = document.add_point(Point2::new(3.0, 4.0));
        document.rebuild_spatial_index();
        assert!(!document.spatial_dirty);

        let json = serialize_document(&document).expect("serialize document");
        let mut loaded = deserialize_document(&json).expect("deserialize document");

        assert!(loaded.spatial_dirty);
        assert_eq!(loaded.pick_object(Point2::new(3.0, 4.0), 0.1), Some(point));
        assert!(!loaded.spatial_dirty);
    }

    #[test]
    fn serialization_rejects_non_finite_statistical_samples() {
        let mut document = Document::new();
        let box_plot = document
            .try_add_object(GeoObject::BoxPlot(BoxPlotObj::new(vec![0.0, 1.0])))
            .expect("valid BoxPlot");
        let Some(GeoObject::BoxPlot(box_plot)) = document.get_object_mut(box_plot) else {
            panic!("expected BoxPlot");
        };
        box_plot.data = vec![f64::NAN, 1.0];

        let error = serialize_document(&document)
            .expect_err("non-finite statistical data must not serialize");

        assert!(error
            .to_string()
            .contains("BoxPlot data contains non-finite values"));
    }

    #[test]
    fn serialization_rejects_non_finite_variables_before_json_encoding() {
        let mut document = Document::new();
        document.variables.insert("a".to_string(), f64::NAN);

        let error = serialize_document(&document)
            .expect_err("non-finite variables must be rejected by semantic validation");

        assert!(matches!(
            error,
            DocumentPersistenceError::SemanticValidation(_)
        ));
        assert!(error.to_string().contains("Variable a"));
    }

    #[test]
    fn serialization_rejects_non_finite_direct_geometry_before_json_encoding() {
        let mut document = Document::new();
        let point = document
            .try_add_point(Point2::new(0.0, 0.0))
            .expect("valid point");
        let Some(GeoObject::Point(point)) = document.get_object_mut(point) else {
            panic!("expected point");
        };
        point.position.x = f64::INFINITY;

        let error = serialize_document(&document)
            .expect_err("non-finite geometry must be rejected by semantic validation");

        assert!(matches!(
            error,
            DocumentPersistenceError::SemanticValidation(_)
        ));
        assert!(error.to_string().contains("Point.position.x"));
    }

    #[test]
    fn persistence_rejects_object_map_keys_that_do_not_match_object_ids() {
        let mut document = Document::new();
        document.add_point(Point2::new(1.0, 2.0));
        let mut raw: Value = serde_json::to_value(&document).expect("serialize raw document");
        raw.as_object_mut()
            .expect("document is an object")
            .remove("constraints");
        let objects = raw["objects"]
            .as_object_mut()
            .expect("objects are represented as a map");
        let (key, object) = objects
            .iter()
            .next()
            .map(|(key, object)| (key.clone(), object.clone()))
            .expect("document contains the point");
        objects.remove(&key);
        objects.insert(crate::ObjectId::new().0.to_string(), object);

        let error = deserialize_document(&raw.to_string())
            .expect_err("object map keys must match the embedded object id");

        assert!(
            error.to_string().contains("Object map key"),
            "unexpected validation error: {error}"
        );
    }

    #[test]
    fn serialization_rejects_dangling_complex_integral_targets_and_oversized_expressions() {
        let dangling = unchecked_document_with_object(GeoObject::ComplexIntegral(
            ComplexIntegralObj::new("z", crate::ObjectId::new(), false),
        ));
        let dangling_error = serialize_document(&dangling)
            .expect_err("complex integral targets must exist in the document");
        assert!(dangling_error
            .to_string()
            .contains("ComplexIntegral target"));

        let mut oversized_expression = Document::new();
        let target = oversized_expression
            .try_add_object(GeoObject::ImplicitCurve(ImplicitCurveObj::new(
                "x",
                "0",
                crate::RelationOperator::Eq,
            )))
            .expect("valid contour target");
        let integral = oversized_expression
            .try_add_object(GeoObject::ComplexIntegral(ComplexIntegralObj::new(
                "z", target, false,
            )))
            .expect("valid integral");
        let Some(GeoObject::ComplexIntegral(integral)) =
            oversized_expression.get_object_mut(integral)
        else {
            panic!("expected complex integral");
        };
        integral.expr = "z".repeat(crate::validation::MAX_EXPR_LENGTH + 1);
        let expression_error = serialize_document(&oversized_expression)
            .expect_err("complex integral expressions must respect expression limits");
        assert!(expression_error.to_string().contains("Expression length"));
    }

    #[test]
    fn serialization_rejects_deeply_nested_transformed_objects() {
        let mut object = GeoObject::Point(crate::PointObj::new(Point2::new(0.0, 0.0)));
        for _ in 0..65 {
            object = GeoObject::Transformed(TransformedObj::new(object, "z"));
        }
        let document = unchecked_document_with_object(object);

        let error = serialize_document(&document)
            .expect_err("nested transformed objects must have a bounded validation depth");

        assert!(error.to_string().contains("Transformed object nesting"));
    }

    #[test]
    fn serialization_rejects_invalid_implicit_contour_configuration() {
        let mut document = Document::new();
        let mut curve = ImplicitCurveObj::new("x", "0", crate::RelationOperator::Eq);
        curve.contour_levels = Some(vec![0.0, 1.0]);
        let curve = document
            .try_add_object(GeoObject::ImplicitCurve(curve))
            .expect("valid contour configuration");
        let Some(GeoObject::ImplicitCurve(curve)) = document.get_object_mut(curve) else {
            panic!("expected implicit curve");
        };
        curve.contour_levels = Some(vec![0.0, f64::NAN]);
        let non_finite =
            serialize_document(&document).expect_err("non-finite contour levels must be rejected");
        assert!(non_finite.to_string().contains("contour level"));

        let mut curve = ImplicitCurveObj::new("x", "0", crate::RelationOperator::Eq);
        curve.contour_levels = Some(vec![0.0, 0.0]);
        let duplicate = unchecked_document_with_object(GeoObject::ImplicitCurve(curve));
        let duplicate_error =
            serialize_document(&duplicate).expect_err("duplicate contour levels must be rejected");
        assert!(duplicate_error.to_string().contains("duplicate contour"));

        let mut curve = ImplicitCurveObj::new("x", "0", crate::RelationOperator::Eq);
        curve.contour_levels = Some((0..9).map(f64::from).collect());
        let over_budget = unchecked_document_with_object(GeoObject::ImplicitCurve(curve));
        let budget_error = serialize_document(&over_budget)
            .expect_err("contour work exceeding the render budget must be rejected");
        assert!(budget_error.to_string().contains("work budget"));
    }

    #[test]
    fn serialization_rejects_julia_iterations_above_the_shared_fractal_limit() {
        let mut fractal = Fractal2DObj::julia(-0.7, 0.3);
        fractal.max_iter = crate::validation::MAX_FRACTAL_ITER + 1;
        let document = unchecked_document_with_object(GeoObject::Fractal2D(fractal));

        let error = serialize_document(&document)
            .expect_err("Julia must use the same iteration ceiling as Mandelbrot");

        assert!(error.to_string().contains("Fractal2D max_iter"));
    }

    #[test]
    fn persistence_rejects_polygon_with_too_many_vertices() {
        let vertices = (0..=crate::validation::MAX_POLYGON_VERTICES)
            .map(|x| Point2::new(x as f64, 0.0))
            .collect();
        let document =
            unchecked_document_with_object(GeoObject::Polygon(PolygonObj::new(vertices)));

        let save_error =
            serialize_document(&document).expect_err("over-cap Polygon must not be serialized");
        assert!(save_error.to_string().contains("Polygon vertices"));

        let raw_json = serde_json::to_string(&document)
            .expect("serialize raw document containing an over-cap Polygon");
        let load_error = deserialize_document(&raw_json)
            .expect_err("over-cap persisted Polygon must not be loaded");
        assert!(load_error.to_string().contains("Polygon vertices"));
    }

    #[test]
    fn persistence_accepts_polygon_at_vertex_limit() {
        let vertices = (0..crate::validation::MAX_POLYGON_VERTICES)
            .map(|x| Point2::new(x as f64, 0.0))
            .collect();
        let mut document = Document::new();
        document.add_object(GeoObject::Polygon(PolygonObj::new(vertices)));

        let json =
            serialize_document(&document).expect("Polygon at the vertex limit must serialize");
        deserialize_document(&json).expect("Polygon at the vertex limit must deserialize");
    }

    #[test]
    fn legacy_raw_document_migrates_to_current_schema() {
        let document = sample_document();
        let legacy_json = serde_json::to_string(&document).expect("serialize legacy document");

        let loaded = deserialize_document(&legacy_json).expect("migrate legacy document");
        assert_eq!(loaded.object_count(), document.object_count());

        let current_json = serialize_document(&loaded).expect("serialize migrated document");
        let current: Value = serde_json::from_str(&current_json).expect("parse current envelope");
        assert_eq!(
            current["schema_version"],
            Value::from(CURRENT_DOCUMENT_SCHEMA_VERSION)
        );
    }

    #[test]
    fn schema_v1_migrates_only_axis_swapped_expression_objects() {
        let mut document = Document::new();
        let point = document.add_object(GeoObject::Point3D(Point3DObj::new(Point3D::new(
            1.0, 2.0, 3.0,
        ))));
        let curve = document.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
            "t", "10+t", "20+t", 0.0, 1.0,
        )));
        let parametric_surface = document.add_object(GeoObject::Surface3D(
            Surface3DObj::new_parametric("u", "10+v", "20+v", (0.0, 1.0), (0.0, 1.0)),
        ));
        let explicit_surface = document.add_object(GeoObject::Surface3D(Surface3DObj::new(
            "x + 2*y",
            (1.0, 2.0),
            (3.0, 4.0),
        )));
        let complex_surface = document.add_object(GeoObject::Surface3D(Surface3DObj::new_complex(
            "z",
            (1.0, 2.0),
            (3.0, 4.0),
        )));
        let field = document.add_object(GeoObject::VectorField3D(
            VectorField3DObj::new("y", "z + y", "x - z").with_bounds(
                (1.0, 2.0),
                (3.0, 4.0),
                (5.0, 6.0),
            ),
        ));
        let legacy = serde_json::json!({
            "schema_version": 1,
            "producer_version": "1.2.20-beta",
            "document": document,
        });

        let migrated = deserialize_document(&legacy.to_string()).expect("migrate schema v1");

        assert!(matches!(
            migrated.get_object(point),
            Some(GeoObject::Point3D(point)) if point.position == Point3D::new(1.0, 2.0, 3.0)
        ));
        assert!(matches!(
            migrated.get_object(curve),
            Some(GeoObject::ParametricCurve3D(curve))
                if curve.expr_y == "20+t" && curve.expr_z == "10+t"
        ));
        assert!(matches!(
            migrated.get_object(parametric_surface),
            Some(GeoObject::Surface3D(surface))
                if surface.expr_y == "20+v" && surface.expr_z == "10+v"
        ));
        let Some(GeoObject::Surface3D(surface)) = migrated.get_object(explicit_surface) else {
            panic!("migrated explicit surface");
        };
        let grid = crate::parametric_sampling::evaluate_surface_3d(surface, 1, &migrated.variables);
        assert_eq!(grid[0][0], Point3D::new(1.0, 7.0, 3.0));
        let Some(GeoObject::Surface3D(surface)) = migrated.get_object(complex_surface) else {
            panic!("migrated complex surface");
        };
        let grid = crate::parametric_sampling::evaluate_surface_3d(surface, 1, &migrated.variables);
        assert_eq!(grid[0][0], Point3D::new(1.0, 10.0_f64.sqrt(), 3.0));
        assert!(matches!(
            migrated.get_object(field),
            Some(GeoObject::VectorField3D(field))
                if field.expr_u == "z"
                    && field.expr_v == "x - y"
                    && field.expr_w == "y + z"
                    && (field.y_min, field.y_max) == (5.0, 6.0)
                    && (field.z_min, field.z_max) == (3.0, 4.0)
        ));

        let current_json = serialize_document(&migrated).expect("serialize migrated schema");
        let current: Value = serde_json::from_str(&current_json).expect("parse current envelope");
        assert_eq!(
            current["schema_version"],
            Value::from(CURRENT_DOCUMENT_SCHEMA_VERSION)
        );
        let reloaded = deserialize_document(&current_json).expect("reload migrated schema");
        assert!(matches!(
            reloaded.get_object(point),
            Some(GeoObject::Point3D(point)) if point.position == Point3D::new(1.0, 2.0, 3.0)
        ));
        assert!(matches!(
            reloaded.get_object(curve),
            Some(GeoObject::ParametricCurve3D(curve))
                if curve.expr_y == "20+t" && curve.expr_z == "10+t"
        ));
        assert!(matches!(
            reloaded.get_object(parametric_surface),
            Some(GeoObject::Surface3D(surface))
                if surface.expr_y == "20+v" && surface.expr_z == "10+v"
        ));
        assert!(matches!(
            reloaded.get_object(field),
            Some(GeoObject::VectorField3D(field))
                if field.expr_u == "z"
                    && field.expr_v == "x - y"
                    && field.expr_w == "y + z"
                    && (field.y_min, field.y_max) == (5.0, 6.0)
                    && (field.z_min, field.z_max) == (3.0, 4.0)
        ));
        let Some(GeoObject::Surface3D(surface)) = reloaded.get_object(explicit_surface) else {
            panic!("round-tripped explicit surface");
        };
        let grid = crate::parametric_sampling::evaluate_surface_3d(surface, 1, &reloaded.variables);
        assert_eq!(grid[0][0], Point3D::new(1.0, 7.0, 3.0));
    }

    #[test]
    fn legacy_axis_migration_renames_only_coordinate_identifiers() {
        assert_eq!(
            swap_y_z_variables("2y + lazy + z2 + z"),
            "2z + lazy + z2 + y"
        );
    }

    #[test]
    fn current_schema_keeps_canonical_3d_objects_unchanged() {
        let mut document = Document::new();
        let point = document.add_object(GeoObject::Point3D(Point3DObj::new(Point3D::new(
            1.0, 2.0, 3.0,
        ))));
        let curve = document.add_object(GeoObject::ParametricCurve3D(ParametricCurve3DObj::new(
            "t", "10+t", "20+t", 0.0, 1.0,
        )));
        let parametric_surface = document.add_object(GeoObject::Surface3D(
            Surface3DObj::new_parametric("u", "10+v", "20+v", (0.0, 1.0), (0.0, 1.0)),
        ));
        let explicit_surface = document.add_object(GeoObject::Surface3D(Surface3DObj::new(
            "x + 2*y",
            (1.0, 2.0),
            (3.0, 4.0),
        )));
        let field = document.add_object(GeoObject::VectorField3D(
            VectorField3DObj::new("y", "z + y", "x - z").with_bounds(
                (1.0, 2.0),
                (3.0, 4.0),
                (5.0, 6.0),
            ),
        ));

        let json = serialize_document(&document).expect("serialize current schema");
        let loaded = deserialize_document(&json).expect("load current schema");

        assert!(matches!(
            loaded.get_object(point),
            Some(GeoObject::Point3D(point)) if point.position == Point3D::new(1.0, 2.0, 3.0)
        ));
        assert!(matches!(
            loaded.get_object(curve),
            Some(GeoObject::ParametricCurve3D(curve))
                if curve.expr_y == "10+t" && curve.expr_z == "20+t"
        ));
        assert!(matches!(
            loaded.get_object(parametric_surface),
            Some(GeoObject::Surface3D(surface))
                if surface.expr_y == "10+v" && surface.expr_z == "20+v"
        ));
        let Some(GeoObject::Surface3D(surface)) = loaded.get_object(explicit_surface) else {
            panic!("current explicit surface");
        };
        let grid = crate::parametric_sampling::evaluate_surface_3d(surface, 1, &loaded.variables);
        assert_eq!(grid[0][0], Point3D::new(1.0, 3.0, 7.0));
        assert!(matches!(
            loaded.get_object(field),
            Some(GeoObject::VectorField3D(field))
                if field.expr_u == "y"
                    && field.expr_v == "z + y"
                    && field.expr_w == "x - z"
                    && (field.y_min, field.y_max) == (3.0, 4.0)
                    && (field.z_min, field.z_max) == (5.0, 6.0)
        ));
    }

    #[test]
    fn legacy_document_without_constraints_migrates_its_objects_to_free() {
        let mut document = Document::new();
        let point = document.add_point(Point2::new(0.0, 0.0));
        let mut legacy: Value = serde_json::to_value(&document).expect("serialize document");
        legacy
            .as_object_mut()
            .expect("legacy document object")
            .remove("constraints");

        let loaded = deserialize_document(&legacy.to_string()).expect("migrate legacy document");
        assert!(loaded.constraints.is_free(&point));
    }

    #[test]
    fn future_schema_is_rejected() {
        let document = sample_document();
        let future = serde_json::json!({
            "schema_version": CURRENT_DOCUMENT_SCHEMA_VERSION + 1,
            "producer_version": "future",
            "document": document,
        });

        let error = deserialize_document(&future.to_string()).expect_err("future schema must fail");
        assert!(matches!(
            error,
            DocumentPersistenceError::UnsupportedFutureSchema { .. }
        ));
    }

    #[test]
    fn future_schema_is_rejected_before_envelope_deserialization() {
        let json = format!(
            r#"{{"schema_version":{}}}"#,
            CURRENT_DOCUMENT_SCHEMA_VERSION + 1
        );

        let error = deserialize_document(&json).expect_err("future schema must fail first");
        assert!(matches!(
            error,
            DocumentPersistenceError::UnsupportedFutureSchema { .. }
        ));
    }

    #[test]
    fn oversized_document_file_is_rejected_before_text_decoding() {
        let path = temporary_path("oversized.json");
        let oversized = vec![b'x'; crate::validation::MAX_DOCUMENT_SIZE_BYTES + 1];
        fs::write(&path, oversized).expect("write oversized document");

        let error = read_document_file(&path).expect_err("oversized file must fail");
        assert!(error.to_string().contains("exceeds maximum"));

        fs::remove_file(path).expect("remove test document");
    }

    #[test]
    fn raw_json_with_too_many_structural_elements_is_rejected_before_value_parse() {
        let mut json = String::from("[");
        for _ in 0..=crate::validation::MAX_JSON_STRUCTURAL_ELEMENTS {
            json.push_str("0,");
        }
        json.push_str("0]");

        let error = crate::validation::validate_document_json(&json)
            .expect_err("structural-element budget must reject the document");
        assert!(error.contains("too many structural elements"));
    }

    #[test]
    fn truncated_json_is_rejected() {
        assert!(deserialize_document(r#"{"schema_version":1,"document":"#).is_err());
    }

    #[test]
    fn documents_with_dangling_constraint_references_are_rejected() {
        let mut legacy: Value =
            serde_json::to_value(sample_document()).expect("serialize document");
        let constraints = legacy["constraints"]["constraints"]
            .as_object_mut()
            .expect("constraints map");
        let constraint = constraints.values_mut().next().expect("sample constraint");
        constraint["inputs"] = serde_json::json!([crate::ObjectId::new()]);

        let error =
            deserialize_document(&legacy.to_string()).expect_err("dangling input must fail");
        assert!(error.to_string().contains("missing input object"));
    }

    #[test]
    fn documents_with_malformed_transform_constraints_are_rejected() {
        let mut document = Document::new();
        let source = document.add_point(Point2::new(1.0, 0.0));
        let output = document.add_point(Point2::new(0.0, 1.0));
        document.constraints.add_constraint(
            "Rotate",
            vec![source],
            vec![output],
            std::collections::HashMap::new(),
        );

        let save_error = serialize_document(&document)
            .expect_err("a Rotate constraint without angle must not serialize");
        assert!(save_error.to_string().contains("angle"), "{save_error}");

        let raw = serde_json::to_string(&document).expect("serialize malformed document");
        let load_error = deserialize_document(&raw)
            .expect_err("a persisted Rotate constraint without angle must not load");
        assert!(load_error.to_string().contains("angle"), "{load_error}");
    }

    #[test]
    fn perpendicular_constraints_reject_overflowing_source_directions() {
        let mut document = Document::new();
        let source = document.add_object(crate::GeoObject::Line(crate::LineObj::new(
            Point2::new(-1.0e308, 0.0),
            Point2::new(1.0e308, 0.0),
        )));
        let point = document.add_point(Point2::new(0.0, 0.0));
        let output = document.add_object(crate::GeoObject::Line(crate::LineObj::new_with_kind(
            Point2::new(0.0, -1.0),
            Point2::new(0.0, 1.0),
            crate::LineKind::Line,
        )));
        document.constraints.add_constraint(
            "Perpendicular",
            vec![source, point],
            vec![output],
            std::collections::HashMap::new(),
        );

        let error = serialize_document(&document)
            .expect_err("an overflowing source direction must not serialize");
        assert!(error.to_string().contains("degenerada"), "{error}");
    }

    #[test]
    fn constraint_indexes_are_rebuilt_after_loading() {
        let mut document = Document::new();
        let input = document.add_point(Point2::new(0.0, 0.0));
        let second_input = document.add_point(Point2::new(2.0, 0.0));
        let output = document.add_point(Point2::new(1.0, 0.0));
        document.constraints.add_constraint(
            "Midpoint",
            vec![input, second_input],
            vec![output],
            std::collections::HashMap::new(),
        );
        let mut legacy: Value = serde_json::to_value(&document).expect("serialize document");
        let constraints = legacy["constraints"]
            .as_object_mut()
            .expect("constraints graph");
        constraints["creator"] = serde_json::json!({
            crate::ObjectId::new().to_string(): 9999
        });
        constraints["dependents"] = serde_json::json!({
            crate::ObjectId::new().to_string(): [9999]
        });

        let loaded = deserialize_document(&legacy.to_string()).expect("load document");
        let constraint = loaded.constraints.iter().next().expect("constraint");
        assert_eq!(
            loaded.constraints.dependents_of(&input),
            Some(&vec![constraint.id])
        );
        assert_eq!(
            loaded
                .constraints
                .creator_of(&output)
                .map(|creator| creator.id),
            Some(constraint.id)
        );
    }

    #[test]
    fn documents_with_duplicate_constraint_creators_are_rejected() {
        let mut document = Document::new();
        let input = document.add_point(Point2::new(0.0, 0.0));
        let output = document.add_point(Point2::new(1.0, 0.0));
        document.constraints.add_constraint(
            "First",
            vec![input],
            vec![output],
            std::collections::HashMap::new(),
        );
        document.constraints.add_constraint(
            "Second",
            vec![input],
            vec![output],
            std::collections::HashMap::new(),
        );

        let save_error = serialize_document(&document)
            .expect_err("duplicate output creators must not be serialized");
        assert!(save_error.to_string().contains("multiple constraints"));

        let legacy = serde_json::to_string(&document).expect("serialize malformed document");
        let error =
            deserialize_document(&legacy).expect_err("duplicate output creators must be rejected");
        assert!(error.to_string().contains("multiple constraints"));
    }

    #[test]
    fn documents_with_constraint_cycles_are_rejected() {
        let mut document = Document::new();
        let input = document.add_point(Point2::new(0.0, 0.0));
        let first_output = document.add_point(Point2::new(1.0, 0.0));
        let second_output = document.add_point(Point2::new(2.0, 0.0));
        document.constraints.add_constraint(
            "First",
            vec![input, second_output],
            vec![first_output],
            std::collections::HashMap::new(),
        );
        document.constraints.add_constraint(
            "Second",
            vec![first_output],
            vec![second_output],
            std::collections::HashMap::new(),
        );

        let save_error =
            serialize_document(&document).expect_err("constraint cycle must not be serialized");
        assert!(save_error.to_string().contains("cycle"));

        let legacy = serde_json::to_string(&document).expect("serialize document");
        let error = deserialize_document(&legacy).expect_err("constraint cycle must be rejected");
        assert!(error.to_string().contains("cycle"));
    }

    #[test]
    fn documents_with_an_incomplete_free_object_partition_are_rejected() {
        let document = sample_document();
        let mut legacy: Value = serde_json::to_value(&document).expect("serialize document");
        legacy["constraints"]["free_objects"] = serde_json::json!([]);

        let error = deserialize_document(&legacy.to_string())
            .expect_err("every unconstrained object must be free");
        assert!(error.to_string().contains("free-object partition"));
    }

    #[test]
    fn serialization_rejects_an_overlong_function_label() {
        let mut document = Document::new();
        let function =
            document.add_object(crate::GeoObject::Function(crate::FunctionObj::new("x")));
        document
            .get_object_mut(function)
            .expect("function exists")
            .set_label("x".repeat(crate::validation::MAX_STRING_LENGTH + 1));

        let error = serialize_document(&document).expect_err("overlong label must not serialize");
        assert!(matches!(
            error,
            DocumentPersistenceError::SemanticValidation(_)
        ));
        assert!(error.to_string().contains("Object label length"));
    }

    #[test]
    fn serialization_rejects_documents_larger_than_the_read_limit() {
        let mut document = Document::new();
        let prefix = "x".repeat(crate::validation::MAX_STRING_LENGTH - 10);
        for index in 0..1_000 {
            document.add_object(crate::GeoObject::Point(
                crate::PointObj::new(Point2::new(index as f64, 0.0))
                    .with_label(format!("{prefix}{index:010}")),
            ));
        }

        let error = serialize_document(&document).expect_err("oversized JSON must not serialize");
        assert!(matches!(
            error,
            DocumentPersistenceError::SemanticValidation(_)
        ));
        assert!(error.to_string().contains("Document size"));
    }

    #[test]
    fn semantically_invalid_document_is_not_serialized_or_written() {
        let mut document = Document::new();
        document.constraints.add_constraint(
            "Invalid",
            vec![crate::ObjectId::new()],
            Vec::new(),
            std::collections::HashMap::new(),
        );
        let path = temporary_path("invalid.json");

        assert!(matches!(
            serialize_document(&document),
            Err(DocumentPersistenceError::SemanticValidation(_))
        ));
        assert!(matches!(
            write_document_atomic(&document, &path),
            Err(DocumentPersistenceError::SemanticValidation(_))
        ));
        assert!(!path.exists());
    }

    #[test]
    fn atomic_write_replaces_destination_only_after_a_complete_write() {
        let path = temporary_path("complete.json");
        fs::write(&path, "old document").expect("seed old document");

        write_document_atomic(&sample_document(), &path).expect("atomic write");

        let saved = fs::read_to_string(&path).expect("read saved document");
        assert_ne!(saved, "old document");
        assert!(deserialize_document(&saved).is_ok());
        fs::remove_file(path).expect("remove test document");
    }

    #[test]
    fn atomic_write_creates_a_missing_destination() {
        let path = temporary_path("new.json");
        assert!(!path.exists());

        write_document_atomic(&sample_document(), &path).expect("atomic write");

        let saved = fs::read_to_string(&path).expect("read saved document");
        assert!(deserialize_document(&saved).is_ok());
        fs::remove_file(path).expect("remove test document");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_destination_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temporary_path("permissions.json");
        fs::write(&path, "old document").expect("seed destination");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))
            .expect("restrict destination permissions");

        write_document_atomic(&sample_document(), &path).expect("atomic write");

        let mode = fs::metadata(&path)
            .expect("destination metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
        fs::remove_file(path).expect("remove test document");
    }

    #[test]
    fn atomic_write_failure_preserves_existing_destination() {
        let path = temporary_path("destination");
        fs::create_dir(&path).expect("create destination directory");
        let marker = path.join("old-document");
        fs::write(&marker, "old document").expect("seed destination marker");

        assert!(write_document_atomic(&sample_document(), &path).is_err());
        assert_eq!(
            fs::read_to_string(&marker).expect("read marker"),
            "old document"
        );

        fs::remove_file(marker).expect("remove marker");
        fs::remove_dir(path).expect("remove destination directory");
    }

    #[cfg(unix)]
    #[test]
    fn document_reader_rejects_symbolic_link_sources() {
        use std::os::unix::fs::symlink;

        let target = temporary_path("target.json");
        let link = temporary_path("link.json");
        write_document_atomic(&sample_document(), &target).expect("write target document");
        symlink(&target, &link).expect("create symbolic link");

        assert!(read_document_file(&link).is_err());

        fs::remove_file(link).expect("remove link");
        fs::remove_file(target).expect("remove target document");
    }
}
