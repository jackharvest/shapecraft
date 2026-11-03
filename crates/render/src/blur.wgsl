// Frosted glass behind the sidebars: the viewport shrunk 8× (an 8×8 box per texel), then a gaussian along x and y.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// One triangle covering the target.
@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> Out {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var o: Out;
    o.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    o.uv = vec2<f32>(x, y);
    return o;
}

// 16 bilinear taps at ±1 and ±3 source texels average the 8×8 block under this texel.
@fragment
fn fs_down(v: Out) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src));
    var sum = vec4<f32>(0.0);
    for (var j = 0; j < 4; j++) {
        for (var i = 0; i < 4; i++) {
            let off = vec2<f32>(f32(i * 2 - 3), f32(j * 2 - 3));
            sum += textureSampleLevel(src, samp, v.uv + off * texel, 0.0);
        }
    }
    return sum / 16.0;
}

fn gauss(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let w = array<f32, 5>(0.2042, 0.1802, 0.1238, 0.0663, 0.0276);
    var sum = textureSampleLevel(src, samp, uv, 0.0) * w[0];
    for (var k = 1; k < 5; k++) {
        let d = dir * f32(k);
        sum += (textureSampleLevel(src, samp, uv + d, 0.0) + textureSampleLevel(src, samp, uv - d, 0.0)) * w[k];
    }
    return sum;
}

@fragment
fn fs_h(v: Out) -> @location(0) vec4<f32> {
    return gauss(v.uv, vec2<f32>(1.0 / f32(textureDimensions(src).x), 0.0));
}

@fragment
fn fs_v(v: Out) -> @location(0) vec4<f32> {
    return gauss(v.uv, vec2<f32>(0.0, 1.0 / f32(textureDimensions(src).y)));
}
