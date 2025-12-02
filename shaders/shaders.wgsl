// ------------------------------------------------------------
// Bindings
// ------------------------------------------------------------

// Texture + sampler (group 0 → bindings 0 and 1)
@group(0) @binding(0) 
var my_texture: texture_2d<f32>;

@group(0) @binding(1) 
var my_sampler: sampler;

// Uniform parameters (group 0 → binding 2)
struct Params {
    time: f32,
    artifact_amplifier: f32,
    crt_amount_adjusted: f32,
    bloom_fac: f32,
}

@group(0) @binding(2)
var<uniform> params: Params;


// ------------------------------------------------------------
// Vertex stage
// ------------------------------------------------------------

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}


// ------------------------------------------------------------
// Effect function (CRT / artifact logic)
// ------------------------------------------------------------

fn apply_effects(
    tc: vec2<f32>,
    offset_l: f32,
    offset_r: f32,
    rgb_result: vec3<f32>
) -> vec3<f32> {

    var color = rgb_result;

    // ----- Flicker sinus effect -----
    if (sin(params.time + tc.y * 200.0) > 0.85) {
        if (offset_l < 0.99 && offset_l > 0.01) {
            color.r = color.g * 1.5;
        }
        if (offset_r > -0.99 && offset_r < -0.01) {
            color.g = color.r * 1.5;
        }
    }

    // ----- Subtract color bias -----
    let bias = 0.55 - 0.02 * (params.artifact_amplifier - 1.0 -
               params.crt_amount_adjusted * params.bloom_fac * 0.7);

    color = color - vec3<f32>(bias);

    // ----- Multiply brightness/contrast -----
    let brightness =
        (1.0 + 0.075 +
        params.crt_amount_adjusted * (0.012 - params.bloom_fac * 0.12));

    color = color * brightness;

    // ----- Add constant offset -----
    color = color + vec3<f32>(0.5);

    return color;
}


// ------------------------------------------------------------
// Fragment stage
// ------------------------------------------------------------

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    // Sample texture
    let tex = textureSample(my_texture, my_sampler, uv);

    // Convert sampled color to mutable vec3
    var rgb = tex.rgb;

    // --------------------------------------------------------
    // Your original shader expects these values:
    // offset_l and offset_r can be anything (horizontal artifacts)
    // For now: default zero (or replace with your real values)
    // --------------------------------------------------------
    let offset_l: f32 = 0.0;
    let offset_r: f32 = 0.0;

    // Apply CRT-style effects
    let result = apply_effects(uv, offset_l, offset_r, rgb);

    return vec4<f32>(result, tex.a);
}
