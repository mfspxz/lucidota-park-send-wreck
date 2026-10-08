//! psw-007 — Observatory: telemetry for the PSW.
//! Append-only event stream. Every event carries a timestamp, kind, source,
//! and payload. Queries filter by kind and recency. The observatory never
//! mutates history — it only appends and reads.
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub ts_utc: String,
    pub kind: String,
    pub source: String,
    pub payload: Value,
}

pub struct Observatory {
    events: Mutex<Vec<Event>>,
}

impl Observatory {
    pub fn new() -> Self {
        Observatory { events: Mutex::new(Vec::new()) }
    }

    pub fn emit(&self, kind: &str, source: &str, payload: Value) -> Event {
        let mut ev = self.events.lock().unwrap();
        let seq = ev.len() as u64 + 1;
        let e = Event { seq, ts_utc: now_utc(), kind: kind.into(), source: source.into(), payload };
        ev.push(e.clone());
        e
    }

    pub fn query(&self, kind: Option<&str>, since_seq: u64, limit: usize) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.seq > since_seq && kind.map(|k| e.kind == k).unwrap_or(true))
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn tail(&self, n: usize) -> Vec<Event> {
        let ev = self.events.lock().unwrap();
        ev.iter().rev().take(n).cloned().collect::<Vec<_>>().into_iter().rev().collect()
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

impl Default for Observatory {
    fn default() -> Self {
        Observatory::new()
    }
}

fn now_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-observatory".into(),
        version: "0.1.0".into(),
        description: "Append-only telemetry stream: emit, query, tail".into(),
        contracts: vec![
            Contract { name: "emit".into(), input_schema: "{\"kind\":str,\"source\":str,\"payload\":json}".into(), output_schema: "Event".into(), idempotent: false },
            Contract { name: "query".into(), input_schema: "{\"kind\":str?,\"since_seq\":int,\"limit\":int}".into(), output_schema: "[Event]".into(), idempotent: true },
            Contract { name: "tail".into(), input_schema: "{\"n\":int}".into(), output_schema: "[Event]".into(), idempotent: true },
        ],
        authority: AuthorityTier::Open,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::OnError,
        entrypoint: "psw_observatory::Observatory".into(),
    }
}

impl Organelle for Observatory {
    fn name(&self) -> &'static str {
        "psw-observatory"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "emit" => {
                let kind = input.get("kind").and_then(|v| v.as_str()).ok_or_else(|| OrganelleError::BadInput("kind".into()))?;
                let source = input.get("source").and_then(|v| v.as_str()).unwrap_or("unknown");
                let payload = input.get("payload").cloned().unwrap_or(Value::Null);
                Ok(serde_json::to_value(self.emit(kind, source, payload)).unwrap())
            }
            "query" => {
                let kind = input.get("kind").and_then(|v| v.as_str());
                let since = input.get("since_seq").and_then(|v| v.as_u64()).unwrap_or(0);
                let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
                Ok(serde_json::to_value(self.query(kind, since, limit)).unwrap())
            }
            "tail" => {
                let n = input.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                Ok(serde_json::to_value(self.tail(n)).unwrap())
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_increments() {
        let o = Observatory::new();
        let e1 = o.emit("a", "s", Value::Null);
        let e2 = o.emit("a", "s", Value::Null);
        assert_eq!(e1.seq + 1, e2.seq);
    }

    #[test]
    fn query_filters_kind() {
        let o = Observatory::new();
        o.emit("alpha", "s", Value::Null);
        o.emit("beta", "s", Value::Null);
        o.emit("alpha", "s", Value::Null);
        assert_eq!(o.query(Some("alpha"), 0, 100).len(), 2);
        assert_eq!(o.query(Some("beta"), 0, 100).len(), 1);
        assert_eq!(o.query(None, 0, 100).len(), 3);
    }

    #[test]
    fn query_since_seq() {
        let o = Observatory::new();
        o.emit("a", "s", Value::Null);
        o.emit("a", "s", Value::Null);
        let res = o.query(None, 1, 100);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].seq, 2);
    }

    #[test]
    fn query_limit() {
        let o = Observatory::new();
        for _ in 0..5 {
            o.emit("a", "s", Value::Null);
        }
        assert_eq!(o.query(None, 0, 2).len(), 2);
    }

    #[test]
    fn tail_returns_newest() {
        let o = Observatory::new();
        for _ in 0..5 {
            o.emit("a", "s", Value::Null);
        }
        let t = o.tail(2);
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].seq, 5);
    }

    #[test]
    fn history_append_only() {
        let o = Observatory::new();
        o.emit("a", "s", serde_json::json!({"v": 1}));
        assert_eq!(o.count(), 1);
        assert_eq!(o.query(None, 0, 10)[0].payload["v"], 1);
    }

    #[test]
    fn organelle_emit_query() {
        let o = Observatory::new();
        o.call("emit", serde_json::json!({"kind": "k", "source": "s", "payload": {"x": 1}})).unwrap();
        let out = o.call("query", serde_json::json!({"kind": "k", "since_seq": 0, "limit": 10})).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
    }
}
