struct CompositeOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var scene_color: texture_2d<f32>;

@group(0) @binding(1)
var scene_sampler: sampler;

@vertex
fn vs_composite(@builtin(vertex_index) vertex_index: u32) -> CompositeOutput {
    var output: CompositeOutput;
    switch vertex_index {
        case 0u: {
            output.position = vec4<f32>(-1.0, -1.0, 0.0, 1.0);
            output.uv = vec2<f32>(0.0, 1.0);
        }
        case 1u: {
            output.position = vec4<f32>(3.0, -1.0, 0.0, 1.0);
            output.uv = vec2<f32>(2.0, 1.0);
        }
        default: {
            output.position = vec4<f32>(-1.0, 3.0, 0.0, 1.0);
            output.uv = vec2<f32>(0.0, -1.0);
        }
    }
    return output;
}

@fragment
fn fs_composite(input: CompositeOutput) -> @location(0) vec4<f32> {
    return textureSample(scene_color, scene_sampler, input.uv);
}
