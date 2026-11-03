//! Checks against projects saved by Shapr3D. They stay out of the repository (plan/shapr-files is local), so each
//! check passes quietly when its file isn't there.

use std::path::PathBuf;

fn sample(name: &str) -> Option<shapecraft_shapr::Project> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plan/shapr-files").join(name);
    let file = shapecraft_shapr::ShaprFile::read(&path).ok()?;
    shapecraft_shapr::Project::read(&file).ok()
}

/// The middle of a body's corners (the boxes here have their vertices at the corners).
fn middle(p: &shapecraft_shapr::Project, name: &str) -> Option<[f64; 3]> {
    let b = p.bodies.iter().find(|b| b.name.as_deref() == Some(name))?;
    let (lo, hi) =
        b.brep.vertices.iter().fold((glam::DVec3::splat(f64::MAX), glam::DVec3::splat(f64::MIN)), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
    Some(((lo + hi) / 2.0).to_array())
}

#[test]
fn a_move_that_is_a_bodys_last_change_is_applied() {
    let (Some(before), Some(after)) = (sample("n3-colour-hidden"), sample("n5-move")) else { return };
    let (a, b) = (middle(&before, "Body 01").unwrap(), middle(&after, "Body 01").unwrap());
    // Shapr3D's Move/Rotate 01: 7 mm up.
    assert!((b[2] - a[2] - 7.0).abs() < 1e-6 && (b[0] - a[0]).abs() < 1e-6 && (b[1] - a[1]).abs() < 1e-6, "{a:?} {b:?}");
    // Body 02 didn't move.
    let (c, d) = (middle(&before, "Body 02").unwrap(), middle(&after, "Body 02").unwrap());
    assert!(c.iter().zip(d).all(|(x, y)| (x - y).abs() < 1e-6));
}

#[test]
fn a_move_a_later_step_built_on_is_not_applied_twice() {
    let (Some(ours), Some(filleted)) = (sample("n7-our-move"), sample("n8-move-then-fillet")) else { return };
    let (a, b) = (middle(&ours, "Body 01").unwrap(), middle(&filleted, "Body 01").unwrap());
    assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6), "{a:?} {b:?}");
}

#[test]
fn simple_extrudes_are_described() {
    let Some(p) = sample("n4-extrudes") else { return };
    let got: Vec<(f64, f64, bool)> =
        p.extrudes.iter().map(|e| ((e.distance * 1e6).round() / 1e6, (e.draft * 1e6).round() / 1e6, e.symmetric)).collect();
    assert_eq!(got, vec![(8.0, 0.0, false), (6.0, 5.0, true)]);
}

#[test]
fn curved_fillets_read_as_rolling_ball_blends() {
    use shapecraft_kernel::Kernel;
    let Some(p) = sample("n6-curved-fillet") else { return };
    assert!(p.unreadable.is_empty(), "{:?}", p.unreadable);
    // The drafted half-disc with every edge rounded 1 mm: OCCT's own 1 mm fillet of n4's body gives 378.8192 mm³.
    let k = shapecraft_kernel_occt::OcctKernel::new();
    let volumes: Vec<f64> =
        p.bodies.iter().filter_map(|b| k.from_brep(&b.brep).ok()).filter_map(|s| k.mass_props(&s).ok()).map(|m| m.volume).collect();
    assert_eq!(volumes.len(), p.bodies.len());
    assert!(volumes.iter().any(|v| (v - 378.82).abs() < 0.01), "{volumes:?}");
    // Extrude 02 is rounded by Fillet 01 and nothing else.
    let e = p.extrudes.iter().find(|e| e.symmetric).expect("Extrude 02");
    assert_eq!(e.fillet.as_ref().map(|f| (f.radius * 1e6).round() / 1e6), Some(1.0));
    let body = p.bodies.iter().find(|b| b.name.as_deref() == Some("Body 04")).expect("Body 04");
    assert_eq!(body.step, Some(e.node));
}
