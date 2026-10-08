// A data line between two nodes: a faint line with packets streaming along
// it. The quad's u axis runs along the line.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::globals,
}
#import nullnet::noise::hash2

struct LinkParams {
    color: vec4<f32>,
    // x: length, y: thickness (world units), z: packet speed, w: seed.
    shape: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> link: LinkParams;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let u = in.uv.x;
    let v = (in.uv.y - 0.5) * 2.0;
    let length = link.shape.x;
    let speed = link.shape.z;
    let seed = link.shape.w;
    let t = globals.time;

    let across = exp(-abs(v) * 3.2);
    var packets = 0.0;
    for (var i = 0; i < 4; i = i + 1) {
        let h = hash2(vec2<f32>(f32(i) * 2.9, seed));
        let pos = fract(t * speed * (0.6 + 0.8 * h) / length + h);
        // A bright head with a tail behind it.
        let behind = (pos - u) * length;
        if behind >= 0.0 && behind < 60.0 {
            packets = packets + exp(-behind / 12.0);
        }
    }
    // Fade at both ends so the line never pokes through a node.
    let ends = smoothstep(0.0, 0.03, u) * smoothstep(1.0, 0.97, u);
    let color = link.color.rgb * (0.45 + 1.4 * packets);
    let alpha = across * clamp(0.5 + packets, 0.0, 1.0) * ends;
    return vec4<f32>(color, alpha);
}
