//! CRDT pizarra `UUID+LWW` mínimo funcional en memoria (sin red).
//!
//! Cerebro puro: sin I/O, sin spawn, sin threads, sin deps nuevas.
//! Cada objeto de pizarra tiene un `CrdtId` de 128 bits (`site:16 + counter:64`,
//! std-only para no sumar `uuid` en este frente; el `uuid` v4 completo
//! con `HLC` real queda como L en [`crate::stubs::crdt_merge_stub`]) y un
//! `HlcTimestamp` (`wall,counter,site`) con `Last-Writer-Wins` por entrada.
//!
//! Propiedades (testeadas abajo):
//! - conmutativa: `a.merge(b)` y `b.merge(a)` dejan el mismo `live` set.
//! - idempotente: mergear dos veces no cambia nada la segunda.
//! - LWW: a igual `id`, gana el `ts` mayor (`wall`, luego `counter`, luego `site`).
//!
//! PII siempre local: valores acotados a 2048 bytes, entradas a 5000
//! (igual que roster), tombstones explícitos con `compact_tombstones`.
//! Sin red: `merge` es en memoria entre dos réplicas locales.
//!
//! Borde hostil (con P2P encima, `merge`/`upsert_remote`/JSON son el canal de
//! inyección): TODO lo entrante pasa por `validate_crdt_value` + techo de
//! reloj (`MAX_HLC_CLOCK_SKEW_SECS`) y `upsert_remote`/`merge` rechazan/saltean
//! `ts.site == self.site` (nadie se suplanta a sí mismo sin autenticar).
//! `Deserialize` es estricto vía `try_from` (valores, `wall` y cap de entradas).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::session::ClassroomError;

/// Tope de entradas totales (vivas + tombstones, igual que `MAX_ROSTER_SIZE`).
pub const MAX_CRDT_ENTRIES: usize = 5_000;
/// Tope por valor de pizarra (igual que `MAX_MESSAGE_BYTES`).
pub const MAX_CRDT_VALUE_BYTES: usize = 2_048;
/// Cota ABSOLUTA de `HlcTimestamp::wall` (año 3000, reloj de pared en secs).
/// `u64::MAX` u otros forzados jamás representan un reloj real: se rechazan.
pub const MAX_HLC_WALL_SECS: u64 = 32_503_680_000;
/// Techo de desfase de reloj admitido en escrituras remotas: `ts.wall` puede
/// aventajar al `now` del caller como mucho estos segundos (5 min). Un `wall`
/// futuro más allá de este techo dejaría los objetos locales indeletables
/// (`remove` compara `existing.ts >= ts`): por eso se corta acá.
pub const MAX_HLC_CLOCK_SKEW_SECS: u64 = 300;

/// Sitio/replica: newtype `u16` (0..=65535, el QR/loopback usa `0` por default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CrdtSiteId(u16);

impl CrdtSiteId {
    /// Construye sin validar (todo `u16` vale).
    #[must_use]
    pub fn new(site: u16) -> Self {
        Self(site)
    }

    /// Sitio raw.
    #[must_use]
    pub fn as_u16(&self) -> u16 {
        self.0
    }
}

/// ID de objeto: 128 bits `site:16 | counter:64 | reservado`.
///
/// std-only a propósito (sin dep `uuid` en este frente): unicidad por
/// `(site, counter)` monótono por réplica. El `uuid` v4 aleatorio completo
/// queda como L (ver `stubs::crdt_merge_stub`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CrdtId(u128);

impl CrdtId {
    /// Construye determinista desde `(site, counter)`.
    #[must_use]
    pub fn from_parts(site: u16, counter: u64) -> Self {
        Self((u128::from(site) << 64) | u128::from(counter))
    }

    /// Vista raw (para logs/dedup, sin PII).
    #[must_use]
    pub fn as_u128(&self) -> u128 {
        self.0
    }

    /// Sitio que generó el ID (bits altos).
    #[must_use]
    pub fn site(&self) -> u16 {
        (self.0 >> 64) as u16
    }
}

impl std::fmt::Display for CrdtId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Timestamp híbrido mínimo (`wall` secs + `counter` lógico + `site` desempate).
///
/// Orden total: `wall`, luego `counter`, luego `site` (LWW determinista sin reloj perfecto).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HlcTimestamp {
    /// Reloj de pared del caller (secs).
    pub wall: u64,
    /// Contador lógico de la réplica (monótono).
    pub counter: u64,
    /// Sitio (desempate total, evita empates entre réplicas).
    pub site: u16,
}

impl HlcTimestamp {
    /// Construye directo (validado por tipos, sin `Result`).
    #[must_use]
    pub fn new(wall: u64, counter: u64, site: u16) -> Self {
        Self {
            wall,
            counter,
            site,
        }
    }
}

/// Entrada de pizarra: valor + LWW + tombstone.
///
/// `Deserialize` es estricto (vía `try_from`): valor acotado sin controles y
/// `ts.wall` dentro de `MAX_HLC_WALL_SECS`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawCrdtEntry")]
pub struct CrdtEntry {
    /// Contenido (texto/JSON corto, `<= MAX_CRDT_VALUE_BYTES` bytes).
    pub value: String,
    /// Último escritor (LWW).
    pub ts: HlcTimestamp,
    /// `true` = borrado lógico (tombstone, se conserva para propagar el delete).
    pub deleted: bool,
}

/// Forma cruda entrante de una entrada (se revalida en `try_from`).
#[derive(Debug, Deserialize)]
struct RawCrdtEntry {
    value: String,
    ts: HlcTimestamp,
    deleted: bool,
}

impl TryFrom<RawCrdtEntry> for CrdtEntry {
    type Error = ClassroomError;

    fn try_from(raw: RawCrdtEntry) -> Result<Self, Self::Error> {
        validate_crdt_value(&raw.value)?;
        if raw.ts.wall > MAX_HLC_WALL_SECS {
            return Err(ClassroomError::InvalidMessage(format!(
                "ts.wall {} excede {MAX_HLC_WALL_SECS}",
                raw.ts.wall
            )));
        }
        Ok(Self {
            value: raw.value,
            ts: raw.ts,
            deleted: raw.deleted,
        })
    }
}

/// Pizarra CRDT en memoria (una réplica local).
///
/// `BTreeMap` para orden determinista (igual que roster). Sin red: dos réplicas
/// se fusionan con [`Self::merge`] en memoria. `Deserialize` es estricto
/// (vía `try_from`): cap `MAX_CRDT_ENTRIES` + entradas ya validadas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawWhiteboardCrdt")]
pub struct WhiteboardCrdt {
    site: u16,
    counter: u64,
    entries: BTreeMap<CrdtId, CrdtEntry>,
}

/// Forma cruda entrante de una réplica (se revalida en `try_from`).
#[derive(Debug, Deserialize)]
struct RawWhiteboardCrdt {
    site: u16,
    counter: u64,
    #[serde(default)]
    entries: BTreeMap<CrdtId, CrdtEntry>,
}

impl TryFrom<RawWhiteboardCrdt> for WhiteboardCrdt {
    type Error = ClassroomError;

    fn try_from(raw: RawWhiteboardCrdt) -> Result<Self, Self::Error> {
        if raw.entries.len() > MAX_CRDT_ENTRIES {
            return Err(ClassroomError::StorageFull { what: "Crdt" });
        }
        Ok(Self {
            site: raw.site,
            counter: raw.counter,
            entries: raw.entries,
        })
    }
}

impl WhiteboardCrdt {
    /// Réplica nueva vacía para `site`.
    #[must_use]
    pub fn new(site: u16) -> Self {
        Self {
            site,
            counter: 0,
            entries: BTreeMap::new(),
        }
    }

    /// Sitio de esta réplica.
    #[must_use]
    pub fn site(&self) -> u16 {
        self.site
    }

    /// Entradas totales (vivas + tombstones).
    #[must_use]
    pub fn len_total(&self) -> usize {
        self.entries.len()
    }

    /// Entradas vivas (no borradas).
    #[must_use]
    pub fn len_live(&self) -> usize {
        self.entries.values().filter(|e| !e.deleted).count()
    }

    /// ¿Sin entradas vivas?
    #[must_use]
    pub fn is_live_empty(&self) -> bool {
        !self.entries.values().any(|e| !e.deleted)
    }

    /// Valor vivo por ID (`None` si ausente o borrado).
    #[must_use]
    pub fn get(&self, id: &CrdtId) -> Option<&str> {
        self.entries.get(id).and_then(|e| {
            if e.deleted {
                None
            } else {
                Some(e.value.as_str())
            }
        })
    }

    /// Pares vivos ordenados por ID (`Vec` acotado por construcción a 5000).
    #[must_use]
    pub fn live_sorted(&self) -> Vec<(CrdtId, String)> {
        self.entries
            .iter()
            .filter_map(|(id, e)| {
                if e.deleted {
                    None
                } else {
                    Some((*id, e.value.clone()))
                }
            })
            .collect()
    }

    /// Inserta un valor local: genera `CrdtId` + `HlcTimestamp` y lo guarda.
    ///
    /// `Err(StorageFull)` si ya hay `MAX_CRDT_ENTRIES` (fail-closed).
    /// `Err(InvalidMessage)` si el valor excede 2048 bytes o trae controles
    /// (salvo `\n\t`, igual que el chat).
    pub fn insert_local(&mut self, value: &str, wall: u64) -> Result<CrdtId, ClassroomError> {
        validate_crdt_value(value)?;
        if self.entries.len() >= MAX_CRDT_ENTRIES {
            return Err(ClassroomError::StorageFull { what: "Crdt" });
        }
        if self.counter == u64::MAX {
            return Err(ClassroomError::StorageFull { what: "Crdt" });
        }
        self.counter = self.counter.saturating_add(1);
        let id = CrdtId::from_parts(self.site, self.counter);
        let ts = HlcTimestamp::new(wall, self.counter, self.site);
        // Colisión imposible en la misma réplica (counter monótono), pero si
        // el ID ya existiera por `upsert_remote` previo, aplica LWW honesto.
        match self.entries.get(&id) {
            Some(existing) if existing.ts >= ts => {}
            _ => {
                self.entries.insert(
                    id,
                    CrdtEntry {
                        value: value.to_string(),
                        ts,
                        deleted: false,
                    },
                );
            }
        }
        Ok(id)
    }

    /// Aplica una escritura remota con LWW.
    ///
    /// Retorna `Ok(true)` si se aplicó (nuevo o más reciente), `Ok(false)` si
    /// el local ya era más reciente (stale honesto). `Err` si valor inválido
    /// o almacén lleno para IDs nuevos.
    ///
    /// Anti-forja (el remoto no es de confianza):
    /// - `ts.site == self.site` → `Err`: nadie se suplanta a esta réplica sin
    ///   autenticar (con P2P encima, forzar `site` propio ganaría LWW "gratis");
    /// - `ts.wall > now + MAX_HLC_CLOCK_SKEW_SECS` (o `> MAX_HLC_WALL_SECS`)
    ///   → `Err`: sin techo, un `wall` arbitrario gana LWW para siempre y deja
    ///   los objetos locales indeletables (`remove` compara `existing.ts >= ts`).
    ///   Con el techo, el desfase queda acotado a la ventana de skew.
    ///
    /// `now` es el reloj de pared del caller (secs), mismo contrato que
    /// [`Self::insert_local`]/[`Self::remove`].
    pub fn upsert_remote(
        &mut self,
        id: CrdtId,
        value: &str,
        ts: HlcTimestamp,
        now: u64,
    ) -> Result<bool, ClassroomError> {
        validate_crdt_value(value)?;
        validate_remote_ts(ts, self.site, now)?;
        match self.entries.get(&id) {
            Some(existing) if existing.ts >= ts => Ok(false),
            Some(_) => {
                if let Some(entry) = self.entries.get_mut(&id) {
                    entry.value = value.to_string();
                    entry.ts = ts;
                    entry.deleted = false;
                }
                Ok(true)
            }
            None => {
                if self.entries.len() >= MAX_CRDT_ENTRIES {
                    return Err(ClassroomError::StorageFull { what: "Crdt" });
                }
                self.entries.insert(
                    id,
                    CrdtEntry {
                        value: value.to_string(),
                        ts,
                        deleted: false,
                    },
                );
                Ok(true)
            }
        }
    }

    /// Borra lógico (tombstone) con timestamp nuevo de esta réplica.
    ///
    /// `Ok(true)` si se marcó, `Ok(false)` si el ID era desconocido (no-op
    /// honesto: no se crean tombstones de IDs jamás vistos, evita llenado
    /// por IDs basura) o si el tombstone local ya era más reciente.
    pub fn remove(&mut self, id: &CrdtId, wall: u64) -> Result<bool, ClassroomError> {
        let Some(existing) = self.entries.get(id) else {
            return Ok(false);
        };
        if self.counter == u64::MAX {
            return Err(ClassroomError::StorageFull { what: "Crdt" });
        }
        self.counter = self.counter.saturating_add(1);
        let ts = HlcTimestamp::new(wall, self.counter, self.site);
        if existing.ts >= ts {
            return Ok(false);
        }
        if let Some(entry) = self.entries.get_mut(id) {
            entry.ts = ts;
            entry.deleted = true;
        }
        Ok(true)
    }

    /// Fusiona `other` en `self` con LWW por entrada (en memoria, sin red).
    ///
    /// Retorna cuántas entradas se aplicaron (nuevas o más recientes).
    ///
    /// Réplica remota = hostil hasta demostrar lo contrario; cada entrada pasa
    /// por `validate_crdt_value` + [`validate_remote_ts`] y las que faltan se
    /// saltean honestamente (el conteo solo cuenta aplicadas). Entradas con
    /// `ts.site == self.site` también se saltean (nadie suplanta a esta
    /// réplica sin autenticar; un fork local debe reclamar un `site` propio).
    ///
    /// Conmutativa e idempotente sobre el set vivo (testeado), incluso con el
    /// almacén lleno: se aplica LWW sin tope y al final se recorta a los
    /// `MAX_CRDT_ENTRIES` más recientes `(ts, id)` — una función determinista
    /// del conjunto resultante, así el orden de fusión no decide qué queda.
    /// Limitación honesta: un recorte puede descartar tombstones antiguos
    /// (igual que [`Self::compact_tombstones`]: llamar solo cuando el delete
    /// ya no volverá).
    pub fn merge(&mut self, other: &Self, now: u64) -> usize {
        let mut applied = 0_usize;
        for (id, remote) in &other.entries {
            if validate_crdt_value(&remote.value).is_err() {
                continue;
            }
            if validate_remote_ts(remote.ts, self.site, now).is_err() {
                continue;
            }
            match self.entries.get(id) {
                Some(local) if local.ts >= remote.ts => {}
                _ => {
                    self.entries.insert(*id, remote.clone());
                    applied = applied.saturating_add(1);
                }
            }
        }
        self.trim_to_capacity();
        applied
    }

    /// Recorta a `MAX_CRDT_ENTRIES` conservando las entradas más recientes
    /// `(ts, id)` (determinista: el resultado no depende del orden de fusión).
    fn trim_to_capacity(&mut self) {
        while self.entries.len() > MAX_CRDT_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(id, entry)| (entry.ts, **id))
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => {
                    self.entries.remove(&id);
                }
                None => break,
            }
        }
    }

    /// Compacta tombstones (los elimina). Retorna cuántos se quitaron.
    ///
    /// Llamar solo cuando todas las réplicas ya vieron el delete (en este
    /// frente sin red: cuando la UI confirma que el objeto no vuelve).
    /// Acotado por construcción (a lo sumo `len_total`).
    pub fn compact_tombstones(&mut self) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, e| !e.deleted);
        before.saturating_sub(self.entries.len())
    }
}

fn validate_crdt_value(value: &str) -> Result<(), ClassroomError> {
    if value.len() > MAX_CRDT_VALUE_BYTES {
        return Err(ClassroomError::InvalidMessage(format!(
            "valor CRDT excede {MAX_CRDT_VALUE_BYTES} bytes"
        )));
    }
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ClassroomError::InvalidMessage(
            "valor CRDT con caracteres de control".to_string(),
        ));
    }
    Ok(())
}

/// Validación de un timestamp remoto (hostil hasta demostrar lo contrario):
/// sin suplantación de `site` propio y con `wall` dentro del techo de reloj.
fn validate_remote_ts(ts: HlcTimestamp, own_site: u16, now: u64) -> Result<(), ClassroomError> {
    if ts.site == own_site {
        return Err(ClassroomError::InvalidMessage(
            "ts.site remoto suplanta a esta réplica".to_string(),
        ));
    }
    if ts.wall > MAX_HLC_WALL_SECS {
        return Err(ClassroomError::InvalidMessage(format!(
            "ts.wall {} excede {MAX_HLC_WALL_SECS}",
            ts.wall
        )));
    }
    let ceiling = now.saturating_add(MAX_HLC_CLOCK_SKEW_SECS);
    if ts.wall > ceiling {
        return Err(ClassroomError::InvalidMessage(format!(
            "ts.wall {} desborda el techo de reloj {ceiling}",
            ts.wall
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get_live_value() {
        let mut board = WhiteboardCrdt::new(1);
        let id = board.insert_local("trazo-1", 100).expect("insert");
        assert_eq!(id.site(), 1);
        assert_eq!(board.get(&id), Some("trazo-1"));
        assert_eq!(board.len_live(), 1);
        assert_eq!(board.len_total(), 1);
        assert!(!board.is_live_empty());
    }

    #[test]
    fn crdt_id_orders_and_displays() {
        let a = CrdtId::from_parts(1, 1);
        let b = CrdtId::from_parts(1, 2);
        let c = CrdtId::from_parts(2, 1);
        assert!(a < b);
        assert!(b < c);
        assert_eq!(a.site(), 1);
        assert_eq!(c.site(), 2);
        assert_eq!(format!("{a}").len(), 32);
        assert_eq!(CrdtSiteId::new(7).as_u16(), 7);
    }

    #[test]
    fn lww_newer_remote_wins_older_loses() {
        let mut board = WhiteboardCrdt::new(1);
        let id = board.insert_local("v1", 100).expect("insert");
        let old_ts = HlcTimestamp::new(50, 1, 2);
        let applied_old = board
            .upsert_remote(id, "viejo", old_ts, 100)
            .expect("upsert viejo");
        assert!(!applied_old);
        assert_eq!(board.get(&id), Some("v1"));
        let new_ts = HlcTimestamp::new(200, 99, 2);
        let applied_new = board
            .upsert_remote(id, "v2", new_ts, 200)
            .expect("upsert nuevo");
        assert!(applied_new);
        assert_eq!(board.get(&id), Some("v2"));
    }

    #[test]
    fn remove_creates_tombstone_and_hides_value() {
        let mut board = WhiteboardCrdt::new(1);
        let id = board.insert_local("x", 10).expect("insert");
        assert!(board.remove(&id, 20).expect("remove"));
        assert_eq!(board.get(&id), None);
        assert_eq!(board.len_live(), 0);
        assert_eq!(board.len_total(), 1);
        // Escritura más vieja no revive al borrado.
        let stale = HlcTimestamp::new(5, 1, 2);
        assert!(!board
            .upsert_remote(id, "revive", stale, 100)
            .expect("stale"));
        assert_eq!(board.get(&id), None);
        // Compactar elimina el tombstone.
        assert_eq!(board.compact_tombstones(), 1);
        assert_eq!(board.len_total(), 0);
    }

    #[test]
    fn remove_unknown_is_honest_noop() {
        let mut board = WhiteboardCrdt::new(1);
        let ghost = CrdtId::from_parts(9, 999);
        assert!(!board.remove(&ghost, 10).expect("noop"));
        assert_eq!(board.len_total(), 0);
    }

    #[test]
    fn merge_is_commutative_and_idempotent_on_live_set() {
        let mut a = WhiteboardCrdt::new(1);
        let mut b = WhiteboardCrdt::new(2);
        let id_a = a.insert_local("de-A", 100).expect("a");
        let id_b = b.insert_local("de-B", 100).expect("b");
        // Merge cruzado en ambos órdenes sobre clones frescos.
        let mut ab = a.clone();
        let mut ba = b.clone();
        let n1 = ab.merge(&b, 100);
        let n2 = ba.merge(&a, 100);
        assert_eq!(n1, 1);
        assert_eq!(n2, 1);
        assert_eq!(ab.live_sorted(), ba.live_sorted());
        assert!(ab.get(&id_a).is_some());
        assert!(ab.get(&id_b).is_some());
        // Idempotente: segunda fusión no aplica nada.
        assert_eq!(ab.merge(&b, 100), 0);
        assert_eq!(ba.merge(&a, 100), 0);
    }

    #[test]
    fn merge_lww_conflict_resolves_to_newest() {
        // Escrituras remotas legítimas: `ts.site` DISTINTO del propio (nadie
        // se suplanta a sí mismo — ver `upsert_remote_rejects_own_site...`).
        let mut a = WhiteboardCrdt::new(1);
        let id = a.insert_local("base", 100).expect("base");
        let ts_old = HlcTimestamp::new(150, 10, 2);
        let ts_new = HlcTimestamp::new(160, 11, 3);
        // Cada clon recibe una escritura remota distinta; merge converge al
        // más nuevo en ambos órdenes.
        let mut c1 = a.clone();
        let mut c2 = a.clone();
        c1.upsert_remote(id, "viejo", ts_old, 160).expect("viejo");
        c2.upsert_remote(id, "nuevo", ts_new, 160).expect("nuevo");
        c1.merge(&c2, 160);
        assert_eq!(c1.get(&id), Some("nuevo"));
        let mut c3 = a.clone();
        let mut c4 = a.clone();
        c3.upsert_remote(id, "viejo", ts_old, 160).expect("viejo");
        c4.upsert_remote(id, "nuevo", ts_new, 160).expect("nuevo");
        c4.merge(&c3, 160);
        assert_eq!(c4.get(&id), Some("nuevo"));
    }

    #[test]
    fn value_validation_and_storage_full_are_honest() {
        let mut board = WhiteboardCrdt::new(0);
        let big = "x".repeat(MAX_CRDT_VALUE_BYTES + 1);
        assert!(board.insert_local(&big, 1).is_err());
        assert!(board.insert_local("a\x00b", 1).is_err());
        assert!(board
            .upsert_remote(
                CrdtId::from_parts(1, 1),
                &big,
                HlcTimestamp::new(1, 1, 2),
                1
            )
            .is_err());
        // Llenar hasta el tope con inserciones locales.
        let mut full = WhiteboardCrdt::new(3);
        for _ in 0..MAX_CRDT_ENTRIES {
            full.insert_local("v", 1).expect("fill");
        }
        let err = full.insert_local("overflow", 1).expect_err("lleno");
        assert!(matches!(err, ClassroomError::StorageFull { what: "Crdt" }));
    }

    #[test]
    fn live_sorted_is_deterministic() {
        let mut board = WhiteboardCrdt::new(1);
        board.insert_local("b", 3).expect("b");
        board.insert_local("a", 1).expect("a");
        let live = board.live_sorted();
        assert_eq!(live.len(), 2);
        assert!(live[0].0 < live[1].0);
    }

    #[test]
    fn crdt_serde_roundtrip() {
        let mut board = WhiteboardCrdt::new(1);
        board.insert_local("hola", 5).expect("insert");
        let json = serde_json::to_string(&board).expect("serialize");
        let back: WhiteboardCrdt = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.live_sorted(), board.live_sorted());
    }

    #[test]
    fn serde_rejects_hostile_replica() {
        // Regresión A3: un `WhiteboardCrdt` deserializado de JSON evadía TODOS
        // los presupuestos `MAX_CRDT_*` (valores >2048 B, controles, 1M de
        // entradas, `wall` forjado). Con P2P encima, canal de inyección.
        let json_of = |entry: CrdtEntry| serde_json::to_string(&entry).expect("json");
        let oversize = CrdtEntry {
            value: "x".repeat(MAX_CRDT_VALUE_BYTES + 1),
            ts: HlcTimestamp::new(1, 1, 2),
            deleted: false,
        };
        assert!(serde_json::from_str::<CrdtEntry>(&json_of(oversize)).is_err());
        let control = CrdtEntry {
            value: "a\x00b".to_string(),
            ts: HlcTimestamp::new(1, 1, 2),
            deleted: false,
        };
        assert!(serde_json::from_str::<CrdtEntry>(&json_of(control)).is_err());
        let forged_wall = CrdtEntry {
            value: "ok".to_string(),
            ts: HlcTimestamp::new(u64::MAX, 1, 2),
            deleted: false,
        };
        assert!(serde_json::from_str::<CrdtEntry>(&json_of(forged_wall)).is_err());

        let mut entries = BTreeMap::new();
        for index in 0..(MAX_CRDT_ENTRIES + 1) {
            entries.insert(
                CrdtId::from_parts(2, index as u64),
                CrdtEntry {
                    value: "v".to_string(),
                    ts: HlcTimestamp::new(1, 1, 2),
                    deleted: false,
                },
            );
        }
        let fat = WhiteboardCrdt {
            site: 1,
            counter: 0,
            entries,
        };
        assert!(serde_json::from_str::<WhiteboardCrdt>(&json_of_board(&fat)).is_err());
    }

    fn json_of_board(board: &WhiteboardCrdt) -> String {
        serde_json::to_string(board).expect("json")
    }

    #[test]
    fn merge_validates_hostile_remote_values() {
        // Regresión A3: `merge` clonaba `remote` sin pasar por
        // `validate_crdt_value` (a diferencia de `insert_local`/`upsert_remote`).
        let mut hostile = WhiteboardCrdt::new(9);
        let oversize = CrdtId::from_parts(9, 1);
        let forged = CrdtId::from_parts(9, 2);
        hostile.entries.insert(
            oversize,
            CrdtEntry {
                value: "x".repeat(MAX_CRDT_VALUE_BYTES + 1),
                ts: HlcTimestamp::new(50, 1, 9),
                deleted: false,
            },
        );
        hostile.entries.insert(
            forged,
            CrdtEntry {
                value: "parece válido".to_string(),
                ts: HlcTimestamp::new(u64::MAX, 2, 9),
                deleted: false,
            },
        );
        let mut board = WhiteboardCrdt::new(1);
        board.merge(&hostile, 100);
        assert_eq!(board.len_total(), 0, "valores hostiles jamás entran");
        assert!(board.get(&oversize).is_none());
        assert!(board.get(&forged).is_none());
    }

    #[test]
    fn upsert_remote_rejects_own_site_and_forged_wall() {
        // Regresión A3 (LWW forgeable): `ts.site == self.site` sin autenticar
        // y `ts.wall` arbitrario (`u64::MAX` = gana "para siempre" y deja los
        // objetos locales indeletables: `remove` compara `existing.ts >= ts`).
        let mut board = WhiteboardCrdt::new(1);
        let id = board.insert_local("v", 100).expect("insert");
        assert!(board
            .upsert_remote(id, "forjado", HlcTimestamp::new(150, 5, 1), 150)
            .is_err());
        assert!(board
            .upsert_remote(id, "eterno", HlcTimestamp::new(u64::MAX, 5, 2), 150)
            .is_err());
        assert!(board
            .upsert_remote(id, "legítimo", HlcTimestamp::new(150, 5, 2), 150)
            .is_ok());
        assert_eq!(board.get(&id), Some("legítimo"));
    }

    #[test]
    fn merge_is_commutative_when_the_store_is_full() {
        // Regresión A8: con el almacén casi lleno, el orden de `merge`
        // decidía qué entraba (no conmutativa). Ahora se recorta a los
        // `MAX_CRDT_ENTRIES` más recientes: resultado independiente del orden.
        let mut full = WhiteboardCrdt::new(1);
        for _ in 0..(MAX_CRDT_ENTRIES - 1) {
            full.insert_local("viejo", 1).expect("fill");
        }
        let mut remote_b = WhiteboardCrdt::new(2);
        let id_b = remote_b.insert_local("de-B", 200).expect("b");
        let mut remote_c = WhiteboardCrdt::new(3);
        let id_c = remote_c.insert_local("de-C", 200).expect("c");
        let mut bc = full.clone();
        let mut cb = full.clone();
        bc.merge(&remote_b, 300);
        bc.merge(&remote_c, 300);
        cb.merge(&remote_c, 300);
        cb.merge(&remote_b, 300);
        assert_eq!(bc.live_sorted(), cb.live_sorted());
        assert!(bc.get(&id_b).is_some(), "lo más reciente no se pierde");
        assert!(bc.get(&id_c).is_some(), "lo más reciente no se pierde");
    }
}
