// The map's backdrop: a faint grid under slow fog, with traces drifting
// along the grid lines, all shifted slightly with the cursor for depth.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::globals,
}
#import nullnet::noise::{fbm, hash2}

struct BackgroundParams {
    // xy: parallax offset in world units.
    offset: vec4<f32>,
    grid: vec4<f32>,
    fog_a: vec4<f32>,
    fog_b: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> background: BackgroundParams;

const CELL: f32 = 80.0;

// `pixel` is the size of one screen pixel in world units, measured once in
// `fragment` so no derivative is taken inside a branch.
fn grid_lines(world: vec2<f32>, pixel: f32) -> f32 {
    let g = abs(fract(world / CELL + 0.5) - 0.5) * CELL;
    let aa = max(pixel, 0.5);
    let line = smoothstep(aa * 1.5, 0.0, min(g.x, g.y));
    // Every fourth line is a little stronger.
    let major = abs(fract(world / (CELL * 4.0) + 0.5) - 0.5) * CELL * 4.0;
    let strong = smoothstep(aa * 1.5, 0.0, min(major.x, major.y));
    return line * 0.5 + strong * 0.5;
}

// Dashes running along horizontal grid lines, a few rows at a time.
fn traces(world: vec2<f32>, pixel: f32, t: f32) -> f32 {
    let row = floor(world.y / CELL + 0.5);
    let on = hash2(vec2<f32>(row, 7.0));
    if on < 0.8 {
        return 0.0;
    }
    let distance = abs(world.y - row * CELL);
    let aa = max(pixel, 0.5);
    let on_line = smoothstep(aa * 1.5, 0.0, distance);
    let speed = (30.0 + 60.0 * hash2(vec2<f32>(row, 3.0))) * (step(0.5, hash2(vec2<f32>(row, 5.0))) * 2.0 - 1.0);
    let along = world.x - t * speed + hash2(vec2<f32>(row, 9.0)) * 1000.0;
    let cycle = 900.0;
    let phase = fract(along / cycle) * cycle;
    let dash = smoothstep(0.0, 6.0, phase) * exp(-phase / 70.0);
    return on_line * dash * (on - 0.8) / 0.2;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xy;
    let pixel = length(fwidth(world)) * 0.7071;
    let t = globals.time;
    let shifted = world + background.offset.xy * 0.4;

    let fog = fbm(vec3<f32>(shifted * 0.0009, t * 0.02));
    let veins = fbm(vec3<f32>(shifted * 0.0023 + vec2<f32>(4.1, 2.7), t * 0.01));
    var color = mix(background.fog_a.rgb, background.fog_b.rgb, smoothstep(0.3, 0.8, veins));
    color = color * smoothstep(0.3, 0.9, fog) * 0.85;

    let lines = grid_lines(world + background.offset.xy * 0.25, pixel);
    color = color + background.grid.rgb * lines * 0.35;
    color = color + background.grid.rgb * traces(world + background.offset.xy * 0.25, pixel, t) * 1.6;

    return vec4<f32>(color, 1.0);
}
