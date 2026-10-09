// A panel in Deus Ex's style: a dark fill with a thin gold line, the top
// left and bottom right corners cut off, brighter brackets on the other
// two corners, fine scanlines and an optional header band. The node's size
// comes from the UI, so the cuts and lines stay the same size in pixels.

#import bevy_ui::ui_vertex_output::UiVertexOutput

struct PanelParams {
    fill: vec4<f32>,
    line: vec4<f32>,
    accent: vec4<f32>,
    // x: corner cut (px), y: line width (px), z: scanline strength,
    // w: header band height (px), 0 for none.
    shape: vec4<f32>,
};

@group(1) @binding(0) var<uniform> panel: PanelParams;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let size = in.size;
    let p = in.uv * size;
    let cut = panel.shape.x;
    let width = panel.shape.y;

    // Distance inside the panel: the edges and the two cut corners.
    let edge = min(min(p.x, size.x - p.x), min(p.y, size.y - p.y));
    let cut_tl = (p.x + p.y - cut) * 0.70710678;
    let cut_br = ((size.x - p.x) + (size.y - p.y) - cut) * 0.70710678;
    let d = min(edge, min(cut_tl, cut_br));
    let coverage = clamp(d + 0.5, 0.0, 1.0);
    if (coverage <= 0.0) {
        discard;
    }

    var rgb = panel.fill.rgb;
    var alpha = panel.fill.a;

    // A faint gold glow from the top edge, and scanlines.
    let glow = exp(-p.y / max(size.y * 0.3, 1.0));
    rgb += panel.line.rgb * 0.05 * glow;
    let scan = 0.5 + 0.5 * cos(p.y * 3.14159265);
    rgb *= 1.0 - panel.shape.z * 0.22 * scan;

    // The header band, with a line under it.
    let header = panel.shape.w;
    if (header > 0.0 && p.y < header) {
        rgb = mix(rgb, panel.accent.rgb, 0.12);
    }
    if (header > 0.0) {
        let under = 1.0 - smoothstep(0.0, 1.0, abs(p.y - header));
        rgb = mix(rgb, panel.line.rgb, under * 0.7);
    }

    // The line around the panel.
    let on_line = 1.0 - smoothstep(width - 0.5, width + 0.5, d);
    rgb = mix(rgb, panel.line.rgb, on_line * panel.line.a);
    alpha = mix(alpha, 1.0, on_line * panel.line.a);

    // Brighter brackets on the corners that are not cut, and on the cuts.
    let tick = min(16.0, min(size.x, size.y) * 0.3);
    let thick = width + 1.0;
    let top_right = (size.x - p.x < tick && p.y < thick) || (p.y < tick && size.x - p.x < thick);
    let bottom_left = (p.x < tick && size.y - p.y < thick) || (size.y - p.y < tick && p.x < thick);
    let on_cut = min(cut_tl, cut_br) < thick && cut > 0.0;
    if (top_right || bottom_left || on_cut) {
        rgb = panel.accent.rgb;
        alpha = max(alpha, panel.accent.a);
    }

    return vec4<f32>(rgb, alpha * coverage);
}
