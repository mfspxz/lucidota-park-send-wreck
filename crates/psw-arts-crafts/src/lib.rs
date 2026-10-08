//! psw-004 — Arts & Crafts: the rendering organelle.
//! Turns grid state + selections into SVG maps. The visual layer of the
//! RTS/GIS workbench — what the operator actually looks at. Rendering is
//! pure: same input -> same SVG bytes, always.
use pw_domain::AuthorityTier;
use pw_manifest::{Contract, OrganelleManifest, ReceiptPolicy, Transport, SCHEMA_VERSION};
use pw_organelles::{Organelle, OrganelleError};
use serde_json::Value;

pub struct ArtsCrafts;

#[derive(Debug, Clone)]
pub struct MapEntity {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub quarantined: bool,
    pub selected: bool,
}

impl ArtsCrafts {
    /// Render entities + selection box as SVG. View box 0..100 square.
    pub fn render_map(&self, entities: &[MapEntity], sel_box: Option<(f64, f64, f64, f64)>) -> String {
        let mut s = String::from(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="400" height="400">"##,
        );
        s.push_str(r##"<rect x="0" y="0" width="100" height="100" fill="#0b0e14"/>"##);
        // grid lines
        for i in (0..=100).step_by(10) {
            s.push_str(&format!(
                r##"<line x1="{i}" y1="0" x2="{i}" y2="100" stroke="#1c2333" stroke-width="0.3"/>"##,
            ));
            s.push_str(&format!(
                r##"<line x1="0" y1="{i}" x2="100" y2="{i}" stroke="#1c2333" stroke-width="0.3"/>"##,
            ));
        }
        if let Some((x0, y0, x1, y1)) = sel_box {
            s.push_str(&format!(
                r##"<rect x="{x0}" y="{y0}" width="{}" height="{}" fill="none" stroke="#7dd3fc" stroke-width="0.8" stroke-dasharray="2,1"/>"##,
                x1 - x0,
                y1 - y0
            ));
        }
        for e in entities {
            let fill = if e.quarantined {
                "#f43f5e"
            } else if e.selected {
                "#7dd3fc"
            } else {
                "#94a3b8"
            };
            let stroke = if e.quarantined { "#fecdd3" } else { "none" };
            s.push_str(&format!(
                r##"<circle cx="{}" cy="{}" r="1.6" fill="{fill}" stroke="{stroke}" stroke-width="0.4"><title>{}</title></circle>"##,
                e.x, e.y, e.id.replace('&', "&amp;").replace('<', "&lt;")
            ));
        }
        s.push_str("</svg>");
        s
    }

    pub fn render_receipt_count(&self, count: usize) -> String {
        format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 40"><text x="10" y="25" font-family="monospace" font-size="14" fill="#e2e8f0">receipts: {count}</text></svg>"##)
    }
}

fn manifest() -> OrganelleManifest {
    OrganelleManifest {
        schema: SCHEMA_VERSION.into(),
        name: "psw-arts-crafts".into(),
        version: "0.1.0".into(),
        description: "Rendering organelle: grid state -> SVG maps".into(),
        contracts: vec![
            Contract { name: "render-map".into(), input_schema: "{\"entities\":[...],\"box\":[...]}".into(), output_schema: "{\"svg\":str}".into(), idempotent: true },
            Contract { name: "render-receipts".into(), input_schema: "{\"count\":int}".into(), output_schema: "{\"svg\":str}".into(), idempotent: true },
        ],
        authority: AuthorityTier::Open,
        transports: vec![Transport::InProcess],
        receipts: ReceiptPolicy::OnError,
        entrypoint: "psw_arts_crafts::ArtsCrafts".into(),
    }
}

fn parse_entities(v: &Value) -> Result<Vec<MapEntity>, OrganelleError> {
    let arr = v.get("entities").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    arr.into_iter()
        .map(|e| {
            Ok(MapEntity {
                id: e.get("id").and_then(|x| x.as_str()).ok_or_else(|| OrganelleError::BadInput("entity.id".into()))?.into(),
                x: e.get("x").and_then(|x| x.as_f64()).unwrap_or(0.0),
                y: e.get("y").and_then(|x| x.as_f64()).unwrap_or(0.0),
                quarantined: e.get("quarantined").and_then(|x| x.as_bool()).unwrap_or(false),
                selected: e.get("selected").and_then(|x| x.as_bool()).unwrap_or(false),
            })
        })
        .collect()
}

impl Organelle for ArtsCrafts {
    fn name(&self) -> &'static str {
        "psw-arts-crafts"
    }
    fn manifest(&self) -> OrganelleManifest {
        manifest()
    }
    fn call(&self, contract: &str, input: Value) -> Result<Value, OrganelleError> {
        match contract {
            "render-map" => {
                let entities = parse_entities(&input)?;
                let sel_box = input.get("box").and_then(|b| {
                    let a = b.as_array()?;
                    if a.len() == 4 {
                        Some((a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?, a[3].as_f64()?))
                    } else {
                        None
                    }
                });
                Ok(serde_json::json!({"svg": self.render_map(&entities, sel_box)}))
            }
            "render-receipts" => {
                let count = input.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                Ok(serde_json::json!({"svg": self.render_receipt_count(count)}))
            }
            _ => Err(OrganelleError::UnknownContract(contract.into(), self.name().into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ents() -> Vec<MapEntity> {
        vec![
            MapEntity { id: "e1".into(), x: 10.0, y: 10.0, quarantined: false, selected: true },
            MapEntity { id: "e2".into(), x: 90.0, y: 90.0, quarantined: true, selected: false },
        ]
    }

    #[test]
    fn renders_valid_svg() {
        let a = ArtsCrafts;
        let svg = a.render_map(&ents(), Some((0.0, 0.0, 50.0, 50.0)));
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert!(svg.contains("e1"));
    }

    #[test]
    fn quarantined_entities_marked_red() {
        let a = ArtsCrafts;
        let svg = a.render_map(&ents(), None);
        assert!(svg.contains("#f43f5e"));
    }

    #[test]
    fn selected_entities_highlighted() {
        let a = ArtsCrafts;
        let svg = a.render_map(&ents(), None);
        assert!(svg.contains("#7dd3fc"));
    }

    #[test]
    fn deterministic_render() {
        let a = ArtsCrafts;
        assert_eq!(a.render_map(&ents(), None), a.render_map(&ents(), None));
    }

    #[test]
    fn empty_map_renders() {
        let a = ArtsCrafts;
        let svg = a.render_map(&[], None);
        assert!(svg.contains("<svg"));
        assert!(!svg.contains("<circle"));
    }

    #[test]
    fn id_escaping() {
        let a = ArtsCrafts;
        let svg = a.render_map(&[MapEntity { id: "a<b".into(), x: 1.0, y: 1.0, quarantined: false, selected: false }], None);
        assert!(svg.contains("a&lt;b"));
    }

    #[test]
    fn organelle_render_map_contract() {
        let a = ArtsCrafts;
        let out = a
            .call(
                "render-map",
                serde_json::json!({"entities": [{"id": "e1", "x": 5.0, "y": 5.0}], "box": [0.0, 0.0, 10.0, 10.0]}),
            )
            .unwrap();
        assert!(out["svg"].as_str().unwrap().contains("<svg"));
    }

    #[test]
    fn organelle_render_receipts_contract() {
        let a = ArtsCrafts;
        let out = a.call("render-receipts", serde_json::json!({"count": 42})).unwrap();
        assert!(out["svg"].as_str().unwrap().contains("42"));
    }
}
