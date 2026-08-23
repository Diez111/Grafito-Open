//! Modelo de la mascota Pou — habitable, evolutiva y vestible.
//!
//! Memoria viva del estudiante: la mascota crece con el nivel, se viste
//! por etapas educativas y guarda su casa. Diseñada para ser
//! determinista, serializable y migrable (`#[serde(default)]`).

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Constantes y tipos base
// ─────────────────────────────────────────────────────────────────────────────

/// Longitud máxima del nombre de la mascota (caracteres).
pub const MAX_NAME: usize = 24;

// ─────────────────────────────────────────────────────────────────────────────
// Especies
// ─────────────────────────────────────────────────────────────────────────────

/// Especie de la mascota. Vectorial, sin emoji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MascotSpecies {
    /// Gota orgánica (Pou clásico).
    #[default]
    Blob = 0,
    /// Ajolote curioso.
    Axolotl = 1,
    /// Slime translúcido.
    Slime = 2,
}

impl MascotSpecies {
    /// Etiqueta corta para UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Blob => "Gota",
            Self::Axolotl => "Ajolote",
            Self::Slime => "Slime",
        }
    }

    /// Descripción breve para tooltip/docs.
    pub fn description(self) -> &'static str {
        match self {
            Self::Blob => "Forma orgánica clásica, suave y adaptable.",
            Self::Axolotl => "Curioso y sonriente, adora explorar.",
            Self::Slime => "Translúcido y juguetón, rebota con energía.",
        }
    }

    /// Todas las especies disponibles.
    pub fn all() -> &'static [Self] {
        &[Self::Blob, Self::Axolotl, Self::Slime]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Personalidad
// ─────────────────────────────────────────────────────────────────────────────

/// Personalidad que colorea el tono de Mora al hablar de la mascota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Personality {
    #[default]
    Chill = 0,
    Curioso = 1,
    Energico = 2,
    Dulce = 3,
}

impl Personality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chill => "Chill",
            Self::Curioso => "Curioso",
            Self::Energico => "Enérgico",
            Self::Dulce => "Dulce",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Chill => "Tranquilo y paciente.",
            Self::Curioso => "Pregunta todo, explora sin miedo.",
            Self::Energico => "Vitalidad alta, siempre en movimiento.",
            Self::Dulce => "Cariñoso y empático.",
        }
    }

    /// Fragmento que Mora inyecta en su system prompt.
    pub fn system_prompt_snippet(self) -> &'static str {
        match self {
            Self::Chill => "La mascota es tranquila: respuestas pausadas y alentadoras.",
            Self::Curioso => "La mascota es curiosa: hace preguntas y propone retos.",
            Self::Energico => "La mascota es enérgica: tono dinámico y motivador.",
            Self::Dulce => "La mascota es dulce: tono cálido y afectuoso.",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Chill, Self::Curioso, Self::Energico, Self::Dulce]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ánimo
// ─────────────────────────────────────────────────────────────────────────────

/// Ánimo visible de la mascota (calculado, no persistido directamente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MascotMood {
    #[default]
    Idle = 0,
    Happy = 1,
    Sleepy = 2,
    Hungry = 3,
    Annoyed = 4,
    Excited = 5,
}

impl MascotMood {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Tranquilo",
            Self::Happy => "Feliz",
            Self::Sleepy => "Somnoliento",
            Self::Hungry => "Hambriento",
            Self::Annoyed => "Molesto",
            Self::Excited => "Emocionado",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Casa
// ─────────────────────────────────────────────────────────────────────────────

/// Temática de la casa/habitáculo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HouseTheme {
    #[default]
    Acogedora = 0,
    Espacial = 1,
    Bosque = 2,
    Minimal = 3,
}

impl HouseTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Acogedora => "Acogedora",
            Self::Espacial => "Espacial",
            Self::Bosque => "Bosque",
            Self::Minimal => "Minimal",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Acogedora => "Madera cálida y luz suave.",
            Self::Espacial => "Estrellas y neón tranquilo.",
            Self::Bosque => "Verde, hojas y musgo.",
            Self::Minimal => "Líneas puras, calma escandinava.",
        }
    }
    pub fn all() -> &'static [Self] {
        &[Self::Acogedora, Self::Espacial, Self::Bosque, Self::Minimal]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ropa
// ─────────────────────────────────────────────────────────────────────────────

/// Capa de la prenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutfitLayer {
    #[default]
    Hat = 0,
    Body = 1,
    Accessory = 2,
}

impl OutfitLayer {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hat => "Sombrero",
            Self::Body => "Cuerpo",
            Self::Accessory => "Accesorio",
        }
    }
}

/// Etapa educativa que desbloquea ropa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OutfitTier {
    #[default]
    Primary = 0,
    Secondary = 1,
    University = 2,
    Master = 3,
}

impl OutfitTier {
    /// Tier según nivel del estudiante.
    pub fn from_level(level: u32) -> Self {
        match level {
            1..=5 => Self::Primary,
            6..=12 => Self::Secondary,
            13..=20 => Self::University,
            _ => Self::Master,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primaria",
            Self::Secondary => "Secundaria",
            Self::University => "Universidad",
            Self::Master => "Máster",
        }
    }

    pub fn level_range(self) -> (u32, u32) {
        match self {
            Self::Primary => (1, 5),
            Self::Secondary => (6, 12),
            Self::University => (13, 20),
            Self::Master => (21, u32::MAX),
        }
    }

    /// Nivel mínimo requerido.
    pub fn min_level(self) -> u32 {
        self.level_range().0
    }
}

/// Prenda equipable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outfit {
    /// Identificador estable (para persistencia).
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Capa.
    pub layer: OutfitLayer,
    /// Nivel mínimo que la desbloquea.
    pub unlocked_by_level: u32,
    /// Tier derivado (informativo).
    pub tier: OutfitTier,
}

impl Outfit {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        layer: OutfitLayer,
        unlocked_by_level: u32,
    ) -> Self {
        let tier = OutfitTier::from_level(unlocked_by_level);
        Self {
            id: id.into(),
            name: name.into(),
            layer,
            unlocked_by_level,
            tier,
        }
    }
}

/// Catálogo estático de ropa por niveles.
pub fn outfit_catalog() -> Vec<Outfit> {
    vec![
        // Primary 1-5
        Outfit::new("cap_prim", "Gorra Primaria", OutfitLayer::Hat, 1),
        Outfit::new("scarf_prim", "Bufanda Escolar", OutfitLayer::Accessory, 2),
        Outfit::new("tee_prim", "Camiseta Básica", OutfitLayer::Body, 3),
        // Secondary 6-12
        Outfit::new("hat_sec", "Gorro Secundaria", OutfitLayer::Hat, 6),
        Outfit::new("hoodie_sec", "Sudadera", OutfitLayer::Body, 8),
        Outfit::new("glasses_sec", "Gafas Estudio", OutfitLayer::Accessory, 10),
        // University 13-20
        Outfit::new("beanie_uni", "Gorro Uni", OutfitLayer::Hat, 13),
        Outfit::new("labcoat_uni", "Bata Lab", OutfitLayer::Body, 15),
        Outfit::new("cape_uni", "Capa Uni", OutfitLayer::Accessory, 18),
        // Master 21+
        Outfit::new("crown_master", "Corona Máster", OutfitLayer::Hat, 21),
        Outfit::new("robe_master", "Toga Máster", OutfitLayer::Body, 22),
        Outfit::new("medal_master", "Medalla", OutfitLayer::Accessory, 24),
    ]
}

/// Filtra prendas desbloqueadas para un nivel.
pub fn outfits_for_level(level: u32) -> Vec<Outfit> {
    outfit_catalog()
        .into_iter()
        .filter(|o| level >= o.unlocked_by_level)
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Wardrobe
// ─────────────────────────────────────────────────────────────────────────────

/// Guardarropa de la mascota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Wardrobe {
    /// IDs de prendas poseídas.
    #[serde(default)]
    pub owned: Vec<String>,
    /// IDs de prendas equipadas (máx. una por capa, pero guardamos lista).
    #[serde(default)]
    pub equipped: Vec<String>,
}

impl Wardrobe {
    /// ¿Posee la prenda?
    pub fn is_owned(&self, id: &str) -> bool {
        self.owned.iter().any(|o| o == id)
    }

    /// ¿Está equipada?
    pub fn is_equipped(&self, id: &str) -> bool {
        self.equipped.iter().any(|e| e == id)
    }

    /// Equipa una prenda si es poseída y respeta capa única.
    pub fn equip(&mut self, outfit: &Outfit) -> bool {
        if !self.is_owned(&outfit.id) {
            return false;
        }
        // Quita otra prenda de la misma capa.
        let catalog = outfit_catalog();
        self.equipped.retain(|eid| {
            if let Some(o) = catalog.iter().find(|o| &o.id == eid) {
                o.layer != outfit.layer
            } else {
                true
            }
        });
        if !self.is_equipped(&outfit.id) {
            self.equipped.push(outfit.id.clone());
        }
        true
    }

    /// Desequipa por id.
    pub fn unequip(&mut self, id: &str) {
        self.equipped.retain(|e| e != id);
    }

    /// Desbloquea automáticamente prendas por nivel (idempotente).
    pub fn unlock_for_level(&mut self, level: u32) {
        for outfit in outfits_for_level(level) {
            if !self.is_owned(&outfit.id) {
                self.owned.push(outfit.id);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MascotConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Configuración completa de la mascota Pou.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MascotConfig {
    /// Nombre visible (máx. MAX_NAME caracteres).
    pub name: String,
    /// Especie.
    pub species: MascotSpecies,
    /// Personalidad.
    pub personality: Personality,
    /// Semilla determinista para variación visual (forma, ojos, color).
    pub dna: u64,
    /// Etapa evolutiva 0..=3 (0 huevo, 1 cría, 2 joven, 3 adulto).
    pub evolution_stage: u8,
    /// Guardarropa.
    #[serde(default)]
    pub wardrobe: Wardrobe,
    /// Última alimentación (epoch segundos).
    #[serde(default)]
    pub last_fed_epoch: u64,
    /// XP de cuidado (alimentar, jugar).
    #[serde(default)]
    pub care_xp: u64,
    /// Hambre 0..=100 (0 saciado, 100 hambriento).
    #[serde(default)]
    pub hunger: u8,
    /// Felicidad 0..=100.
    #[serde(default)]
    pub happiness: u8,
    /// Tema de la casa.
    #[serde(default)]
    pub house_theme: HouseTheme,
    /// Monedas para cosméticos (futura economía).
    #[serde(default)]
    pub coins: u32,
}

impl Default for MascotConfig {
    fn default() -> Self {
        Self {
            name: "Pou".to_string(),
            species: MascotSpecies::default(),
            personality: Personality::default(),
            dna: 0x9E37_79B9_7F4A_7C15,
            evolution_stage: 0,
            wardrobe: Wardrobe::default(),
            last_fed_epoch: 0,
            care_xp: 0,
            hunger: 20,
            happiness: 80,
            house_theme: HouseTheme::default(),
            coins: 0,
        }
    }
}

impl MascotConfig {
    /// Valida invariantes. Mensajes en español para UI.
    pub fn validate(&self) -> Result<(), String> {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return Err("El nombre no puede estar vacío".to_string());
        }
        if trimmed.chars().count() > MAX_NAME {
            return Err(format!("El nombre no puede superar {MAX_NAME} caracteres"));
        }
        // Solo letras, números, espacios, guiones y apóstrofes.
        if trimmed.chars().any(|c| c.is_control()) {
            return Err("El nombre contiene caracteres no permitidos".to_string());
        }
        if self.evolution_stage > 3 {
            return Err("Etapa de evolución fuera de rango (0..3)".to_string());
        }
        if self.hunger > 100 || self.happiness > 100 {
            return Err("Hambre/felicidad fuera de rango 0..100".to_string());
        }
        Ok(())
    }

    /// Nombre saneado para mostrar (trim + fallback + trunc).
    pub fn sanitized_name(&self) -> String {
        let trimmed = self.name.trim();
        if trimmed.is_empty() {
            return "Pou".to_string();
        }
        let mut sanitized: String = trimmed.chars().filter(|c| !c.is_control()).collect();
        if sanitized.chars().count() > MAX_NAME {
            sanitized = sanitized.chars().take(MAX_NAME).collect();
        }
        sanitized.trim().to_string()
    }

    /// Calcula el ánimo actual.
    ///
    /// - `now_epoch`: segundos actuales.
    /// - `poked`: si el usuario acaba de molestar (click).
    pub fn update_mood(&self, now_epoch: u64, poked: bool) -> MascotMood {
        if poked {
            return MascotMood::Annoyed;
        }
        let hours_since_fed = now_epoch.saturating_sub(self.last_fed_epoch) / 3600;
        if self.hunger >= 80 || hours_since_fed >= 24 {
            return MascotMood::Hungry;
        }
        if self.happiness <= 20 {
            return MascotMood::Annoyed;
        }
        if hours_since_fed >= 12 && self.hunger >= 60 {
            return MascotMood::Sleepy;
        }
        if self.happiness >= 80 && self.hunger <= 30 {
            return MascotMood::Happy;
        }
        if self.happiness >= 60 && self.care_xp > 100 {
            return MascotMood::Excited;
        }
        MascotMood::Idle
    }

    /// Etapa evolutiva según nivel (1-5→0, 6-12→1, 13-20→2, 21+→3).
    pub fn evolution_stage_for_level(level: u32) -> u8 {
        OutfitTier::from_level(level) as u8
    }

    /// Sincroniza evolución con nivel y cantidad de ramas cubiertas (usado al guardar).
    /// Fórmula del app: min(3, level/4 + covered/4)
    pub fn sync_evolution(&mut self, level: u32, covered_branches: u32) {
        let stage = (level / 4 + covered_branches / 4).min(3) as u8;
        self.evolution_stage = stage;
    }

    /// Alimenta la mascota (baja hambre, sube felicidad y care_xp).
    pub fn feed(&mut self, now_epoch: u64) {
        self.hunger = self.hunger.saturating_sub(30).min(100);
        self.happiness = (self.happiness.saturating_add(10)).min(100);
        self.last_fed_epoch = now_epoch;
        self.care_xp = self.care_xp.saturating_add(5);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AvatarConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Configuración de avatar/perfil del usuario (se persiste en StudentProfile).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarConfig {
    /// Nombre para mostrar (puede diferir de StudentProfile.name por migración).
    #[serde(default)]
    pub display_name: String,
    /// Semilla para avatar blob (nombre o hash).
    #[serde(default)]
    pub seed: String,
    /// Preset de acento (0..5).
    #[serde(default)]
    pub accent_preset: u8,
    /// Mascota asociada (habitáculo Pou).
    #[serde(default)]
    pub mascot: Option<MascotConfig>,
}

impl Default for AvatarConfig {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            seed: "Estudiante".to_string(),
            accent_preset: 0,
            mascot: Some(MascotConfig::default()),
        }
    }
}

impl AvatarConfig {
    /// Valida el avatar. Mensajes en español.
    pub fn validate(&self) -> Result<(), String> {
        if self.display_name.chars().count() > 32 {
            return Err("El nombre no puede superar 32 caracteres".to_string());
        }
        if self.display_name.chars().any(|c| c.is_control()) {
            return Err("El nombre contiene caracteres no permitidos".to_string());
        }
        if self.accent_preset > 5 {
            return Err("Preset de acento fuera de rango (0..5)".to_string());
        }
        if let Some(m) = &self.mascot {
            m.validate()?;
        }
        Ok(())
    }

    /// Paleta de acentos. Devuelve (nombre, rgb, descripción). El tercer campo
    /// existe por compatibilidad con `let (_name, rgb, _) = ...` del app.
    pub fn accent_palette(preset: u8) -> (&'static str, [u8; 3], &'static str) {
        match preset % 6 {
            0 => ("Sage", [107, 122, 111], "sage #6B7A6F — Scandinavian calm"),
            1 => ("Stone", [120, 113, 108], "stone #78716C"),
            2 => ("Slate", [100, 116, 139], "slate #64748B"),
            3 => ("Moss", [101, 119, 90], "moss #65775A"),
            4 => ("Clay", [168, 123, 110], "clay #A87B6E"),
            _ => ("Ink", [68, 68, 68], "ink #444444"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_name_is_24() {
        assert_eq!(MAX_NAME, 24);
    }

    #[test]
    fn species_labels_and_descriptions() {
        assert_eq!(MascotSpecies::Blob.label(), "Gota");
        assert_eq!(MascotSpecies::Axolotl.label(), "Ajolote");
        assert_eq!(MascotSpecies::Slime.label(), "Slime");
        assert!(!MascotSpecies::Blob.description().is_empty());
    }

    #[test]
    fn personality_snippets() {
        assert!(Personality::Chill
            .system_prompt_snippet()
            .contains("tranquila"));
        assert!(Personality::Curioso
            .system_prompt_snippet()
            .contains("curiosa"));
        assert!(Personality::Energico
            .system_prompt_snippet()
            .contains("enérgica"));
        assert!(Personality::Dulce.system_prompt_snippet().contains("dulce"));
    }

    #[test]
    fn house_themes() {
        assert_eq!(HouseTheme::Acogedora.label(), "Acogedora");
        assert_eq!(HouseTheme::Minimal.label(), "Minimal");
    }

    #[test]
    fn outfit_tier_from_level() {
        assert_eq!(OutfitTier::from_level(1), OutfitTier::Primary);
        assert_eq!(OutfitTier::from_level(5), OutfitTier::Primary);
        assert_eq!(OutfitTier::from_level(6), OutfitTier::Secondary);
        assert_eq!(OutfitTier::from_level(12), OutfitTier::Secondary);
        assert_eq!(OutfitTier::from_level(13), OutfitTier::University);
        assert_eq!(OutfitTier::from_level(20), OutfitTier::University);
        assert_eq!(OutfitTier::from_level(21), OutfitTier::Master);
        assert_eq!(OutfitTier::from_level(100), OutfitTier::Master);
    }

    #[test]
    fn wardrobe_unlock_and_equip() {
        let mut w = Wardrobe::default();
        w.unlock_for_level(6);
        assert!(w.is_owned("cap_prim"));
        assert!(w.is_owned("hat_sec"));
        // No posee master aún
        assert!(!w.is_owned("crown_master"));
        let outfit = outfit_catalog()
            .into_iter()
            .find(|o| o.id == "hat_sec")
            .unwrap();
        assert!(w.equip(&outfit));
        assert!(w.is_equipped("hat_sec"));
        // Equipar otro sombrero reemplaza capa Hat
        let mut w2 = Wardrobe::default();
        w2.unlock_for_level(21);
        let hat1 = outfit_catalog()
            .into_iter()
            .find(|o| o.id == "cap_prim")
            .unwrap();
        let hat2 = outfit_catalog()
            .into_iter()
            .find(|o| o.id == "crown_master")
            .unwrap();
        w2.equip(&hat1);
        w2.equip(&hat2);
        assert!(!w2.is_equipped("cap_prim"));
        assert!(w2.is_equipped("crown_master"));
    }

    #[test]
    fn mascot_validate_and_sanitized() {
        let mut m = MascotConfig {
            name: "  Pou  ".to_string(),
            ..Default::default()
        };
        assert_eq!(m.sanitized_name(), "Pou");
        m.name = "a".repeat(30);
        assert!(m.validate().is_err());
        m.name = "a".repeat(MAX_NAME);
        assert!(m.validate().is_ok());
        m.name = String::new();
        assert_eq!(m.sanitized_name(), "Pou");
        assert!(m.validate().is_err());
    }

    #[test]
    fn evolution_stage_for_level() {
        assert_eq!(MascotConfig::evolution_stage_for_level(1), 0);
        assert_eq!(MascotConfig::evolution_stage_for_level(6), 1);
        assert_eq!(MascotConfig::evolution_stage_for_level(13), 2);
        assert_eq!(MascotConfig::evolution_stage_for_level(21), 3);
    }

    #[test]
    fn update_mood_logic() {
        let mut m = MascotConfig {
            hunger: 90,
            ..Default::default()
        };
        assert_eq!(m.update_mood(1000, false), MascotMood::Hungry);
        m.hunger = 10;
        m.happiness = 90;
        m.last_fed_epoch = 0;
        assert_eq!(m.update_mood(1000, false), MascotMood::Happy);
        assert_eq!(m.update_mood(1000, true), MascotMood::Annoyed);
        m.happiness = 10;
        assert_eq!(m.update_mood(1000, false), MascotMood::Annoyed);
    }

    #[test]
    fn avatar_validate() {
        let mut a = AvatarConfig::default();
        assert!(a.validate().is_ok());
        a.display_name = "a".repeat(33);
        assert!(a.validate().is_err());
    }

    #[test]
    fn mascot_serde_roundtrip() {
        let m = MascotConfig {
            name: "Michi".to_string(),
            species: MascotSpecies::Axolotl,
            personality: Personality::Dulce,
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: MascotConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }
}
