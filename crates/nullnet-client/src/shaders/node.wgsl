// Nodes on the network map: hexagonal hosts, round subsystems, the backbone
// beam and fields of abandoned caches, all drawn on flat quads.
//
// The quad spans `quad.x` node radii from the centre; the node itself is the
// unit hexagon or disc. Colours arrive in linear space.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::globals,
}
#import nullnet::noise::{fbm, hash2}

struct NodeParams {
    fill: vec4<f32>,
    // rgb: firewall glow, a: strength.
    ring: vec4<f32>,
    // x: kind, y: seed, z: hover highlight, w: pulse rate.
    shape: vec4<f32>,
    // x: quad half extent in node radii, y: height over width, z: flash
    // (1 just after the host changed hands, fading to 0).
    quad: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> node: NodeParams;

const KIND_HOST: f32 = 0.0;
const KIND_SUBSYSTEM: f32 = 1.0;
const KIND_BACKBONE: f32 = 2.0;
const KIND_CACHE_FIELD: f32 = 3.0;

// Signed distance to a pointy-top hexagon of circumradius `r`.
fn sd_hex(p: vec2<f32>, r: f32) -> f32 {
    let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
    var q = abs(p);
    q = q - 2.0 * min(dot(k.xy, q), 0.0) * k.xy;
    q = q - vec2<f32>(clamp(q.x, -k.z * r, k.z * r), r);
    return length(q) * sign(q.y);
}

fn sd_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

fn rotate(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
}

fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    if a <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a;
    return vec4<f32>(rgb, a);
}

// A host or subsystem: a firewall glow outside, a bright edge, and a dim
// interior with scanlines and drifting activity.
fn node_body(p: vec2<f32>, d: f32, t: f32) -> vec4<f32> {
    let seed = node.shape.y;
    let highlight = node.shape.z;
    let flash = node.quad.z;
    let extent = node.quad.x;
    let pulse = 0.5 + 0.5 * sin(t * node.shape.w + seed * 6.0);
    let aa = fwidth(d) * 1.5;
    var out = vec4<f32>(0.0);

    // Firewall glow in the holder's colour, faded out well before the
    // quad's edge so it never shows a seam.
    let strength = node.ring.a * (0.6 + 0.4 * pulse) * (1.0 + 0.8 * highlight + 1.5 * flash);
    if d > -aa && strength > 0.0 {
        let fade = smoothstep(extent, extent * 0.55, length(p));
        let glow = exp(-max(d, 0.0) * 3.2) * strength * fade;
        out = vec4<f32>(node.ring.rgb, clamp(glow * 0.7, 0.0, 1.0));
    }

    if d < aa {
        let activity = fbm(vec3<f32>(p * 2.2 + seed, t * 0.12));
        let scan = 0.02 * sin(p.y * 26.0 - t * 1.5);
        var color = node.fill.rgb * (1.0 + 0.6 * activity) + scan;
        // Activity drifting across the interior, in the firewall colour.
        let traffic = smoothstep(0.55, 0.85, fbm(vec3<f32>(p * 4.0 + seed * 3.0, t * 0.35)));
        color = color + node.ring.rgb * traffic * 0.35;
        color = color * (1.0 + 0.25 * highlight + 0.8 * flash);
        out = over(out, vec4<f32>(color, smoothstep(aa, -aa, d)));
    }

    // Edge line just inside the outline.
    let edge = smoothstep(0.075 + aa, 0.0, abs(d + 0.05));
    let edge_color = mix(node.fill.rgb * 2.5, node.ring.rgb, 0.7);
    out = over(out, vec4<f32>(edge_color, edge * (0.75 + 0.25 * highlight)));

    // Selection halo, and a ring that spreads out from a host that just
    // changed hands.
    if highlight > 0.0 {
        let halo = smoothstep(0.045, 0.0, abs(d - 0.28)) * highlight * 0.8;
        out = over(out, vec4<f32>(0.75, 0.93, 1.0, halo));
    }
    if flash > 0.0 {
        let radius = 0.1 + (1.0 - flash) * 0.9;
        let ring = smoothstep(0.06, 0.0, abs(d - radius)) * flash;
        out = over(out, vec4<f32>(1.0, 1.0, 1.0, ring * 0.9));
    }
    return out;
}

// The backbone: a vertical beam with traffic streaming up it.
fn backbone(p: vec2<f32>, t: f32) -> vec4<f32> {
    let aspect = node.quad.y;
    let x = p.x;
    let y = p.y;
    let flow = fbm(vec3<f32>(x * 2.0 + node.shape.y, y * 0.8 - t * 0.45, 1.5));
    let core = exp(-abs(x) * 9.0);
    let beam = exp(-abs(x) * 2.2) * 0.4;

    var packets = 0.0;
    for (var i = 0; i < 7; i = i + 1) {
        let h = hash2(vec2<f32>(f32(i) * 3.7, node.shape.y));
        let lane = (hash2(vec2<f32>(f32(i) * 1.3, 2.0)) - 0.5) * 0.5;
        let speed = 0.35 + 0.5 * h;
        let pos = (fract(t * speed * 0.25 + h) * 2.0 - 1.0) * aspect;
        let behind = pos - y;
        if behind >= 0.0 && behind < 0.9 {
            packets = packets + exp(-behind * 5.0) * exp(-abs(x - lane) * 14.0);
        }
    }

    let fade = smoothstep(aspect, aspect * 0.85, abs(y));
    let color = node.fill.rgb * beam * (0.7 + 0.6 * flow) * 2.0
        + node.ring.rgb * (core * (0.55 + 0.45 * flow) + packets * 1.3);
    let alpha = clamp(beam * 1.6 + core + packets, 0.0, 1.0) * fade;
    return vec4<f32>(color, alpha);
}

// A field of abandoned caches: scattered fragments, some still blinking.
fn cache_field(p: vec2<f32>, t: f32) -> vec4<f32> {
    let seed = node.shape.y;
    let highlight = node.shape.z;
    var out = vec4<f32>(0.0);
    for (var i = 0; i < 14; i = i + 1) {
        let fi = f32(i);
        let a = hash2(vec2<f32>(fi, seed)) * 6.2832;
        let r = 0.25 + 0.65 * hash2(vec2<f32>(fi * 2.1, seed + 1.0));
        let centre = vec2<f32>(cos(a), sin(a)) * r;
        let size = 0.06 + 0.1 * hash2(vec2<f32>(fi * 3.3, seed + 2.0));
        let tilt = hash2(vec2<f32>(fi * 4.7, seed + 3.0)) * 3.1416;
        let d = sd_box(rotate(p - centre, tilt), vec2<f32>(size, size * 0.7));
        let aa = fwidth(d) * 1.5;
        let blink = step(0.7, hash2(vec2<f32>(fi * 5.1, seed + 4.0)))
            * (0.5 + 0.5 * sin(t * (1.0 + fi * 0.3) + fi));
        let body = smoothstep(aa, -aa, d);
        let glow = exp(-max(d, 0.0) * 6.0) * blink * 0.6;
        let color = node.fill.rgb * (0.8 + 0.6 * blink) * (1.0 + 0.3 * highlight) + node.ring.rgb * glow;
        out = over(out, vec4<f32>(color, max(body * 0.9, glow)));
    }
    if highlight > 0.0 {
        let halo = smoothstep(0.03, 0.0, abs(length(p) - 1.05)) * highlight * 0.7;
        out = over(out, vec4<f32>(0.75, 0.93, 1.0, halo));
    }
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let extent = node.quad.x;
    let p = vec2<f32>(in.uv.x - 0.5, 0.5 - in.uv.y) * 2.0 * extent;
    let t = globals.time;
    let kind = node.shape.x;

    if kind > KIND_CACHE_FIELD - 0.5 {
        return cache_field(p, t);
    }
    if kind > KIND_BACKBONE - 0.5 {
        return backbone(vec2<f32>(p.x, p.y * node.quad.y), t);
    }
    if kind > KIND_SUBSYSTEM - 0.5 {
        return node_body(p, length(p) - 1.0, t);
    }
    return node_body(p, sd_hex(p, 1.0), t);
}
