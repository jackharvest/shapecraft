//! Sketch exports: the elements of a sketch, in its own plane's x/y (millimetres), as an ASCII DXF or a DWG.

use std::fmt::Write as _;

use glam::DVec2;
use shapecraft_doc::{SketchEntity, bspline_points, circle_through, spline_points};

fn line(out: &mut String, a: DVec2, b: DVec2) {
    let _ = write!(out, "0\nLINE\n8\n0\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n", a.x, a.y, b.x, b.y);
}

fn polyline(out: &mut String, pts: &[DVec2], closed: bool) {
    for w in pts.windows(2) {
        line(out, w[0], w[1]);
    }
    if closed && pts.len() > 2 {
        line(out, pts[pts.len() - 1], pts[0]);
    }
}

/// An R12 DXF (millimetres) of the sketch elements; text and construction elements are left out.
pub fn write_dxf(entities: &[(SketchEntity, bool)]) -> String {
    let mut out = String::from("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (e, construction) in entities {
        if *construction {
            continue;
        }
        match e {
            SketchEntity::Line { a, b } => line(&mut out, *a, *b),
            SketchEntity::Rectangle { min, max } => {
                polyline(&mut out, &[*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)], true)
            }
            SketchEntity::Circle { center, radius } => {
                let _ = write!(out, "0\nCIRCLE\n8\n0\n10\n{}\n20\n{}\n30\n0\n40\n{}\n", center.x, center.y, radius);
            }
            SketchEntity::Polygon { center, radius, sides, angle } => {
                let n = (*sides).clamp(3, 1000);
                let pts: Vec<DVec2> = (0..n)
                    .map(|k| *center + DVec2::from_angle(angle + std::f64::consts::TAU * f64::from(k) / f64::from(n)) * *radius)
                    .collect();
                polyline(&mut out, &pts, true);
            }
            SketchEntity::Arc { a, mid, b } => {
                let Some((c, r)) = circle_through(*a, *mid, *b) else {
                    line(&mut out, *a, *b);
                    continue;
                };
                // DXF arcs run counter-clockwise from start to end.
                let ccw = (*mid - *a).perp_dot(*b - *mid) > 0.0;
                let (s, e) = if ccw { (*a, *b) } else { (*b, *a) };
                let ang = |p: DVec2| (p - c).y.atan2((p - c).x).to_degrees();
                let _ = write!(out, "0\nARC\n8\n0\n10\n{}\n20\n{}\n30\n0\n40\n{}\n50\n{}\n51\n{}\n", c.x, c.y, r, ang(s), ang(e));
            }
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let (major, ratio, turn) =
                    if rx >= ry { (*rx, ry / rx, *angle) } else { (*ry, rx / ry, angle + std::f64::consts::FRAC_PI_2) };
                let m = DVec2::from_angle(turn) * major;
                let _ = write!(
                    out,
                    "0\nELLIPSE\n8\n0\n100\nAcDbEntity\n100\nAcDbEllipse\n10\n{}\n20\n{}\n30\n0\n11\n{}\n21\n{}\n31\n0\n40\n{}\n41\n0\n42\n{}\n",
                    center.x,
                    center.y,
                    m.x,
                    m.y,
                    ratio,
                    std::f64::consts::TAU
                );
            }
            SketchEntity::Spline { points } => polyline(&mut out, &spline_points(points, 16), false),
            SketchEntity::ControlSpline { poles } => polyline(&mut out, &bspline_points(poles, 16), false),
            SketchEntity::Text { .. } => {}
        }
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

/// A sketch's curves as an AutoCAD R2000 DWG, in millimetres in the sketch's plane: lines, circles and arcs as
/// themselves, rectangles and polygons as closed polylines, ellipses and splines as fine polylines. Construction
/// curves and text are left out, as in the DXF.
pub fn write_dwg(entities: &[(SketchEntity, bool)]) -> std::result::Result<Vec<u8>, String> {
    use shapecraft_dwg as dwg;
    let mut d = dwg::Drawing { layers: Vec::new(), linetypes: Vec::new(), entities: Vec::new(), limits: None, date: None };
    let p3 = |p: DVec2| [p.x, p.y, 0.0];
    let poly =
        |pts: &[DVec2], closed: bool| dwg::Shape::Polyline { points: pts.iter().map(|p| [p.x, p.y]).collect(), bulges: Vec::new(), closed };
    for (e, construction) in entities {
        if *construction {
            continue;
        }
        let shape = match e {
            SketchEntity::Line { a, b } => dwg::Shape::Line { a: p3(*a), b: p3(*b) },
            SketchEntity::Rectangle { min, max } => poly(&[*min, DVec2::new(max.x, min.y), *max, DVec2::new(min.x, max.y)], true),
            SketchEntity::Circle { center, radius } => dwg::Shape::Circle { center: p3(*center), radius: *radius },
            SketchEntity::Polygon { center, radius, sides, angle } => {
                let n = (*sides).clamp(3, 1000);
                let pts: Vec<DVec2> = (0..n)
                    .map(|k| *center + DVec2::from_angle(angle + std::f64::consts::TAU * f64::from(k) / f64::from(n)) * *radius)
                    .collect();
                poly(&pts, true)
            }
            SketchEntity::Arc { a, mid, b } => match circle_through(*a, *mid, *b) {
                Some((c, r)) => {
                    // Counter-clockwise from start to end.
                    let ccw = (*mid - *a).perp_dot(*b - *mid) > 0.0;
                    let (s, e) = if ccw { (*a, *b) } else { (*b, *a) };
                    let ang = |p: DVec2| (p - c).y.atan2((p - c).x);
                    dwg::Shape::Arc { center: p3(c), radius: r, start: ang(s), end: ang(e) }
                }
                None => dwg::Shape::Line { a: p3(*a), b: p3(*b) },
            },
            SketchEntity::Ellipse { center, rx, ry, angle } => {
                let (u, v) = (DVec2::from_angle(*angle), DVec2::from_angle(angle + std::f64::consts::FRAC_PI_2));
                let pts: Vec<DVec2> = (0..96)
                    .map(|k| {
                        let t = std::f64::consts::TAU * f64::from(k) / 96.0;
                        *center + u * (rx * t.cos()) + v * (ry * t.sin())
                    })
                    .collect();
                poly(&pts, true)
            }
            SketchEntity::Spline { points } => poly(&spline_points(points, 16), false),
            SketchEntity::ControlSpline { poles } => poly(&bspline_points(poles, 16), false),
            SketchEntity::Text { .. } => continue,
        };
        d.entities.push(dwg::Entity::new("0", shape));
    }
    dwg::write(&d).map_err(|e| e.to_string())
}
