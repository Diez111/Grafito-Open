//! Sesión de aula: statem `Idle → Lobby → Live → Closed` + roster + manos.
//!
//! Cerebro puro: sin I/O, sin spawn, sin egui, sin red. Todo el transporte
//! real vive en [`crate::transport`]; esta sesión es el estado autoritativo
//! que la Piel renderiza (`fn render(&Estado)`).
//!
//! PII siempre local: `roster` guarda solo nombres display ya saneados
//! (1..=64 chars) y nunca sale del proceso en este frente (solo Loopback).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Tope de miembros (coherente con `MAX_DASHBOARD_NAMES = 5_000`).
pub const MAX_ROSTER_SIZE: usize = 5_000;
/// Longitud máxima del código de sala (igual que `ShareCode` en app).
pub const MAX_CLASSROOM_CODE_LEN: usize = 32;
/// Longitud máxima por nombre display (igual que dashboard).
pub const MAX_LEARNER_NAME_LEN: usize = 64;
/// Tope del ejercicio activo (igual que `RequestBudget` texto acotado).
pub const MAX_EXERCISE_CHARS: usize = 2_000;
/// TTL default de un código de sala en segundos (1h, GeoGebra Classroom expira por sesión).
pub const DEFAULT_CODE_TTL_SECS: u64 = 3_600;
/// TTL máximo de un código (24h, evita salas eternas con PII local).
pub const MAX_CODE_TTL_SECS: u64 = 86_400;
/// TTL mínimo (60s, evita expiración inmediata por error de tipeo).
pub const MIN_CODE_TTL_SECS: u64 = 60;
/// Tope del CSV del roster (512 KiB: 5000 filas × ~100 bytes, fail-closed por truncado).
pub const MAX_ROSTER_CSV_BYTES: usize = 512 * 1_024;

/// Errores tipados de la sesión (sin pánicos, todo `Result`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassroomError {
    /// Código de sala inválido (vacío, largo o con caracteres no permitidos).
    InvalidCode(String),
    /// Nombre de alumno inválido (vacío tras trim).
    InvalidName(String),
    /// Transición de fase ilegal (statem).
    InvalidTransition {
        /// Fase actual.
        from: ClassroomPhase,
        /// Acción intentada (`open_lobby`, `start_live`, `close`, `join`, …).
        action: &'static str,
    },
    /// Roster lleno (`MAX_ROSTER_SIZE`).
    RosterFull,
    /// Alumno desconocido para `leave` / manos.
    UnknownLearner(String),
    /// Cola del transporte llena (Loopback acotado, fail-closed).
    QueueFull,
    /// Mensaje excede `MAX_MESSAGE_BYTES` o es inválido.
    InvalidMessage(String),
    /// Código expirado (`now >= expires`): hay que regenerar en lobby.
    CodeExpired,
    /// TTL inválido para expiración (`MIN..=MAX_CODE_TTL_SECS`).
    InvalidTtl(String),
    /// Almacén acotado lleno (CRDT/offline: qué se llenó).
    StorageFull {
        /// Qué se llenó (`Crdt`, `OfflineOutbox`, …).
        what: &'static str,
    },
    /// L no implementado: diseño + motivo honesto.
    NotImplemented {
        /// Nombre estable del feature (`IrohP2P`, `Crdt`, …).
        feature: &'static str,
        /// Qué faltaría para implementarlo + alternativa actual.
        hint: String,
    },
}

impl std::fmt::Display for ClassroomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCode(detail) => write!(f, "código de aula inválido: {detail}"),
            Self::InvalidName(detail) => write!(f, "nombre de alumno inválido: {detail}"),
            Self::InvalidTransition { from, action } => {
                write!(f, "transición ilegal: {from:?} no admite '{action}'")
            }
            Self::RosterFull => write!(f, "roster lleno (máximo {MAX_ROSTER_SIZE})"),
            Self::UnknownLearner(name) => write!(f, "alumno desconocido: {name}"),
            Self::QueueFull => write!(f, "cola de aula llena (máximo 128 mensajes)"),
            Self::InvalidMessage(detail) => write!(f, "mensaje de aula inválido: {detail}"),
            Self::CodeExpired => {
                write!(f, "código de sala expirado: regenerá el código en el lobby")
            }
            Self::InvalidTtl(detail) => write!(f, "TTL de código inválido: {detail}"),
            Self::StorageFull { what } => write!(f, "{what} lleno (almacén acotado, fail-closed)"),
            Self::NotImplemented { feature, hint } => {
                write!(f, "{feature} no implementado: {hint}")
            }
        }
    }
}

impl std::error::Error for ClassroomError {}

/// Código de sala: newtype validado `1..=32`, ASCII alfanumérico + `-`/`_`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassroomCode(String);

impl ClassroomCode {
    /// Valida y construye. `Err(InvalidCode)` si vacío, largo o con caracteres fuera de `[A-Za-z0-9_-]`.
    pub fn try_new(code: &str) -> Result<Self, ClassroomError> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Err(ClassroomError::InvalidCode("vacío".to_string()));
        }
        if trimmed.len() > MAX_CLASSROOM_CODE_LEN {
            return Err(ClassroomError::InvalidCode(format!(
                "excede {MAX_CLASSROOM_CODE_LEN} bytes"
            )));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ClassroomError::InvalidCode(
                "solo ASCII alfanumérico + '-'/'_'".to_string(),
            ));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Vista del código (ya validado).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Nombre display de alumno: newtype saneado `1..=64` chars tras trim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnerName(String);

impl LearnerName {
    /// Sanea (trim + cap 64 chars). `Err(InvalidName)` si queda vacío.
    pub fn try_new(raw: &str) -> Result<Self, ClassroomError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ClassroomError::InvalidName("vacío".to_string()));
        }
        let capped: String = trimmed.chars().take(MAX_LEARNER_NAME_LEN).collect();
        if capped.trim().is_empty() {
            return Err(ClassroomError::InvalidName("vacío".to_string()));
        }
        Ok(Self(capped))
    }

    /// Vista del nombre (ya saneado).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// TTL de un código de sala: newtype `MIN..=MAX_CODE_TTL_SECS` segundos.
///
/// Evita salas eternas (PII local igual se acota en tiempo, como GeoGebra
/// Classroom que expira por sesión). `60s..=24h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeTtlSecs(u64);

impl CodeTtlSecs {
    /// Valida `MIN_CODE_TTL_SECS..=MAX_CODE_TTL_SECS`. `Err(InvalidTtl)` si fuera de rango.
    pub fn try_new(secs: u64) -> Result<Self, ClassroomError> {
        if (MIN_CODE_TTL_SECS..=MAX_CODE_TTL_SECS).contains(&secs) {
            Ok(Self(secs))
        } else {
            Err(ClassroomError::InvalidTtl(format!(
                "{secs}s fuera de {MIN_CODE_TTL_SECS}..={MAX_CODE_TTL_SECS}s"
            )))
        }
    }

    /// TTL default (1h).
    #[must_use]
    pub fn default_ttl() -> Self {
        Self(DEFAULT_CODE_TTL_SECS)
    }

    /// Segundos validados.
    #[must_use]
    pub fn as_secs(&self) -> u64 {
        self.0
    }
}

/// Fase de la sesión (`/statem`: toda transición pasa por un método que valida `phase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassroomPhase {
    /// Sesión creada, sin lobby.
    Idle,
    /// Lobby abierto: se puede unir, aún no hay clase en vivo.
    Lobby,
    /// Clase en vivo: roster + manos + ejercicio activos.
    Live,
    /// Cerrada: terminal hasta reapertura explícita.
    Closed,
}

/// Miembro del roster: nombre + mano + epoch de unión (para orden estable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterMember {
    /// Nombre display saneado.
    pub name: String,
    /// ¿Mano levantada?
    pub hand_raised: bool,
    /// Epoch de unión (solo orden; no es reloj de pared).
    pub joined_epoch: u64,
}

/// Sesión autoritativa del aula (Cerebro puro).
///
/// Transiciones válidas (el resto retorna `Err(InvalidTransition)`):
/// - `Idle → Lobby` (`open_lobby`), `Closed → Lobby` (reapertura limpia).
/// - `Lobby → Live` (`start_live`).
/// - `Lobby → Closed` / `Live → Closed` (`close`).
/// - `join/leave/manos/ejercicio` solo en `Lobby` o `Live`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassroomSession {
    phase: ClassroomPhase,
    code: Option<ClassroomCode>,
    roster: BTreeMap<String, RosterMember>,
    exercise: Option<String>,
    snapshot_digest: String,
    /// Expiración del código (`epoch` secs, reloj de pared del caller).
    /// `None` = sin expiración (compat con sesiones viejas, `#[serde(default)]`).
    #[serde(default)]
    code_expires_epoch: Option<u64>,
}

impl ClassroomSession {
    /// Sesión nueva en `Idle`, roster vacío.
    #[must_use]
    pub fn new_idle() -> Self {
        Self {
            phase: ClassroomPhase::Idle,
            code: None,
            roster: BTreeMap::new(),
            exercise: None,
            snapshot_digest: String::new(),
            code_expires_epoch: None,
        }
    }

    /// Fase actual (la Piel solo renderiza esto).
    #[must_use]
    pub fn phase(&self) -> ClassroomPhase {
        self.phase
    }

    /// Código de sala si hay lobby/live.
    #[must_use]
    pub fn code(&self) -> Option<&ClassroomCode> {
        self.code.as_ref()
    }

    /// ¿Acepta uniones? Solo `Lobby` o `Live`.
    #[must_use]
    pub fn accepts_joins(&self) -> bool {
        matches!(self.phase, ClassroomPhase::Lobby | ClassroomPhase::Live)
    }

    /// Abre el lobby (`Idle → Lobby`, o `Closed → Lobby` con roster limpio).
    ///
    /// Sin expiración (`code_expires_epoch = None`, compat histórica).
    /// Para expiración usá [`Self::open_lobby_with_expiry`].
    pub fn open_lobby(&mut self, code: ClassroomCode) -> Result<(), ClassroomError> {
        match self.phase {
            ClassroomPhase::Idle => {
                self.code = Some(code);
                self.code_expires_epoch = None;
                self.phase = ClassroomPhase::Lobby;
                Ok(())
            }
            ClassroomPhase::Closed => {
                self.roster.clear();
                self.exercise = None;
                self.snapshot_digest.clear();
                self.code = Some(code);
                self.code_expires_epoch = None;
                self.phase = ClassroomPhase::Lobby;
                Ok(())
            }
            _ => Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "open_lobby",
            }),
        }
    }

    /// Abre el lobby con expiración (`Idle|Closed → Lobby`).
    ///
    /// `expires = now_epoch + ttl`. `now_epoch` es reloj de pared del caller
    /// (segundos); `ttl` validado `60s..=24h`. Puro, sin I/O.
    pub fn open_lobby_with_expiry(
        &mut self,
        code: ClassroomCode,
        now_epoch: u64,
        ttl: CodeTtlSecs,
    ) -> Result<(), ClassroomError> {
        self.open_lobby(code)?;
        self.code_expires_epoch = Some(now_epoch.saturating_add(ttl.as_secs()));
        Ok(())
    }

    /// Expiración del código (`None` = sin expiración, compat).
    #[must_use]
    pub fn expiry_epoch(&self) -> Option<u64> {
        self.code_expires_epoch
    }

    /// ¿El código expiró en `now`? `false` si no hay expiración o no hay lobby/live.
    ///
    /// `now >= expires` ⇒ expirado (igual que `is_due` del scheduler).
    #[must_use]
    pub fn is_code_expired(&self, now: u64) -> bool {
        match self.code_expires_epoch {
            None => false,
            Some(expires) => now >= expires,
        }
    }

    /// ¿Acepta uniones en `now`? Fase `Lobby|Live` Y código no expirado.
    ///
    /// La Piel debe usar esta (no [`Self::accepts_joins`]) cuando el lobby
    /// se abrió con [`Self::open_lobby_with_expiry`].
    #[must_use]
    pub fn accepts_joins_at(&self, now: u64) -> bool {
        self.accepts_joins() && !self.is_code_expired(now)
    }

    /// Renueva la expiración (`Lobby|Live` + no expirado aún o expirado: lo extiende igual).
    ///
    /// `Err(InvalidTransition)` si no hay lobby/live. Útil para el docente
    /// que extiende la clase sin regenerar código.
    pub fn renew_expiry(&mut self, now_epoch: u64, ttl: CodeTtlSecs) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "renew_expiry",
            });
        }
        if self.code.is_none() {
            return Err(ClassroomError::InvalidCode(
                "sin código que renovar".to_string(),
            ));
        }
        self.code_expires_epoch = Some(now_epoch.saturating_add(ttl.as_secs()));
        Ok(())
    }

    /// Inicia la clase (`Lobby → Live`).
    pub fn start_live(&mut self) -> Result<(), ClassroomError> {
        match self.phase {
            ClassroomPhase::Lobby => {
                self.phase = ClassroomPhase::Live;
                Ok(())
            }
            _ => Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "start_live",
            }),
        }
    }

    /// Cierra (`Lobby|Live → Closed`). Limpia ejercicio pero conserva roster
    /// para el dashboard final (la Piel lo lee antes de reabrir).
    pub fn close(&mut self) -> Result<(), ClassroomError> {
        match self.phase {
            ClassroomPhase::Lobby | ClassroomPhase::Live => {
                self.phase = ClassroomPhase::Closed;
                self.exercise = None;
                Ok(())
            }
            _ => Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "close",
            }),
        }
    }

    /// Une un alumno (idempotente: si ya existe, `Ok` sin duplicar).
    /// Solo en `Lobby`/`Live`; `Err(RosterFull)` si se supera el tope.
    ///
    /// `epoch` es a la vez `joined_epoch` y reloj para expiración: si el lobby
    /// se abrió con [`Self::open_lobby_with_expiry`] y `epoch >= expires`,
    /// retorna `Err(CodeExpired)` (el docente debe renovar o regenerar).
    /// Sesiones sin expiración (`open_lobby`) nunca expiran: compat total.
    pub fn join(&mut self, name: &str, epoch: u64) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "join",
            });
        }
        if self.is_code_expired(epoch) {
            return Err(ClassroomError::CodeExpired);
        }
        let clean = LearnerName::try_new(name)?;
        let key = clean.as_str().to_string();
        if self.roster.contains_key(&key) {
            return Ok(());
        }
        if self.roster.len() >= MAX_ROSTER_SIZE {
            return Err(ClassroomError::RosterFull);
        }
        self.roster.insert(
            key.clone(),
            RosterMember {
                name: key,
                hand_raised: false,
                joined_epoch: epoch,
            },
        );
        Ok(())
    }

    /// Sale un alumno. `Err(UnknownLearner)` si no estaba.
    pub fn leave(&mut self, name: &str) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "leave",
            });
        }
        let clean = LearnerName::try_new(name)?;
        if self.roster.remove(clean.as_str()).is_none() {
            return Err(ClassroomError::UnknownLearner(clean.as_str().to_string()));
        }
        Ok(())
    }

    /// Levanta la mano. `Err(UnknownLearner)` si no está en roster.
    pub fn raise_hand(&mut self, name: &str) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "raise_hand",
            });
        }
        let clean = LearnerName::try_new(name)?;
        match self.roster.get_mut(clean.as_str()) {
            Some(member) => {
                member.hand_raised = true;
                Ok(())
            }
            None => Err(ClassroomError::UnknownLearner(clean.as_str().to_string())),
        }
    }

    /// Baja la mano. `Err(UnknownLearner)` si no está en roster.
    pub fn lower_hand(&mut self, name: &str) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "lower_hand",
            });
        }
        let clean = LearnerName::try_new(name)?;
        match self.roster.get_mut(clean.as_str()) {
            Some(member) => {
                member.hand_raised = false;
                Ok(())
            }
            None => Err(ClassroomError::UnknownLearner(clean.as_str().to_string())),
        }
    }

    /// Presentes (tamaño del roster).
    #[must_use]
    pub fn present(&self) -> usize {
        self.roster.len()
    }

    /// Manos levantadas.
    #[must_use]
    pub fn hands(&self) -> usize {
        self.roster.values().filter(|m| m.hand_raised).count()
    }

    /// Nombres ordenados (BTreeMap ya ordena por clave: determinista).
    #[must_use]
    pub fn names_sorted(&self) -> Vec<String> {
        self.roster.keys().cloned().collect()
    }

    /// Ejercicio activo (si hay).
    #[must_use]
    pub fn exercise(&self) -> Option<&str> {
        self.exercise.as_deref()
    }

    /// Define/limpia el ejercicio (solo `Lobby`/`Live`; cap 2000 chars).
    pub fn set_exercise(&mut self, exercise: Option<&str>) -> Result<(), ClassroomError> {
        if !self.accepts_joins() {
            return Err(ClassroomError::InvalidTransition {
                from: self.phase,
                action: "set_exercise",
            });
        }
        match exercise {
            None => {
                self.exercise = None;
                Ok(())
            }
            Some(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    self.exercise = None;
                    return Ok(());
                }
                let capped: String = trimmed.chars().take(MAX_EXERCISE_CHARS).collect();
                self.exercise = Some(capped);
                Ok(())
            }
        }
    }

    /// Digest del snapshot (trunca a 256 chars, nunca falla).
    pub fn set_snapshot_digest(&mut self, digest: &str) {
        let trimmed = digest.trim();
        if trimmed.is_empty() {
            self.snapshot_digest.clear();
        } else {
            self.snapshot_digest = trimmed
                .chars()
                .take(crate::MAX_SNAPSHOT_DIGEST_LEN)
                .collect();
        }
    }

    /// Digest actual.
    #[must_use]
    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }

    /// Exporta el roster a CSV RFC 4180 mínimo (`name,hand_raised,joined_epoch` + CRLF).
    ///
    /// Puro, determinista (BTreeMap ordena por nombre), sin I/O ni PII extra:
    /// solo lo ya guardado en `roster`. Campos con `, " \n \r` se entrecomillan
    /// y `"` se duplica. `hand_raised` como `true/false`. Acotado a
    /// `MAX_ROSTER_CSV_BYTES` por truncado de filas (fail-closed, nunca OOM:
    /// header siempre presente aunque el roster esté lleno).
    #[must_use]
    pub fn export_roster_csv(&self) -> String {
        let mut out = String::from("name,hand_raised,joined_epoch\r\n");
        for member in self.roster.values() {
            let row = format!(
                "{},{},{}\r\n",
                escape_csv_field(&member.name),
                member.hand_raised,
                member.joined_epoch
            );
            if out.len().saturating_add(row.len()) > MAX_ROSTER_CSV_BYTES {
                break;
            }
            out.push_str(&row);
        }
        out
    }

    /// Dashboard de asistencia + métricas desde [`crate::LearnerSnapshot`].
    #[must_use]
    pub fn to_dashboard(&self, profiles: &[crate::LearnerSnapshot]) -> crate::TeacherDashboard {
        let code = self
            .code
            .as_ref()
            .map_or_else(|| "GRAF-0000".to_string(), |c| c.as_str().to_string());
        crate::TeacherDashboard::from_live_with_profiles(
            code,
            self.present(),
            self.hands(),
            self.names_sorted(),
            self.exercise.clone(),
            self.snapshot_digest.clone(),
            profiles,
        )
    }

    /// Dashboard real desde `StudentProfile` (sin que el caller mapee a mano).
    ///
    /// Mapea cada perfil a [`crate::LearnerSnapshot`] con
    /// [`crate::LearnerSnapshot::from_student_profile`] (`now` para
    /// `branches_due`) y delega en [`Self::to_dashboard`]. Puro y local.
    #[must_use]
    pub fn to_dashboard_with_student_profiles(
        &self,
        profiles: &[grafito_profile::StudentProfile],
        now: u64,
    ) -> crate::TeacherDashboard {
        let snapshots: Vec<crate::LearnerSnapshot> = profiles
            .iter()
            .map(|p| crate::LearnerSnapshot::from_student_profile(p, now))
            .collect();
        self.to_dashboard(&snapshots)
    }
}

/// Escapa un campo CSV (RFC 4180 mínimo): entrecomilla si contiene `, " \n \r` y duplica `"`.
fn escape_csv_field(raw: &str) -> String {
    if raw.contains([',', '"', '\n', '\r']) {
        let mut quoted = String::with_capacity(raw.len().saturating_add(2));
        quoted.push('"');
        for ch in raw.chars() {
            if ch == '"' {
                quoted.push('"');
            }
            quoted.push(ch);
        }
        quoted.push('"');
        quoted
    } else {
        raw.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_fixture() -> ClassroomCode {
        ClassroomCode::try_new("AULA-123").expect("código fixture")
    }

    #[test]
    fn code_newtype_rejects_empty_long_and_dirty() {
        assert!(ClassroomCode::try_new("").is_err());
        assert!(ClassroomCode::try_new("   ").is_err());
        assert!(ClassroomCode::try_new("bad code!").is_err());
        assert!(ClassroomCode::try_new(&"a".repeat(33)).is_err());
        assert!(ClassroomCode::try_new("AULA-1_ok").is_ok());
    }

    #[test]
    fn learner_name_trims_and_caps() {
        assert!(LearnerName::try_new("   ").is_err());
        let name = LearnerName::try_new("  Ana  ").expect("nombre fixture");
        assert_eq!(name.as_str(), "Ana");
        let long = "x".repeat(100);
        let capped = LearnerName::try_new(&long).expect("cap");
        assert_eq!(capped.as_str().len(), MAX_LEARNER_NAME_LEN);
    }

    #[test]
    fn statem_idle_lobby_live_closed_happy_path() {
        let mut session = ClassroomSession::new_idle();
        assert_eq!(session.phase(), ClassroomPhase::Idle);
        assert!(!session.accepts_joins());
        assert!(session.join("Ana", 1).is_err());

        session.open_lobby(code_fixture()).expect("lobby");
        assert_eq!(session.phase(), ClassroomPhase::Lobby);
        assert!(session.accepts_joins());
        assert!(session.open_lobby(code_fixture()).is_err());

        session.join("Ana", 10).expect("join");
        session.join("Luis", 11).expect("join");
        assert_eq!(session.present(), 2);
        session.raise_hand("Ana").expect("mano");
        assert_eq!(session.hands(), 1);
        session.lower_hand("Ana").expect("bajar");
        assert_eq!(session.hands(), 0);

        session.start_live().expect("live");
        assert_eq!(session.phase(), ClassroomPhase::Live);
        assert!(session.start_live().is_err());
        session.join("Mia", 12).expect("join en live");
        assert_eq!(session.present(), 3);

        session.close().expect("close");
        assert_eq!(session.phase(), ClassroomPhase::Closed);
        assert!(!session.accepts_joins());
        assert!(session.close().is_err());
        assert!(session.join("Otro", 13).is_err());
    }

    #[test]
    fn statem_closed_reopens_clean() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Ana", 1).expect("join");
        session.set_exercise(Some("x+2=5")).expect("ejercicio");
        session.set_snapshot_digest("abc");
        session.start_live().expect("live");
        session.close().expect("close");
        assert_eq!(session.present(), 1);

        let other = ClassroomCode::try_new("AULA-999").expect("otro código");
        session.open_lobby(other).expect("reapertura");
        assert_eq!(session.phase(), ClassroomPhase::Lobby);
        assert_eq!(session.present(), 0);
        assert_eq!(session.exercise(), None);
        assert_eq!(session.snapshot_digest(), "");
        assert_eq!(session.code().map(|c| c.as_str()), Some("AULA-999"));
    }

    #[test]
    fn roster_join_is_idempotent_and_leave_requires_member() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Ana", 1).expect("join");
        session.join("Ana", 2).expect("idempotente");
        assert_eq!(session.present(), 1);
        assert!(session.leave("Desconocido").is_err());
        assert!(session.raise_hand("Desconocido").is_err());
        session.leave("Ana").expect("leave");
        assert_eq!(session.present(), 0);
    }

    #[test]
    fn exercise_only_in_lobby_or_live_and_caps() {
        let mut session = ClassroomSession::new_idle();
        assert!(session.set_exercise(Some("x")).is_err());
        session.open_lobby(code_fixture()).expect("lobby");
        session
            .set_exercise(Some("  Resolver x+2=5  "))
            .expect("set");
        assert_eq!(session.exercise(), Some("Resolver x+2=5"));
        session.set_exercise(Some("   ")).expect("limpia");
        assert_eq!(session.exercise(), None);
        let long = "e".repeat(5_000);
        session.set_exercise(Some(&long)).expect("cap");
        assert_eq!(session.exercise().map(str::len), Some(MAX_EXERCISE_CHARS));
    }

    #[test]
    fn names_sorted_is_deterministic() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Zoe", 3).expect("join");
        session.join("Ana", 1).expect("join");
        session.join("Luis", 2).expect("join");
        assert_eq!(session.names_sorted(), vec!["Ana", "Luis", "Zoe"]);
    }

    #[test]
    fn dashboard_from_session_uses_roster_counts() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Ana", 1).expect("join");
        session.join("Luis", 2).expect("join");
        session.raise_hand("Ana").expect("mano");
        session.set_snapshot_digest("dig123");
        let dashboard = session.to_dashboard(&[]);
        assert_eq!(dashboard.code, "AULA-123");
        assert_eq!(dashboard.present, 2);
        assert_eq!(dashboard.hands, 1);
        assert_eq!(dashboard.names, vec!["Ana", "Luis"]);
        assert_eq!(dashboard.snapshot_digest, "dig123");
    }

    #[test]
    fn dashboard_with_student_profiles_maps_real_data() {
        let mut ana = grafito_profile::StudentProfile::new("Ana");
        ana.record_outcome("algebra", "Álgebra", 100, true);
        ana.record_outcome("algebra", "Álgebra", 200, false);
        let luis = grafito_profile::StudentProfile::new("Luis");

        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Ana", 1).expect("join");
        session.join("Luis", 2).expect("join");

        let dashboard = session.to_dashboard_with_student_profiles(&[ana, luis], 10_000_000);
        assert_eq!(dashboard.present, 2);
        assert_eq!(dashboard.bkt_summary.len(), 2);
        assert_eq!(dashboard.bkt_summary[0].0, "Ana");
        assert_eq!(dashboard.bkt_summary[1].0, "Luis");
        assert!(dashboard.avg_mastery > 0.0);
    }

    #[test]
    fn code_ttl_validates_range_and_default() {
        assert!(CodeTtlSecs::try_new(59).is_err());
        assert!(CodeTtlSecs::try_new(60).is_ok());
        assert!(CodeTtlSecs::try_new(3_600).is_ok());
        assert!(CodeTtlSecs::try_new(86_400).is_ok());
        assert!(CodeTtlSecs::try_new(86_401).is_err());
        assert_eq!(CodeTtlSecs::default_ttl().as_secs(), DEFAULT_CODE_TTL_SECS);
        assert!(matches!(
            CodeTtlSecs::try_new(0).expect_err("ttl 0"),
            ClassroomError::InvalidTtl(_)
        ));
    }

    #[test]
    fn lobby_with_expiry_accepts_before_and_rejects_after() {
        let mut session = ClassroomSession::new_idle();
        let ttl = CodeTtlSecs::try_new(3_600).expect("ttl");
        session
            .open_lobby_with_expiry(code_fixture(), 1_000, ttl)
            .expect("lobby con ttl");
        assert_eq!(session.expiry_epoch(), Some(4_600));
        assert!(!session.is_code_expired(1_000));
        assert!(!session.is_code_expired(4_599));
        assert!(session.is_code_expired(4_600));
        assert!(session.accepts_joins_at(4_599));
        assert!(!session.accepts_joins_at(4_600));
        session.join("Ana", 2_000).expect("join antes de expirar");
        let err = session.join("Luis", 5_000).expect_err("expirado");
        assert_eq!(err, ClassroomError::CodeExpired);
        assert_eq!(
            err.to_string(),
            "código de sala expirado: regenerá el código en el lobby"
        );
    }

    #[test]
    fn lobby_without_expiry_never_expires_and_reopen_clears() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        assert_eq!(session.expiry_epoch(), None);
        assert!(!session.is_code_expired(u64::MAX));
        assert!(session.accepts_joins_at(u64::MAX));
        session
            .join("Ana", u64::MAX)
            .expect("sin expiración siempre une");

        let mut expiring = ClassroomSession::new_idle();
        let ttl = CodeTtlSecs::try_new(60).expect("ttl");
        expiring
            .open_lobby_with_expiry(code_fixture(), 0, ttl)
            .expect("lobby");
        assert!(expiring.is_code_expired(60));
        expiring.start_live().expect("live");
        expiring.close().expect("close");
        expiring.open_lobby(code_fixture()).expect("reapertura");
        assert_eq!(expiring.expiry_epoch(), None);
        assert!(!expiring.is_code_expired(1_000_000));
    }

    #[test]
    fn renew_expiry_extends_and_requires_lobby() {
        let mut session = ClassroomSession::new_idle();
        assert!(session.renew_expiry(0, CodeTtlSecs::default_ttl()).is_err());
        let ttl = CodeTtlSecs::try_new(60).expect("ttl");
        session
            .open_lobby_with_expiry(code_fixture(), 0, ttl)
            .expect("lobby");
        session
            .renew_expiry(1_000, CodeTtlSecs::try_new(3_600).expect("ttl"))
            .expect("renueva");
        assert_eq!(session.expiry_epoch(), Some(4_600));
        assert!(!session.is_code_expired(4_599));
        session.join("Ana", 4_000).expect("join tras renovar");
    }

    #[test]
    fn export_roster_csv_header_only_when_empty() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        assert_eq!(
            session.export_roster_csv(),
            "name,hand_raised,joined_epoch\r\n"
        );
    }

    #[test]
    fn export_roster_csv_sorted_and_honest() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Zoe", 3).expect("join");
        session.join("Ana", 1).expect("join");
        session.raise_hand("Ana").expect("mano");
        let csv = session.export_roster_csv();
        assert_eq!(
            csv,
            "name,hand_raised,joined_epoch\r\nAna,true,1\r\nZoe,false,3\r\n"
        );
        assert!(csv.len() <= MAX_ROSTER_CSV_BYTES);
    }

    #[test]
    fn export_roster_csv_quotes_commas_and_quotes() {
        let mut session = ClassroomSession::new_idle();
        session.open_lobby(code_fixture()).expect("lobby");
        session.join("Doe, John", 7).expect("join con coma");
        session
            .join("Ana \"La\" Profe", 8)
            .expect("join con comillas");
        let csv = session.export_roster_csv();
        assert!(csv.contains("\"Doe, John\",false,7\r\n"), "{csv}");
        assert!(
            csv.contains("\"Ana \"\"La\"\" Profe\",false,8\r\n"),
            "{csv}"
        );
    }
}
