use glam::DVec3;
use serde_json::json;
use shapecraft_doc::{ItemId, Pick};
use shapecraft_kernel::{Axis, BoolOp};

use super::session;
use crate::Session;

fn blank(s: &mut Session, diameter: f64, length: f64, internal: bool) -> (ItemId, u32) {
    s.execute("sketch", &json!({"plane":"xy"})).unwrap();
    s.execute("circle", &json!({"center":[0,0], "radius":diameter/2.0})).unwrap();
    if internal {
        s.execute("circle", &json!({"center":[0,0], "radius":diameter/2.0 + 3.0})).unwrap();
    }
    let sk = s.active_sketch.unwrap();
    let region = s.doc.sketch(sk).unwrap().regions().iter().max_by(|a, b| a.area.total_cmp(&b.area)).unwrap().index;
    let out = s.execute("extrude", &json!({"sketch":sk.0, "region":region, "distance":length})).unwrap();
    let body = ItemId(out.value["body"].as_u64().unwrap());
    let shape = &s.doc.body(body).unwrap().shape;
    let face = (0..s.kernel.face_count(shape).unwrap())
        .find(|f| s.kernel.cylinder_face(shape, *f).unwrap().is_some_and(|w| w.internal == internal))
        .unwrap();
    (body, face)
}

fn vol(s: &Session, body: ItemId) -> f64 {
    s.kernel.mass_props(&s.doc.body(body).unwrap().shape).unwrap().volume.abs()
}

#[test]
fn thread_metric_external_is_solid_and_undoable_with_editable_history() {
    let mut s = session();
    let (body, face) = blank(&mut s, 6.0, 6.0, false);
    let original = vol(&s, body);
    let wall = s.kernel.cylinder_face(&s.doc.body(body).unwrap().shape, face).unwrap().unwrap();
    s.selection = vec![Pick::Face { body, face }];
    let out = s.execute("thread", &json!({"preset":"M6x1", "length":3.0})).unwrap();
    assert_eq!(out.value["pitch"], 1.0);
    let cut = vol(&s, body);
    assert!(cut < original - 5.0 && cut > original * 0.6, "{cut} / {original}");
    assert_eq!(s.kernel.solids(&s.doc.body(body).unwrap().shape).unwrap().len(), 1);
    let step = s.all_steps().len() - 1;
    assert_eq!(s.all_steps()[step].kind, "thread");
    s.execute("undo", &json!({})).unwrap();
    assert!((vol(&s, body) - original).abs() < 1e-7);
    s.execute("redo", &json!({})).unwrap();
    assert!((vol(&s, body) - cut).abs() < 1e-7);
    s.execute("edit_step", &json!({"step":step, "params":{"length":2.0}})).unwrap();
    assert!(vol(&s, body) > cut);
    // Outside the requested span there is no cut.
    let upper = s.kernel.extrude_circle(wall.axis.origin + wall.axis.dir * 2.01, 3.0, wall.axis.dir * 3.98).unwrap();
    let shared = s.kernel.boolean(BoolOp::Intersect, &upper, &s.doc.body(body).unwrap().shape).unwrap();
    assert!((s.kernel.mass_props(&shared).unwrap().volume - s.kernel.mass_props(&upper).unwrap().volume).abs() < 1e-4);
}

#[test]
fn thread_imperial_internal_left_hand_and_reverse() {
    let mut s = session();
    let p = crate::threads::preset("1/4-20 UNC").unwrap();
    let (body, face) = blank(&mut s, p.diameter - p.pitch, 6.0, true);
    let before = vol(&s, body);
    let original = s.doc.body(body).unwrap().shape.clone();
    let wall = s.kernel.cylinder_face(&original, face).unwrap().unwrap();
    s.execute(
        "thread",
        &json!({"body":body.0,"face":face,"preset":p.id,"kind":"internal","length":2.54,
        "hand":"left","reverse":true,"offset":0.3,"clearance":0.05}),
    )
    .unwrap();
    assert!(vol(&s, body) < before - 2.0);
    assert_eq!(s.kernel.solids(&s.doc.body(body).unwrap().shape).unwrap().len(), 1);
    // Compare the material at the untouched end, independently of face splitting/merging.
    let probe = s.kernel.extrude_circle(wall.axis.origin, 10.0, wall.axis.dir * 2.0).unwrap();
    let before_slice = s.kernel.boolean(BoolOp::Intersect, &original, &probe).unwrap();
    let after_slice = s.kernel.boolean(BoolOp::Intersect, &s.doc.body(body).unwrap().shape, &probe).unwrap();
    assert!((s.kernel.mass_props(&before_slice).unwrap().volume - s.kernel.mass_props(&after_slice).unwrap().volume).abs() < 1e-4);
}

#[test]
fn thread_accepts_rotated_faces_and_preserves_direction() {
    let mut s = session();
    let (body, _) = blank(&mut s, 6.0, 4.0, false);
    s.execute("rotate", &json!({"items":[body.0],"origin":[0,0,0],"axis":[1,0,0],"angle":90})).unwrap();
    let shape = &s.doc.body(body).unwrap().shape;
    let face = (0..s.kernel.face_count(shape).unwrap()).find(|f| s.kernel.cylinder_face(shape, *f).unwrap().is_some()).unwrap();
    let before = vol(&s, body);
    s.execute("thread", &json!({"body":body.0,"face":face,"preset":"M6x1","length":2.0,"clearance":0.1})).unwrap();
    assert!(vol(&s, body) < before);
    let bounds = s.kernel.bounds(&s.doc.body(body).unwrap().shape).unwrap();
    assert!(bounds.max.y < 1e-3 && bounds.min.y < -3.99);
}

#[test]
fn thread_rejects_bad_dimensions_faces_and_types_without_changes() {
    let mut s = session();
    let (body, face) = blank(&mut s, 6.0, 5.0, false);
    let before = s.doc.body(body).unwrap().shape.clone();
    let steps = s.all_steps().len();
    for extra in [
        json!({"preset":"unknown"}),
        json!({"preset":"M8x1.25"}),
        json!({"length":-1}),
        json!({"length":0}),
        json!({"length":1e100}),
        json!({"offset":-1}),
        json!({"offset":4}),
        json!({"clearance":-0.1}),
        json!({"clearance":1}),
        json!({"hand":"backward"}),
        json!({"hand":1}),
        json!({"kind":"internal"}),
        json!({"kind":"other"}),
        json!({"kind":1}),
        json!({"face":0.5}),
        json!({"face":-1}),
        json!({"face":9999999}),
        json!({"reverse":"true"}),
    ] {
        let mut params = json!({"body":body.0,"face":face,"preset":"M6x1","length":2});
        params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(s.execute("thread", &params).is_err(), "{params}");
        assert!(s.doc.body(body).unwrap().shape.ptr_eq(&before));
        assert_eq!(s.all_steps().len(), steps);
    }
    s.selection.clear();
    assert!(s.execute("thread", &json!({"preset":"M6x1","length":2})).is_err());
    let planar = (0..s.kernel.face_count(&before).unwrap()).find(|f| s.kernel.face_plane(&before, *f).unwrap().is_some()).unwrap();
    assert!(s.execute("thread", &json!({"body":body.0,"face":planar,"preset":"M6x1","length":2})).is_err());
}

#[test]
fn thread_presets_are_unique_and_convert_inches_exactly() {
    let mut s = session();
    let all = s.execute("thread_presets", &json!({})).unwrap().value;
    assert_eq!(all["presets"].as_array().unwrap().len(), 58);
    let ids: std::collections::HashSet<_> = crate::threads::PRESETS.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), crate::threads::PRESETS.len());
    let p = crate::threads::preset("1/4-20 UNC").unwrap();
    assert!((p.diameter - 6.35).abs() < 1e-12 && (p.pitch - 1.27).abs() < 1e-12);
    let p = crate::threads::preset("#10-32 UNF").unwrap();
    assert!((p.diameter - 4.826).abs() < 1e-12 && (p.pitch - 0.79375).abs() < 1e-12);
    let wall = shapecraft_kernel::CylinderFace { axis: Axis::new(DVec3::ZERO, DVec3::Z), radius: 3., length: 500., internal: false };
    let spec = shapecraft_kernel::threads::ThreadSpec {
        diameter: 6.,
        pitch: 1.,
        length: 101.,
        offset: 0.,
        clearance: 0.,
        internal: false,
        left_hand: false,
        reverse: false,
    };
    assert!(spec.validate(wall).is_err());
    assert!(shapecraft_kernel::threads::ThreadSpec { length: f64::NAN, ..spec }.validate(wall).is_err());
}

#[test]
fn thread_mating_pair_has_clearance_and_survives_project_roundtrip() {
    let mut bolt = session();
    let (b, f) = blank(&mut bolt, 6.0, 3.0, false);
    bolt.execute("thread", &json!({"body":b.0,"face":f,"preset":"M6x1","length":3.0})).unwrap();
    let mut nut = session();
    let (n, f) = blank(&mut nut, 5.0, 3.0, true);
    nut.execute("thread", &json!({"body":n.0,"face":f,"preset":"M6x1","length":3.0,"clearance":0.05})).unwrap();
    // Groove centers of the male and female are half a pitch apart.
    let male = bolt.kernel.rotate(&bolt.doc.body(b).unwrap().shape, Axis::new(DVec3::ZERO, DVec3::Z), std::f64::consts::PI).unwrap();
    let overlap = bolt.kernel.boolean(BoolOp::Intersect, &male, &nut.doc.body(n).unwrap().shape).unwrap();
    let collision = bolt.kernel.mass_props(&overlap).unwrap().volume.abs();
    assert!(collision < 1e-3, "mating threads intersect by {collision} mm³");
    let path = std::env::temp_dir().join(format!("shapecraft-thread-roundtrip-{}", std::process::id()));
    nut.execute("save", &json!({"path":path.to_string_lossy()})).unwrap();
    let doc = shapecraft_doc::project::load(nut.kernel.as_ref(), &path).unwrap();
    let mut loaded = Session::with_document(nut.kernel.clone(), doc, Some(path.clone()));
    // Mass properties use adaptive integration at 1e-4 relative precision.
    assert!((vol(&loaded, n) - vol(&nut, n)).abs() < vol(&nut, n) * 1e-5, "loaded={} before={}", vol(&loaded, n), vol(&nut, n));
    let step = loaded.all_steps().len() - 1;
    assert_eq!(loaded.all_steps()[step].entries[0].params["kind"], "internal");
    loaded.execute("edit_step", &json!({"step":step,"params":{"length":2.0}})).unwrap();
    assert!(vol(&loaded, n) > vol(&nut, n));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn thread_fine_and_imperial_presets_work_on_both_wall_types() {
    for (id, internal) in [("M3x0.35", false), ("M12x1.5", true), ("#2-56 UNC", false), ("#10-32 UNF", true), ("1-12 UNF", false)] {
        let mut s = session();
        let p = crate::threads::preset(id).unwrap();
        let (body, face) = blank(&mut s, p.diameter - if internal { p.pitch } else { 0.0 }, 2.0 * p.pitch, internal);
        let before = vol(&s, body);
        s.execute("thread", &json!({"body":body.0,"face":face,"preset":id,"length":1.5*p.pitch})).unwrap();
        assert!(vol(&s, body) < before, "{id}");
    }
}
