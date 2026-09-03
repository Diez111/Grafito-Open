//! Currículum — mapea niveles a objetivos de aprendizaje (UTN y secundaria).

use crate::level::UTNProgram;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Objetivo de aprendizaje atómico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearningObjective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub program: Option<UTNProgram>,
    /// Nivel mínimo requerido (ver `PedagogicalLevel::level_value`).
    #[serde(default = "default_level_min")]
    pub level_min: u32,
    /// IDs de LOs prerequisito.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Etiquetas para búsqueda y recomendación.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Horas estimadas de trabajo.
    #[serde(default = "default_hours")]
    pub estimated_hours: f32,
}

fn default_level_min() -> u32 {
    1
}
fn default_hours() -> f32 {
    2.0
}

impl LearningObjective {
    /// Constructor compatible con versión anterior (level_min 1, sin prereqs/tags).
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        program: Option<UTNProgram>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            program,
            level_min: 1,
            requires: Vec::new(),
            tags: Vec::new(),
            estimated_hours: 2.0,
        }
    }

    /// Constructor completo.
    #[allow(clippy::too_many_arguments)]
    pub fn new_full(
        id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        program: Option<UTNProgram>,
        level_min: u32,
        requires: Vec<String>,
        tags: Vec<String>,
        estimated_hours: f32,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            program,
            level_min,
            requires,
            tags,
            estimated_hours,
        }
    }

    /// Builder: nivel mínimo.
    pub fn with_level_min(mut self, v: u32) -> Self {
        self.level_min = v;
        self
    }
    /// Builder: prerequisitos.
    pub fn with_requires(mut self, r: Vec<String>) -> Self {
        self.requires = r;
        self
    }
    /// Builder: etiquetas.
    pub fn with_tags(mut self, t: Vec<String>) -> Self {
        self.tags = t;
        self
    }
    /// Builder: horas estimadas.
    pub fn with_hours(mut self, h: f32) -> Self {
        self.estimated_hours = h;
        self
    }
}

/// Currículum estático — primaria, secundaria y UTN (AM1/AM2/Álgebra/Probabilidad).
#[derive(Debug, Clone)]
pub struct Curriculum;

impl Curriculum {
    // ---- helpers internos para no repetir verbosidad ----
    #[allow(clippy::too_many_arguments)]
    fn lo(
        id: &str,
        title: &str,
        desc: &str,
        program: Option<UTNProgram>,
        level_min: u32,
        requires: &[&str],
        tags: &[&str],
        hours: f32,
    ) -> LearningObjective {
        LearningObjective {
            id: id.into(),
            title: title.into(),
            description: desc.into(),
            program,
            level_min,
            requires: requires.iter().map(|s| (*s).into()).collect(),
            tags: tags.iter().map(|s| (*s).into()).collect(),
            estimated_hours: hours,
        }
    }

    /// Primaria — 5 LOs.
    pub fn primary() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "pri-conteo",
                "Conteo",
                "Conteo, números naturales, orden y comparación",
                None,
                1,
                &[],
                &["conteo", "numeros", "orden", "primaria"],
                2.0,
            ),
            Self::lo(
                "pri-fracc-vis",
                "Fracciones visuales",
                "Fracciones con dibujos, mitad y cuarto, representación pictórica",
                None,
                1,
                &["pri-conteo"],
                &["fraccion", "visual", "mitad", "primaria"],
                2.5,
            ),
            Self::lo(
                "pri-perim-area",
                "Perímetro y área",
                "Perímetro y área de figuras simples, cuadrados y rectángulos",
                None,
                2,
                &["pri-conteo"],
                &["perimetro", "area", "geometria", "primaria"],
                3.0,
            ),
            Self::lo(
                "pri-proporciones",
                "Proporciones simples",
                "Doble, mitad, proporcionalidad simple con ejemplos concretos",
                None,
                2,
                &["pri-fracc-vis"],
                &["proporcion", "doble", "mitad", "primaria"],
                2.0,
            ),
            Self::lo(
                "pri-datos",
                "Datos simples",
                "Tablas, gráficos de barras, promedio simple y recolección de datos",
                None,
                2,
                &["pri-conteo"],
                &["datos", "tablas", "barras", "primaria", "estadistica"],
                2.0,
            ),
        ]
    }

    /// Todos los LOs de UTN AM1 (8).
    pub fn utn_am1() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "am1-func",
                "Funciones",
                "Dominio, imagen, composición, inversa, clasificación",
                Some(UTNProgram::AM1),
                10,
                &[],
                &["funcion", "dominio", "imagen", "composicion", "inversa"],
                6.0,
            ),
            Self::lo(
                "am1-lim",
                "Límites",
                "Límites laterales, indeterminaciones, asintotas, límites infinitos",
                Some(UTNProgram::AM1),
                11,
                &["am1-func"],
                &["limite", "asintota", "indeterminacion", "continuidad"],
                5.0,
            ),
            Self::lo(
                "am1-cont",
                "Continuidad",
                "Continuidad, teorema de Bolzano, clasificación de discontinuidades",
                Some(UTNProgram::AM1),
                11,
                &["am1-lim"],
                &["continuidad", "bolzano", "discontinuidad", "limite"],
                4.0,
            ),
            Self::lo(
                "am1-der",
                "Derivadas",
                "Definición, reglas, recta tangente, extremos, derivada",
                Some(UTNProgram::AM1),
                12,
                &["am1-cont"],
                &["derivada", "tangente", "extremos", "reglas"],
                7.0,
            ),
            Self::lo(
                "am1-der-aplic",
                "Aplicaciones de derivadas",
                "Crecimiento, concavidad, máximos y mínimos, L'Hôpital, optimización",
                Some(UTNProgram::AM1),
                12,
                &["am1-der"],
                &[
                    "derivada",
                    "optimizacion",
                    "extremos",
                    "concavidad",
                    "lhopital",
                ],
                6.0,
            ),
            Self::lo(
                "am1-int",
                "Integrales",
                "Primitivas, área, Barrow, impropias, integral definida",
                Some(UTNProgram::AM1),
                12,
                &["am1-der"],
                &["integral", "primitiva", "barrow", "area", "impropia"],
                7.0,
            ),
            Self::lo(
                "am1-int-aplic",
                "Aplicaciones de integrales",
                "Área entre curvas, volumen de revolución, longitud de arco",
                Some(UTNProgram::AM1),
                12,
                &["am1-int"],
                &["integral", "area", "volumen", "revolucion", "arco"],
                5.0,
            ),
            Self::lo(
                "am1-sucesiones",
                "Sucesiones",
                "Sucesiones numéricas, convergencia, criterio, límite de sucesión",
                Some(UTNProgram::AM1),
                11,
                &["am1-lim"],
                &["sucesion", "convergencia", "limite", "numerica"],
                4.0,
            ),
        ]
    }

    /// UTN AM2 — 7 LOs.
    pub fn utn_am2() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "am2-edo",
                "EDO",
                "Variables separables, lineales, aplicaciones, ecuaciones diferenciales",
                Some(UTNProgram::AM2),
                13,
                &["am1-der", "am1-int"],
                &["edo", "diferencial", "separable", "lineal"],
                8.0,
            ),
            Self::lo(
                "am2-series",
                "Series numéricas",
                "Criterios de convergencia, series alternadas, geométricas",
                Some(UTNProgram::AM2),
                13,
                &["am1-sucesiones"],
                &["serie", "convergencia", "numerica", "criterio"],
                6.0,
            ),
            Self::lo(
                "am2-taylor",
                "Taylor y Fourier",
                "Series de Taylor, Fourier, aproximación, convergencia",
                Some(UTNProgram::AM2),
                14,
                &["am2-series"],
                &["taylor", "fourier", "serie", "aproximacion"],
                6.0,
            ),
            Self::lo(
                "am2-multivariable",
                "Cálculo multivariable",
                "Funciones de varias variables, derivadas parciales, gradiente",
                Some(UTNProgram::AM2),
                13,
                &["am1-der"],
                &["multivariable", "parcial", "gradiente", "varias variables"],
                7.0,
            ),
            Self::lo(
                "am2-int-multi",
                "Integrales dobles y triples",
                "Integrales dobles, triples, cambio de variables, Jacobiano",
                Some(UTNProgram::AM2),
                14,
                &["am2-multivariable"],
                &[
                    "integral",
                    "doble",
                    "triple",
                    "jacobiano",
                    "cambio variable",
                ],
                7.0,
            ),
            Self::lo(
                "am2-campos",
                "Campos vectoriales",
                "Campos vectoriales, rotacional, divergencia, potencial",
                Some(UTNProgram::AM2),
                14,
                &["am2-multivariable", "alg-vectores"],
                &[
                    "campo",
                    "vectorial",
                    "rotacional",
                    "divergencia",
                    "gradiente",
                ],
                6.0,
            ),
            Self::lo(
                "am2-teoremas",
                "Teoremas integrales",
                "Green, Stokes, Gauss (divergencia), aplicaciones",
                Some(UTNProgram::AM2),
                14,
                &["am2-campos", "am2-int-multi"],
                &["green", "stokes", "gauss", "teorema", "integral"],
                6.0,
            ),
        ]
    }

    /// UTN Álgebra — 6 LOs.
    pub fn utn_algebra() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "alg-vectores",
                "Vectores",
                "Vectores en R2/R3, producto escalar y vectorial, norma",
                Some(UTNProgram::Algebra),
                11,
                &["sec-vect"],
                &["vector", "escalar", "vectorial", "norma", "r2", "r3"],
                5.0,
            ),
            Self::lo(
                "alg-rectas-planos",
                "Rectas y planos",
                "Ecuaciones de rectas y planos, posiciones relativas, distancias",
                Some(UTNProgram::Algebra),
                12,
                &["alg-vectores"],
                &["recta", "plano", "ecuacion", "posicion", "distancia"],
                5.0,
            ),
            Self::lo(
                "alg-matrices",
                "Matrices",
                "Operaciones, rango, sistemas lineales, Gauss-Jordan",
                Some(UTNProgram::Algebra),
                11,
                &["sec-ec"],
                &["matriz", "rango", "sistema", "gauss", "lineal"],
                6.0,
            ),
            Self::lo(
                "alg-determinantes",
                "Determinantes",
                "Propiedades, cálculo, matriz inversa, regla de Cramer",
                Some(UTNProgram::Algebra),
                12,
                &["alg-matrices"],
                &["determinante", "inversa", "cramer", "matriz"],
                4.0,
            ),
            Self::lo(
                "alg-conicas",
                "Cónicas",
                "Circunferencia, elipse, parábola, hipérbola, ecuaciones canónicas",
                Some(UTNProgram::Algebra),
                12,
                &["alg-rectas-planos"],
                &[
                    "conica",
                    "elipse",
                    "parabola",
                    "hiperbola",
                    "circunferencia",
                ],
                5.0,
            ),
            Self::lo(
                "alg-transformaciones",
                "Transformaciones lineales",
                "Núcleo, imagen, matriz asociada, autovalores y autovectores",
                Some(UTNProgram::Algebra),
                13,
                &["alg-matrices", "alg-determinantes"],
                &["transformacion", "lineal", "nucleo", "imagen", "autovalor"],
                6.0,
            ),
        ]
    }

    /// UTN Probabilidad — 6 LOs.
    pub fn utn_probabilidad() -> Vec<LearningObjective> {
        // Alias en inglés para compatibilidad si se usa utn_probability
        Self::utn_probability()
    }
    /// UTN Probabilidad (alias inglés).
    pub fn utn_probability() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "prob-basica",
                "Probabilidad básica",
                "Espacio muestral, eventos, probabilidad condicional, Bayes",
                Some(UTNProgram::Probabilidad),
                11,
                &["sec-prob"],
                &["probabilidad", "muestral", "bayes", "condicional", "evento"],
                5.0,
            ),
            Self::lo(
                "prob-var",
                "Variables aleatorias",
                "Variables aleatorias discretas y continuas, esperanza, varianza",
                Some(UTNProgram::Probabilidad),
                12,
                &["prob-basica"],
                &["variable", "aleatoria", "esperanza", "varianza", "discreta"],
                5.0,
            ),
            Self::lo(
                "prob-distribuciones",
                "Distribuciones",
                "Binomial, Poisson, Normal, exponencial, propiedades",
                Some(UTNProgram::Probabilidad),
                12,
                &["prob-var"],
                &[
                    "distribucion",
                    "binomial",
                    "poisson",
                    "normal",
                    "exponencial",
                ],
                6.0,
            ),
            Self::lo(
                "prob-inferencia",
                "Inferencia estadística",
                "Estimación puntual, intervalos de confianza, test de hipótesis",
                Some(UTNProgram::Probabilidad),
                13,
                &["prob-distribuciones"],
                &[
                    "inferencia",
                    "estimacion",
                    "confianza",
                    "hipotesis",
                    "intervalo",
                ],
                6.0,
            ),
            Self::lo(
                "prob-regresion",
                "Regresión",
                "Regresión lineal, correlación, mínimos cuadrados, predicción",
                Some(UTNProgram::Probabilidad),
                13,
                &["prob-distribuciones"],
                &["regresion", "correlacion", "lineal", "minimos cuadrados"],
                5.0,
            ),
            Self::lo(
                "prob-muestreo",
                "Muestreo",
                "Técnicas de muestreo, teorema central del límite, distribuciones muestrales",
                Some(UTNProgram::Probabilidad),
                13,
                &["prob-inferencia"],
                &["muestreo", "central limite", "muestral", "tecnica"],
                4.0,
            ),
        ]
    }

    /// Secundaria — 11 LOs (incluye `sec-pitagoras`).
    pub fn secondary() -> Vec<LearningObjective> {
        vec![
            Self::lo(
                "sec-fracc",
                "Fracciones",
                "Operaciones con fracciones, simplificación, fracciones equivalentes",
                None,
                4,
                &["pri-fracc-vis"],
                &["fraccion", "simplificacion", "equivalente", "secundaria"],
                3.0,
            ),
            Self::lo(
                "sec-prop",
                "Proporciones",
                "Razones, proporciones, regla de tres, porcentaje",
                None,
                5,
                &["sec-fracc"],
                &["proporcion", "razon", "regla de tres", "porcentaje"],
                3.0,
            ),
            Self::lo(
                "sec-ec",
                "Ecuaciones",
                "Ecuaciones lineales y cuadráticas, sistemas de ecuaciones",
                None,
                6,
                &["sec-prop"],
                &["ecuacion", "lineal", "cuadratica", "sistema"],
                4.0,
            ),
            Self::lo(
                "sec-lineal",
                "Funciones lineales",
                "Recta, pendiente, ordenada al origen, gráfica de función lineal",
                None,
                6,
                &["sec-ec"],
                &["funcion", "lineal", "recta", "pendiente", "grafica"],
                4.0,
            ),
            Self::lo(
                "sec-cuad",
                "Funciones cuadráticas",
                "Parábola, vértice, raíces, discriminante, gráfica cuadrática",
                None,
                7,
                &["sec-lineal"],
                &["funcion", "cuadratica", "parabola", "vertice", "raiz"],
                4.0,
            ),
            Self::lo(
                "sec-pend",
                "Pendiente",
                "Pendiente de recta, tangente intuitiva, inclinación",
                None,
                6,
                &["sec-lineal"],
                &["pendiente", "recta", "tangente", "inclinacion"],
                2.0,
            ),
            Self::lo(
                "sec-area",
                "Área",
                "Área bajo curva, aproximación, área de figuras",
                None,
                6,
                &["pri-perim-area"],
                &["area", "curva", "aproximacion", "figura"],
                2.5,
            ),
            Self::lo(
                "sec-trig",
                "Trigonometría",
                "Seno, coseno, círculo unitario, identidades trigonométricas",
                None,
                7,
                &["sec-lineal"],
                &["trigonometria", "seno", "coseno", "circulo", "identidad"],
                4.0,
            ),
            Self::lo(
                "sec-vect",
                "Vectores intro",
                "Vectores, componentes, suma, noción geométrica",
                None,
                6,
                &["sec-pend"],
                &["vector", "componente", "suma", "geometrico"],
                3.0,
            ),
            Self::lo(
                "sec-prob",
                "Probabilidad básica secundaria",
                "Eventos, probabilidad simple, diagramas, frecuencia",
                None,
                5,
                &["sec-prop", "pri-datos"],
                &[
                    "probabilidad",
                    "evento",
                    "diagrama",
                    "frecuencia",
                    "secundaria",
                ],
                3.0,
            ),
            Self::lo(
                "sec-pitagoras",
                "Teorema de Pitágoras",
                "Triángulo rectángulo, catetos e hipotenusa, c²=a²+b², demostración y aplicaciones",
                None,
                8,
                &["sec-area"],
                &[
                    "pitagoras",
                    "triangulo",
                    "hipotenusa",
                    "cateto",
                    "secundaria",
                    "geometria",
                ],
                3.0,
            ),
        ]
    }

    /// Todos los LOs (primaria + secundaria + UTN).
    pub fn all() -> Vec<LearningObjective> {
        let mut v = Vec::new();
        v.extend(Self::primary());
        v.extend(Self::secondary());
        v.extend(Self::utn_am1());
        v.extend(Self::utn_am2());
        v.extend(Self::utn_algebra());
        v.extend(Self::utn_probabilidad());
        v
    }

    /// Obtiene un LO por id.
    pub fn get(id: &str) -> Option<LearningObjective> {
        Self::all().into_iter().find(|lo| lo.id == id)
    }

    /// Prerequisitos directos de un LO (solo los que existen en el currículum).
    pub fn prerequisites_for(id: &str) -> Vec<LearningObjective> {
        match Self::get(id) {
            Some(lo) => lo
                .requires
                .iter()
                .filter_map(|req| Self::get(req))
                .collect(),
            None => Vec::new(),
        }
    }

    /// LOs desbloqueados para un nivel numérico dado.
    pub fn all_unlocked_for_level(level: u32) -> Vec<LearningObjective> {
        Self::all()
            .into_iter()
            .filter(|lo| lo.level_min <= level)
            .collect()
    }

    /// Orden topológico (Kahn). Error si hay ciclo.
    pub fn topological_order() -> Result<Vec<LearningObjective>, String> {
        let all = Self::all();
        let mut id_to_lo: HashMap<String, LearningObjective> = HashMap::new();
        for lo in &all {
            id_to_lo.insert(lo.id.clone(), lo.clone());
        }
        let mut indegree: HashMap<String, usize> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for lo in &all {
            indegree.entry(lo.id.clone()).or_insert(0);
        }
        for lo in &all {
            for req in &lo.requires {
                if id_to_lo.contains_key(req) {
                    // arista req -> lo.id
                    if let Some(d) = indegree.get_mut(&lo.id) {
                        *d += 1;
                    }
                    adj.entry(req.clone()).or_default().push(lo.id.clone());
                }
            }
        }
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut zero: Vec<String> = indegree
            .iter()
            .filter_map(|(k, &v)| if v == 0 { Some(k.clone()) } else { None })
            .collect();
        zero.sort();
        for z in zero {
            queue.push_back(z);
        }
        let mut result = Vec::new();
        let mut visited = 0usize;
        while let Some(id) = queue.pop_front() {
            visited += 1;
            if let Some(lo) = id_to_lo.get(&id) {
                result.push(lo.clone());
            }
            if let Some(neigh) = adj.get(&id) {
                let mut newly_zero = Vec::new();
                for n in neigh {
                    if let Some(d) = indegree.get_mut(n) {
                        if *d > 0 {
                            *d -= 1;
                        }
                        if *d == 0 {
                            newly_zero.push(n.clone());
                        }
                    }
                }
                newly_zero.sort();
                for nz in newly_zero {
                    queue.push_back(nz);
                }
            }
        }
        if visited != all.len() {
            return Err("ciclo detectado en prerequisitos del currículum".into());
        }
        Ok(result)
    }

    /// Busca LOs que contengan el concepto (case-insensitive, substring en tags+título+descripción+id).
    /// Retorna ordenado por relevancia (cantidad de campos/tags que matchean).
    pub fn find_for_concept(concept: &str) -> Vec<LearningObjective> {
        let q = concept.to_lowercase();
        let q = q.trim().to_string();
        if q.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(usize, LearningObjective)> = Self::all()
            .into_iter()
            .filter_map(|lo| {
                let mut score = 0usize;
                if lo.title.to_lowercase().contains(&q) {
                    score += 1;
                }
                if lo.description.to_lowercase().contains(&q) {
                    score += 1;
                }
                if lo.id.to_lowercase().contains(&q) {
                    score += 1;
                }
                let tag_matches = lo
                    .tags
                    .iter()
                    .filter(|t| t.to_lowercase().contains(&q))
                    .count();
                score += tag_matches;
                if score > 0 {
                    Some((score, lo))
                } else {
                    None
                }
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.cmp(&b.1.title)));
        scored.into_iter().map(|(_, lo)| lo).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn find_deriva_returns_am1_der() {
        let los = Curriculum::find_for_concept("deriv");
        assert!(los.iter().any(|lo| lo.id == "am1-der"));
    }
    #[test]
    fn find_tags_taylor() {
        let los = Curriculum::find_for_concept("taylor");
        assert!(los.iter().any(|lo| lo.id == "am2-taylor"));
        // debe venir primero el que tiene tag taylor
        assert_eq!(los[0].id, "am2-taylor");
    }
    #[test]
    fn find_case_insensitive() {
        let los = Curriculum::find_for_concept("DERIVADA");
        assert!(!los.is_empty());
    }
    #[test]
    fn all_has_at_least_30() {
        assert!(Curriculum::all().len() >= 30);
    }
    #[test]
    fn all_counts() {
        assert_eq!(Curriculum::primary().len(), 5);
        assert_eq!(Curriculum::secondary().len(), 11);
        assert_eq!(Curriculum::utn_am1().len(), 8);
        assert_eq!(Curriculum::utn_am2().len(), 7);
        assert_eq!(Curriculum::utn_algebra().len(), 6);
        assert_eq!(Curriculum::utn_probabilidad().len(), 6);
        assert_eq!(Curriculum::all().len(), 43);
    }
    #[test]
    fn get_and_prereqs() {
        let lo = Curriculum::get("am1-der").expect("debe existir");
        assert_eq!(lo.id, "am1-der");
        let prereqs = Curriculum::prerequisites_for("am1-der");
        assert!(prereqs.iter().any(|p| p.id == "am1-cont"));
        assert!(Curriculum::get("no-existe").is_none());
        assert!(Curriculum::prerequisites_for("no-existe").is_empty());
    }
    #[test]
    fn topological_no_cycle() {
        let order = Curriculum::topological_order().expect("sin ciclo");
        assert_eq!(order.len(), Curriculum::all().len());
        // verificar orden: cada prereq aparece antes
        let mut pos: HashMap<String, usize> = HashMap::new();
        for (i, lo) in order.iter().enumerate() {
            pos.insert(lo.id.clone(), i);
        }
        for lo in Curriculum::all() {
            let p = pos[&lo.id];
            for req in lo.requires {
                if let Some(&rp) = pos.get(&req) {
                    assert!(rp < p, "prereq {} debe estar antes que {}", req, lo.id);
                }
            }
        }
    }
    #[test]
    fn unlocked_for_level() {
        let low = Curriculum::all_unlocked_for_level(2);
        // primaria desbloqueada, pero no AM1
        assert!(low.iter().any(|lo| lo.id == "pri-conteo"));
        assert!(!low.iter().any(|lo| lo.id == "am1-der"));
        let high = Curriculum::all_unlocked_for_level(15);
        assert_eq!(high.len(), Curriculum::all().len());
    }
    #[test]
    fn find_order_by_relevance() {
        // "integral" aparece en varios, pero am1-int debe tener alta relevancia
        let los = Curriculum::find_for_concept("integral");
        assert!(los.len() >= 3);
        // el primero debe tener al menos 2 matches (titulo+tag)
        let first = &los[0];
        assert!(
            first.title.to_lowercase().contains("integral")
                || first.tags.iter().any(|t| t.contains("integral"))
        );
    }
    #[test]
    fn new_compat_defaults() {
        let lo = LearningObjective::new("test", "T", "D", None);
        assert_eq!(lo.level_min, 1);
        assert!(lo.requires.is_empty());
        assert!(lo.tags.is_empty());
        assert!((lo.estimated_hours - 2.0).abs() < f32::EPSILON);
    }
}
