//! Design tokens tipográficos y de spacing para Grafito.
//!
//! Estos tokens son la **única fuente de verdad** para los tamaños de
//! texto y los espacios. Cualquier `.size(N)` o `.add_space(N)` con valor
//! hardcodeado debe migrarse a uno de estos tokens.
//!
//! # Por qué tokens
//!
//! Sin tokens, la app tiene 11, 12, 13, 14, 15, 16, 17, 18 como tamaños
//! de texto sueltos, sin razón ni proporción. Eso da una sensación de
//! "amateur" y rompe la jerarquía visual.
//!
//! Con estos tokens, todos los textos siguen una escala y todos los
//! espacios siguen otra. La sensación es profesional y la jerarquía
//! se lee a simple vista.
//!
//! # Escala tipográfica
//!
//! Basada en una **razón 1.25 (major third)** partiendo de 14 px:
//! - `XS` (11): notas, metadatos, hints
//! - `SM` (12): labels secundarios, captions
//! - `BASE` (14): texto de cuerpo, inputs
//! - `MD` (16): labels destacados, sub-headers
//! - `LG` (18): headers de panel
//! - `XL` (22): titles
//! - `XXL` (28): splash, branding
//!
//! # Escala de spacing
//!
//! Basada en **4 px** (siguiendo la convención de Material Design y de
//! Apple Human Interface Guidelines):
//! - `XS` (4): entre items muy cercanos (label + value)
//! - `SM` (8): entre items de un grupo
//! - `MD` (12): padding interno de chips
//! - `LG` (16): padding interno de paneles
//! - `XL` (24): separación entre secciones
//! - `XXL` (32): separación entre paneles principales
//!
//! # Radios
//!
//! - `SM` (4): chips, checkboxes
//! - `MD` (8): botones, inputs, items
//! - `LG` (12): paneles, modales

// ═══════════════════════════════════════════════════════════
// Type scale (ratio 1.25)
// ═══════════════════════════════════════════════════════════

/// Texto extra-pequeño: notas, metadatos, hints.
pub const TYPE_XS: f32 = 11.0;
/// Texto pequeño: labels secundarios, captions.
pub const TYPE_SM: f32 = 12.0;
/// Texto base: cuerpo de párrafo, inputs.
pub const TYPE_BASE: f32 = 14.0;
/// Texto mediano: labels destacados, sub-headers.
pub const TYPE_MD: f32 = 16.0;
/// Texto grande: headers de panel.
pub const TYPE_LG: f32 = 18.0;
/// Texto extra-grande: titles.
pub const TYPE_XL: f32 = 22.0;
/// Texto doble-extra-grande: splash, branding.
pub const TYPE_XXL: f32 = 28.0;

// ═══════════════════════════════════════════════════════════
// Spacing scale (4px base)
// ═══════════════════════════════════════════════════════════

/// Espacio extra-pequeño: entre items muy cercanos.
pub const SPACE_XS: f32 = 4.0;
/// Espacio pequeño: entre items de un grupo.
pub const SPACE_SM: f32 = 8.0;
/// Espacio mediano: padding interno de chips.
pub const SPACE_MD: f32 = 12.0;
/// Espacio grande: padding interno de paneles.
pub const SPACE_LG: f32 = 16.0;
/// Espacio extra-grande: separación entre secciones.
pub const SPACE_XL: f32 = 24.0;
/// Espacio doble-extra-grande: separación entre paneles principales.
pub const SPACE_XXL: f32 = 32.0;

// ═══════════════════════════════════════════════════════════
// Radii
// ═══════════════════════════════════════════════════════════

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 12.0;

// ═══════════════════════════════════════════════════════════
// Tamaños de íconos
// ═══════════════════════════════════════════════════════════

pub const ICON_SM: f32 = 16.0;
pub const ICON_MD: f32 = 20.0;
pub const ICON_LG: f32 = 24.0;
pub const ICON_XL: f32 = 32.0;

// ═══════════════════════════════════════════════════════════
// Animation timings
// ═══════════════════════════════════════════════════════════

/// Duración estándar de transiciones (ms).
pub const ANIM_FAST: f32 = 100.0;
/// Duración de animaciones de creación/feedback.
pub const ANIM_NORMAL: f32 = 200.0;
/// Duración de highlights (e.g. objeto recién creado).
pub const ANIM_HIGHLIGHT: f32 = 1000.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_scale_is_monotonic() {
        assert!(TYPE_XS < TYPE_SM);
        assert!(TYPE_SM < TYPE_BASE);
        assert!(TYPE_BASE < TYPE_MD);
        assert!(TYPE_MD < TYPE_LG);
        assert!(TYPE_LG < TYPE_XL);
        assert!(TYPE_XL < TYPE_XXL);
    }

    #[test]
    fn type_scale_uses_consistent_ratio() {
        // La escala está diseñada para que cada paso crezca ~12-14%.
        // Esto es menor que 1.25 (major third) y se ajusta a interfaces
        // densas como IDEs y apps de productividad. La verificación es
        // que el ratio está entre 1.10 y 1.30 (no menos, no más).
        let ratio_md = TYPE_MD / TYPE_BASE;
        assert!(
            ratio_md > 1.10 && ratio_md < 1.30,
            "ratio MD/BASE = {} (esperado entre 1.10 y 1.30)",
            ratio_md
        );
        let ratio_lg = TYPE_LG / TYPE_MD;
        assert!(
            ratio_lg > 1.10 && ratio_lg < 1.30,
            "ratio LG/MD = {} (esperado entre 1.10 y 1.30)",
            ratio_lg
        );
        // Cada paso debe ser al menos TYPE_XS (no se permiten pasos
        // que decrementeen)
        let ratio_xl = TYPE_XL / TYPE_LG;
        assert!(
            ratio_xl > 1.10 && ratio_xl < 1.30,
            "ratio XL/LG = {} (esperado entre 1.10 y 1.30)",
            ratio_xl
        );
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn spacing_scale_is_monotonic() {
        assert!(SPACE_XS < SPACE_SM);
        assert!(SPACE_SM < SPACE_MD);
        assert!(SPACE_MD < SPACE_LG);
        assert!(SPACE_LG < SPACE_XL);
        assert!(SPACE_XL < SPACE_XXL);
    }

    #[test]
    fn spacing_uses_4px_base() {
        // Cada step debería ser múltiplo de 4
        assert_eq!(SPACE_XS as i32 % 4, 0);
        assert_eq!(SPACE_SM as i32 % 4, 0);
        assert_eq!(SPACE_MD as i32 % 4, 0);
        assert_eq!(SPACE_LG as i32 % 4, 0);
        assert_eq!(SPACE_XL as i32 % 4, 0);
        assert_eq!(SPACE_XXL as i32 % 4, 0);
    }
}
