// Reference pictures (Insert ▸ Image): a textured quad on its plane, see-through by its opacity.

struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    viewport: vec4<f32>,
    // w: 1 when Section View is on
    light: vec4<f32>,
    section: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
// x: opacity
@group(1) @binding(2) var<uniform> params: vec4<f32>;

struct ImageIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct ImageOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_image(v: ImageIn) -> ImageOut {
    var o: ImageOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.uv = v.uv;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_image(v: ImageOut) -> @location(0) vec4<f32> {
    if g.light.w > 0.5 && dot(g.section.xyz, v.world) > g.section.w {
        discard;
    }
    let c = textureSample(tex, samp, v.uv);
    return vec4<f32>(c.rgb, c.a * params.x);
}
