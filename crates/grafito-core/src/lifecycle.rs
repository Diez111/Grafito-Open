//! Document lifecycle statem — fail-closed transitions.

use std::path::PathBuf;

/// Ciclo de vida tipado del documento persistido.
/// Solo `Ready` permite mutaciones o persistencia.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DocumentLifecycle {
    #[default]
    Empty,
    Loading {
        path: PathBuf,
    },
    Validating {
        bytes: usize,
    },
    Ready {
        baseline: String,
    },
    Mutating,
    Persisting {
        tmp: PathBuf,
    },
    Failed {
        reason: String,
    },
}

impl DocumentLifecycle {
    pub fn state_name(&self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Loading { .. } => "Loading",
            Self::Validating { .. } => "Validating",
            Self::Ready { .. } => "Ready",
            Self::Mutating => "Mutating",
            Self::Persisting { .. } => "Persisting",
            Self::Failed { .. } => "Failed",
        }
    }

    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    pub fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::Failed { reason } => Some(reason),
            _ => None,
        }
    }

    pub fn baseline_path(&self) -> Option<&str> {
        match self {
            Self::Ready { baseline } => Some(baseline),
            _ => None,
        }
    }

    pub fn begin_load(&mut self, path: PathBuf) -> Result<(), String> {
        match self {
            Self::Empty | Self::Ready { .. } | Self::Failed { .. } => {
                *self = Self::Loading { path };
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Loading",
                self.state_name()
            )),
        }
    }

    pub fn begin_validating(&mut self, bytes: usize) -> Result<(), String> {
        match self {
            Self::Loading { .. } => {
                *self = Self::Validating { bytes };
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Validating",
                self.state_name()
            )),
        }
    }

    pub fn mark_ready(&mut self, baseline: String) -> Result<(), String> {
        match self {
            Self::Validating { .. } => {
                *self = Self::Ready { baseline };
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Ready",
                self.state_name()
            )),
        }
    }

    pub fn begin_mutating(&mut self) -> Result<(), String> {
        match self {
            Self::Ready { .. } => {
                *self = Self::Mutating;
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Mutating",
                self.state_name()
            )),
        }
    }

    pub fn begin_persisting(&mut self, tmp: PathBuf) -> Result<(), String> {
        match self {
            Self::Mutating | Self::Ready { .. } => {
                *self = Self::Persisting { tmp };
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Persisting",
                self.state_name()
            )),
        }
    }

    pub fn persist_succeeded(&mut self, baseline: String) -> Result<(), String> {
        match self {
            Self::Persisting { .. } => {
                *self = Self::Ready { baseline };
                Ok(())
            }
            _ => Err(format!(
                "transición inválida {} -> Ready",
                self.state_name()
            )),
        }
    }

    pub fn fail(&mut self, reason: String) {
        *self = Self::Failed { reason };
    }

    pub fn reset(&mut self) {
        *self = Self::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path() {
        let mut s = DocumentLifecycle::default();
        assert_eq!(s.state_name(), "Empty");
        s.begin_load(PathBuf::from("/tmp/a.grafito")).unwrap();
        s.begin_validating(100).unwrap();
        s.mark_ready("baseline".into()).unwrap();
        assert!(s.can_submit());
        s.begin_mutating().unwrap();
        s.begin_persisting(PathBuf::from("/tmp/b.tmp")).unwrap();
        s.persist_succeeded("new".into()).unwrap();
        assert_eq!(s.state_name(), "Ready");
    }

    #[test]
    fn invalid_transitions() {
        let mut s = DocumentLifecycle::Empty;
        assert!(s.begin_validating(1).is_err());
        assert!(s.mark_ready("x".into()).is_err());
    }

    #[test]
    fn can_submit_only_ready() {
        assert!(!DocumentLifecycle::Empty.can_submit());
        assert!(DocumentLifecycle::Ready {
            baseline: "x".into()
        }
        .can_submit());
        assert!(!DocumentLifecycle::Mutating.can_submit());
    }

    fn state_empty() -> DocumentLifecycle {
        DocumentLifecycle::Empty
    }
    fn state_loading() -> DocumentLifecycle {
        DocumentLifecycle::Loading {
            path: PathBuf::from("/tmp/a"),
        }
    }
    fn state_validating() -> DocumentLifecycle {
        DocumentLifecycle::Validating { bytes: 10 }
    }
    fn state_ready() -> DocumentLifecycle {
        DocumentLifecycle::Ready {
            baseline: "base".into(),
        }
    }
    fn state_mutating() -> DocumentLifecycle {
        DocumentLifecycle::Mutating
    }
    fn state_persisting() -> DocumentLifecycle {
        DocumentLifecycle::Persisting {
            tmp: PathBuf::from("/tmp/b.tmp"),
        }
    }
    fn state_failed() -> DocumentLifecycle {
        DocumentLifecycle::Failed {
            reason: "err".into(),
        }
    }

    #[test]
    fn begin_load_valid_only_from_empty_ready_failed() {
        // Válidos
        let mut s = state_empty();
        assert!(s.begin_load(PathBuf::from("/x")).is_ok());
        assert_eq!(s.state_name(), "Loading");
        let mut s = state_ready();
        assert!(s.begin_load(PathBuf::from("/x")).is_ok());
        let mut s = state_failed();
        assert!(s.begin_load(PathBuf::from("/x")).is_ok());

        // Inválidos uno por uno
        let mut s = state_loading();
        let err = s.begin_load(PathBuf::from("/x")).unwrap_err();
        assert!(err.contains("Loading -> Loading"), "got {err}");
        let mut s = state_validating();
        let err = s.begin_load(PathBuf::from("/x")).unwrap_err();
        assert!(err.contains("Validating -> Loading"), "got {err}");
        let mut s = state_mutating();
        let err = s.begin_load(PathBuf::from("/x")).unwrap_err();
        assert!(err.contains("Mutating -> Loading"), "got {err}");
        let mut s = state_persisting();
        let err = s.begin_load(PathBuf::from("/x")).unwrap_err();
        assert!(err.contains("Persisting -> Loading"), "got {err}");
    }

    #[test]
    fn begin_validating_valid_only_from_loading() {
        let mut s = state_loading();
        assert!(s.begin_validating(1).is_ok());
        assert_eq!(s.state_name(), "Validating");

        for mut s in [
            state_empty(),
            state_validating(),
            state_ready(),
            state_mutating(),
            state_persisting(),
            state_failed(),
        ] {
            let from = s.state_name().to_string();
            let err = s.begin_validating(1).unwrap_err();
            assert!(
                err.contains(&format!("{from} -> Validating")),
                "expected {from} -> Validating invalid, got {err}"
            );
        }
    }

    #[test]
    fn mark_ready_valid_only_from_validating() {
        let mut s = state_validating();
        assert!(s.mark_ready("b".into()).is_ok());
        assert_eq!(s.state_name(), "Ready");

        for mut s in [
            state_empty(),
            state_loading(),
            state_ready(),
            state_mutating(),
            state_persisting(),
            state_failed(),
        ] {
            let from = s.state_name().to_string();
            let err = s.mark_ready("b".into()).unwrap_err();
            assert!(
                err.contains(&format!("{from} -> Ready")),
                "expected {from} -> Ready invalid, got {err}"
            );
        }
    }

    #[test]
    fn begin_mutating_valid_only_from_ready() {
        let mut s = state_ready();
        assert!(s.begin_mutating().is_ok());
        assert_eq!(s.state_name(), "Mutating");

        for mut s in [
            state_empty(),
            state_loading(),
            state_validating(),
            state_mutating(),
            state_persisting(),
            state_failed(),
        ] {
            let from = s.state_name().to_string();
            let err = s.begin_mutating().unwrap_err();
            assert!(
                err.contains(&format!("{from} -> Mutating")),
                "expected {from} -> Mutating invalid, got {err}"
            );
        }
    }

    #[test]
    fn begin_persisting_valid_only_from_ready_or_mutating() {
        let mut s = state_ready();
        assert!(s.begin_persisting(PathBuf::from("/t")).is_ok());
        let mut s = state_mutating();
        assert!(s.begin_persisting(PathBuf::from("/t")).is_ok());

        for mut s in [
            state_empty(),
            state_loading(),
            state_validating(),
            state_persisting(),
            state_failed(),
        ] {
            let from = s.state_name().to_string();
            let err = s.begin_persisting(PathBuf::from("/t")).unwrap_err();
            assert!(
                err.contains(&format!("{from} -> Persisting")),
                "expected {from} -> Persisting invalid, got {err}"
            );
        }
    }

    #[test]
    fn persist_succeeded_valid_only_from_persisting() {
        let mut s = state_persisting();
        assert!(s.persist_succeeded("b".into()).is_ok());
        assert_eq!(s.state_name(), "Ready");

        for mut s in [
            state_empty(),
            state_loading(),
            state_validating(),
            state_ready(),
            state_mutating(),
            state_failed(),
        ] {
            let from = s.state_name().to_string();
            // persist_succeeded reports as "-> Ready"
            let err = s.persist_succeeded("b".into()).unwrap_err();
            assert!(
                err.contains(&format!("{from} -> Ready")),
                "expected {from} -> Ready invalid, got {err}"
            );
        }
    }

    #[test]
    fn fail_and_reset_are_always_valid() {
        for mut s in [
            state_empty(),
            state_loading(),
            state_validating(),
            state_ready(),
            state_mutating(),
            state_persisting(),
            state_failed(),
        ] {
            s.fail("boom".into());
            assert_eq!(s.state_name(), "Failed");
            assert!(s.is_failed());
            assert_eq!(s.failure_reason(), Some("boom"));
            s.reset();
            assert_eq!(s.state_name(), "Empty");
            assert!(!s.is_failed());
        }
    }

    #[test]
    fn exhaustive_transition_coverage() {
        // Lista explícita de transiciones válidas para asegurar 100% cobertura.
        // Cada método se prueba en los 7 estados, total 7*6 =42 intentos.
        let states = [
            state_empty(),
            state_loading(),
            state_validating(),
            state_ready(),
            state_mutating(),
            state_persisting(),
            state_failed(),
        ];
        let valid_begin_load = ["Empty", "Ready", "Failed"];
        let valid_begin_validating = ["Loading"];
        let valid_mark_ready = ["Validating"];
        let valid_begin_mutating = ["Ready"];
        let valid_begin_persisting = ["Ready", "Mutating"];
        let valid_persist_succeeded = ["Persisting"];

        for base in &states {
            let name = base.state_name();
            // begin_load
            let mut s = base.clone();
            let ok = s.begin_load(PathBuf::from("/x")).is_ok();
            assert_eq!(
                ok,
                valid_begin_load.contains(&name),
                "begin_load from {name} ok={ok} unexpected"
            );
            // begin_validating
            let mut s = base.clone();
            let ok = s.begin_validating(1).is_ok();
            assert_eq!(
                ok,
                valid_begin_validating.contains(&name),
                "begin_validating from {name} ok={ok} unexpected"
            );
            // mark_ready
            let mut s = base.clone();
            let ok = s.mark_ready("b".into()).is_ok();
            assert_eq!(
                ok,
                valid_mark_ready.contains(&name),
                "mark_ready from {name} ok={ok} unexpected"
            );
            // begin_mutating
            let mut s = base.clone();
            let ok = s.begin_mutating().is_ok();
            assert_eq!(
                ok,
                valid_begin_mutating.contains(&name),
                "begin_mutating from {name} ok={ok} unexpected"
            );
            // begin_persisting
            let mut s = base.clone();
            let ok = s.begin_persisting(PathBuf::from("/t")).is_ok();
            assert_eq!(
                ok,
                valid_begin_persisting.contains(&name),
                "begin_persisting from {name} ok={ok} unexpected"
            );
            // persist_succeeded
            let mut s = base.clone();
            let ok = s.persist_succeeded("b".into()).is_ok();
            assert_eq!(
                ok,
                valid_persist_succeeded.contains(&name),
                "persist_succeeded from {name} ok={ok} unexpected"
            );
        }
    }
}
