// Deep-space backdrop: three parallax layers of stars over a faint nebula.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::globals,
}
#import nullnet::noise::{fbm, hash2}

struct BackgroundParams {
    // xy: parallax offset in world units.
    offset: vec4<f32>,
    nebula_a: vec4<f32>,
    nebula_b: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> background: BackgroundParams;

// `pixel` is the size of one screen pixel in world units. It is measured once
// in `fragment`, because derivatives taken inside the per-cell branches
// below jump at cell edges and draw lines across the sky.
fn star_layer(world: vec2<f32>, pixel: f32, cell_size: f32, parallax: f32, layer: f32, t: f32) -> f32 {
    let pos = (world + background.offset.xy * parallax) / cell_size;
    let cell = floor(pos);
    let chance = hash2(cell + vec2<f32>(layer * 17.0, layer * 31.0));
    if chance < 0.86 {
        return 0.0;
    }
    let centre = vec2<f32>(
        hash2(cell + vec2<f32>(3.1, layer)),
        hash2(cell + vec2<f32>(7.7, layer + 1.0)),
    ) * 0.7 + 0.15;
    let d = length(fract(pos) - centre) * cell_size;
    let size = 0.6 + 1.4 * hash2(cell + vec2<f32>(11.3, layer));
    let twinkle = 0.75 + 0.25 * sin(t * (0.6 + chance * 2.0) + chance * 40.0);
    let aa = max(pixel, 0.5);
    return smoothstep(size + aa, size - aa, d) * (chance - 0.86) / 0.14 * twinkle;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xy;
    let pixel = length(fwidth(world)) * 0.7071;
    let t = globals.time;

    let cloud = fbm(vec3<f32>((world + background.offset.xy * 0.1) * 0.0011, t * 0.004));
    let wisps = fbm(vec3<f32>((world + background.offset.xy * 0.1) * 0.0027 + vec2<f32>(5.2, 1.3), 2.0));
    var color = mix(background.nebula_a.rgb, background.nebula_b.rgb, smoothstep(0.35, 0.8, wisps));
    color = color * smoothstep(0.38, 0.85, cloud) * 0.9;

    var stars = 0.0;
    stars = stars + star_layer(world, pixel, 90.0, 0.15, 0.0, t) * 0.5;
    stars = stars + star_layer(world, pixel, 140.0, 0.35, 1.0, t) * 0.75;
    stars = stars + star_layer(world, pixel, 230.0, 0.7, 2.0, t);
    color = color + vec3<f32>(0.85, 0.9, 1.0) * stars;

    return vec4<f32>(color, 1.0);
}
