#define_import_path nullnet::noise

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * 0.3183099 + vec3<f32>(0.71, 0.113, 0.419));
    let r = q * 17.0;
    return fract(r.x * r.y * r.z * (r.x + r.y + r.z));
}

fn hash2(p: vec2<f32>) -> f32 {
    return hash3(vec3<f32>(p, 0.37));
}

// Smooth value noise in [0, 1].
fn noise3(x: vec3<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(
            mix(hash3(i + vec3<f32>(0.0, 0.0, 0.0)), hash3(i + vec3<f32>(1.0, 0.0, 0.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 0.0)), hash3(i + vec3<f32>(1.0, 1.0, 0.0)), u.x),
            u.y,
        ),
        mix(
            mix(hash3(i + vec3<f32>(0.0, 0.0, 1.0)), hash3(i + vec3<f32>(1.0, 0.0, 1.0)), u.x),
            mix(hash3(i + vec3<f32>(0.0, 1.0, 1.0)), hash3(i + vec3<f32>(1.0, 1.0, 1.0)), u.x),
            u.y,
        ),
        u.z,
    );
}

// Five octaves of value noise, roughly in [0, 1].
fn fbm(p: vec3<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var x = p;
    for (var i = 0; i < 5; i = i + 1) {
        value = value + amplitude * noise3(x);
        x = x * 2.03 + vec3<f32>(1.7, 9.2, 3.1);
        amplitude = amplitude * 0.5;
    }
    return value / 0.97;
}
