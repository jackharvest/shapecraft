//! Golden structural tests: write drawings, read every section back, check CRCs, objects and geometry.
//! Set `SHAPECRAFT_DWG_OUT=<dir>` to keep the files for checking in other DWG readers.
#![allow(clippy::unwrap_used)]

use shapecraft_dwg::{Drawing, Entity, HAlign, Layer, LineWeight, Linetype, Shape, VAlign, inspect, write};

fn sample() -> Drawing {
    let mut d = Drawing {
        layers: vec![
            Layer { name: "VISIBLE".into(), color: 7, lineweight: LineWeight::Mm(0.5), linetype: None },
            Layer { name: "HIDDEN".into(), color: 8, lineweight: LineWeight::Mm(0.25), linetype: Some("DASHED".into()) },
            Layer::new("FRAME", 3),
        ],
        linetypes: vec![Linetype { name: "DASHED".into(), description: "__ __ __".into(), dashes: vec![1.5, -1.0] }],
        entities: Vec::new(),
        limits: Some(([0.0, 0.0], [420.0, 297.0])),
        date: Some((2026, 11, 1)),
    };
    let e = |layer: &str, shape: Shape| Entity::new(layer, shape);
    d.entities.push(e("VISIBLE", Shape::Line { a: [10.0, 20.0, 0.0], b: [110.0, 20.0, 0.0] }));
    d.entities.push(e("VISIBLE", Shape::Line { a: [10.0, 20.0, 5.0], b: [10.0, 80.5, -2.0] }));
    d.entities.push(e(
        "HIDDEN",
        Shape::Polyline { points: vec![[0.0, 0.0], [50.0, 0.0], [50.0, 30.0], [0.0, 30.0]], bulges: vec![], closed: true },
    ));
    d.entities.push(e(
        "VISIBLE",
        Shape::Polyline { points: vec![[200.0, 100.0], [220.0, 100.0], [220.0, 120.0]], bulges: vec![0.0, 0.414, 0.0], closed: false },
    ));
    d.entities.push(e("VISIBLE", Shape::Arc { center: [60.0, 60.0, 0.0], radius: 12.5, start: 0.0, end: std::f64::consts::PI }));
    d.entities.push(e("VISIBLE", Shape::Circle { center: [300.0, 200.0, 0.0], radius: 25.0 }));
    d.entities.push(e(
        "FRAME",
        Shape::Spline {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            control: vec![[0.0, 150.0, 0.0], [30.0, 190.0, 0.0], [60.0, 110.0, 0.0], [90.0, 150.0, 0.0]],
            weights: vec![],
            closed: false,
        },
    ));
    d.entities.push(e(
        "FRAME",
        Shape::Text {
            at: [300.0, 20.0, 0.0],
            height: 3.5,
            rotation: 0.0,
            value: "TITLE Ø50".into(),
            h_align: HAlign::Left,
            v_align: VAlign::Baseline,
        },
    ));
    d.entities.push(e(
        "FRAME",
        Shape::Text {
            at: [350.0, 30.0, 0.0],
            height: 2.5,
            rotation: 0.0,
            value: "1:2".into(),
            h_align: HAlign::Center,
            v_align: VAlign::Middle,
        },
    ));
    d.entities.push(e(
        "FRAME",
        Shape::MText { at: [300.0, 60.0, 0.0], height: 2.5, width: 80.0, attachment: 1, value: "Notes\nline two".into() },
    ));
    let mut thick = e("FRAME", Shape::Line { a: [10.0, 10.0, 0.0], b: [410.0, 10.0, 0.0] });
    thick.lineweight = LineWeight::Mm(0.7);
    thick.color = Some(1);
    thick.linetype = Some("DASHED".into());
    d.entities.push(thick);
    d
}

fn keep(name: &str, bytes: &[u8]) {
    if let Ok(dir) = std::env::var("SHAPECRAFT_DWG_OUT") {
        let _ = std::fs::write(std::path::Path::new(&dir).join(name), bytes);
    }
}

#[test]
fn sample_reads_back_with_every_crc_and_entity() {
    let d = sample();
    let bytes = write(&d).unwrap();
    keep("sample.dwg", &bytes);
    assert_eq!(&bytes[..6], b"AC1015");
    let f = inspect(&bytes).unwrap();
    assert_eq!(f.version, "AC1015");
    assert_eq!(f.classes, ["ACDBDICTIONARYWDFLT", "ACDBPLACEHOLDER", "LAYOUT", "LWPOLYLINE"]);
    // Tables and dictionaries.
    for (kind, n) in [
        ("BLOCK_CONTROL", 1),
        ("LAYER_CONTROL", 1),
        ("LTYPE_CONTROL", 1),
        ("LAYER", 4),
        ("LTYPE", 4),
        ("STYLE", 1),
        ("APPID", 1),
        ("DIMSTYLE", 1),
        ("VPORT", 1),
        ("BLOCK_HEADER", 2),
        ("DICTIONARY", 5),
        ("ACDBDICTIONARYWDFLT", 1),
        ("ACDBPLACEHOLDER", 1),
        ("LAYOUT", 2),
        ("MLINESTYLE", 1),
    ] {
        assert_eq!(f.count(kind), n, "{kind}");
    }
    let layers: Vec<_> = f.objects.iter().filter(|o| o.kind == "LAYER").filter_map(|o| o.name.clone()).collect();
    assert_eq!(layers, ["0", "VISIBLE", "HIDDEN", "FRAME"]);
    let blocks: Vec<_> = f.objects.iter().filter(|o| o.kind == "BLOCK_HEADER").filter_map(|o| o.name.clone()).collect();
    assert_eq!(blocks, ["*Model_Space", "*Paper_Space"]);
    // Entities: the BLOCK/ENDBLK pairs plus the drawing's own.
    for (kind, n) in
        [("LINE", 3), ("LWPOLYLINE", 2), ("ARC", 1), ("CIRCLE", 1), ("SPLINE", 1), ("TEXT", 2), ("MTEXT", 1), ("BLOCK", 2), ("ENDBLK", 2)]
    {
        assert_eq!(f.count(kind), n, "{kind}");
    }
    let lines: Vec<_> = f.objects.iter().filter(|o| o.kind == "LINE").collect();
    assert_eq!(lines[0].points, [[10.0, 20.0, 0.0], [110.0, 20.0, 0.0]]);
    assert_eq!(lines[1].points, [[10.0, 20.0, 5.0], [10.0, 80.5, -2.0]]);
    assert_eq!(lines[0].layer.as_deref(), Some("VISIBLE"));
    assert_eq!(lines[2].lineweight, Some(14), "0.7 mm is index 14");
    let poly = f.objects.iter().find(|o| o.kind == "LWPOLYLINE").unwrap();
    assert_eq!(poly.points.len(), 4);
    assert_eq!(poly.points[2], [50.0, 30.0, 0.0]);
    assert_eq!(poly.layer.as_deref(), Some("HIDDEN"));
    let circle = f.objects.iter().find(|o| o.kind == "CIRCLE").unwrap();
    assert_eq!((circle.points[0], circle.radius), ([300.0, 200.0, 0.0], Some(25.0)));
    let texts: Vec<_> = f.objects.iter().filter(|o| o.kind == "TEXT").filter_map(|o| o.text.clone()).collect();
    assert_eq!(texts, ["TITLE \\U+00D850", "1:2"]);
    let mtext = f.objects.iter().find(|o| o.kind == "MTEXT").unwrap();
    assert_eq!(mtext.text.as_deref(), Some("Notes\\Pline two"));
    // Layer lineweights: 0.5 mm is index 11, 0.25 mm index 7, Default 31.
    let lw: Vec<_> = f.objects.iter().filter(|o| o.kind == "LAYER").filter_map(|o| o.lineweight).collect();
    assert_eq!(lw, [31, 11, 7, 31]);
}

#[test]
fn output_is_deterministic() {
    assert_eq!(write(&sample()).unwrap(), write(&sample()).unwrap());
}

#[test]
fn empty_drawing_is_valid() {
    let bytes = write(&Drawing::default()).unwrap();
    keep("empty.dwg", &bytes);
    let f = inspect(&bytes).unwrap();
    assert_eq!(f.count("LAYER"), 1);
    assert_eq!(f.entities().filter(|o| o.kind != "BLOCK" && o.kind != "ENDBLK").count(), 0);
}

#[test]
fn many_entities_span_several_object_map_sections() {
    let mut d = Drawing { layers: vec![Layer::new("L", 2)], ..Default::default() };
    for i in 0..3000 {
        let x = i as f64 * 0.37;
        d.entities.push(Entity::new("L", Shape::Line { a: [x, 0.0, 0.0], b: [x, 100.0 + x, 0.0] }));
    }
    let bytes = write(&d).unwrap();
    keep("many.dwg", &bytes);
    let f = inspect(&bytes).unwrap();
    assert_eq!(f.count("LINE"), 3000);
    let last = f.objects.iter().rfind(|o| o.kind == "LINE").unwrap();
    assert_eq!(last.points[1][1], 100.0 + 2999.0 * 0.37);
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    let mut d = sample();
    d.entities.push(Entity::new("NOPE", Shape::Line { a: [0.0; 3], b: [1.0; 3] }));
    assert!(write(&d).is_err());
    let mut d = sample();
    d.entities.push(Entity::new("VISIBLE", Shape::Line { a: [f64::NAN, 0.0, 0.0], b: [1.0; 3] }));
    assert!(write(&d).is_err());
    let mut d = sample();
    d.layers.push(Layer::new("bad/name", 1));
    assert!(write(&d).is_err());
    let mut d = sample();
    d.entities.push(Entity::new("VISIBLE", Shape::Circle { center: [0.0; 3], radius: -1.0 }));
    assert!(write(&d).is_err());
    // Corrupt bytes are reported, not trusted.
    let mut bytes = write(&sample()).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 0x55;
    assert!(inspect(&bytes).is_err());
    assert!(inspect(&bytes[..100]).is_err());
    assert!(inspect(b"AC1015").is_err());
}

#[test]
fn objects_over_32_kib_use_a_two_word_size() {
    let points: Vec<[f64; 2]> = (0..6000).map(|i| [i as f64 * 0.1, (i as f64 * 0.01).sin() * 50.0]).collect();
    let d = Drawing {
        layers: vec![Layer::new("WAVE", 4)],
        entities: vec![Entity::new("WAVE", Shape::Polyline { points: points.clone(), bulges: vec![], closed: false })],
        ..Default::default()
    };
    let bytes = write(&d).unwrap();
    keep("big-polyline.dwg", &bytes);
    let f = inspect(&bytes).unwrap();
    let poly = f.objects.iter().find(|o| o.kind == "LWPOLYLINE").unwrap();
    assert_eq!(poly.points.len(), 6000);
    assert_eq!(poly.points[5999], [points[5999][0], points[5999][1], 0.0]);
}
