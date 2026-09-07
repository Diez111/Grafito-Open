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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::session::ClassroomError;

/// Tope de entradas totales (vivas + tombstones, igual que `MAX_ROSTER_SIZE`).
pub const MAX_CRDT_ENTRIES: usize = 5_000;
/// Tope por valor de pizarra (igual que `MAX_MESSAGE_BYTES`).
pub const MAX_CRDT_VALUE_BYTES: usize = 2_048;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrdtEntry {
    /// Contenido (texto/JSON corto, `<= MAX_CRDT_VALUE_BYTES` bytes).
    pub value: String,
    /// Último escritor (LWW).
    pub ts: HlcTimestamp,
    /// `true` = borrado lógico (tombstone, se conserva para propagar el delete).
    pub deleted: bool,
}

/// Pizarra CRDT en memoria (una réplica local).
///
/// `BTreeMap` para orden determinista (igual que roster). Sin red: dos réplicas
/// se fusionan con [`Self::merge`] en memoria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteboardCrdt {
    site: u16,
    counter: u64,
    entries: BTreeMap<CrdtId, CrdtEntry>,
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
    pub fn upsert_remote(
        &mut self,
        id: CrdtId,
        value: &str,
        ts: HlcTimestamp,
    ) -> Result<bool, ClassroomError> {
        validate_crdt_value(value)?;
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
    /// Conmutativa e idempotente sobre el set vivo (testeado): el orden de
    /// `merge` no cambia el resultado final si los `ts` son fijos.
    /// Si el almacén está lleno, los IDs nuevos se saltean honestamente
    /// (no se pierde lo ya guardado; el conteo solo cuenta aplicados).
    pub fn merge(&mut self, other: &Self) -> usize {
        let mut applied = 0_usize;
        for (id, remote) in &other.entries {
            match self.entries.get(id) {
                Some(local) if local.ts >= remote.ts => {}
                Some(_) => {
                    if let Some(slot) = self.entries.get_mut(id) {
                        *slot = remote.clone();
                        applied = applied.saturating_add(1);
                    }
                }
                None => {
                    if self.entries.len() >= MAX_CRDT_ENTRIES {
                        continue;
                    }
                    self.entries.insert(*id, remote.clone());
                    applied = applied.saturating_add(1);
                }
            }
        }
        // Avanza el contador para que futuros `ts` locales superen a los vistos
        // (HLC mínimo: max local/remoto por wall igual). Sin reloj perfecto,
        // basta con no retroceder: si el remoto trae `counter` mayor con el
        // mismo `wall`, lo adoptamos para evitar empates eternos.
        for remote in other.entries.values() {
            if remote.ts.site == self.site && remote.ts.counter > self.counter {
                self.counter = remote.ts.counter;
            }
        }
        applied
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
            .upsert_remote(id, "viejo", old_ts)
            .expect("upsert viejo");
        assert!(!applied_old);
        assert_eq!(board.get(&id), Some("v1"));
        let new_ts = HlcTimestamp::new(200, 99, 2);
        let applied_new = board.upsert_remote(id, "v2", new_ts).expect("upsert nuevo");
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
        assert!(!board.upsert_remote(id, "revive", stale).expect("stale"));
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
        let n1 = ab.merge(&b);
        let n2 = ba.merge(&a);
        assert_eq!(n1, 1);
        assert_eq!(n2, 1);
        assert_eq!(ab.live_sorted(), ba.live_sorted());
        assert!(ab.get(&id_a).is_some());
        assert!(ab.get(&id_b).is_some());
        // Idempotente: segunda fusión no aplica nada.
        assert_eq!(ab.merge(&b), 0);
        assert_eq!(ba.merge(&a), 0);
    }

    #[test]
    fn merge_lww_conflict_resolves_to_newest() {
        let mut a = WhiteboardCrdt::new(1);
        let id = a.insert_local("base", 100).expect("base");
        let b = a.clone();
        // Misma réplica lógica, dos escrituras con ts distintos.
        let ts_old = HlcTimestamp::new(150, 10, 1);
        let ts_new = HlcTimestamp::new(160, 11, 1);
        // Simula divergencia: cada clon recibe una escritura distinta.
        let mut c1 = a.clone();
        let mut c2 = b.clone();
        c1.upsert_remote(id, "viejo", ts_old).expect("viejo");
        c2.upsert_remote(id, "nuevo", ts_new).expect("nuevo");
        c1.merge(&c2);
        assert_eq!(c1.get(&id), Some("nuevo"));
        // Y al revés también converge al más nuevo.
        let mut c3 = b.clone();
        c3.upsert_remote(id, "viejo", ts_old).expect("viejo");
        let mut c4 = a.clone();
        c4.upsert_remote(id, "nuevo", ts_new).expect("nuevo");
        c4.merge(&c3);
        assert_eq!(c4.get(&id), Some("nuevo"));
    }

    #[test]
    fn value_validation_and_storage_full_are_honest() {
        let mut board = WhiteboardCrdt::new(0);
        let big = "x".repeat(MAX_CRDT_VALUE_BYTES + 1);
        assert!(board.insert_local(&big, 1).is_err());
        assert!(board.insert_local("a\x00b", 1).is_err());
        assert!(board
            .upsert_remote(CrdtId::from_parts(1, 1), &big, HlcTimestamp::new(1, 1, 1))
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
}
