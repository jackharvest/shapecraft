// Decals (Visualization): a picture projected onto a body, drawn over the body's own triangles.

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    viewport: vec4<f32>,
    // xyz: towards the light; w: 1 when Section View is on
    light: vec4<f32>,
    section: vec4<f32>,
    analysis: vec4<f32>,
    // x: light intensity (Visualization)
    visual: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct Decal {
    // xyz centre, w opacity
    center: vec4<f32>,
    // the picture's right and up (unit) with half its width and height
    right: vec4<f32>,
    up: vec4<f32>,
    // xyz out of the surface; w 1 = wrapped around an axis
    out: vec4<f32>,
    // cylindrical: a point on the axis (which runs along `up`) and the radius
    axis: vec4<f32>,
    // x, y: cos and sin of the rotation; z: the only face it shows on (-1: any facing it); w: how far it reaches
    misc: vec4<f32>,
};
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(1) @binding(2) var<uniform> d: Decal;

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) face: u32,
    @location(3) curv: f32,
};

struct DecalOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) face: u32,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_decal(v: MeshIn) -> DecalOut {
    var o: DecalOut;
    // A little towards the eye, so the picture wins over the face it lies on.
    let to_eye = g.eye.xyz - v.pos * g.eye.w;
    o.clip = g.view_proj * vec4<f32>(v.pos + to_eye * 0.0008, 1.0);
    o.normal = v.normal;
    o.face = v.face;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_decal(v: DecalOut) -> @location(0) vec4<f32> {
    let n = normalize(v.normal);
    if (d.out.w > 1.5) {
        return material(v, n);
    }
    let rel = v.world - d.center.xyz;
    var s = dot(rel, d.right.xyz);
    var t = dot(rel, d.up.xyz);
    var facing = dot(n, d.out.xyz);
    var reach = abs(dot(rel, d.out.xyz));
    if (d.out.w > 0.5) {
        // Wrapped: the arc length around the axis, and the height along it.
        let q = v.world - d.axis.xyz;
        let r = q - d.up.xyz * dot(q, d.up.xyz);
        let len = max(length(r), 1e-6);
        s = atan2(dot(r, d.right.xyz), dot(r, d.out.xyz)) * d.axis.w;
        facing = dot(n, r / len);
        reach = abs(len - d.axis.w);
    }
    // Turned by the rotation (the picture turns counter-clockwise, so it's sampled the other way).
    let rs = d.misc.x * s + d.misc.y * t;
    let rt = -d.misc.y * s + d.misc.x * t;
    let uv = vec2<f32>(rs / (2.0 * d.right.w) + 0.5, 0.5 - rt / (2.0 * d.up.w));
    let c = textureSampleLevel(tex, samp, uv, 0.0);
    let face_ok = d.misc.z < 0.0 || abs(f32(v.face) - d.misc.z) < 0.5;
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || facing < 0.2 || reach > d.misc.w || !face_ok) {
        discard;
    }
    if (g.light.w > 0.5 && dot(g.section.xyz, v.world) > g.section.w) {
        discard;
    }
    // Lit a little by the surface's light: printed pictures stay bright on the darker walls (s13-009).
    let l = normalize(g.light.xyz);
    let key = pow(max(dot(n, l), 0.0), 1.5);
    let top = pow(l.z, 1.5);
    let shade = mix(1.0, min((0.08 + 0.92 * key) / (0.08 + 0.92 * top), 1.0), 0.35);
    return vec4<f32>(c.rgb * shade * g.visual.x, c.a * d.center.w);
}

// A custom material: the picture tiled over the body, each face taking the two axes it faces least (box projection),
// lit like a matte body.
fn material(v: DecalOut, n: vec3<f32>) -> vec4<f32> {
    if (g.light.w > 0.5 && dot(g.section.xyz, v.world) > g.section.w) {
        discard;
    }
    let q = v.world - d.center.xyz;
    let a = abs(n);
    var st = vec2<f32>(q.x, q.y);
    if (a.x >= a.y && a.x >= a.z) {
        st = vec2<f32>(q.y, q.z);
    } else if (a.y >= a.z) {
        st = vec2<f32>(q.x, q.z);
    }
    let uv = vec2<f32>(fract(st.x / d.right.w), fract(-st.y / d.up.w));
    let c = textureSampleLevel(tex, samp, uv, 0.0);
    let l = normalize(g.light.xyz);
    let key = pow(max(dot(n, l), 0.0), 1.5);
    let top = pow(l.z, 1.5);
    let shade = (0.08 + 0.92 * key) / (0.08 + 0.92 * top);
    return vec4<f32>(c.rgb * shade * g.visual.x, d.center.w);
}
