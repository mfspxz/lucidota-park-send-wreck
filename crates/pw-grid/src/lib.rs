//! parkwreck-grid — proven crate name from the Aug 2026 tree.
//! The RTS/GIS interaction surface: entities on a 2D grid, box selection.
//!
//! SEL-BOX-ADD (named failure, ledger seq 265): the box-select-add
//! interaction. The original implementation added every entity under the
//! selection box to the operator's selection set — including quarantined
//! and out-of-authority entities, and duplicating entries on repeat adds.
//! Original semantics: UNVERIFIED (name only, from Zackary's pasted records).
//! This crate reconstructs the failure class faithfully:
//!   - `add_unfiltered` preserves the historical (buggy) behavior,
//!     documented and tested as the reproduction;
//!   - `add` is the fix: authority-filtered, quarantine-aware, deduplicated.
//! Trinary: RECONSTRUCTION, not a byte-faithful restore.
mod organelle;
pub use organelle::*;
use pw_domain::{AuthorityTier, EntityId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// An entity placed on the grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridEntity {
    pub id: EntityId,
    pub x: f64,
    pub y: f64,
    /// Minimum authority tier required to interact with this entity.
    pub tier: AuthorityTier,
    /// Quarantined entities must never enter an operator selection.
    pub quarantined: bool,
}

/// The spatial index. v1: linear scan (correctness first; grid buckets later).
#[derive(Debug, Default)]
pub struct Grid {
    entities: Vec<GridEntity>,
}

impl Grid {
    pub fn new() -> Self {
        Grid::default()
    }

    pub fn insert(&mut self, e: GridEntity) {
        if let Some(slot) = self.entities.iter_mut().find(|x| x.id == e.id) {
            *slot = e;
        } else {
            self.entities.push(e);
        }
    }

    pub fn get(&self, id: &EntityId) -> Option<&GridEntity> {
        self.entities.iter().find(|e| &e.id == id)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// All entities whose position falls inside the box (inclusive).
    pub fn query_box(&self, b: &SelectionBox) -> Vec<&GridEntity> {
        self.entities.iter().filter(|e| b.contains(e.x, e.y)).collect()
    }
}

/// Axis-aligned selection box from the RTS/GIS interaction layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SelectionBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl SelectionBox {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        SelectionBox {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    pub fn area(&self) -> f64 {
        (self.x1 - self.x0) * (self.y1 - self.y0)
    }

    /// HISTORICAL (buggy) behavior — the SEL-BOX-ADD failure, preserved for
    /// the record. Adds every entity under the box: no dedup, no authority
    /// filter, quarantined entities included.
    pub fn add_unfiltered(&self, grid: &Grid, sel: &mut Selection) {
        for e in grid.query_box(self) {
            sel.set.insert(e.id.clone());
            sel.ids.push(e.id.clone());
        }
    }

    /// FIXED behavior. Adds entities under the box that the caller may
    /// interact with: skips quarantined entities, skips entities above the
    /// caller's authority tier, never duplicates.
    pub fn add(&self, grid: &Grid, sel: &mut Selection, authority: AuthorityTier) -> AddedReport {
        let mut report = AddedReport::default();
        for e in grid.query_box(self) {
            if e.quarantined {
                report.skipped_quarantined += 1;
                continue;
            }
            if !authority.satisfies(e.tier) {
                report.skipped_authority += 1;
                continue;
            }
            if !sel.set.insert(e.id.clone()) {
                report.duplicates += 1;
                continue;
            }
            sel.ids.push(e.id.clone());
            report.added += 1;
        }
        report
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AddedReport {
    pub added: usize,
    pub skipped_quarantined: usize,
    pub skipped_authority: usize,
    pub duplicates: usize,
}

/// The operator's selection set: ordered, deduplicated.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Selection {
    ids: Vec<EntityId>,
    #[serde(skip)]
    set: HashSet<EntityId>,
}

impl Selection {
    pub fn new() -> Self {
        Selection::default()
    }

    pub fn contains(&self, id: &EntityId) -> bool {
        self.set.contains(id)
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn ids(&self) -> &[EntityId] {
        &self.ids
    }

    pub fn clear(&mut self) {
        self.ids.clear();
        self.set.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Grid {
        let mut g = Grid::new();
        g.insert(GridEntity { id: EntityId::new("e1"), x: 1.0, y: 1.0, tier: AuthorityTier::Open, quarantined: false });
        g.insert(GridEntity { id: EntityId::new("e2"), x: 2.0, y: 2.0, tier: AuthorityTier::Open, quarantined: false });
        g.insert(GridEntity { id: EntityId::new("e3"), x: 3.0, y: 3.0, tier: AuthorityTier::Admin, quarantined: false });
        g.insert(GridEntity { id: EntityId::new("e4"), x: 4.0, y: 4.0, tier: AuthorityTier::Open, quarantined: true });
        g.insert(GridEntity { id: EntityId::new("far"), x: 100.0, y: 100.0, tier: AuthorityTier::Open, quarantined: false });
        g
    }

    fn big_box() -> SelectionBox {
        SelectionBox::new(0.0, 0.0, 10.0, 10.0)
    }

    #[test]
    fn insert_and_get() {
        let g = fixture();
        assert_eq!(g.len(), 5);
        assert!(g.get(&EntityId::new("e1")).is_some());
        assert!(g.get(&EntityId::new("nope")).is_none());
    }

    #[test]
    fn insert_replaces_same_id() {
        let mut g = Grid::new();
        g.insert(GridEntity { id: EntityId::new("a"), x: 0.0, y: 0.0, tier: AuthorityTier::Open, quarantined: false });
        g.insert(GridEntity { id: EntityId::new("a"), x: 9.0, y: 9.0, tier: AuthorityTier::Open, quarantined: false });
        assert_eq!(g.len(), 1);
        assert_eq!(g.get(&EntityId::new("a")).unwrap().x, 9.0);
    }

    #[test]
    fn box_query_respects_bounds() {
        let g = fixture();
        let hits = g.query_box(&big_box());
        assert_eq!(hits.len(), 4); // e1..e4, not "far"
        let tiny = SelectionBox::new(0.5, 0.5, 1.5, 1.5);
        let hits = g.query_box(&tiny);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.0, "e1");
    }

    #[test]
    fn box_normalizes_corners() {
        let b = SelectionBox::new(10.0, 10.0, 0.0, 0.0);
        assert!(b.contains(5.0, 5.0));
        assert_eq!(b.area(), 100.0);
    }

    #[test]
    fn sel_box_add_bug_repro_unfiltered_includes_quarantined() {
        // REPRODUCTION of SEL-BOX-ADD: the historical path adds the
        // quarantined entity e4. This test documents the failure.
        let g = fixture();
        let mut sel = Selection::new();
        big_box().add_unfiltered(&g, &mut sel);
        assert!(sel.contains(&EntityId::new("e4")), "bug: quarantined entity added");
    }

    #[test]
    fn sel_box_add_bug_repro_unfiltered_duplicates() {
        // REPRODUCTION: repeat adds duplicate entries.
        let g = fixture();
        let mut sel = Selection::new();
        big_box().add_unfiltered(&g, &mut sel);
        big_box().add_unfiltered(&g, &mut sel);
        assert_eq!(sel.len(), 8, "bug: duplicates on repeat add");
    }

    #[test]
    fn sel_box_add_bug_repro_unfiltered_ignores_authority() {
        // REPRODUCTION: Admin-tier e3 added by an Open caller.
        let g = fixture();
        let mut sel = Selection::new();
        big_box().add_unfiltered(&g, &mut sel);
        assert!(sel.contains(&EntityId::new("e3")), "bug: out-of-authority entity added");
    }

    #[test]
    fn sel_box_add_fixed_skips_quarantined() {
        let g = fixture();
        let mut sel = Selection::new();
        let r = big_box().add(&g, &mut sel, AuthorityTier::Admin);
        assert!(!sel.contains(&EntityId::new("e4")));
        assert_eq!(r.skipped_quarantined, 1);
    }

    #[test]
    fn sel_box_add_fixed_enforces_authority() {
        let g = fixture();
        let mut sel = Selection::new();
        let r = big_box().add(&g, &mut sel, AuthorityTier::Operator);
        assert!(!sel.contains(&EntityId::new("e3")), "Admin entity must not be added by Operator");
        assert_eq!(r.skipped_authority, 1);
        assert!(sel.contains(&EntityId::new("e1")));
        assert!(sel.contains(&EntityId::new("e2")));
    }

    #[test]
    fn sel_box_add_fixed_dedups() {
        let g = fixture();
        let mut sel = Selection::new();
        let r1 = big_box().add(&g, &mut sel, AuthorityTier::Admin);
        let r2 = big_box().add(&g, &mut sel, AuthorityTier::Admin);
        assert_eq!(r1.added, 3); // e1, e2, e3 (e4 quarantined)
        assert_eq!(r2.added, 0);
        assert_eq!(r2.duplicates, 3);
        assert_eq!(sel.len(), 3);
    }

    #[test]
    fn sel_box_add_fixed_empty_box() {
        let g = fixture();
        let mut sel = Selection::new();
        let r = SelectionBox::new(50.0, 50.0, 60.0, 60.0).add(&g, &mut sel, AuthorityTier::Admin);
        assert_eq!(r.added, 0);
        assert!(sel.is_empty());
    }

    #[test]
    fn selection_clear() {
        let g = fixture();
        let mut sel = Selection::new();
        big_box().add(&g, &mut sel, AuthorityTier::Admin);
        sel.clear();
        assert!(sel.is_empty());
    }
}
