struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // width, height, 1/width, 1/height (pixels)
    viewport: vec4<f32>,
    // w: 1 when Section View is on
    light: vec4<f32>,
    // section plane normal (xyz) and offset (w); n·p > w is cut away
    section: vec4<f32>,
    // surface analysis: x mode (0 none, 1 zebra, 2 curvature), y 1 = vertical stripes, z scale
    analysis: vec4<f32>,
    // x: light intensity (Visualization)
    visual: vec4<f32>,
    // The camera's right, up and back directions; right.w / up.w: tan of half the field of view across / up,
    // back.w: the backdrop's mip level
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    cam_back: vec4<f32>,
    // A custom environment (the user's panorama): x 1 when selected, y log2 of its width, z its highest mip level
    envp: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
// The custom environment, prefiltered (envmap.rs): radiance with blurrier mip levels, and diffuse irradiance.
@group(0) @binding(1) var env_radiance: texture_2d<f32>;
@group(0) @binding(2) var env_irradiance: texture_2d<f32>;
@group(0) @binding(3) var env_sampler: sampler;

// Where a direction falls in the equirectangular panorama (as envmap.rs maps it), turned by Light Rotation.
fn env_uv(d: vec3<f32>) -> vec2<f32> {
    let phi = atan2(d.y, d.x) - g.visual.z;
    let u = fract(0.75 - phi / 6.28318531);
    let v = acos(clamp(d.z / max(length(d), 1e-6), -1.0, 1.0)) / 3.14159265;
    return vec2<f32>(u, v);
}

// The panorama's light along a direction, blurred as a surface this rough would see it: lod is counted for a
// 2048-wide map (0 sharp .. 9 a 4×2 blur), whatever size the panorama is.
fn env_radiance_at(d: vec3<f32>, lod2048: f32) -> vec3<f32> {
    let lod = clamp(lod2048 - (11.0 - g.envp.y), 0.0, g.envp.z);
    return textureSampleLevel(env_radiance, env_sampler, env_uv(d), lod).rgb;
}

// ---- the panorama backdrop ----

struct BackdropOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_backdrop(@builtin(vertex_index) i: u32) -> BackdropOut {
    let x = f32((i << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(i & 2u) * 2.0 - 1.0;
    var o: BackdropOut;
    o.clip = vec4<f32>(x, y, 1.0, 1.0);
    o.ndc = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_backdrop(v: BackdropOut) -> @location(0) vec4<f32> {
    let d = -g.cam_back.xyz + g.cam_right.xyz * (v.ndc.x * g.cam_right.w) + g.cam_up.xyz * (v.ndc.y * g.cam_up.w);
    let lod = clamp(g.cam_back.w, 0.0, g.envp.z);
    let c = textureSampleLevel(env_radiance, env_sampler, env_uv(d), lod).rgb;
    return vec4<f32>(c * g.visual.x, 1.0);
}

fn cut_away(p: vec3<f32>) -> bool {
    return g.light.w > 0.5 && dot(g.section.xyz, p) > g.section.w;
}

// ---- bodies ----

struct Body {
    color: vec4<f32>,
    sel_color: vec4<f32>,
    hover_color: vec4<f32>,
    // x: metal, y: gloss, z: 1 when the body has an appearance, w: 1 in Visualization
    material: vec4<f32>,
    // x: whole body selected, y: hovered face + 1 (0 = none), z: number of selected faces, w: body hovered
    flags: vec4<u32>,
    sel: array<vec4<u32>, 4>,
    // x: number of painted faces
    paint: vec4<u32>,
    paint_faces: array<vec4<u32>, 4>,
    paint_colors: array<vec4<f32>, 16>,
};
@group(1) @binding(0) var<uniform> b: Body;

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) face: u32,
    @location(3) curv: f32,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) face: u32,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) curv: f32,
};

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    var o: MeshOut;
    // Selected faces win over faces lying on them (coplanar bodies), as observed; lines still win over both.
    let lift = select(0.0, 0.0005, b.flags.x != 0u || face_selected(v.face));
    o.clip = g.view_proj * vec4<f32>(nudge(v.pos, lift), 1.0);
    o.normal = v.normal;
    o.face = v.face;
    o.world = v.pos;
    o.curv = v.curv;
    return o;
}

fn face_selected(face: u32) -> bool {
    for (var i = 0u; i < min(b.flags.z, 16u); i = i + 1u) {
        if (b.sel[i / 4u][i % 4u] == face) {
            return true;
        }
    }
    return false;
}

@fragment
fn fs_mesh(v: MeshOut) -> @location(0) vec4<f32> {
    if (cut_away(v.world)) {
        discard;
    }
    var n = normalize(v.normal);
    let to_eye = normalize(g.eye.xyz - v.world * g.eye.w);
    if (dot(n, to_eye) < 0.0) {
        // Seeing the inside of a cut body: draw it as the solid cap, hatched lightly.
        if (g.light.w > 0.5) {
            let hatch = fract((v.clip.x + v.clip.y) * 0.08) < 0.5;
            let cap = select(vec3<f32>(0.30, 0.31, 0.34), vec3<f32>(0.26, 0.27, 0.30), hatch);
            return vec4<f32>(cap, 1.0);
        }
        n = -n;
    }
    // Light from above and slightly in front, tuned so tops show the body color as measured in the
    // reference and walls fall off to darker greys.
    let key = pow(max(dot(n, normalize(g.light.xyz)), 0.0), 1.5);
    let top = pow(1.0 / length(g.light.xyz) * g.light.z, 1.5);
    let shade = (0.08 + 0.92 * key) / (0.08 + 0.92 * top);
    var base = b.color.rgb;
    for (var k = 0u; k < min(b.paint.x, 16u); k = k + 1u) {
        if (b.paint_faces[k / 4u][k % 4u] == v.face) {
            base = b.paint_colors[k].rgb;
        }
    }
    // Surface analysis replaces the colour (selection still shows).
    if (g.analysis.x > 0.5 && !face_selected(v.face) && b.flags.x == 0u) {
        if (g.analysis.x < 1.5) {
            let r = reflect(-to_eye, n);
            let a = select(asin(clamp(r.z, -1.0, 1.0)), atan2(r.y, r.x), g.analysis.y > 0.5);
            let band = fract(a * 8.0 / 3.14159265 * max(g.analysis.z, 0.05));
            let ink = select(0.0, 1.0, band < 0.5);
            return vec4<f32>(vec3<f32>(ink, ink, ink), b.color.a);
        }
        // Measured (L14): flat tops #1bff19, saddles #1c20db, walls darker by the usual shading (linear values).
        let t = clamp(v.curv * 6.0 * max(g.analysis.z, 0.05), -1.0, 1.0);
        var c = vec3<f32>(0.011, 1.0, 0.010);
        if (t < 0.0) {
            c = mix(c, vec3<f32>(0.012, 0.016, 1.0), -t);
        } else {
            c = mix(c, vec3<f32>(1.0, 0.012, 0.010), t);
        }
        return vec4<f32>(c * min(shade, 1.0), b.color.a);
    }
    if (face_selected(v.face)) {
        base = b.sel_color.rgb;
        return vec4<f32>(base * (0.55 + 0.45 * min(shade, 1.0)), max(b.color.a, 0.55));
    }
    if (b.flags.x != 0u) {
        // A whole selected body turns the selection cyan (shaded, so its shape still reads), as observed.
        base = b.sel_color.rgb;
    }
    // Under the pointer (a face, or the whole body from the Items list): ~8% lighter on screen (183 -> 198 on a
    // light top face, s30-004/005). The colour here is linear, so that takes ~1.2.
    if (b.flags.y == v.face + 1u || b.flags.w == 1u) {
        base = base * 1.2;
    }
    if (b.material.z < 0.5 && b.material.w < 0.5) {
        // A faint, tight highlight: rounds and cylinders catch a bright streak as in the reference (s30-001), while
        // flat faces at the usual angles sit far from the half vector and keep their measured colours.
        let hv = normalize(normalize(g.light.xyz) + to_eye);
        let gleam = pow(max(dot(n, hv), 0.0), 48.0) * 0.16;
        return vec4<f32>(base * shade + vec3<f32>(gleam, gleam, gleam), b.color.a);
    }
    // Materials: diffuse that fades out for metals, a highlight that tightens with gloss, and a soft
    // studio reflection (bright above, dark below) tinted by the color for metals.
    let metal = b.material.x;
    let gloss = select(0.35, b.material.y, b.material.z > 0.5);
    let l = normalize(g.light.xyz);
    let h = normalize(l + to_eye);
    let spec = pow(max(dot(n, h), 0.0), mix(12.0, 220.0, gloss * gloss)) * mix(0.15, 1.2, gloss);
    let r = reflect(-to_eye, n);
    var env = mix(vec3<f32>(0.10, 0.10, 0.11), vec3<f32>(0.95, 0.95, 0.97), smoothstep(-0.3, 0.6, r.z));
    if (g.visual.w > 0.5) {
        // Visualization's studio: the floor reflects the backdrop's brightness, and two soft boxes high on either
        // side turn with Light Rotation.
        let floor = mix(0.06, 0.45, g.visual.y);
        env = mix(vec3<f32>(floor, floor, floor * 1.02), vec3<f32>(0.95, 0.95, 0.97), smoothstep(-0.3, 0.6, r.z));
        // Tall soft boxes stand around the model: they show as bright upright stripes wherever the reflection
        // looks their way, from just below the horizon up.
        let a = g.visual.z;
        let flat = normalize(vec2<f32>(r.x, r.y) + vec2<f32>(1e-5, 0.0));
        let tall = smoothstep(-0.75, -0.35, r.z) * (1.0 - smoothstep(0.55, 0.85, r.z));
        let box1 = smoothstep(0.86, 0.96, dot(flat, vec2<f32>(cos(a + 0.8), sin(a + 0.8))));
        let box2 = smoothstep(0.90, 0.98, dot(flat, vec2<f32>(cos(a + 3.6), sin(a + 3.6))));
        env = env + vec3<f32>(1.3, 1.3, 1.3) * box1 * tall + vec3<f32>(0.6, 0.6, 0.6) * box2 * tall;
    }
    let f0 = mix(vec3<f32>(0.04, 0.04, 0.04), base, metal);
    let fresnel = f0 + (vec3<f32>(1.0, 1.0, 1.0) - f0) * pow(1.0 - max(dot(n, to_eye), 0.0), 5.0) * gloss * 0.6;
    if (g.envp.x > 0.5 && g.visual.w > 0.5) {
        // The user's panorama lights the body: its irradiance for the diffuse part, and its reflection, sharp on
        // glossy surfaces and blurred on rough ones (no separate sun highlight: the panorama's own sun is it).
        let irr = textureSampleLevel(env_irradiance, env_sampler, env_uv(n), 0.0).rgb;
        let refl = env_radiance_at(r, 1.0 + 8.0 * pow(1.0 - clamp(gloss, 0.0, 1.0), 0.7));
        let diffuse_ibl = base * irr * (1.0 - 0.85 * metal);
        let lit_ibl = diffuse_ibl + fresnel * refl * mix(0.3, 1.0, gloss) * (0.35 + 0.65 * metal);
        return vec4<f32>(lit_ibl * g.visual.x, b.color.a);
    }
    let diffuse = base * shade * (1.0 - 0.85 * metal);
    let lit = diffuse + fresnel * (env * mix(0.15, 0.9, gloss) * (0.35 + 0.65 * metal)) + f0 * spec;
    return vec4<f32>(lit * g.visual.x, b.color.a);
}

// ---- screen-space lines ----

struct LineIn {
    @location(0) a: vec3<f32>,
    @location(1) b: vec3<f32>,
    @location(2) color: vec4<f32>,
    // x: 0 at a, 1 at b; y: side (-1/1); z: width in pixels
    @location(3) params: vec4<f32>,
};

struct ColorOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local: vec2<f32>,
    // xyz world position, w: 1 if the section plane cuts this line
    @location(2) world: vec4<f32>,
};

// Moves a point a little towards the eye, so lines and fills win the depth test against the faces they lie on.
fn nudge(p: vec3<f32>, amount: f32) -> vec3<f32> {
    let to_eye = g.eye.xyz - p * g.eye.w;
    return p + to_eye * amount;
}

@vertex
fn vs_line(v: LineIn) -> ColorOut {
    var ca = g.view_proj * vec4<f32>(nudge(v.a, 0.002), 1.0);
    var cb = g.view_proj * vec4<f32>(nudge(v.b, 0.002), 1.0);
    // Clip against the near plane so lines through the camera don't flip.
    let eps = 1e-5;
    if (ca.w < eps && cb.w < eps) {
        var o: ColorOut;
        o.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
        o.color = vec4<f32>(0.0);
        o.world = vec4<f32>(0.0);
        return o;
    }
    if (ca.w < eps) {
        ca = mix(ca, cb, (eps - ca.w) / (cb.w - ca.w));
    }
    if (cb.w < eps) {
        cb = mix(cb, ca, (eps - cb.w) / (ca.w - cb.w));
    }
    let sa = ca.xy / ca.w * g.viewport.xy * 0.5;
    let sb = cb.xy / cb.w * g.viewport.xy * 0.5;
    var dir = sb - sa;
    if (length(dir) < 1e-6) {
        dir = vec2<f32>(1.0, 0.0);
    }
    dir = normalize(dir);
    let perp = vec2<f32>(-dir.y, dir.x);
    let t = v.params.x;
    let half_w = v.params.z * 0.5 + 0.5;
    var c = mix(ca, cb, t);
    let offset_px = perp * v.params.y * half_w + dir * (t * 2.0 - 1.0) * half_w;
    c = vec4<f32>(c.xy + offset_px * g.viewport.zw * 2.0 * c.w, c.z, c.w);
    var o: ColorOut;
    o.clip = c;
    o.color = v.color;
    o.local = vec2<f32>(v.params.y * half_w, v.params.z * 0.5);
    o.world = vec4<f32>(mix(v.a, v.b, t), v.params.w);
    return o;
}

@fragment
fn fs_line(v: ColorOut) -> @location(0) vec4<f32> {
    if (v.world.w > 0.5 && cut_away(v.world.xyz)) {
        discard;
    }
    // Antialias the long sides.
    let d = abs(v.local.x);
    let a = clamp(v.local.y + 0.5 - d, 0.0, 1.0);
    return vec4<f32>(v.color.rgb, v.color.a * a);
}

// ---- dots ----

struct DotIn {
    @location(0) center: vec3<f32>,
    @location(1) color: vec4<f32>,
    // xy: corner (-1..1), z: diameter in pixels
    @location(2) params: vec4<f32>,
};

struct DotOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) offset: vec2<f32>,
    @location(2) radius: f32,
    // > 0: a hollow ring this many pixels thick.
    @location(3) ring: f32,
};

@vertex
fn vs_dot(v: DotIn) -> DotOut {
    var c = g.view_proj * vec4<f32>(nudge(v.center, 0.003), 1.0);
    let r = v.params.z * 0.5;
    let quad = r + 1.0;
    c = vec4<f32>(c.xy + v.params.xy * quad * g.viewport.zw * 2.0 * c.w, c.z, c.w);
    var o: DotOut;
    o.clip = c;
    o.color = v.color;
    o.offset = v.params.xy * quad;
    o.radius = r;
    o.ring = v.params.w;
    return o;
}

@fragment
fn fs_dot(v: DotOut) -> @location(0) vec4<f32> {
    let d = length(v.offset);
    var a = clamp(v.radius + 0.5 - d, 0.0, 1.0);
    if (v.ring > 0.0) {
        a = a * clamp(d - (v.radius - v.ring) + 0.5, 0.0, 1.0);
    }
    return vec4<f32>(v.color.rgb, v.color.a * a);
}

// ---- translucent fills ----

struct FillIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_fill(v: FillIn) -> ColorOut {
    var o: ColorOut;
    o.clip = g.view_proj * vec4<f32>(nudge(v.pos, 0.001), 1.0);
    o.color = v.color;
    o.local = vec2<f32>(0.0);
    o.world = vec4<f32>(v.pos, 0.0);
    return o;
}

@fragment
fn fs_fill(v: ColorOut) -> @location(0) vec4<f32> {
    return v.color;
}

// ---- depth of field: each body pixel's distance from the eye (an orthographic view measures along the view) ----

@fragment
fn fs_depth(v: MeshOut) -> @location(0) vec4<f32> {
    if (cut_away(v.world)) {
        discard;
    }
    var d = length(v.world - g.eye.xyz);
    if (g.eye.w < 0.5) {
        d = 100000.0 - dot(v.world, normalize(g.eye.xyz));
    }
    return vec4<f32>(d, 0.0, 0.0, 1.0);
}
