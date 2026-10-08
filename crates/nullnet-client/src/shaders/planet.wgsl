// Procedural planet, ring and star rendering on a flat quad.
//
// The quad spans `quad.x` planet radii in each direction from the centre;
// the planet itself is the unit disc. Colours arrive in linear space.

#import bevy_sprite::{
    mesh2d_vertex_output::VertexOutput,
    mesh2d_view_bindings::globals,
}
#import nullnet::noise::fbm

struct PlanetParams {
    color_a: vec4<f32>,
    color_b: vec4<f32>,
    color_c: vec4<f32>,
    // rgb: atmosphere colour, a: strength.
    atmosphere: vec4<f32>,
    // x: kind, y: noise scale, z: spin speed, w: seed.
    shape: vec4<f32>,
    // x: ring inner radius, y: ring outer radius (0 = no ring), z: ring tilt, w: hover highlight.
    extra: vec4<f32>,
    // x: quad half extent in planet radii.
    quad: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> planet: PlanetParams;

const KIND_ROCKY: f32 = 0.0;
const KIND_GAS: f32 = 1.0;
const KIND_EARTH: f32 = 2.0;
const KIND_ICE: f32 = 3.0;
const KIND_STAR: f32 = 4.0;

const SUN_DIRECTION: vec3<f32> = vec3<f32>(-0.86, 0.22, 0.46);

fn spin_y(n: vec3<f32>, angle: f32) -> vec3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    return vec3<f32>(n.x * c + n.z * s, n.y, -n.x * s + n.z * c);
}

fn seed_offset() -> vec3<f32> {
    let s = planet.shape.w;
    return vec3<f32>(s * 13.1, s * 7.7, s * 3.3);
}

// Albedo at surface normal `n`; `ocean` reports how much of the point is water.
fn surface(n: vec3<f32>, t: f32, ocean: ptr<function, f32>) -> vec3<f32> {
    let kind = planet.shape.x;
    let scale = planet.shape.y;
    let seed = seed_offset();
    let q = spin_y(n, t * planet.shape.z);
    *ocean = 0.0;

    if kind < KIND_GAS - 0.5 {
        let h = fbm(q * scale + seed);
        let detail = fbm(q * scale * 4.0 + seed.zxy);
        let base = mix(planet.color_a.rgb, planet.color_b.rgb, smoothstep(0.3, 0.7, h));
        return base * (0.75 + 0.5 * detail);
    }
    if kind < KIND_EARTH - 0.5 {
        let warp = fbm(q * scale + seed);
        let band = 0.5 + 0.5 * sin(q.y * 13.0 + warp * 6.0);
        var color = mix(planet.color_a.rgb, planet.color_b.rgb, band);
        let storms = smoothstep(0.58, 0.8, fbm(q * scale * 2.3 + seed.zxy));
        return mix(color, planet.color_c.rgb, storms * 0.7);
    }
    if kind < KIND_ICE - 0.5 {
        let h = fbm(q * scale + seed);
        let land = smoothstep(0.5, 0.53, h);
        *ocean = 1.0 - land;
        var color = mix(planet.color_a.rgb * (0.8 + 0.4 * h), planet.color_b.rgb, land);
        color = mix(color, vec3<f32>(0.42, 0.36, 0.25), land * smoothstep(0.62, 0.78, h));
        let ice = smoothstep(0.8, 0.88, abs(q.y) + (h - 0.5) * 0.25);
        color = mix(color, vec3<f32>(0.9, 0.93, 0.97), ice);
        let cq = spin_y(n, t * planet.shape.z * 1.35);
        let clouds = smoothstep(0.52, 0.74, fbm(cq * scale * 1.6 + seed.yzx));
        *ocean = *ocean * (1.0 - clouds) * (1.0 - ice);
        return mix(color, planet.color_c.rgb, clouds * 0.9);
    }
    let h = fbm(q * scale + seed);
    let cracks = 1.0 - smoothstep(0.0, 0.04, abs(fbm(q * scale * 2.0 + seed.yxz) - 0.5));
    return mix(planet.color_a.rgb, planet.color_b.rgb, h) - cracks * 0.12;
}

// Colour and coverage of the ring at `p`, or zero alpha where there is none.
fn ring_layer(p: vec2<f32>) -> vec4<f32> {
    let inner = planet.extra.x;
    let outer = planet.extra.y;
    if outer <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rr = length(vec2<f32>(p.x, p.y / planet.extra.z));
    if rr < inner || rr > outer {
        return vec4<f32>(0.0);
    }
    let u = (rr - inner) / (outer - inner);
    let bands = fbm(vec3<f32>(rr * 9.0, 0.5, planet.shape.w));
    let gap = smoothstep(0.015, 0.0, abs(u - 0.62)) * 0.85;
    let edge = smoothstep(0.0, 0.06, u) * smoothstep(1.0, 0.9, u);
    let density = (0.35 + 0.65 * bands) * (1.0 - gap) * edge;
    let color = mix(planet.color_c.rgb, planet.color_b.rgb, bands) * 0.95;
    return vec4<f32>(color, density * 0.85);
}

fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    if a <= 0.0 {
        return vec4<f32>(0.0);
    }
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a;
    return vec4<f32>(rgb, a);
}

fn star(p: vec2<f32>, r: f32, t: f32, extent: f32) -> vec4<f32> {
    let aa = fwidth(r) * 1.5;
    var out = vec4<f32>(0.0);
    if r > 1.0 - aa {
        let flicker = fbm(vec3<f32>(normalize(p) * 2.5, t * 0.08));
        // Fade out before the quad edge so the corona never shows a seam.
        let fade = smoothstep(extent, extent * 0.7, r);
        let corona = exp(-(r - 1.0) * 3.6) * (0.7 + 0.3 * flicker) * fade;
        out = vec4<f32>(planet.color_c.rgb * 1.2, clamp(corona, 0.0, 1.0));
    }
    if r < 1.0 {
        let z = sqrt(max(1.0 - r * r, 0.0));
        let n = vec3<f32>(p, z);
        let granules = fbm(n * 7.0 + vec3<f32>(0.0, 0.0, t * 0.05));
        let limb = pow(z, 0.45);
        let color = mix(planet.color_b.rgb, planet.color_a.rgb, limb) * (0.85 + 0.3 * granules);
        let disc = vec4<f32>(color * 1.25, smoothstep(1.0, 1.0 - aa, r));
        out = over(out, disc);
    }
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let extent = planet.quad.x;
    let p = vec2<f32>(in.uv.x - 0.5, 0.5 - in.uv.y) * 2.0 * extent;
    let r = length(p);
    let t = globals.time;

    if planet.shape.x > KIND_STAR - 0.5 {
        return star(p, r, t, extent);
    }

    let light = normalize(SUN_DIRECTION);
    let highlight = planet.extra.w;
    let aa = fwidth(r) * 1.5;

    // Rings are tilted; the half nearer the viewer passes in front of the planet.
    let ring_angle = -0.32;
    let pr = vec2<f32>(
        p.x * cos(ring_angle) - p.y * sin(ring_angle),
        p.x * sin(ring_angle) + p.y * cos(ring_angle),
    );
    let ring = ring_layer(pr);
    let ring_in_front = pr.y < 0.0;

    var out = vec4<f32>(0.0);

    // Atmosphere glow beyond the limb, brighter on the day side.
    let strength = planet.atmosphere.a;
    if r > 1.0 - aa && strength > 0.0 {
        let width = 0.16;
        let fall = clamp(1.0 - (r - 1.0) / width, 0.0, 1.0);
        let day = smoothstep(-0.45, 0.6, dot(vec3<f32>(p / max(r, 1e-4), 0.0), light));
        out = vec4<f32>(planet.atmosphere.rgb, pow(fall, 3.0) * strength * (0.15 + 0.85 * day));
    }

    if !ring_in_front {
        out = over(out, ring);
    }

    if r < 1.0 {
        let z = sqrt(max(1.0 - r * r, 0.0));
        let n = vec3<f32>(p, z);
        var ocean = 0.0;
        let albedo = surface(n, t, &ocean);
        let d = dot(n, light);
        let lit = 0.03 + 1.05 * smoothstep(-0.12, 0.85, d);
        let glint = pow(max(dot(reflect(-light, n), vec3<f32>(0.0, 0.0, 1.0)), 0.0), 40.0) * ocean * 0.55;
        let rim = pow(1.0 - z, 2.5) * strength * (0.2 + max(d, 0.0));
        var color = albedo * lit + vec3<f32>(glint) + planet.atmosphere.rgb * rim;
        color = color * (1.0 + 0.18 * highlight);
        out = over(out, vec4<f32>(color, smoothstep(1.0, 1.0 - aa, r)));
    }

    if ring_in_front {
        out = over(out, ring);
    }

    // Selection halo.
    if highlight > 0.0 {
        let halo = smoothstep(0.035, 0.0, abs(r - 1.24)) * highlight * 0.8;
        out = over(out, vec4<f32>(0.55, 0.85, 1.0, halo));
    }

    return out;
}
