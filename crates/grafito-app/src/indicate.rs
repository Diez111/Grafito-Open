//! Resaltado temporal estilo Manim `Indicate` (~1.2 s, expira solo).
//!
//! Cerebro puro: sin egui, sin I/O, sin spawn. El `GrafitoApp` guarda un
//! `Option<IndicateHighlight>`, lo dispara con [`IndicateHighlight::try_new`]
//! y lo expira por frame; `render_2d` solo lee `alpha`/`radio`/`grosor`.
//! Tiempos en milisegundos como `f64` (vienen de `ui_time` en segundos):
//! lo no-finito se rechaza con `Err` en el constructor y se sanea con clamp
//! en las curvas (nunca pánico, nunca fantasma: lo vencido no se dibuja).

use grafito_core::ObjectId;

/// Duración del pulso en ms (Manim `Indicate` ronda 1 s; 1200 da 2 latidos).
pub const INDICATE_TTL_MS: u64 = 1200;
/// Tope sano para un TTL custom (`try_new_with_ttl`): 60 s.
pub const INDICATE_MAX_TTL_MS: u64 = 60_000;
/// Radio base del anillo en px.
pub const INDICATE_BASE_RADIUS_PX: f32 = 14.0;
/// Oscilación del radio por latido en px.
pub const INDICATE_RADIUS_SWING_PX: f32 = 6.0;
/// Crecimiento extra del radio hacia el final del pulso en px.
pub const INDICATE_RADIUS_GROW_PX: f32 = 10.0;
/// Grosor mínimo del anillo en px (el máximo suma 2.0 según alpha).
pub const INDICATE_BASE_WIDTH_PX: f32 = 1.0;

/// Pulso sobre un objeto: late 2 veces y expira solo a los 1200 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndicateHighlight {
    target: ObjectId,
    start_ms: u64,
    ttl_ms: u64,
}

impl IndicateHighlight {
    /// Crea el pulso con TTL fijo ([`INDICATE_TTL_MS`]).
    /// `now_ms` no-finito o negativo → `Err` honesto (no se dispara nada).
    pub fn try_new(target: ObjectId, now_ms: f64) -> Result<Self, String> {
        Self::try_new_with_ttl(target, now_ms, INDICATE_TTL_MS)
    }

    /// Variante con TTL explícito para tests; `ttl_ms` 0 o > 60 s → `Err`.
    pub fn try_new_with_ttl(target: ObjectId, now_ms: f64, ttl_ms: u64) -> Result<Self, String> {
        if !now_ms.is_finite() {
            return Err("Indicar: tiempo no finito".to_string());
        }
        if now_ms < 0.0 {
            return Err("Indicar: tiempo negativo".to_string());
        }
        // R1-9: `as u64` satura sin pánico pero mentiría (1e30 → MAX):
        // más allá de `u64::MAX` es `Err` honesto.
        if now_ms > u64::MAX as f64 {
            return Err("Indicar: tiempo excede el máximo representable".to_string());
        }
        if ttl_ms == 0 || ttl_ms > INDICATE_MAX_TTL_MS {
            return Err(format!("Indicar: TTL {ttl_ms} ms fuera de rango"));
        }
        let start_ms = now_ms.round() as u64;
        Ok(Self {
            target,
            start_ms,
            ttl_ms,
        })
    }

    /// Objeto resaltado.
    pub const fn target(&self) -> ObjectId {
        self.target
    }

    /// Progreso 0..=1. Lo no-finito cuenta como vencido (sin fantasma).
    pub fn progreso(&self, now_ms: f64) -> f64 {
        if !now_ms.is_finite() {
            return 1.0;
        }
        let elapsed = now_ms - self.start_ms as f64;
        if elapsed.is_nan() {
            return 1.0;
        }
        if elapsed <= 0.0 {
            return 0.0;
        }
        if !elapsed.is_finite() {
            return 1.0;
        }
        // `ttl_ms` > 0 por constructor: división segura.
        (elapsed / self.ttl_ms as f64).clamp(0.0, 1.0)
    }

    /// `true` cuando el pulso venció (a los 1200 ms por defecto).
    pub fn expirado(&self, now_ms: f64) -> bool {
        self.progreso(now_ms) >= 1.0
    }

    /// Opacidad 0..=1 del anillo: 2 latidos que se apagan con el progreso.
    /// NaN/inf → 0.0 (no se dibuja nada).
    pub fn alpha(&self, now_ms: f64) -> f32 {
        let p = self.progreso(now_ms);
        if !p.is_finite() {
            return 0.0;
        }
        let latido = (p * std::f64::consts::TAU * 2.0).sin();
        let a = (1.0 - p) * (0.7 + 0.3 * latido);
        if !a.is_finite() {
            return 0.0;
        }
        a.clamp(0.0, 1.0) as f32
    }

    /// Radio en px del anillo: base + latido + leve expansión final.
    /// Siempre finito en 1..=256 (lo vencido da el radio final, con alpha 0
    /// así que no se dibuja igual).
    pub fn radio(&self, now_ms: f64) -> f32 {
        let p = self.progreso(now_ms);
        if !p.is_finite() {
            return INDICATE_BASE_RADIUS_PX;
        }
        let latido = (p * std::f64::consts::TAU * 2.0).sin();
        let r = f64::from(INDICATE_BASE_RADIUS_PX)
            + f64::from(INDICATE_RADIUS_SWING_PX) * latido * (1.0 - p)
            + f64::from(INDICATE_RADIUS_GROW_PX) * p;
        if !r.is_finite() {
            return INDICATE_BASE_RADIUS_PX;
        }
        r.clamp(1.0, 256.0) as f32
    }

    /// Grosor en px del anillo: acompaña al alpha (1.0..=3.0).
    pub fn grosor(&self, now_ms: f64) -> f32 {
        INDICATE_BASE_WIDTH_PX + 2.0 * self.alpha(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulso_en(cero: f64) -> IndicateHighlight {
        IndicateHighlight::try_new(ObjectId::new(), cero).expect("tiempo válido")
    }

    #[test]
    fn ttl_por_defecto_es_1200_ms() {
        assert_eq!(INDICATE_TTL_MS, 1200);
        let hl = pulso_en(0.0);
        assert!(!hl.expirado(1199.0));
        assert!(hl.expirado(1200.0));
    }

    #[test]
    fn alpha_arranca_fuerte_y_muere_en_cero() {
        let hl = pulso_en(100.0);
        let inicial = hl.alpha(100.0);
        assert!(
            inicial > 0.5 && inicial <= 1.0,
            "alpha inicial {inicial} debería latir fuerte"
        );
        assert_eq!(hl.alpha(100.0 + 1200.0), 0.0);
        assert_eq!(hl.alpha(100.0 + 5000.0), 0.0);
    }

    #[test]
    fn radio_late_dentro_de_banda_sana() {
        let hl = pulso_en(0.0);
        for t_ms in [0.0, 150.0, 300.0, 600.0, 900.0, 1199.0] {
            let r = hl.radio(t_ms);
            assert!(
                r.is_finite() && (1.0..=64.0).contains(&r),
                "radio {r} fuera de banda en t={t_ms}"
            );
        }
        assert!(hl.grosor(0.0) > hl.grosor(1200.0));
    }

    #[test]
    fn expira_solo_pasado_el_ttl() {
        let hl = pulso_en(0.0);
        assert!(!hl.expirado(0.0));
        assert!(!hl.expirado(1199.0));
        assert!(hl.expirado(1200.0));
        assert!(hl.expirado(1300.0));
    }

    #[test]
    fn constructor_rechaza_tiempo_no_finito() {
        let id = ObjectId::new();
        assert!(IndicateHighlight::try_new(id, f64::NAN).is_err());
        assert!(IndicateHighlight::try_new(id, f64::INFINITY).is_err());
        assert!(IndicateHighlight::try_new(id, f64::NEG_INFINITY).is_err());
        assert!(IndicateHighlight::try_new(id, -1.0).is_err());
        assert!(IndicateHighlight::try_new_with_ttl(id, 0.0, 0).is_err());
        assert!(IndicateHighlight::try_new_with_ttl(id, 0.0, INDICATE_MAX_TTL_MS + 1).is_err());
    }

    #[test]
    fn constructor_rechaza_mas_alla_de_u64_max() {
        // R1-9: `now_ms > u64::MAX as f64 → Err` (el `as u64` saturaría a MAX
        // y mentiría con un pulso en el futuro lejano).
        let id = ObjectId::new();
        assert!(IndicateHighlight::try_new(id, 1e30).is_err());
        assert!(IndicateHighlight::try_new(id, u64::MAX as f64 * 2.0).is_err());
        // El borde sano sigue pasando.
        assert!(IndicateHighlight::try_new(id, 1_000_000.0).is_ok());
    }

    #[test]
    fn curvas_sanean_nan_inf_sin_panico() {
        let hl = pulso_en(0.0);
        assert_eq!(hl.alpha(f64::NAN), 0.0);
        assert_eq!(hl.alpha(f64::INFINITY), 0.0);
        // Progreso vencido (p=1): radio finito en banda, nunca basura.
        let r_nan = hl.radio(f64::NAN);
        assert!(r_nan.is_finite() && (1.0..=256.0).contains(&r_nan));
        assert!(hl.radio(f64::INFINITY).is_finite());
        assert!(hl.expirado(f64::NAN));
        assert!(hl.expirado(f64::INFINITY));
        assert_eq!(hl.progreso(-5.0), 0.0);
    }
}
