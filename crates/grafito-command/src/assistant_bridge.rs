//! Puente validado de comandos de texto para el asistente.
//!
//! Todo puro, sin E/S ni red ni spawn: valida la forma del texto, decide contra
//! el `command_registry` (`MutationClass`/`RiskLevel`/categoría) y ejecuta en un
//! clon del documento vía `process_input` (transaccional + `validate_document`).
//!
//! Contrato con la app:
//! - `assistant_may_execute` decide sin mutar: permite `ReadOnly`,
//!   `CreatesObject`, `TransformsObject` y `AddsConstraint` con `RiskLevel`
//!   `Low`/`Medium`; veta destructivos (`erase`, `delete`, `clear`, `reset`,
//!   `restore`, `load` y sus alias ES), `LoadsExternalData`, `Unclassified` y
//!   `RiskLevel::High`. Desconocido → `Err` honesto.
//! - `prepare_assistant_command` devuelve el clon ya validado; la app lo aplica
//!   con undo así: reemplaza el documento + push del snapshot previo. El
//!   documento original nunca se toca acá.
//! - Nombres ES de la toolbar (`Punto`, `Recta`, …) se normalizan al canónico
//!   inglés antes de resolver y ejecutar.

use crate::{
    cas_parse::parse_cas_command,
    command_registry::{self, MutationClass, RiskLevel},
    commands,
};
use grafito_assistant_types::MAX_RUN_COMMAND_CHARS;
use grafito_core::{validation::validate_document, Document};
use std::fmt::{self, Display, Formatter};

/// Cota de forma del texto del comando (paridad `MAX_RUN_COMMAND_CHARS`).
pub const MAX_BRIDGE_COMMAND_CHARS: usize = MAX_RUN_COMMAND_CHARS;
/// Cota del resumen devuelto en `PreparedCommand`.
pub const MAX_BRIDGE_RESUMEN_CHARS: usize = 1_024;

/// Comandos que borran o reemplazan estado, vetados aunque resuelvan en el registro.
const COMANDOS_DESTRUCTIVOS: &[&str] = &[
    "erase", "eraseall", "delete", "clear", "clearall", "reset", "restore", "load", "borrar",
    "eliminar",
];

/// Ficha de un comando que el asistente puede ejecutar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInfo {
    /// Nombre canónico del dispatcher (`spec.canonical`).
    pub nombre_canonico: String,
    /// Categoría del registro (paleta/documentación).
    pub categoria: String,
    /// Efecto persistente esperado.
    pub mutation_class: MutationClass,
    /// Riesgo operativo asociado.
    pub risk_level: RiskLevel,
}

/// Comando ya ejecutado en un clon validado, listo para aplicar con undo.
#[derive(Debug, Clone)]
pub struct PreparedCommand {
    /// Clon del documento con el comando aplicado y validado.
    pub documento_resultante: Document,
    /// Nombre canónico ejecutado.
    pub nombre_canonico: String,
    /// Mensaje del comando (o confirmación), acotado a `MAX_BRIDGE_RESUMEN_CHARS`.
    pub resumen: String,
}

/// Errores tipados del puente, siempre honestos y sin pánicos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// Texto vacío o solo espacios.
    TextoVacio,
    /// Más de 2000 caracteres.
    TextoMuyLargo { largo: usize, maximo: usize },
    /// Contiene NUL.
    ContieneNul,
    /// Contiene salto de línea (debe ser una sola línea).
    Multilinea,
    /// No resolvió en `command_registry` ni como nombre pelado.
    ComandoDesconocido { nombre: String },
    /// Destructivo o con efecto externo no permitido.
    ComandoProhibido { nombre: String, motivo: String },
    /// `RiskLevel::High` (o sin clasificar).
    RiesgoNoPermitido { nombre: String },
    /// El dispatcher rechazó el comando con su mensaje real.
    EjecucionFallida { nombre: String, mensaje: String },
    /// El resultado no pasó `validate_document`.
    ResultadoInvalido { mensaje: String },
}

impl Display for BridgeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextoVacio => write!(f, "assistant command text is empty"),
            Self::TextoMuyLargo { largo, maximo } => write!(
                f,
                "assistant command text has {largo} characters (max {maximo})"
            ),
            Self::ContieneNul => write!(f, "assistant command text contains NUL"),
            Self::Multilinea => write!(f, "assistant command text must be a single line"),
            Self::ComandoDesconocido { nombre } => {
                write!(f, "assistant command '{nombre}' is unknown")
            }
            Self::ComandoProhibido { nombre, motivo } => {
                write!(f, "assistant command '{nombre}' is forbidden: {motivo}")
            }
            Self::RiesgoNoPermitido { nombre } => write!(
                f,
                "assistant command '{nombre}' has a risk level the assistant may not execute"
            ),
            Self::EjecucionFallida { nombre, mensaje } => {
                write!(f, "assistant command '{nombre}' failed: {mensaje}")
            }
            Self::ResultadoInvalido { mensaje } => {
                write!(f, "assistant command result is invalid: {mensaje}")
            }
        }
    }
}

impl std::error::Error for BridgeError {}

/// Decide si el asistente puede ejecutar `nombre_o_texto`, sin mutar nada.
///
/// Acepta el nombre pelado (`Midpoint`) o el comando completo (`Midpoint[A, B]`,
/// `Punto[(1, 2)]` con alias ES normalizado).
pub fn assistant_may_execute(nombre_o_texto: &str) -> Result<CommandInfo, BridgeError> {
    let texto = validar_forma(nombre_o_texto)?;
    let crudo = extraer_nombre(texto)?;
    let normalizado = normalizar_nombre(&crudo);
    rechazar_destructivo(&crudo, &normalizado)?;
    let especificacion =
        command_registry::resolve(&normalizado).ok_or_else(|| BridgeError::ComandoDesconocido {
            nombre: truncar_nombre(&crudo),
        })?;
    rechazar_destructivo(especificacion.canonical, especificacion.canonical)?;
    match especificacion.risk {
        RiskLevel::Low | RiskLevel::Medium => {}
        RiskLevel::High | RiskLevel::Unclassified => {
            return Err(BridgeError::RiesgoNoPermitido {
                nombre: especificacion.canonical.to_string(),
            });
        }
    }
    match especificacion.mutation {
        MutationClass::ReadOnly
        | MutationClass::CreatesObject
        | MutationClass::TransformsObject
        | MutationClass::AddsConstraint => {}
        MutationClass::LoadsExternalData | MutationClass::Unclassified => {
            return Err(BridgeError::ComandoProhibido {
                nombre: especificacion.canonical.to_string(),
                motivo: "loads external data or is unclassified".into(),
            });
        }
    }
    Ok(CommandInfo {
        nombre_canonico: especificacion.canonical.to_string(),
        categoria: especificacion.category.to_string(),
        mutation_class: especificacion.mutation,
        risk_level: especificacion.risk,
    })
}

/// Reescribe el texto al canónico inglés del dispatcher (`Recta[A, B]` →
/// `Line[A, B]`), tras pasar la misma allowlist que `assistant_may_execute`.
pub fn canonical_command_texto(texto: &str) -> Result<String, BridgeError> {
    let texto = validar_forma(texto)?;
    let informacion = assistant_may_execute(texto)?;
    if let Some(analizado) = parse_cas_command(texto) {
        Ok(format!(
            "{}[{}]",
            informacion.nombre_canonico,
            analizado.args.join(", ")
        ))
    } else {
        Ok(informacion.nombre_canonico)
    }
}

/// Ejecuta el comando en un clon y devuelve el clon validado para aplicar con undo.
///
/// El `document` original queda intacto: todo el trabajo ocurre en
/// `detached_clone_for_staging` + `process_input` (transaccional) +
/// `validate_document` explícito.
pub fn prepare_assistant_command(
    texto: &str,
    document: &Document,
) -> Result<PreparedCommand, BridgeError> {
    let texto = validar_forma(texto)?;
    let informacion = assistant_may_execute(texto)?;
    let mut entrada = canonical_command_texto(texto)?;
    let mut clon = document.detached_clone_for_staging();
    let resumen = match commands::process_input(&mut clon, &mut entrada) {
        commands::CommandOutcome::Ok => format!("{} aplicado", informacion.nombre_canonico),
        commands::CommandOutcome::Message(mensaje) => truncar_resumen(&mensaje),
        commands::CommandOutcome::Error(mensaje) => {
            return Err(BridgeError::EjecucionFallida {
                nombre: informacion.nombre_canonico,
                mensaje,
            });
        }
    };
    validate_document(&clon).map_err(|mensaje| BridgeError::ResultadoInvalido { mensaje })?;
    Ok(PreparedCommand {
        documento_resultante: clon,
        nombre_canonico: informacion.nombre_canonico,
        resumen,
    })
}

fn validar_forma(texto: &str) -> Result<&str, BridgeError> {
    let recortado = texto.trim();
    if recortado.is_empty() {
        return Err(BridgeError::TextoVacio);
    }
    let largo = recortado.chars().count();
    if largo > MAX_BRIDGE_COMMAND_CHARS {
        return Err(BridgeError::TextoMuyLargo {
            largo,
            maximo: MAX_BRIDGE_COMMAND_CHARS,
        });
    }
    if recortado.contains('\0') {
        return Err(BridgeError::ContieneNul);
    }
    if recortado.contains(['\n', '\r']) {
        return Err(BridgeError::Multilinea);
    }
    Ok(recortado)
}

fn extraer_nombre(texto: &str) -> Result<String, BridgeError> {
    // Nombre crudo hasta el primer `[`: `parse_cas_command` ya normaliza por
    // alias (`Recta` → `Line3D`) y acá se necesita el ES original para mapear
    // a la 2D (`Recta` → `Line`).
    if let Some(apertura) = texto.find('[') {
        let crudo = texto[..apertura].trim();
        if !crudo.is_empty()
            && crudo
                .chars()
                .all(|letra| letra.is_alphanumeric() || letra == '_' || letra == '-')
        {
            return Ok(crudo.to_string());
        }
        return Err(BridgeError::ComandoDesconocido {
            nombre: truncar_nombre(texto),
        });
    }
    let candidato = texto.trim();
    if candidato
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
    {
        return Ok(candidato.to_string());
    }
    Err(BridgeError::ComandoDesconocido {
        nombre: truncar_nombre(texto),
    })
}

fn normalizar_nombre(nombre: &str) -> String {
    match nombre.to_lowercase().as_str() {
        "punto" => "Point".into(),
        "recta" => "Line".into(),
        "segmento" => "Segment".into(),
        "circulo" | "círculo" | "circunferencia" => "Circle".into(),
        "puntomedio" | "punto_medio" | "punto-medio" | "puntomedio_" => "Midpoint".into(),
        _ => nombre.trim().to_string(),
    }
}

fn rechazar_destructivo(crudo: &str, normalizado: &str) -> Result<(), BridgeError> {
    for candidato in [crudo, normalizado] {
        let minusculas = candidato.to_lowercase();
        if COMANDOS_DESTRUCTIVOS.contains(&minusculas.as_str()) {
            return Err(BridgeError::ComandoProhibido {
                nombre: truncar_nombre(candidato),
                motivo: "erases, clears or replaces document state".into(),
            });
        }
    }
    Ok(())
}

fn truncar_nombre(nombre: &str) -> String {
    const MAXIMO: usize = 64;
    if nombre.chars().count() <= MAXIMO {
        nombre.to_string()
    } else {
        nombre.chars().take(MAXIMO).collect()
    }
}

fn truncar_resumen(mensaje: &str) -> String {
    if mensaje.chars().count() <= MAX_BRIDGE_RESUMEN_CHARS {
        mensaje.to_string()
    } else {
        let mut recortado: String = mensaje.chars().take(MAX_BRIDGE_RESUMEN_CHARS).collect();
        recortado.push('…');
        recortado
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandOutcome;

    fn documento_con_a_y_b() -> Document {
        let mut documento = Document::new();
        for texto in ["A = (1, 2)", "B = (5, 6)"] {
            let resultado = commands::process_input(&mut documento, &mut texto.to_string());
            assert!(
                matches!(resultado, CommandOutcome::Ok | CommandOutcome::Message(_)),
                "{texto} debe crear el punto: {resultado:?}"
            );
        }
        documento
    }

    #[test]
    fn punto_recta_y_midpoint_estan_permitidos() {
        for (texto, canonico) in [
            ("Point[(1, 2)]", "Point"),
            ("Punto[(1, 2)]", "Point"),
            ("Recta[A, B]", "Line"),
            ("Midpoint[A, B]", "Midpoint"),
        ] {
            let informacion = assistant_may_execute(texto).expect("{texto} permitido");
            assert_eq!(informacion.nombre_canonico, canonico, "{texto}");
            assert!(
                matches!(informacion.risk_level, RiskLevel::Low | RiskLevel::Medium),
                "{texto}"
            );
        }
    }

    #[test]
    fn erase_y_borrado_quedan_vetados() {
        for texto in [
            "Erase[A]",
            "EraseAll[]",
            "EraseAll",
            "Delete[A]",
            "Borrar[A]",
            "Eliminar[A]",
            "Clear[A]",
            "Reset[A]",
        ] {
            assert!(
                assistant_may_execute(texto).is_err(),
                "{texto} debe estar vetado"
            );
            assert!(
                prepare_assistant_command(texto, &Document::new()).is_err(),
                "{texto} no debe ejecutarse"
            );
        }
    }

    #[test]
    fn riesgo_alto_queda_vetado() {
        let informacion = assistant_may_execute("Contour[x^2 + y^2, -3, 3, -3, 3, 1]")
            .expect_err("Contour es CreatesObject/High y debe vetarse por riesgo");
        assert!(matches!(informacion, BridgeError::RiesgoNoPermitido { .. }));
    }

    #[test]
    fn comando_desconocido_da_error_honesto() {
        let error = assistant_may_execute("Inventado[X]").expect_err("desconocido debe fallar");
        assert!(matches!(error, BridgeError::ComandoDesconocido { .. }));
        assert!(prepare_assistant_command("Inventado[X]", &Document::new()).is_err());
    }

    #[test]
    fn cotas_de_forma_rechazan_texto_invalido() {
        assert!(matches!(
            assistant_may_execute("   ").expect_err("vacío"),
            BridgeError::TextoVacio
        ));
        let largo = format!("Point[{}]", "1,".repeat(2000));
        assert!(matches!(
            assistant_may_execute(&largo).expect_err("muy largo"),
            BridgeError::TextoMuyLargo { .. }
        ));
        assert!(matches!(
            assistant_may_execute("Point[(1,\0 2)]").expect_err("NUL"),
            BridgeError::ContieneNul
        ));
        assert!(matches!(
            assistant_may_execute("Point[(1, 2)]\nMidpoint[A, B]").expect_err("multilínea"),
            BridgeError::Multilinea
        ));
    }

    #[test]
    fn el_clon_resultante_difiere_y_el_original_queda_intacto() {
        let original = Document::new();
        let antes = serde_json::to_value(&original).expect("documento serializable");
        let preparado =
            prepare_assistant_command("Point[(1, 2)]", &original).expect("Point permitido");
        assert_eq!(preparado.nombre_canonico, "Point");
        assert!(!preparado.resumen.trim().is_empty());
        validate_document(&preparado.documento_resultante).expect("el clon valida");
        assert_eq!(preparado.documento_resultante.object_count(), 1);
        assert_eq!(original.object_count(), 0);
        assert_eq!(
            serde_json::to_value(&original).expect("documento serializable"),
            antes,
            "el original queda intacto"
        );
    }

    #[test]
    fn recta_y_midpoint_crean_objetos_vivos() {
        let documento = documento_con_a_y_b();
        let linea = prepare_assistant_command("Recta[A, B]", &documento).expect("Recta permitida");
        assert_eq!(linea.nombre_canonico, "Line");
        assert_eq!(
            linea.documento_resultante.object_count(),
            documento.object_count() + 1
        );
        let medio =
            prepare_assistant_command("Midpoint[A, B]", &documento).expect("Midpoint permitido");
        assert_eq!(
            medio.documento_resultante.object_count(),
            documento.object_count() + 1
        );
        assert_eq!(documento.object_count(), 2, "el original queda intacto");
    }

    #[test]
    fn error_de_ejecucion_devuelve_el_mensaje_real() {
        let documento = Document::new();
        let error = prepare_assistant_command("Midpoint[A, B]", &documento)
            .expect_err("sin A y B debe fallar en ejecución");
        assert!(
            matches!(error, BridgeError::EjecucionFallida { mensaje, .. } if !mensaje.trim().is_empty())
        );
    }
}
