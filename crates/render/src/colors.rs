/// A color in sRGB, 0–255 per channel, as written in the design notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub const fn hex(rgb: u32) -> Self {
        Rgba((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
    }

    pub const fn with_alpha(self, a: u8) -> Self {
        Rgba(self.0, self.1, self.2, a)
    }

    /// Linear-light floats for the GPU (the render target is sRGB).
    pub fn linear(self) -> [f32; 4] {
        let f = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        [f(self.0), f(self.1), f(self.2), self.3 as f32 / 255.0]
    }
}

/// Viewport colors, measured from the reference app (plan/shapr3d/01-observed-ui.md).
#[derive(Clone, Copy, Debug)]
pub struct SceneColors {
    pub background: Rgba,
    pub body: Rgba,
    pub edge: Rgba,
    pub selected: Rgba,
    pub selected_pre: Rgba,
    pub hover: Rgba,
    /// A sketch curve under the pointer (measured orange).
    pub sketch_hover: Rgba,
    pub sketch_line: Rgba,
    /// A picked sketch curve (measured 426: #faa84b).
    pub sketch_selected: Rgba,
    pub sketch_preview: Rgba,
    pub sketch_point: Rgba,
    pub region_hover: Rgba,
    pub grid_minor: Rgba,
    pub grid_major: Rgba,
    pub axis_x: Rgba,
    pub axis_y: Rgba,
    pub axis_z: Rgba,
    pub plane_tile: Rgba,
    pub plane_tile_hover: Rgba,
    pub hidden_sketch: Rgba,
    /// The faint tint on every region of a sketch that isn't being edited.
    pub region_tint: Rgba,
    /// How opaque bodies are in X-Ray (0..1).
    pub xray: f32,
}

impl Default for SceneColors {
    fn default() -> Self {
        Self {
            background: Rgba::hex(0x1a1a1a),
            body: Rgba::hex(0xa3a3a7),
            // Measured (s22-002): body edges are black, one pixel with a soft neighbour.
            edge: Rgba::hex(0x000000),
            selected: Rgba::hex(0x1db3dc),
            selected_pre: Rgba::hex(0xc37b14),
            // Measured: hovered edges and face outlines use the selection cyan.
            hover: Rgba::hex(0x1db3dc),
            sketch_hover: Rgba::hex(0xd0861a),
            sketch_line: Rgba::hex(0x1d4eff),
            sketch_selected: Rgba::hex(0xfaa84b),
            sketch_preview: Rgba::hex(0xe8a23a),
            sketch_point: Rgba::hex(0x2e68ff),
            region_hover: Rgba::hex(0xc37b14).with_alpha(90),
            grid_minor: Rgba::hex(0x3c3c3c),
            grid_major: Rgba::hex(0x484848),
            axis_x: Rgba::hex(0xa8312c),
            axis_y: Rgba::hex(0x3c8a35),
            axis_z: Rgba::hex(0x2a5bb5),
            plane_tile: Rgba::hex(0x5a5a5a).with_alpha(70),
            plane_tile_hover: Rgba::hex(0x8a8a8a).with_alpha(110),
            hidden_sketch: Rgba::hex(0x3a3a3a),
            region_tint: Rgba::hex(0x0034ff).with_alpha(10),
            xray: 0.28,
        }
    }
}

impl SceneColors {
    /// The Light theme's view (measured side by side with the dark one, session 23: the same pixels read
    /// #1A1A1A → #EDEDED, axes #8C2F2A/#2D7237/#134F8D → #F5948E/#8ACC9C/#73A9E6, a region #181D37 → #CAD1E9).
    pub fn light() -> Self {
        Self {
            background: Rgba::hex(0xededed),
            hidden_sketch: Rgba::hex(0xc8c8c8),
            sketch_line: Rgba::hex(0x2e68ff),
            grid_minor: Rgba::hex(0xc9c9c9),
            grid_major: Rgba::hex(0xbababa),
            axis_x: Rgba::hex(0xf5948e),
            axis_y: Rgba::hex(0x8acc9c),
            axis_z: Rgba::hex(0x73a9e6),
            plane_tile: Rgba::hex(0x9a9a9a).with_alpha(70),
            plane_tile_hover: Rgba::hex(0x6a6a6a).with_alpha(110),
            region_tint: Rgba::hex(0x1a4ad8).with_alpha(80),
            ..Self::default()
        }
    }
}
