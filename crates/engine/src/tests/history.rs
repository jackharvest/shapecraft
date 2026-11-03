//! Undo and redo, failures that leave the document alone, and the parametric history: editing, deleting,
//! suppressing and merging steps.

use serde_json::json;
use shapecraft_doc::Pick;

use super::{session, top_face, volume};
use crate::{EngineError, Session};

#[test]
fn undo_redo_walks_history() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 5})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 2})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.bodies.is_empty());
    assert!(s.active_sketch.is_some(), "undo returns to the sketch");
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.doc.bodies.len(), 1);
    assert_eq!(s.doc.history.len(), 2, "the sketch and its circle are one step");
}

#[test]
fn failures_leave_the_document_alone() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let id = s.doc.bodies[0].id;
    let history = s.doc.history.len();
    assert!(s.execute("fillet", &json!({"body": id.0, "radius": 100})).is_err());
    assert_eq!(s.doc.history.len(), history);
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    assert!(matches!(s.execute("nope", &json!({})), Err(EngineError::UnknownCommand(_))));
    assert!(matches!(s.execute("line", &json!({"a": [0, 0], "b": [1, 1]})), Err(EngineError::NotAvailable(_))));
}

#[test]
fn hostile_params_never_panic() {
    let mut s = session();
    let junk = [
        json!(null),
        json!([]),
        json!({"distance": "x"}),
        json!({"corner": [1e308, -1e308], "opposite": [f64::MAX, 0]}),
        json!({"picks": [{"kind": "face", "body": 99, "face": 4294967295u64}]}),
        json!({"plane": "zz"}),
        json!({"path": ""}),
        json!({"items": [1, 2, 3], "by": [1, 2]}),
        json!({"radius": -1, "edges": [1, 2]}),
    ];
    for c in crate::registry() {
        for j in &junk {
            let _ = s.execute(c.id, j);
        }
    }
}

#[test]
fn preview_changes_nothing() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let history = s.doc.history.len();
    let doc = s.preview("extrude", &json!({"sketch": 1, "region": 0, "distance": 3})).unwrap();
    assert_eq!(doc.bodies.len(), 1);
    assert!(s.doc.bodies.is_empty());
    assert_eq!(s.doc.history.len(), history);
    assert!(s.active_sketch.is_some());
    assert!(s.preview("extrude", &json!({"sketch": 1, "region": 0, "distance": 0})).is_err());
}

#[test]
fn parametric_history_rebuilds_from_edited_steps() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("select", &json!({"picks": [{"kind": "region", "sketch": sketch.0, "region": 0}]})).unwrap();
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    s.execute("extrude", &json!({"distance": 5})).unwrap();
    s.execute("rename", &json!({"item": s.doc.bodies[0].id.0, "name": "Block"})).unwrap();
    let labels: Vec<&str> = s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["Sketch 01", "Extrude 01", "Extrude 02"]);
    assert_eq!(crate::history::profile_count(&s.doc.history[1]), Some(1));

    // A longer first extrusion: the second one (on the cap face) still follows.
    s.execute("edit_step", &json!({"step": 1, "params": {"distance": 8}})).unwrap();
    assert!((volume(&mut s) - 2600.0).abs() < 1e-6);
    assert_eq!(s.doc.bodies[0].name, "Block", "quiet steps replay too");
    assert_eq!(s.doc.history.len(), 4);

    // A step that can't be rebuilt changes nothing.
    assert!(s.execute("edit_step", &json!({"step": 1, "params": {"distance": 0}})).is_err());
    assert!((volume(&mut s) - 2600.0).abs() < 1e-6);

    s.execute("undo", &json!({})).unwrap();
    assert!((volume(&mut s) - 2000.0).abs() < 1e-6);

    // The history survives a save and load and can still be rebuilt.
    let dir = std::env::temp_dir().join(format!("shapecraft-history-{}", std::process::id()));
    s.execute("save", &json!({"path": dir.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(s.kernel.as_ref(), &dir).unwrap();
    let mut t = Session::with_document(s.kernel.clone(), doc, Some(dir.clone()));
    t.execute("edit_step", &json!({"step": 2, "params": {"distance": 1}})).unwrap();
    assert!((volume(&mut t) - 1200.0).abs() < 1e-6);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn editing_a_sketch_in_the_past_rebuilds_later_steps() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sketch = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sketch.0, "region": 0, "distance": 5})).unwrap();
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);

    s.execute("edit_sketch_step", &json!({"step": 0})).unwrap();
    assert_eq!(s.active_sketch, Some(sketch));
    assert!(s.doc.bodies.is_empty(), "back to just after the sketch");
    assert_eq!(s.doc.future.len(), 1);
    s.execute("circle", &json!({"center": [10, 5], "radius": 2})).unwrap();
    assert_eq!(s.doc.history.len(), 1, "the new curve joins the sketch step");
    s.execute("exit_sketching", &json!({})).unwrap();
    assert!(s.doc.future.is_empty());
    assert_eq!(s.doc.bodies.len(), 1, "the extrusion came back");
    assert!((volume(&mut s) - 1000.0).abs() > 1e-6, "and follows the edited sketch");
    let labels: Vec<&str> = s.doc.history.iter().filter(|h| !h.quiet).map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["Sketch 01", "Extrude 01"]);

    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.future.len(), 1, "undo goes back into the sketch");
    assert_eq!(s.active_sketch, Some(sketch));
}

#[test]
fn deleting_a_history_step_rebuilds_without_it() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 5})).unwrap();
    s.execute("fillet", &json!({"radius": 1, "body": s.doc.bodies[0].id.0})).unwrap();
    let filleted = volume(&mut s);
    assert!(filleted < 1000.0);
    // Without the fillet: the plain block.
    s.execute("delete_step", &json!({"step": 2})).unwrap();
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    assert_eq!(s.doc.history.len(), 2);
    // Without the sketch the extrusion can't be rebuilt: it stays in the History marked as failing, as the
    // reference marks steps that lost what they were built on, and undo brings everything back.
    s.execute("delete_step", &json!({"step": 0})).unwrap();
    assert_eq!(s.doc.history.len(), 1);
    assert!(s.doc.history[0].error.is_some());
    assert!(s.doc.bodies.is_empty());
    s.execute("undo", &json!({})).unwrap();
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    s.execute("undo", &json!({})).unwrap();
    assert!((volume(&mut s) - filleted).abs() < 1e-6);
}

#[test]
fn variables_drive_history_steps() {
    let mut s = session();
    s.execute("set_variable", &json!({"name": "depth", "value": "2 cm"})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": "depth / 2"})).unwrap();
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    // Changing the variable rebuilds the extrusion.
    s.execute("set_variable", &json!({"name": "depth", "value": 40})).unwrap();
    assert!((volume(&mut s) - 2000.0).abs() < 1e-6);
    // Renaming breaks the step that uses the old name, so it's refused; so is deleting it.
    assert!(s.execute("set_variable", &json!({"name": "depth", "value": 40, "rename": "d"})).is_err());
    assert!(s.execute("delete_variable", &json!({"name": "depth"})).is_err());
    assert!(s.execute("set_variable", &json!({"name": "2bad", "value": 1})).is_err());
    assert!(s.execute("set_variable", &json!({"name": "x", "value": "x + 1"})).is_err());
    // History edits can use variables too.
    s.execute("set_variable", &json!({"name": "tall", "value": "depth * 1.5"})).unwrap();
    s.execute("edit_step", &json!({"step": 1, "params": {"distance": "tall"}})).unwrap();
    assert!((volume(&mut s) - 6000.0).abs() < 1e-6);
    s.execute("undo", &json!({})).unwrap();
    assert!((volume(&mut s) - 2000.0).abs() < 1e-6);
}

#[test]
fn amend_replaces_the_last_step() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"center": [0, 0], "size": [4, 5]})).unwrap();
    let sk = s.active_sketch.unwrap().0;
    s.execute("exit_sketching", &json!({})).unwrap();
    let pull = |d: f64| json!({"sketch": sk, "region": 0, "distance": d});
    s.execute("extrude", &pull(2.0)).unwrap();
    // Pulled again: the same step, now 3 mm; the preview shows it without changing anything.
    let shown = s.preview_amend("extrude", &pull(1.0)).unwrap();
    assert_eq!(shown.bodies.len(), 1);
    assert!((volume(&mut s) - 40.0).abs() < 1e-6);
    let steps = s.doc.history.len();
    s.amend("extrude", &pull(3.0)).unwrap();
    assert_eq!(s.doc.history.len(), steps);
    assert!((volume(&mut s) - 60.0).abs() < 1e-6);
    // A failing amend leaves the step alone; one undo takes the whole pull back.
    assert!(s.amend("extrude", &pull(f64::NAN)).is_err());
    assert!((volume(&mut s) - 60.0).abs() < 1e-6);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.doc.bodies.is_empty());
    // Dragged back to nothing: the step goes without a redo.
    s.execute("extrude", &pull(2.0)).unwrap();
    assert!(s.drop_last_step());
    assert!(s.doc.bodies.is_empty());
}

#[test]
fn undo_steps_back_selections() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = shapecraft_doc::ItemId(out.value["body"].as_u64().unwrap());
    let cap = s.selection.clone();
    assert!(!cap.is_empty(), "the extrusion leaves its cap selected");
    // Selecting the body (a selection-only step), then undoing it brings the cap back.
    let before = s.selection.clone();
    s.selection = vec![Pick::Body { body }];
    s.record_selection(before, s.active_sketch);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.selection, cap);
    s.execute("redo", &json!({})).unwrap();
    assert_eq!(s.selection, vec![Pick::Body { body }]);
    // Undoing a model step brings back what was selected before it.
    let face = Pick::Face { body, face: 0 };
    s.selection = vec![face];
    s.record_selection(vec![Pick::Body { body }], s.active_sketch);
    s.execute("fillet", &json!({"body": body.0, "edges": [0], "radius": 1})).unwrap();
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.selection, vec![face]);
}

#[test]
fn history_steps_related_to_a_body() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [30, 0], "opposite": [40, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 5})).unwrap();
    let second = shapecraft_doc::ItemId(out.value["body"].as_u64().unwrap());
    let first = s.doc.bodies.iter().find(|b| b.id != second).unwrap().id;
    s.execute("fillet", &json!({"body": first.0, "edges": [0], "radius": 1})).unwrap();
    let labels = |rel: Vec<bool>, s: &crate::Session| {
        s.doc.history.iter().zip(rel).filter(|(_, r)| *r).map(|(h, _)| h.label.clone()).collect::<Vec<_>>()
    };
    assert_eq!(labels(s.related_steps(&[second], &[]).unwrap(), &s), ["Sketch 02", "Extrude 02"]);
    assert_eq!(labels(s.related_steps(&[first], &[]).unwrap(), &s), ["Sketch 01", "Extrude 01", "Fillet 01"]);
}

#[test]
fn undo_after_leaving_a_sketch_goes_back_into_it() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let sketch = s.active_sketch;
    let selection = s.selection.clone();
    s.execute("exit_sketching", &json!({})).unwrap();
    s.selection.clear();
    // What the workspace records when the sketch is left with no command changing the document.
    s.record_selection(selection.clone(), sketch);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.active_sketch, sketch);
    assert_eq!(s.selection, selection);
    assert_eq!(s.active_sketch().map(|k| k.entities.len()), Some(4), "the rectangle is still there");
}

#[test]
fn merged_steps_replay_and_undo() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [10, 10]})).unwrap();
    let out = s.execute("extrude", &json!({"sketch": s.active_sketch.unwrap().0, "region": 0, "distance": 10})).unwrap();
    let body = out.value["body"].as_u64().unwrap();
    assert!(s.execute("merge_steps", &json!({})).unwrap().changed);
    assert_eq!(s.doc.history.len(), 1);
    assert_eq!(s.doc.history[0].label, "Merged Steps");
    assert!(s.doc.sketches.is_empty(), "sketches go by default");
    assert!((volume(&mut s) - 1000.0).abs() < 1e-3);
    // Later steps build on the merged model, and editing them replays through it.
    let top = top_face(&mut s);
    s.execute("extrude", &json!({"body": body, "face": top, "distance": 5})).unwrap();
    assert!((volume(&mut s) - 1500.0).abs() < 1e-3);
    s.execute("edit_step", &json!({"step": 1, "params": {"distance": 2}})).unwrap();
    assert!((volume(&mut s) - 1200.0).abs() < 1e-3, "{}", volume(&mut s));
    assert_eq!(s.doc.bodies[0].id.0, body, "ids survive the merge");
    s.execute("undo", &json!({})).unwrap();
    s.execute("undo", &json!({})).unwrap();
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(s.doc.history.len(), 2, "undo brings the steps back");
    assert_eq!(s.doc.sketches.len(), 1);
}

#[test]
fn suppress_breakpoint_and_failing_steps() {
    let mut s = session();
    // Steps: 0 sketch A, 1 extrude A, 2 fillet A, 3 sketch B (apart), 4 extrude B.
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [0, 0], "opposite": [20, 10]})).unwrap();
    let a = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": a.0, "region": 0, "distance": 5})).unwrap();
    let body_a = s.doc.bodies[0].id;
    s.execute("fillet", &json!({"radius": 1, "body": body_a.0})).unwrap();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("rectangle", &json!({"corner": [50, 0], "opposite": [60, 10]})).unwrap();
    let b = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": b.0, "region": 0, "distance": 10})).unwrap();
    assert_eq!(s.doc.history.len(), 5);
    let full = volume(&mut s);

    // Suppressing the first extrusion: the fillet on its body fails, the other body still builds (ids stay put).
    s.execute("suppress_step", &json!({"step": 1})).unwrap();
    assert!(s.doc.history[1].suppressed);
    assert!(s.doc.history[2].error.is_some(), "the fillet lost its body");
    assert!(s.doc.history[4].error.is_none(), "{:?}", s.doc.history[4].error);
    assert_eq!(s.doc.bodies.len(), 1);
    assert!((volume(&mut s) - 1000.0).abs() < 1e-6);
    // Back again: everything works.
    s.execute("suppress_step", &json!({"step": 1})).unwrap();
    assert!(s.doc.history.iter().all(|h| h.error.is_none() && !h.suppressed));
    assert!((volume(&mut s) - full).abs() < 1e-6);

    // A breakpoint after the fillet: the second body waits; a new step goes in before the breakpoint.
    s.execute("set_breakpoint", &json!({"before": 3})).unwrap();
    assert!(s.at_breakpoint() && !s.editing_past());
    assert_eq!(s.doc.history.len(), 3);
    assert_eq!(s.doc.bodies.len(), 1);
    s.execute("rename", &json!({"item": body_a.0, "name": "Part"})).unwrap();
    assert_eq!(s.doc.history.len(), 4, "the rename is logged before the breakpoint");
    s.execute("rename_step", &json!({"step": 1, "name": "Base"})).unwrap();
    assert_eq!(s.doc.history[1].label, "Base");
    assert!(s.execute("rename_step", &json!({"step": 1, "name": "  "})).is_err());
    // Removing it brings the held steps back.
    s.execute("set_breakpoint", &json!({})).unwrap();
    assert!(s.doc.future.is_empty());
    assert_eq!(s.doc.history.len(), 6);
    assert!((volume(&mut s) - full).abs() < 1e-6);
    s.execute("undo", &json!({})).unwrap();
    assert!(s.at_breakpoint());
}

#[test]
fn a_steps_profile_can_be_picked_again() {
    let mut s = session();
    s.execute("sketch", &json!({"plane": "xy"})).unwrap();
    s.execute("circle", &json!({"center": [0, 0], "radius": 10})).unwrap();
    s.execute("line", &json!({"a": [-12, 0], "b": [12, 0]})).unwrap();
    let sk = s.active_sketch.unwrap();
    s.execute("extrude", &json!({"sketch": sk.0, "region": 0, "distance": 5})).unwrap();
    let half = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().volume;
    let step = s.all_steps().iter().position(|h| h.kind == "extrude").unwrap();
    s.execute("edit_step", &json!({"step": step, "params": {"sketch": sk.0, "region": 0, "regions": [0, 1]}})).unwrap();
    let whole = s.kernel.mass_props(&s.doc.bodies[0].shape).unwrap().volume;
    assert!((whole - 2.0 * half).abs() < 1e-3, "{half} → {whole}");
    // Variables with a type and the new functions drive steps too.
    s.execute("set_variable", &json!({"name": "depth", "value": "max(3, 2) * 2 mm", "kind": "length"})).unwrap();
    s.execute("set_variable", &json!({"name": "turn", "value": "atan2(1, 1)", "kind": "angle"})).unwrap();
    assert_eq!(s.doc.variables[1].kind, shapecraft_doc::VarKind::Angle);
    s.execute("edit_step", &json!({"step": step, "params": {"distance": "depth"}})).unwrap();
    let bb = s.kernel.bounds(&s.doc.bodies[0].shape).unwrap();
    assert!((bb.max.z - 6.0).abs() < 1e-6);
    assert!(s.execute("set_variable", &json!({"name": "x", "value": "1", "kind": "volume"})).is_err());
}
