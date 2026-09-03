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
}
