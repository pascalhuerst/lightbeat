use super::*;

// ---------------------------------------------------------------------------
// Free functions (no &self needed)
// ---------------------------------------------------------------------------

/// Get port screen pos using precomputed node rects (for use during interaction handling).
pub(super) fn port_screen_pos_from_rects(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    node_rects: &[Rect],
    port_id: PortId,
    zoom: f32,
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let rect = node_rects[idx];
    Some(nodes[idx].port_pos(rect, port_id.dir, port_id.index, zoom))
}


/// What a level's wires need to know about each node, gathered once per frame:
/// the port types (with promoted ports), which ports are hidden and the layout.
/// Without it every wire end rebuilds port lists and locks the node's mutex.
pub(super) struct PortCache {
    index: std::collections::HashMap<NodeId, usize>,
    nodes: Vec<CachedPorts>,
}

struct CachedPorts {
    layout: NodeLayout,
    in_types: Vec<PortType>,
    out_types: Vec<PortType>,
    in_hidden: Vec<bool>,
    out_hidden: Vec<bool>,
}

impl PortCache {
    pub(super) fn build(level: &GraphLevel) -> Self {
        let index = level.states.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
        let nodes = level.nodes.iter().map(|n| CachedPorts {
            layout: n.layout(),
            in_types: n.ui_inputs_full().iter().map(|p| p.def.port_type).collect(),
            out_types: n.ui_outputs().iter().map(|p| p.def.port_type).collect(),
            in_hidden: n.hidden_ports(PortDir::Input),
            out_hidden: n.hidden_ports(PortDir::Output),
        }).collect();
        Self { index, nodes }
    }

    pub(super) fn index_of(&self, id: NodeId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    pub(super) fn port_type(&self, port_id: PortId) -> Option<PortType> {
        let n = &self.nodes[self.index_of(port_id.node)?];
        let types = match port_id.dir {
            PortDir::Input => &n.in_types,
            PortDir::Output => &n.out_types,
        };
        types.get(port_id.index).copied()
    }

    /// Same as `NodeWidget::port_pos` for the node drawn in `node_rects[i]`.
    pub(super) fn port_pos(&self, node_rects: &[Rect], port_id: PortId, zoom: f32) -> Option<Pos2> {
        let i = self.index_of(port_id.node)?;
        let n = &self.nodes[i];
        let (types, hidden) = match port_id.dir {
            PortDir::Input => (&n.in_types, &n.in_hidden),
            PortDir::Output => (&n.out_types, &n.out_hidden),
        };
        Some(port_pos_in(node_rects[i], &n.layout, port_id.dir, types, hidden, port_id.index, zoom))
    }
}

/// Rect of the title-bar enable/disable toggle (a small button at the
/// right edge of the title bar). Shared between the renderer and the
/// interaction hit-test so the click region and the visible glyph are
/// always aligned.
pub(super) fn title_toggle_rect(node_rect: Rect, title_height: f32, zoom: f32) -> Rect {
    let title_h = title_height * zoom;
    let btn_size = 14.0 * zoom;
    let inset = 5.0 * zoom;
    let cx = node_rect.max.x - inset - btn_size * 0.5;
    let cy = node_rect.min.y + title_h * 0.5;
    Rect::from_center_size(Pos2::new(cx, cy), Vec2::splat(btn_size))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_node_chrome(
    painter: &Painter,
    title: &str,
    resizable: bool,
    accent: Option<Color32>,
    accent_icon: Option<&str>,
    inputs: &[UiPortDef],
    outputs: &[UiPortDef],
    hidden_in: &[bool],
    hidden_out: &[bool],
    in_highlights: &[f32],
    out_highlights: &[f32],
    disabled: bool,
    rect: Rect,
    layout: &NodeLayout,
    selected: bool,
    portal_peer: bool,
    show_port_labels: bool,
    // `time` is seconds since an arbitrary epoch (callers pass
    // `ctx.input(|i| i.time)`). Used to animate the portal-peer halo.
    time: f64,
    zoom: f32,
) {
    // Shadow
    if !layout.bare {
        let shadow_rect = rect.translate(Vec2::new(3.0, 3.0));
        painter.rect_filled(shadow_rect, NODE_CORNER_RADIUS, Color32::from_black_alpha(60));
    }

    // Link halo: every node in the same linked group as a selected node (a
    // portal's peers, an effect stack's effects) pulses amber.
    // Suppressed when disabled so "off" reads as pure grayscale.
    if !disabled && portal_peer {
        // 1 Hz pulse from 0 to 1, strong enough to find a node in a busy
        // graph: a glow that swells and a ring around it.
        let env = 0.5 + 0.5 * (time * std::f64::consts::TAU).sin() as f32;
        let halo = rect.expand(4.0 + 6.0 * env);
        painter.rect_filled(halo, NODE_CORNER_RADIUS + 6.0, theme::SEM_WARNING.linear_multiply(0.10 + 0.25 * env));
        painter.rect_stroke(
            halo,
            NODE_CORNER_RADIUS + 6.0,
            Stroke::new(2.0_f32, theme::SEM_WARNING.linear_multiply(0.35 + 0.65 * env)),
            egui::StrokeKind::Outside,
        );
    }

    // Body — darker flat gray when disabled so the overlay later looks
    // uniform, regular dark when enabled.
    // A bare node has no body of its own: its widget fills the node.
    if !layout.bare {
        painter.rect_filled(rect, NODE_CORNER_RADIUS, theme::BG);
    }

    // Title bar — accent-tinted when enabled; when disabled, strip the
    // accent and use a neutral darker gray so no colour leaks into the
    // title chrome.
    let title_bg = if disabled {
        theme::BG
    } else {
        accent
            .map(|a| mix_color(a, theme::BG_HIGH, 0.7))
            .unwrap_or(theme::BG_HIGH)
    };
    let title_rect = title_bar_rect(rect, layout, zoom);
    painter.rect_filled(
        title_rect,
        egui::CornerRadius { nw: NODE_CORNER_RADIUS as u8, ne: NODE_CORNER_RADIUS as u8, sw: 0, se: 0 },
        title_bg,
    );
    // Title text is nudged slightly left so it doesn't crash into the
    // disable toggle we draw on the right edge. When the widget exposes
    // an accent_icon, render it immediately before the title as a visual
    // hook (portals, macros, etc.).
    let title_color = if disabled { theme::TEXT_DIM } else { theme::TEXT_BRIGHT };
    let title_font = egui::FontId::proportional(if layout.bare { 10.0 } else { 13.0 } * zoom);
    let title = if layout.show_title { title } else { "" };
    let title_galley = painter.layout_no_wrap(title.to_string(), title_font.clone(), title_color);
    let icon_galley = accent_icon.map(|g| {
        painter.layout_no_wrap(g.to_string(), title_font.clone(), title_color)
    });
    let gap = 4.0 * zoom;
    let total_w = title_galley.size().x
        + icon_galley.as_ref().map(|g| g.size().x + gap).unwrap_or(0.0);
    // Shift the block left so it doesn't crash into the disable toggle.
    let toggle_room = if layout.toggle { 10.0 } else { 0.0 };
    let mut cursor_x = title_rect.center().x - total_w * 0.5 - toggle_room * zoom;
    let center_y = title_rect.center().y;
    if let Some(ig) = icon_galley {
        let pos = Pos2::new(cursor_x, center_y - ig.size().y * 0.5);
        cursor_x += ig.size().x + gap;
        painter.galley(pos, ig, title_color);
    }
    let title_pos = Pos2::new(cursor_x, center_y - title_galley.size().y * 0.5);
    painter.galley(title_pos, title_galley, title_color);

    // Enable / disable toggle at the right of the title bar.
    if layout.toggle {
        let toggle = title_toggle_rect(rect, layout.title_h, zoom);
        let (fg, stroke_col) = if disabled {
            (theme::STROKE, theme::TEXT_DIM)
        } else {
            (theme::SEM_SUCCESS, Color32::from_rgb(200, 240, 220))
        };
        // Outer ring.
        painter.circle_stroke(toggle.center(), toggle.width() * 0.4,
            Stroke::new(1.5 * zoom.max(0.5), stroke_col));
        // Inner "power" stub — a short vertical tick through the top of the ring.
        let tick_top = Pos2::new(toggle.center().x, toggle.center().y - toggle.height() * 0.38);
        let tick_bot = Pos2::new(toggle.center().x, toggle.center().y - toggle.height() * 0.08);
        painter.line_segment([tick_top, tick_bot], Stroke::new(1.5 * zoom.max(0.5), stroke_col));
        // Filled dot when enabled so the active state reads at a glance.
        if !disabled {
            painter.circle_filled(toggle.center(), toggle.width() * 0.15, fg);
        }
    }

    // Border. Priority: selected > disabled > portal_peer > accent > default.
    // Disabled nodes render with neutral gray chrome — only "selected"
    // wins over it since selection must always be visible.
    let border = if selected {
        Stroke::new(2.0_f32, theme::SEM_PRIMARY)
    } else if disabled {
        Stroke::new(1.0_f32, theme::STROKE)
    } else if portal_peer {
        Stroke::new(2.0_f32, theme::SEM_WARNING)
    } else if let Some(a) = accent {
        Stroke::new(1.5_f32, a)
    } else if layout.bare {
        // No frame unless something marks the node: the widget is the node.
        Stroke::NONE
    } else {
        Stroke::new(1.0_f32, theme::STROKE)
    };
    painter.rect_stroke(rect, NODE_CORNER_RADIUS, border, StrokeKind::Inside);

    // Input ports
    let in_types: Vec<PortType> = inputs.iter().map(|p| p.def.port_type).collect();
    let out_types: Vec<PortType> = outputs.iter().map(|p| p.def.port_type).collect();
    for (i, ui_port) in inputs.iter().enumerate() {
        if hidden_in.get(i).copied().unwrap_or(false) { continue; }
        let pos = port_pos_in(rect, layout, PortDir::Input, &in_types, hidden_in, i, zoom);
        let hi = in_highlights.get(i).copied().unwrap_or(0.0);
        draw_port(painter, pos, ui_port, hi, zoom, disabled);
        if show_port_labels {
            let pad = 6.0 * zoom;
            let label_x = pos.x + PORT_RADIUS * zoom + pad;
            let mut right_limit = rect.max.x - (PORT_RADIUS + 8.0) * zoom;
            if !outputs.is_empty() {
                // Output names use the other half.
                right_limit = right_limit.min(rect.center().x - 4.0 * zoom);
            }
            draw_port_label(painter, &ui_port.def.name, label_x, pos.y,
                (right_limit - label_x).max(0.0), egui::Align::LEFT, zoom);
        }
    }

    // Output ports
    for (i, ui_port) in outputs.iter().enumerate() {
        if hidden_out.get(i).copied().unwrap_or(false) { continue; }
        let pos = port_pos_in(rect, layout, PortDir::Output, &out_types, hidden_out, i, zoom);
        let hi = out_highlights.get(i).copied().unwrap_or(0.0);
        draw_port(painter, pos, ui_port, hi, zoom, disabled);
        if show_port_labels {
            let pad = 6.0 * zoom;
            let label_right = pos.x - PORT_RADIUS * zoom - pad;
            let mut left_limit = rect.min.x + (PORT_RADIUS + 8.0) * zoom;
            if !inputs.is_empty() {
                left_limit = left_limit.max(rect.center().x + 4.0 * zoom);
            }
            draw_port_label(painter, &ui_port.def.name, label_right, pos.y,
                (label_right - left_limit).max(0.0), egui::Align::RIGHT, zoom);
        }
    }

    // Resize handle (small triangle in bottom-right corner)
    if resizable {
        let s = RESIZE_HANDLE_SIZE;
        let br = rect.max;
        let handle_color = Color32::from_gray(80);
        painter.line_segment(
            [Pos2::new(br.x - s, br.y), Pos2::new(br.x, br.y - s)],
            Stroke::new(1.0_f32, handle_color),
        );
        painter.line_segment(
            [Pos2::new(br.x - s * 0.5, br.y), Pos2::new(br.x, br.y - s * 0.5)],
            Stroke::new(1.0_f32, handle_color),
        );
    }
}

use crate::theme::mix_color;

/// Solid colour swatch for hovering Color ports. Reads channels 0..3 as RGB.
pub(super) fn draw_color_preview(ui: &mut Ui, values: &[f32]) {
    let size = Vec2::new(180.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    let r = values.first().copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let g = values.get(1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    let b = values.get(2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
    painter.rect_filled(rect, 3.0,
        Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
}

/// Four-swatch row for Palette ports. Channels are laid out as 4 × RGB.
pub(super) fn draw_palette_preview(ui: &mut Ui, values: &[f32]) {
    let size = Vec2::new(180.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    let slot_w = rect.width() / 4.0;
    for i in 0..4 {
        let base = i * 3;
        let r = values.get(base).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let g = values.get(base + 1).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let b = values.get(base + 2).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let slot_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + i as f32 * slot_w, rect.min.y),
            Vec2::new(slot_w, rect.height()),
        );
        painter.rect_filled(slot_rect, 0.0,
            Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8));
    }
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
}

/// Sampled gradient preview (same renderer as Gradient Source etc., just
/// inlined here to avoid threading a `color::Gradient` helper through the
/// graph module).
pub(super) fn draw_gradient_preview(ui: &mut Ui, values: &[f32]) {
    use crate::color::Gradient;
    let size = Vec2::new(200.0, 40.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;

    // Checkerboard so alpha reads clearly.
    let cell = 5.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { theme::BG } else { theme::STROKE };
            let cr = Rect::from_min_size(
                Pos2::new(rect.min.x + x as f32 * cell, rect.min.y + y as f32 * cell),
                Vec2::splat(cell),
            ).intersect(rect);
            painter.rect_filled(cr, 0.0, color);
        }
    }

    let g = Gradient::from_channels(values);
    if !g.stops().is_empty() {
        let samples = (rect.width() as usize).max(16).min(256);
        for i in 0..samples {
            let t = i as f32 / (samples - 1).max(1) as f32;
            let x = rect.min.x + (i as f32 / samples as f32) * rect.width();
            let (rgb, alpha) = g.sample_with_alpha(t);
            let c = Color32::from_rgba_unmultiplied(
                (rgb.r.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.g.clamp(0.0, 1.0) * 255.0) as u8,
                (rgb.b.clamp(0.0, 1.0) * 255.0) as u8,
                (alpha.clamp(0.0, 1.0) * 255.0) as u8,
            );
            painter.line_segment(
                [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
                Stroke::new(rect.width() / samples as f32 + 0.5, c),
            );
        }
    }
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
}

/// Small pan/tilt crosshair preview. Pan 0..1 is x-axis, tilt 0..1 is y-axis.
pub(super) fn draw_position_preview(ui: &mut Ui, pan: f32, tilt: f32) {
    let size = Vec2::new(80.0, 80.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 3.0, theme::BG_DEEP);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
    let mid = rect.center();
    let guide = Stroke::new(0.5_f32, theme::BG_HIGH);
    painter.line_segment([Pos2::new(rect.min.x, mid.y), Pos2::new(rect.max.x, mid.y)], guide);
    painter.line_segment([Pos2::new(mid.x, rect.min.y), Pos2::new(mid.x, rect.max.y)], guide);
    let dot = Pos2::new(
        rect.min.x + pan.clamp(0.0, 1.0) * rect.width(),
        rect.min.y + tilt.clamp(0.0, 1.0) * rect.height(),
    );
    painter.circle_filled(dot, 4.0, theme::SEM_SUCCESS);
    painter.circle_stroke(dot, 4.0, Stroke::new(1.0_f32, Color32::from_gray(20)));
}

/// Mini scope for the port-hover tooltip. Plots the samples on a dark
/// background with a subtle zero line; y-scale auto-fits the observed
/// range (clamped to at least a small epsilon so constant signals still
/// render as a visible baseline).
pub(super) fn draw_scope(ui: &mut Ui, samples: &[f32], line_color: Color32) {
    let size = Vec2::new(180.0, 60.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let rect = resp.rect;
    painter.rect_filled(rect, 3.0, theme::BG_DEEP);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
    if samples.is_empty() { return; }
    let mut min_v = f32::INFINITY;
    let mut max_v = f32::NEG_INFINITY;
    for &v in samples {
        if v < min_v { min_v = v; }
        if v > max_v { max_v = v; }
    }
    // Include 0 in the visible range when the signal is unipolar so the
    // baseline sits flush with the bottom instead of floating.
    if min_v > 0.0 { min_v = 0.0; }
    if max_v < 0.0 { max_v = 0.0; }
    let range = (max_v - min_v).max(1e-3);
    let n = samples.len();
    let dx = rect.width() / (n.max(2) - 1) as f32;

    // Zero line.
    let zero_y = rect.max.y - ((0.0 - min_v) / range) * rect.height();
    if zero_y >= rect.min.y && zero_y <= rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, zero_y), Pos2::new(rect.max.x, zero_y)],
            Stroke::new(0.5_f32, theme::BG_HIGH),
        );
    }

    // Signal line.
    let pts: Vec<Pos2> = samples.iter().enumerate().map(|(i, &v)| {
        let x = rect.min.x + i as f32 * dx;
        let y = rect.max.y - ((v - min_v) / range) * rect.height();
        Pos2::new(x, y)
    }).collect();
    if pts.len() >= 2 {
        for w in pts.windows(2) {
            painter.line_segment([w[0], w[1]], Stroke::new(1.5_f32, line_color));
        }
    }
}

fn draw_port(
    painter: &Painter,
    pos: Pos2,
    ui_port: &UiPortDef,
    highlight: f32,
    zoom: f32,
    node_disabled: bool,
) {
    let type_color = ui_port.def.port_type.color();
    let fill = ui_port.fill_color.unwrap_or(type_color);
    let stroke_width = if ui_port.fill_color.is_some() { 3.0 } else { 1.5_f32 };
    let r = PORT_RADIUS * zoom;
    // Outer glow when port is transiently highlighted.
    if highlight > 0.0 && !ui_port.disabled && !node_disabled {
        let alpha = (highlight.clamp(0.0, 1.0) * 255.0) as u8;
        let glow = Color32::from_rgba_unmultiplied(255, 255, 255, alpha);
        painter.circle_filled(pos, r + 4.0 * zoom, glow.linear_multiply(0.25));
        painter.circle_stroke(pos, r + 3.0 * zoom, Stroke::new(2.0_f32, glow));
    }
    if ui_port.disabled || node_disabled {
        // Hollow grayed-out port. Node-disabled ports read the same as
        // individually-disabled ones so the whole node looks uniformly off.
        painter.circle_filled(pos, r, theme::BG);
        painter.circle_stroke(pos, r, Stroke::new(1.0_f32, theme::STROKE));
    } else {
        painter.circle_filled(pos, r, fill);
        painter.circle_stroke(pos, r, Stroke::new(stroke_width, type_color));
    }
    // Optional centered glyph (e.g. "+" for the variadic add port).
    if let Some(glyph) = ui_port.marker {
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(11.0 * zoom),
            Color32::BLACK,
        );
    }
}

/// Render a port-name label next to a port dot. `anchor_x` is where the
/// label edge closest to the dot sits; `align` picks which edge (LEFT =
/// label grows rightward from `anchor_x`, RIGHT = label grows leftward).
/// If the full text doesn't fit within `max_width`, the label is truncated
/// with an ellipsis. If even a 2-character truncation doesn't fit, the
/// label is skipped entirely — better nothing than "…".
fn draw_port_label(
    painter: &Painter,
    text: &str,
    anchor_x: f32,
    center_y: f32,
    max_width: f32,
    align: egui::Align,
    zoom: f32,
) {
    if max_width <= 0.0 || text.is_empty() { return; }
    let font = egui::FontId::proportional(11.0 * zoom);
    let color = theme::TEXT_DIM;

    let full = painter.layout_no_wrap(text.to_string(), font.clone(), color);
    let galley = if full.size().x <= max_width {
        full
    } else {
        // Try progressively shorter prefixes with an ellipsis until one fits.
        let chars: Vec<char> = text.chars().collect();
        let mut chosen = None;
        for n in (2..chars.len()).rev() {
            let trunc: String = chars[..n].iter().collect::<String>() + "…";
            let g = painter.layout_no_wrap(trunc, font.clone(), color);
            if g.size().x <= max_width {
                chosen = Some(g);
                break;
            }
        }
        match chosen { Some(g) => g, None => return }
    };

    let x = match align {
        egui::Align::LEFT => anchor_x,
        egui::Align::RIGHT => anchor_x - galley.size().x,
        egui::Align::Center => anchor_x - galley.size().x * 0.5,
    };
    let y = center_y - galley.size().y * 0.5;
    painter.galley(Pos2::new(x, y), galley, color);
}

pub(super) fn draw_grid(painter: &Painter, rect: Rect, pan: Vec2, zoom: f32) {
    painter.rect_filled(rect, 0.0, theme::BG_DEEP);

    let spacing = GRID_SPACING * zoom;
    let offset_x = pan.x.rem_euclid(spacing);
    let offset_y = pan.y.rem_euclid(spacing);

    let mut x = rect.min.x + offset_x;
    while x < rect.max.x {
        painter.line_segment(
            [Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)],
            Stroke::new(1.0_f32, theme::BG_GRID),
        );
        x += spacing;
    }
    let mut y = rect.min.y + offset_y;
    while y < rect.max.y {
        painter.line_segment(
            [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
            Stroke::new(1.0_f32, theme::BG_GRID),
        );
        y += spacing;
    }
}

/// Dashed rectangle outline used to flag crossing-mode selection lassos.
pub(super) fn draw_dashed_rect(painter: &Painter, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.0_f32, color);
    let dash = 6.0;
    let gap = 4.0;
    let seg = dash + gap;

    let draw_dashed_segment = |from: Pos2, to: Pos2| {
        let diff = to - from;
        let len = diff.length();
        if len <= 0.0 { return; }
        let dir = diff / len;
        let mut d = 0.0f32;
        while d < len {
            let end = (d + dash).min(len);
            let a = from + dir * d;
            let b = from + dir * end;
            painter.line_segment([a, b], stroke);
            d += seg;
        }
    };

    draw_dashed_segment(rect.left_top(), rect.right_top());
    draw_dashed_segment(rect.right_top(), rect.right_bottom());
    draw_dashed_segment(rect.right_bottom(), rect.left_bottom());
    draw_dashed_segment(rect.left_bottom(), rect.left_top());
}

/// Is `inner` fully contained within `outer` (edges touching counts)?
pub(super) fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    outer.min.x <= inner.min.x
        && outer.min.y <= inner.min.y
        && outer.max.x >= inner.max.x
        && outer.max.y >= inner.max.y
}

/// Sample a wire bezier between `from` and `to` at `segments+1` points.
/// Shared by the renderer and hit-test paths so selection matches the
/// curve the user sees.
pub(super) fn bezier_sample(from: Pos2, to: Pos2, segments: usize) -> Vec<Pos2> {
    let dx = (to.x - from.x).abs() * 0.5;
    let ctrl1 = Pos2::new(from.x + dx, from.y);
    let ctrl2 = Pos2::new(to.x - dx, to.y);
    let mut points = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f32 / segments as f32;
        let inv = 1.0 - t;
        let x = inv * inv * inv * from.x
            + 3.0 * inv * inv * t * ctrl1.x
            + 3.0 * inv * t * t * ctrl2.x
            + t * t * t * to.x;
        let y = inv * inv * inv * from.y
            + 3.0 * inv * inv * t * ctrl1.y
            + 3.0 * inv * t * t * ctrl2.y
            + t * t * t * to.y;
        points.push(Pos2::new(x, y));
    }
    points
}

pub(super) fn draw_bezier(painter: &Painter, from: Pos2, to: Pos2, color: Color32, thickness: f32) {
    // A bezier stays inside the hull of its control points, so a wire whose
    // hull is off screen cannot be seen. Most wires are off screen once the
    // user zooms in.
    let dx = (to.x - from.x).abs() * 0.5;
    let hull = Rect::from_points(&[
        from,
        to,
        Pos2::new(from.x + dx, from.y),
        Pos2::new(to.x - dx, to.y),
    ])
    .expand(thickness);
    if !painter.clip_rect().intersects(hull) {
        return;
    }

    // One path rather than a shape per segment: every shape is tessellated on
    // its own, and 40 per wire across a few hundred wires dominated the
    // frame's CPU time. Short wires need far fewer segments to look smooth.
    let length = (to - from).length().max(1.0);
    let segments = ((length / WIRE_SEGMENT_PX).ceil() as usize).clamp(MIN_BEZIER_SEGMENTS, BEZIER_SEGMENTS);
    let points = bezier_sample(from, to, segments);
    painter.add(egui::Shape::line(points, Stroke::new(thickness, color)));
}

/// Shortest distance from `p` to the polyline `pts`. Used to hit-test
/// clicks against wire beziers.
pub(super) fn point_to_polyline_distance(p: Pos2, pts: &[Pos2]) -> f32 {
    let mut best = f32::INFINITY;
    for w in pts.windows(2) {
        let d = point_to_segment_distance(p, w[0], w[1]);
        if d < best { best = d; }
    }
    best
}

fn point_to_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = Vec2::new(b.x - a.x, b.y - a.y);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 < 1e-6 {
        return (p.to_vec2() - a.to_vec2()).length();
    }
    let ap = Vec2::new(p.x - a.x, p.y - a.y);
    let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
    let proj = Pos2::new(a.x + t * ab.x, a.y + t * ab.y);
    (p - proj).length()
}

pub(super) fn port_screen_pos(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    port_id: PortId,
    origin: Vec2,
    zoom: f32,
) -> Option<Pos2> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let pos = Pos2::new(states[idx].pos.x * zoom, states[idx].pos.y * zoom) + origin;
    let size = node_size(nodes[idx].as_ref(), states[idx].size_override) * zoom;
    let rect = Rect::from_min_size(pos, size);
    Some(nodes[idx].port_pos(rect, port_id.dir, port_id.index, zoom))
}

pub(super) fn port_type_for(
    nodes: &[Box<dyn NodeWidget>],
    states: &[NodeState],
    port_id: PortId,
) -> Option<PortType> {
    let idx = states.iter().position(|s| s.id == port_id.node)?;
    let ports = match port_id.dir {
        PortDir::Input => nodes[idx].ui_inputs_full(),
        PortDir::Output => nodes[idx].ui_outputs(),
    };
    ports.get(port_id.index).map(|p| p.def.port_type)
}
