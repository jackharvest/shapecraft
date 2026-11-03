//! Keeps sketch constraints true after every command: renumbers them when elements come and go, and re-solves
//! the sketch when an element was changed (the changed elements hold still; the others follow).

use std::collections::BTreeSet;

use shapecraft_doc::constraints;
use shapecraft_doc::{Document, SketchEntity};

use crate::{EngineError, Result, Session};

/// For each old element, where it went in the new list (matching unchanged elements in order).
fn match_elements(old: &[SketchEntity], new: &[SketchEntity]) -> Vec<Option<u32>> {
    let mut used = vec![false; new.len()];
    old.iter()
        .map(|e| {
            let j = new.iter().enumerate().position(|(j, n)| !used.get(j).copied().unwrap_or(true) && n == e)?;
            if let Some(u) = used.get_mut(j) {
                *u = true;
            }
            Some(j as u32)
        })
        .collect()
}

impl Session {
    /// Marks what a drawing command added to a sketch as construction geometry ("construction": true).
    pub(crate) fn mark_construction(&mut self, before: &Document) {
        for sk in &mut self.doc.sketches {
            let old = before.sketches.iter().find(|o| o.id == sk.id).map_or(0, |o| o.entities.len());
            for i in old..sk.entities.len() {
                sk.construction.insert(i as u32);
            }
        }
    }

    pub(crate) fn keep_constraints(&mut self, before: &Document) -> Result<()> {
        self.keep_constraints_only(before)?;
        self.keep_patterns(before);
        Ok(())
    }

    /// Linked sketch patterns: renumbered when curves come and go (dropped when one of theirs is gone), and their
    /// copies remade from the source curves after every change.
    fn keep_patterns(&mut self, before: &Document) {
        for sk in &mut self.doc.sketches {
            if sk.patterns.is_empty() {
                continue;
            }
            let old = before.sketches.iter().find(|o| o.id == sk.id);
            if let Some(old) = old.filter(|o| o.entities.len() != sk.entities.len() && o.patterns == sk.patterns) {
                let map = match_elements(&old.entities, &sk.entities);
                let f = |e: &u32| map.get(*e as usize).copied().flatten();
                sk.patterns = std::mem::take(&mut sk.patterns)
                    .into_iter()
                    .filter_map(|mut pt| {
                        pt.source = pt.source.iter().map(f).collect::<Option<_>>()?;
                        pt.copies = pt.copies.iter().map(|c| c.iter().map(f).collect::<Option<_>>()).collect::<Option<_>>()?;
                        Some(pt)
                    })
                    .collect();
            }
            if old.is_some_and(|o| o.entities == sk.entities) {
                continue;
            }
            let mut patterns = std::mem::take(&mut sk.patterns);
            patterns.retain(|pt| {
                let mut k = 0;
                for m in &pt.maps {
                    let m = glam::DAffine2::from_cols_array(m);
                    for &src in &pt.source {
                        let (Some(e), Some(slots)) = (sk.entities.get(src as usize), pt.copies.get(k)) else { return false };
                        let made = shapecraft_doc::sketch_ops::transform(e, m);
                        // A source curve that now makes a different number of pieces unlinks the pattern.
                        if made.len() != slots.len() || slots.iter().any(|&i| i as usize >= sk.entities.len()) {
                            return false;
                        }
                        for (&i, piece) in slots.iter().zip(made) {
                            sk.entities[i as usize] = piece;
                        }
                        k += 1;
                    }
                }
                true
            });
            sk.patterns = patterns;
        }
    }

    fn keep_constraints_only(&mut self, before: &Document) -> Result<()> {
        for sk in &mut self.doc.sketches {
            if sk.constraints.is_empty() && sk.construction.is_empty() {
                continue;
            }
            let Some(old) = before.sketches.iter().find(|o| o.id == sk.id) else { continue };
            if old.entities == sk.entities {
                continue;
            }
            if old.entities.len() != sk.entities.len() {
                // Elements were added, removed or split: follow the unchanged ones, drop what depended on the rest.
                // (Constraints added by this command already use the new numbering.)
                if old.constraints == sk.constraints {
                    let map = match_elements(&old.entities, &sk.entities);
                    let f = |e: u32| map.get(e as usize).copied().flatten();
                    sk.constraints = old.constraints.iter().filter_map(|c| c.remapped(&f)).collect();
                    // Keep what this command itself marked: new elements nothing old maps onto.
                    let images: BTreeSet<u32> = map.iter().flatten().copied().collect();
                    let added: Vec<u32> =
                        sk.construction.iter().copied().filter(|&i| i as usize >= old.entities.len() && !images.contains(&i)).collect();
                    sk.construction = old.construction.iter().filter_map(|&e| f(e)).chain(added).collect();
                }
                continue;
            }
            // Same elements, some reshaped: those stay as the user made them and the rest follow.
            let pinned: BTreeSet<u32> =
                old.entities.iter().zip(&sk.entities).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i as u32).collect();
            // If the changed elements can't hold still (they're constrained themselves), let everything move a little.
            match constraints::solve(&sk.entities, &sk.constraints, &pinned)
                .or_else(|| constraints::solve(&sk.entities, &sk.constraints, &BTreeSet::new()))
            {
                Some(solved) => sk.entities = solved,
                None => return Err(EngineError::NotAvailable("that change breaks the sketch's constraints".into())),
            }
        }
        Ok(())
    }
}
