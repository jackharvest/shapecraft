// Depth of field (Visualization ▸ Camera): the sharp view mixed with its blurred copy by how far each pixel lies
// from the focus distance.

@group(0) @binding(0) var sharp: texture_2d<f32>;
@group(0) @binding(1) var blurred: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
@group(0) @binding(3) var depth: texture_2d<f32>;
// x: focus distance, y: the distance differences are measured against, z: strength
@group(0) @binding(4) var<uniform> p: vec4<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: Out;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

@fragment
fn fs_dof(v: Out) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(depth));
    let px = vec2<i32>(clamp(v.uv * size, vec2<f32>(0.0), size - vec2<f32>(1.0)));
    let z = textureLoad(depth, px, 0).r;
    // The backdrop (nothing drawn) is one color, so blurring it only spreads the bodies' outlines: as much as the
    // aperture asks for, not all the way.
    var coc = clamp(p.z * 0.1, 0.0, 1.0);
    if (z > 0.0) {
        coc = clamp(abs(z - p.x) / max(p.y, 1e-6) * p.z, 0.0, 1.0);
    }
    let a = textureSampleLevel(sharp, samp, v.uv, 0.0);
    let b = textureSampleLevel(blurred, samp, v.uv, 0.0);
    return mix(a, b, smoothstep(0.0, 1.0, coc));
}
