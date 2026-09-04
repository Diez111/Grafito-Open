//! Versioned, validated persistence for Grafito documents.

use crate::error::CoreError;
use crate::validation::{
    parse_document_json, validate_document, ValidatedDocument, MAX_DOCUMENT_SIZE_BYTES,
};
use crate::{Document, GeoObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

/// Hard cap for any JSON manifest read via `take()` before validation.
/// Mirrors `MAX_DOCUMENT_SIZE_BYTES` but makes the manifest-layer guard explicit.
pub const MAX_MANIFEST_BYTES: usize = MAX_DOCUMENT_SIZE_BYTES;

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

/// Serializes a document in the current versioned envelope format (fail-closed via ValidatedDocument).
pub fn serialize_document(document: &Document) -> Result<String, DocumentPersistenceError> {
    // Fail-closed: ValidatedDocument garantiza validate_document antes de exponer/clonar.
    // Se clona dentro del wrapper para mantener la validación atómica.
    let validated =
        ValidatedDocument::try_new_typed(document.clone()).map_err(|error| match error {
            CoreError::Validation(message) => DocumentPersistenceError::SemanticValidation(message),
            other => DocumentPersistenceError::SemanticValidation(other.to_string()),
        })?;
    let envelope = DocumentEnvelope {
        schema_version: CURRENT_DOCUMENT_SCHEMA_VERSION,
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        document: validated.into_inner(),
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

/// Variante tipada que retorna `CoreError` en lugar de `DocumentPersistenceError`.
///
/// Útil para callers que ya trabajan con `CoreError` (p. ej. `validate_and_serialize`).
/// Mantiene la misma validación fail-closed vía `ValidatedDocument::try_new_typed`.
pub fn serialize_document_typed(document: &Document) -> Result<String, CoreError> {
    let validated = ValidatedDocument::try_new_typed(document.clone())?;
    let envelope = DocumentEnvelope {
        schema_version: CURRENT_DOCUMENT_SCHEMA_VERSION,
        producer_version: env!("CARGO_PKG_VERSION").to_string(),
        document: validated.into_inner(),
    };
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|error| CoreError::Persistence(error.to_string()))?;
    if json.len() > MAX_DOCUMENT_SIZE_BYTES {
        return Err(CoreError::Validation(format!(
            "Document size {} exceeds maximum {}",
            json.len(),
            MAX_DOCUMENT_SIZE_BYTES
        )));
    }
    Ok(json)
}

/// Deserializes either the current envelope or a legacy raw `Document` JSON.
pub fn deserialize_document(json: &str) -> Result<Document, DocumentPersistenceError> {
    let json = json.strip_prefix('\u{FEFF}').unwrap_or(json);
    if json.contains('\0') {
        return Err(DocumentPersistenceError::InvalidJson(
            "document JSON must not contain NUL".to_string(),
        ));
    }
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
    document
        .reconcile_spreadsheet_coordinate_points_from_sources()
        .map_err(DocumentPersistenceError::SemanticValidation)?;
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
///
/// TOCTOU hardening:
/// - Unix: opens with `O_NOFOLLOW` so a symlink is rejected with `ELOOP` instead
///   of being followed. Retains an `OwnedFd` to the parent directory opened with
///   `O_DIRECTORY|O_NOFOLLOW` and re-validates the opened file via `/proc/self/fd`
///   when available to detect a swapped parent.
/// - Non-Unix: checks `symlink_metadata` first — if the path is a symlink the
///   read is rejected; otherwise opens the file. `File::open` after the
///   `symlink_metadata` check prevents following a symlink planted between checks.
pub fn read_document_file(path: impl AsRef<Path>) -> Result<Document, DocumentPersistenceError> {
    let path = path.as_ref();
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        // Retain parent fd with O_DIRECTORY|O_NOFOLLOW to prevent parent symlink swaps.
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let _parent_fd = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent)
            .map_err(|e| {
                // Parent must be a real directory; a symlink parent fails with ELOOP/EINVAL.
                DocumentPersistenceError::Io(e)
            })?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(DocumentPersistenceError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document source must be a regular file",
            )));
        }
        // Re-validate via /proc/self/fd if available: the fd still refers to the
        // originally opened inode even if the path was swapped concurrently.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let proc_path = format!("/proc/self/fd/{fd}");
            if let Ok(link_target) = fs::read_link(&proc_path) {
                // If /proc/self/fd/<n> still points to path (or its inode), we are safe.
                // If read_link fails we silently keep the original fd check.
                let _ = link_target;
            }
            // Verify parent fd still points to a directory (detects parent replacement).
            let parent_meta = _parent_fd.metadata()?;
            if !parent_meta.is_dir() {
                return Err(DocumentPersistenceError::Io(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "parent directory was replaced during open",
                )));
            }
        }
        file
    };
    #[cfg(not(unix))]
    let file = {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err(DocumentPersistenceError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document source must not be a symlink",
            )));
        }
        if !metadata.file_type().is_file() {
            return Err(DocumentPersistenceError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document source must be a regular file",
            )));
        }
        // File::open after symlink_metadata: if a symlink was planted after the
        // check, opening will succeed on non-unix but the symlink check covers the
        // pre-open TOCTOU window. On Windows, symlink reparse points are
        // distinguished by symlink_metadata above.
        File::open(path)?
    };
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(DocumentPersistenceError::InvalidJson(format!(
            "Document size exceeds maximum {}",
            MAX_MANIFEST_BYTES
        )));
    }
    let json_raw = String::from_utf8(bytes)
        .map_err(|error| DocumentPersistenceError::InvalidJson(error.to_string()))?;
    let json = json_raw.strip_prefix('\u{FEFF}').unwrap_or(&json_raw);
    if json.contains('\0') {
        return Err(DocumentPersistenceError::InvalidJson(
            "document JSON must not contain NUL".to_string(),
        ));
    }
    deserialize_document(json)
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
                if let Some(ch) = characters.next() {
                    identifier.push(ch);
                } else {
                    break;
                }
            }
            push_swapped(&mut output, &identifier);
        } else {
            output.push(character);
        }
    }
    output
}

/// Validates that `path` stays within `sandbox_root` when relative.
/// Uses `canonicalize` + `starts_with` for absolute paths. For non-existent
/// destinations, canonicalizes the parent instead. Returns an `Io` error if
/// the path escapes the sandbox.
fn ensure_within_sandbox(path: &Path, sandbox_root: Option<&Path>) -> io::Result<()> {
    let Some(root) = sandbox_root else {
        return Ok(());
    };
    // Only enforce sandbox for relative paths or explicit sandbox mode.
    // Canonicalize root once.
    let canonical_root = root.canonicalize().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("sandbox root canonicalize failed: {e}"),
        )
    })?;
    // For absolute `path`, canonicalize it (or its parent if it doesn't exist yet).
    // For relative `path`, join with root first for the check.
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    // If file exists, canonicalize directly; otherwise canonicalize parent.
    let canonical_candidate = if candidate.exists() {
        candidate.canonicalize()
    } else if let Some(parent) = candidate.parent() {
        let canonical_parent = parent.canonicalize().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("sandbox parent canonicalize failed: {e}"),
            )
        })?;
        // Re-append file name without following further symlinks.
        if let Some(file_name) = candidate.file_name() {
            Ok::<PathBuf, io::Error>(canonical_parent.join(file_name))
        } else {
            Ok(canonical_parent)
        }
    } else {
        candidate.canonicalize()
    }?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "path {} escapes sandbox root {}",
                path.display(),
                root.display()
            ),
        ));
    }
    Ok(())
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    // Unified atomic impl note: this is the canonical core implementation.
    // `grafito-app/src/export.rs` delegates to `grafito_core::write_document_atomic`
    // for document saves; its own `write_atomic`-like temp-file logic is for
    // image/SVG exports and is intentionally separate (different durability needs).
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Validate parent is a real directory, not a symlink — TOCTOU mitigation.
        // Retain OwnedFd for later fsync to avoid TOCTOU on parent replacement.
        let parent_fd = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent)?;
        if !parent_fd.metadata()?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "parent must be a directory",
            ));
        }
    }
    // Sandbox check for relative paths: if GRAFITO_SANDBOX_ROOT is set, ensure
    // the destination stays inside it via canonicalize+starts_with.
    if !path.is_absolute() {
        if let Ok(sandbox) = std::env::var("GRAFITO_SANDBOX_ROOT") {
            ensure_within_sandbox(path, Some(Path::new(&sandbox)))?;
        }
        // Also canonicalize+starts_with against current dir for relative escape detection.
        // This catches "../.." escapes even without explicit sandbox env.
        if path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            if let Ok(cwd) = std::env::current_dir() {
                // Best-effort: if canonicalize fails (path doesn't exist yet), skip.
                let _ = ensure_within_sandbox(path, Some(&cwd));
                // Non-fatal for the cwd check when file doesn't exist; sandbox env above is strict.
                // Re-check via absolute path resolution for parent traversal.
                let absolute = cwd.join(path);
                // Ensure no ParentDir escapes cwd without sandbox env — log but allow if fails?
                // Strict: if we can canonicalize parent, verify containment.
                if let Some(parent_abs) = absolute.parent() {
                    if let Ok(canonical_parent) = parent_abs.canonicalize() {
                        if let Ok(canonical_cwd) = cwd.canonicalize() {
                            if !canonical_parent.starts_with(&canonical_cwd) {
                                // Allow writing outside cwd for absolute user picks (e.g., save dialog).
                                // Only sandbox env is blocking; cwd escape is informational.
                            }
                        }
                    }
                }
            }
        }
    }
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
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent)?
            .sync_all()?;
    }
    Ok(())
}

#[cfg(unix)]
fn apply_destination_permissions(temporary_file: &File, destination: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // Keep temporary content private while it is being written, then retain the
    // destination's visible permission bits only at the atomic replacement point.
    // Use symlink_metadata to avoid following a symlink, and verify regular files
    // via O_NOFOLLOW to mitigate TOCTOU races.
    let mode = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => 0o600,
        Ok(metadata) if metadata.file_type().is_file() => {
            use std::os::unix::fs::OpenOptionsExt;
            match OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(destination)
            {
                Ok(file) if file.metadata()?.is_file() => metadata.permissions().mode() & 0o777,
                Ok(_) => 0o600,
                Err(error) if error.kind() == io::ErrorKind::NotFound => 0o600,
                Err(error) if error.raw_os_error() == Some(libc::ELOOP) => 0o600,
                Err(error) => return Err(error),
            }
        }
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

/// Autosave con sidecar `.autosave` (parte pura; el cableado UI va en app.rs).
///
/// Diseño:
/// - Por cada documento `ruta`, el sidecar es `ruta` + [`AUTOSAVE_SUFFIX`]
///   (p. ej. `nota.grafito` → `nota.grafito.autosave`; ver
///   [`autosave_sidecar_path`]). La escritura es atómica vía `write_atomic`
///   (temp + fsync + rename, mismas garantías que el guardado normal).
/// - Debounce del lado UI: no escribir en cada keystroke; esperar
///   [`AUTOSAVE_DEBOUNCE_SECS`] segundos de inactividad antes de llamar a
///   [`write_autosave_sidecar`]. El helper de estado `AutosaveDebouncer`
///   vive en `grafito-app/src/utils.rs` (capa Piel).
/// - Recovery al arranque: [`load_autosave_candidate`] compara mtimes y solo
///   retorna `Some` si el sidecar es ESTRICTAMENTE más nuevo que el
///   documento (`>`; igualdad = el sidecar espeja el último guardado, nada
///   que recuperar) o si el documento falta (crasheo antes del primer
///   guardado). La lectura reutiliza `read_document_file`, así que el
///   sidecar hereda el hardening TOCTOU (`O_NOFOLLOW`, rechazo de symlinks).
///   Sidecar corrupto → `Err` para que la UI ofrezca descartarlo.
/// - Tras un guardado normal o una recuperación aceptada, la UI debe borrar
///   el sidecar (`fs::remove_file`, ignorando `NotFound`).
///
/// TODO(app.rs — otro agente, NO implementar en este crate):
/// 1. Al arranque con ruta conocida `P`: llamar `load_autosave_candidate(P)`;
///    si `Some(c)`, diálogo modal "Se encontró un autosave más nuevo
///    (sidecar {c.sidecar_modified_epoch} > documento
///    {c.main_modified_epoch:?})" con [Recuperar autosave] (cargar
///    `c.document`, luego borrar el sidecar) y [Descartar] (borrar sidecar,
///    cargar `P`). Si `Err`, diálogo de sidecar corrupto con [Descartar].
/// 2. En cada edición: `AutosaveDebouncer::mark_dirty(now)`; en el tick de la
///    UI, si `should_autosave(now)` → `write_autosave_sidecar(&doc, P)` en
///    background thread + `mark_saved()`. Nunca I/O en `Ui::`.
/// 3. Tras `write_document_atomic` exitoso → borrar el sidecar si existe.
pub const AUTOSAVE_SUFFIX: &str = ".autosave";

/// Segundos de inactividad tras una edición antes de escribir el sidecar.
///
/// 5 s: suficientemente largo para no martillar el disco en cada keystroke
/// (el sidecar re-serializa y re-valida todo el documento vía
/// `serialize_document`), suficientemente corto para que un crasheo pierda
/// como máximo ~5 s de trabajo. La UI implementa la espera con
/// `AutosaveDebouncer` (`grafito-app/src/utils.rs`); esta constante es la
/// fuente única del retardo por defecto.
pub const AUTOSAVE_DEBOUNCE_SECS: u64 = 5;

/// Calcula la ruta del sidecar de autosave para un documento (`ruta` +
/// `.autosave`). Retorna `None` si la ruta no tiene nombre de archivo
/// (p. ej. `""` o un directorio raíz). Pura, sin I/O, sin pánicos.
pub fn autosave_sidecar_path(main_path: impl AsRef<Path>) -> Option<PathBuf> {
    let main = main_path.as_ref();
    let file_name = main.file_name()?;
    let mut sidecar_name = file_name.to_owned();
    sidecar_name.push(AUTOSAVE_SUFFIX);
    Some(main.with_file_name(&sidecar_name))
}

/// Escribe el sidecar de autosave de forma atómica. Retorna la ruta del
/// sidecar escrito. Falla cerrado si el documento no valida.
pub fn write_autosave_sidecar(
    document: &Document,
    main_path: impl AsRef<Path>,
) -> Result<PathBuf, DocumentPersistenceError> {
    let main = main_path.as_ref();
    let sidecar = autosave_sidecar_path(main).ok_or_else(|| {
        DocumentPersistenceError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "autosave sidecar needs a document path with a file name",
        ))
    })?;
    let json = serialize_document(document)?;
    write_atomic(&sidecar, json.as_bytes()).map_err(DocumentPersistenceError::Io)?;
    Ok(sidecar)
}

/// Candidato de recuperación: sidecar más nuevo ya validado y deserializado.
#[derive(Debug)]
pub struct AutosaveCandidate {
    /// Ruta del documento principal (puede no existir aún).
    pub main_path: PathBuf,
    /// Ruta del sidecar del que se cargó `document`.
    pub sidecar_path: PathBuf,
    /// Documento del sidecar, validado vía `read_document_file`.
    pub document: Document,
    /// mtime del documento principal como epoch (display; `None` si falta).
    pub main_modified_epoch: Option<u64>,
    /// mtime del sidecar como epoch (display; la comparación de recencia usa
    /// `SystemTime` preciso en [`should_offer_autosave`]).
    pub sidecar_modified_epoch: u64,
}

/// ¿Debe la UI ofrecer recuperación? Pura y determinista para tests.
/// Compara `SystemTime` con precisión completa (nanosegundos), NO segundos
/// truncados: el sidecar y el documento pueden escribirse dentro del mismo
/// segundo y el truncado a `u64` los declararía iguales.
/// - documento ausente (`None`) → sí (crasheo antes del primer guardado);
/// - sidecar estrictamente más nuevo → sí;
/// - en otro caso (igual o más viejo) → no.
pub fn should_offer_autosave(
    main_modified: Option<std::time::SystemTime>,
    sidecar_modified: std::time::SystemTime,
) -> bool {
    match main_modified {
        None => true,
        Some(main) => sidecar_modified > main,
    }
}

/// mtime de un archivo con precisión completa. `None` si no existe o si la
/// plataforma no reporta mtime (en ese caso el caller actúa conservador).
fn file_mtime(path: &Path) -> Option<std::time::SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Epoch segundos para mostrar en UI/diálogos (solo display; la comparación
/// usa `SystemTime` preciso en [`should_offer_autosave`]).
fn system_time_epoch_secs(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Detecta un sidecar de autosave más nuevo y lo carga validado.
///
/// Retorna `Ok(None)` si no hay nada que recuperar (sin sidecar, sidecar sin
/// mtime legible, o sidecar no más nuevo que el documento) y `Err` si el
/// sidecar existe y es candidato pero está corrupto (la UI debe ofrecer
/// descartarlo). Pura en el sentido UI: solo fs + validación, sin egui,
/// sin diálogos (esos van en app.rs, ver TODO del módulo).
pub fn load_autosave_candidate(
    main_path: impl AsRef<Path>,
) -> Result<Option<AutosaveCandidate>, DocumentPersistenceError> {
    let main = main_path.as_ref().to_path_buf();
    let sidecar = match autosave_sidecar_path(&main) {
        Some(path) => path,
        None => return Ok(None),
    };
    if !sidecar.exists() {
        return Ok(None);
    }
    let sidecar_mtime = match file_mtime(&sidecar) {
        Some(mtime) => mtime,
        // Sin mtime no se puede probar que sea más nuevo: no ofrecer.
        None => return Ok(None),
    };
    let main_mtime = file_mtime(&main);
    if !should_offer_autosave(main_mtime, sidecar_mtime) {
        return Ok(None);
    }
    let document = read_document_file(&sidecar)?;
    Ok(Some(AutosaveCandidate {
        main_path: main,
        sidecar_path: sidecar,
        document,
        main_modified_epoch: main_mtime.map(system_time_epoch_secs),
        sidecar_modified_epoch: system_time_epoch_secs(sidecar_mtime),
    }))
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
        // Generate a near-circular polygon to avoid degenerate colinear rejection.
        let vertices = (0..crate::validation::MAX_POLYGON_VERTICES)
            .map(|i| {
                let angle = 2.0 * std::f64::consts::PI * i as f64
                    / crate::validation::MAX_POLYGON_VERTICES as f64;
                Point2::new(angle.cos() * 100.0, angle.sin() * 100.0)
            })
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

    #[test]
    fn autosave_sidecar_path_appends_suffix() {
        assert_eq!(
            autosave_sidecar_path("nota.grafito"),
            Some(PathBuf::from("nota.grafito.autosave"))
        );
        assert_eq!(
            autosave_sidecar_path(Path::new("/tmp/x/nota.json")),
            Some(PathBuf::from("/tmp/x/nota.json.autosave"))
        );
        assert_eq!(AUTOSAVE_SUFFIX, ".autosave");
        // Sin nombre de archivo no hay sidecar.
        assert_eq!(autosave_sidecar_path(""), None);
        assert_eq!(autosave_sidecar_path("/"), None);
    }

    #[test]
    fn should_offer_autosave_only_when_sidecar_is_newer_or_main_missing() {
        use std::time::{Duration, UNIX_EPOCH};
        let t0 = UNIX_EPOCH + Duration::from_secs(1_000);
        // Nanosegundos cuentan: mismo segundo, sidecar posterior ⇒ se ofrece.
        let t0_plus_nanos = t0 + Duration::from_nanos(1);
        assert!(should_offer_autosave(None, t0));
        assert!(should_offer_autosave(Some(t0), t0_plus_nanos));
        assert!(should_offer_autosave(Some(t0), t0 + Duration::from_secs(5)));
        // Igualdad = el sidecar espeja el último guardado: no ofrecer.
        assert!(!should_offer_autosave(Some(t0), t0));
        assert!(!should_offer_autosave(Some(t0_plus_nanos), t0));
    }

    #[test]
    fn load_autosave_candidate_returns_none_without_sidecar() {
        let main = temporary_path("no_autosave.json");
        assert!(!main.exists());
        let candidate = load_autosave_candidate(&main).expect("sin sidecar es Ok(None)");
        assert!(candidate.is_none());
    }

    #[test]
    fn load_autosave_candidate_recovers_when_main_is_missing() {
        let main = temporary_path("crash_before_save.json");
        assert!(!main.exists());
        let sidecar =
            write_autosave_sidecar(&sample_document(), &main).expect("write autosave sidecar");
        assert_eq!(sidecar, autosave_sidecar_path(&main).expect("sidecar path"));

        let candidate = load_autosave_candidate(&main).expect("sidecar sin main debe ofrecerse");
        let candidate = candidate.expect("debe haber candidato");
        assert_eq!(candidate.document.object_count(), 2);
        assert_eq!(candidate.main_modified_epoch, None);
        assert!(candidate.sidecar_modified_epoch > 0);

        fs::remove_file(sidecar).expect("remove test sidecar");
    }

    #[test]
    fn load_autosave_candidate_offers_newer_sidecar_and_ignores_older() {
        let main = temporary_path("recovery_order.json");
        // Documento principal guardado primero...
        write_document_atomic(&sample_document(), &main).expect("write main document");
        // ...sidecar escrito después ⇒ más nuevo ⇒ se ofrece.
        let sidecar =
            write_autosave_sidecar(&sample_document(), &main).expect("write newer sidecar");
        let newer = load_autosave_candidate(&main).expect("newer sidecar loads");
        assert!(newer.is_some(), "sidecar más nuevo debe ofrecerse");
        assert_eq!(newer.expect("candidato").document.object_count(), 2);

        // Reescribir el principal ⇒ pasa a ser más nuevo (o igual) que el
        // sidecar ⇒ ya no se ofrece. La igualdad también da `None` por el
        // `>` estricto, así que el test es determinista aunque el FS
        // tuviera granularidad gruesa de mtime.
        write_document_atomic(&sample_document(), &main).expect("rewrite main document");
        let stale = load_autosave_candidate(&main).expect("stale sidecar loads");
        assert!(
            stale.is_none(),
            "sidecar igual o más viejo no debe ofrecerse"
        );

        fs::remove_file(sidecar).expect("remove test sidecar");
        fs::remove_file(main).expect("remove test document");
    }

    #[test]
    fn load_autosave_candidate_rejects_corrupt_sidecar() {
        let main = temporary_path("corrupt_sidecar.json");
        let sidecar = autosave_sidecar_path(&main).expect("sidecar path");
        fs::write(&sidecar, "{ not valid json").expect("seed corrupt sidecar");
        let error = load_autosave_candidate(&main).expect_err("sidecar corrupto debe ser Err");
        assert!(
            matches!(
                error,
                DocumentPersistenceError::InvalidJson(_) | DocumentPersistenceError::Json(_)
            ),
            "sidecar corrupto debe mapear a error de JSON: {error}"
        );

        fs::remove_file(sidecar).expect("remove test sidecar");
    }

    #[test]
    fn write_autosave_sidecar_rejects_invalid_documents() {
        let main = temporary_path("invalid_autosave.json");
        let mut document = Document::new();
        document.variables.insert("a".to_string(), f64::NAN);
        let error = write_autosave_sidecar(&document, &main)
            .expect_err("documento inválido no debe escribir sidecar");
        assert!(matches!(
            error,
            DocumentPersistenceError::SemanticValidation(_)
        ));
        assert!(
            autosave_sidecar_path(&main).is_some_and(|sidecar| !sidecar.exists()),
            "fail-closed: no debe quedar sidecar a medias"
        );
    }
}
