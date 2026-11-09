//! Common metric and Unified inch thread presets. All dimensions passed to the kernel are millimeters.
//! Basic dimensions and sources are documented in docs/threads.md.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct ThreadPreset {
    pub id: &'static str,
    pub series: &'static str,
    pub diameter: f64,
    pub pitch: f64,
}

pub const PRESETS: &[ThreadPreset] = &[
    ThreadPreset { id: "M2x0.4", series: "Metric coarse", diameter: 2.0, pitch: 0.4 },
    ThreadPreset { id: "M2.5x0.45", series: "Metric coarse", diameter: 2.5, pitch: 0.45 },
    ThreadPreset { id: "M3x0.5", series: "Metric coarse", diameter: 3.0, pitch: 0.5 },
    ThreadPreset { id: "M4x0.7", series: "Metric coarse", diameter: 4.0, pitch: 0.7 },
    ThreadPreset { id: "M5x0.8", series: "Metric coarse", diameter: 5.0, pitch: 0.8 },
    ThreadPreset { id: "M6x1", series: "Metric coarse", diameter: 6.0, pitch: 1.0 },
    ThreadPreset { id: "M8x1.25", series: "Metric coarse", diameter: 8.0, pitch: 1.25 },
    ThreadPreset { id: "M10x1.5", series: "Metric coarse", diameter: 10.0, pitch: 1.5 },
    ThreadPreset { id: "M12x1.75", series: "Metric coarse", diameter: 12.0, pitch: 1.75 },
    ThreadPreset { id: "M14x2", series: "Metric coarse", diameter: 14.0, pitch: 2.0 },
    ThreadPreset { id: "M16x2", series: "Metric coarse", diameter: 16.0, pitch: 2.0 },
    ThreadPreset { id: "M18x2.5", series: "Metric coarse", diameter: 18.0, pitch: 2.5 },
    ThreadPreset { id: "M20x2.5", series: "Metric coarse", diameter: 20.0, pitch: 2.5 },
    ThreadPreset { id: "M24x3", series: "Metric coarse", diameter: 24.0, pitch: 3.0 },
    ThreadPreset { id: "M3x0.35", series: "Metric fine", diameter: 3.0, pitch: 0.35 },
    ThreadPreset { id: "M4x0.5", series: "Metric fine", diameter: 4.0, pitch: 0.5 },
    ThreadPreset { id: "M5x0.5", series: "Metric fine", diameter: 5.0, pitch: 0.5 },
    ThreadPreset { id: "M6x0.75", series: "Metric fine", diameter: 6.0, pitch: 0.75 },
    ThreadPreset { id: "M8x1", series: "Metric fine", diameter: 8.0, pitch: 1.0 },
    ThreadPreset { id: "M10x1", series: "Metric fine", diameter: 10.0, pitch: 1.0 },
    ThreadPreset { id: "M10x1.25", series: "Metric fine", diameter: 10.0, pitch: 1.25 },
    ThreadPreset { id: "M12x1.25", series: "Metric fine", diameter: 12.0, pitch: 1.25 },
    ThreadPreset { id: "M12x1.5", series: "Metric fine", diameter: 12.0, pitch: 1.5 },
    ThreadPreset { id: "M14x1.5", series: "Metric fine", diameter: 14.0, pitch: 1.5 },
    ThreadPreset { id: "M16x1.5", series: "Metric fine", diameter: 16.0, pitch: 1.5 },
    ThreadPreset { id: "M18x1.5", series: "Metric fine", diameter: 18.0, pitch: 1.5 },
    ThreadPreset { id: "M20x1.5", series: "Metric fine", diameter: 20.0, pitch: 1.5 },
    ThreadPreset { id: "M24x2", series: "Metric fine", diameter: 24.0, pitch: 2.0 },
    ThreadPreset { id: "#2-56 UNC", series: "UNC", diameter: 0.086 * 25.4, pitch: 25.4 / 56.0 },
    ThreadPreset { id: "#4-40 UNC", series: "UNC", diameter: 0.112 * 25.4, pitch: 25.4 / 40.0 },
    ThreadPreset { id: "#6-32 UNC", series: "UNC", diameter: 0.138 * 25.4, pitch: 25.4 / 32.0 },
    ThreadPreset { id: "#8-32 UNC", series: "UNC", diameter: 0.164 * 25.4, pitch: 25.4 / 32.0 },
    ThreadPreset { id: "#10-24 UNC", series: "UNC", diameter: 0.19 * 25.4, pitch: 25.4 / 24.0 },
    ThreadPreset { id: "#12-24 UNC", series: "UNC", diameter: 0.216 * 25.4, pitch: 25.4 / 24.0 },
    ThreadPreset { id: "1/4-20 UNC", series: "UNC", diameter: 0.25 * 25.4, pitch: 25.4 / 20.0 },
    ThreadPreset { id: "5/16-18 UNC", series: "UNC", diameter: 0.3125 * 25.4, pitch: 25.4 / 18.0 },
    ThreadPreset { id: "3/8-16 UNC", series: "UNC", diameter: 0.375 * 25.4, pitch: 25.4 / 16.0 },
    ThreadPreset { id: "7/16-14 UNC", series: "UNC", diameter: 0.4375 * 25.4, pitch: 25.4 / 14.0 },
    ThreadPreset { id: "1/2-13 UNC", series: "UNC", diameter: 0.5 * 25.4, pitch: 25.4 / 13.0 },
    ThreadPreset { id: "5/8-11 UNC", series: "UNC", diameter: 0.625 * 25.4, pitch: 25.4 / 11.0 },
    ThreadPreset { id: "3/4-10 UNC", series: "UNC", diameter: 0.75 * 25.4, pitch: 25.4 / 10.0 },
    ThreadPreset { id: "7/8-9 UNC", series: "UNC", diameter: 0.875 * 25.4, pitch: 25.4 / 9.0 },
    ThreadPreset { id: "1-8 UNC", series: "UNC", diameter: 1.0 * 25.4, pitch: 25.4 / 8.0 },
    ThreadPreset { id: "#2-64 UNF", series: "UNF", diameter: 0.086 * 25.4, pitch: 25.4 / 64.0 },
    ThreadPreset { id: "#4-48 UNF", series: "UNF", diameter: 0.112 * 25.4, pitch: 25.4 / 48.0 },
    ThreadPreset { id: "#6-40 UNF", series: "UNF", diameter: 0.138 * 25.4, pitch: 25.4 / 40.0 },
    ThreadPreset { id: "#8-36 UNF", series: "UNF", diameter: 0.164 * 25.4, pitch: 25.4 / 36.0 },
    ThreadPreset { id: "#10-32 UNF", series: "UNF", diameter: 0.19 * 25.4, pitch: 25.4 / 32.0 },
    ThreadPreset { id: "#12-28 UNF", series: "UNF", diameter: 0.216 * 25.4, pitch: 25.4 / 28.0 },
    ThreadPreset { id: "1/4-28 UNF", series: "UNF", diameter: 0.25 * 25.4, pitch: 25.4 / 28.0 },
    ThreadPreset { id: "5/16-24 UNF", series: "UNF", diameter: 0.3125 * 25.4, pitch: 25.4 / 24.0 },
    ThreadPreset { id: "3/8-24 UNF", series: "UNF", diameter: 0.375 * 25.4, pitch: 25.4 / 24.0 },
    ThreadPreset { id: "7/16-20 UNF", series: "UNF", diameter: 0.4375 * 25.4, pitch: 25.4 / 20.0 },
    ThreadPreset { id: "1/2-20 UNF", series: "UNF", diameter: 0.5 * 25.4, pitch: 25.4 / 20.0 },
    ThreadPreset { id: "5/8-18 UNF", series: "UNF", diameter: 0.625 * 25.4, pitch: 25.4 / 18.0 },
    ThreadPreset { id: "3/4-16 UNF", series: "UNF", diameter: 0.75 * 25.4, pitch: 25.4 / 16.0 },
    ThreadPreset { id: "7/8-14 UNF", series: "UNF", diameter: 0.875 * 25.4, pitch: 25.4 / 14.0 },
    ThreadPreset { id: "1-12 UNF", series: "UNF", diameter: 1.0 * 25.4, pitch: 25.4 / 12.0 },
];

pub fn preset(id: &str) -> Option<&'static ThreadPreset> {
    PRESETS.iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

pub fn nearest(diameter: f64, internal: bool, series: &str) -> &'static ThreadPreset {
    PRESETS
        .iter()
        .filter(|p| p.series == series)
        .min_by(|a, b| {
            let target = |p: &ThreadPreset| if internal { p.diameter - p.pitch } else { p.diameter };
            (target(a) - diameter).abs().total_cmp(&(target(b) - diameter).abs())
        })
        .unwrap_or(&PRESETS[5])
}
