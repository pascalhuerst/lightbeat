use super::*;

impl NodeGraph {
    /// Add a new decorative frame at the given world position with default
    /// size and color. Returns the new frame's id.
    pub fn add_frame_at(&mut self, world_pos: Pos2) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let frame = GraphFrame {
            id,
            title: "Frame".to_string(),
            color: theme::FRAME_DEFAULT,
            notes: String::new(),
            pos: world_pos,
            size: Vec2::new(280.0, 180.0),
        };
        self.active_mut().frames.push(frame);
        id
    }

    /// Mutable access for the inspector to edit titles/colors/notes.
    pub fn frames_mut(&mut self) -> &mut Vec<GraphFrame> { &mut self.active_mut().frames }
    pub fn selected_frame_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.selected_frames.iter().copied()
    }

    /// Indices of nodes whose screen-rect centre currently falls inside the
    /// frame with id `frame_id`. Captured once at frame-drag start so those
    /// nodes ride along — centre-based so a node that's only partially over
    /// a frame only gets hitched when most of it is inside.
    pub(super) fn nodes_on_frame(&self, frame_id: u64, node_rects: &[Rect]) -> Vec<usize> {
        let level = self.active();
        let z = level.zoom;
        let origin = self.canvas_rect.min.to_vec2() + level.pan;
        let Some(frame) = level.frames.iter().find(|f| f.id == frame_id) else {
            return Vec::new();
        };
        let world_rect = frame.rect();
        let screen_min = Pos2::new(world_rect.min.x * z, world_rect.min.y * z) + origin;
        let screen_max = Pos2::new(world_rect.max.x * z, world_rect.max.y * z) + origin;
        let screen_rect = Rect::from_min_max(screen_min, screen_max);
        node_rects.iter().enumerate()
            .filter(|(_, r)| screen_rect.contains(r.center()))
            .map(|(i, _)| i)
            .collect()
    }

    pub(super) fn frame_hit_at(&self, pointer: Pos2, canvas_rect: Rect) -> Option<FrameHit> {
        let level = self.active();
        let z = level.zoom;
        let origin = canvas_rect.min.to_vec2() + level.pan;
        // Topmost frame wins; iterate in reverse.
        for f in level.frames.iter().rev() {
            let world_rect = f.rect();
            let screen_min = Pos2::new(world_rect.min.x * z, world_rect.min.y * z) + origin;
            let screen_max = Pos2::new(world_rect.max.x * z, world_rect.max.y * z) + origin;
            let screen_rect = Rect::from_min_max(screen_min, screen_max);
            if !screen_rect.contains(pointer) { continue; }
            // Hit-test sizes must match render sizes (scale with canvas zoom).
            let handle_size = 10.0 * z;
            let corner = Rect::from_min_size(
                Pos2::new(screen_rect.max.x - handle_size, screen_rect.max.y - handle_size),
                Vec2::splat(handle_size),
            );
            if corner.contains(pointer) { return Some(FrameHit::Corner(f.id)); }
            let title_h = 24.0 * z;
            let title_rect = Rect::from_min_size(
                screen_rect.min,
                Vec2::new(screen_rect.width(), title_h.min(screen_rect.height())),
            );
            if title_rect.contains(pointer) { return Some(FrameHit::TitleBar(f.id)); }
            return Some(FrameHit::Body(f.id));
        }
        None
    }

    /// Request that the view fits all content on the next frame.
    pub fn fit_to_content(&mut self) {
        self.fit_pending = true;
    }

    /// Compute and apply pan/zoom so all nodes fit within canvas_rect with some padding.
    pub(super) fn apply_fit_to_content(&mut self, canvas_rect: Rect) {
        let level = self.active();
        if level.nodes.is_empty() {
            return;
        }

        // Compute bounding box of all nodes in canvas-space (unzoomed).
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        let only_selected = !self.selected_nodes.is_empty();
        for (i, node) in level.nodes.iter().enumerate() {
            if only_selected && !self.selected_nodes.contains(&i) {
                continue;
            }
            let pos = level.states[i].pos;
            let size = node_size(node.as_ref(), level.states[i].size_override);

            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
            max_x = max_x.max(pos.x + size.x);
            max_y = max_y.max(pos.y + size.y);
        }

        if min_x > max_x {
            return;
        }
        let content_w = max_x - min_x;
        let content_h = max_y - min_y;

        if content_w <= 0.0 || content_h <= 0.0 {
            return;
        }

        let padding = 40.0;
        let available_w = (canvas_rect.width() - padding * 2.0).max(1.0);
        let available_h = (canvas_rect.height() - padding * 2.0).max(1.0);

        // Compute zoom to fit, clamped to reasonable range.
        let zoom = (available_w / content_w)
            .min(available_h / content_h)
            .clamp(0.2, 1.5);

        // Compute pan to center the content.
        let content_center_x = (min_x + max_x) / 2.0;
        let content_center_y = (min_y + max_y) / 2.0;
        let canvas_center_x = canvas_rect.width() / 2.0;
        let canvas_center_y = canvas_rect.height() / 2.0;

        let pan = Vec2::new(
            canvas_center_x - content_center_x * zoom,
            canvas_center_y - content_center_y * zoom,
        );

        let level = self.active_mut();
        level.zoom = zoom;
        level.pan = pan;
    }

    const MINIMAP_SIZE: Vec2 = Vec2::new(200.0, 140.0);

    /// Where the overview sits: the top right of the canvas.
    pub(super) fn minimap_rect(&self, canvas: Rect) -> Rect {
        if !self.show_minimap {
            return Rect::NOTHING;
        }
        Rect::from_min_size(
            Pos2::new(canvas.max.x - Self::MINIMAP_SIZE.x - 10.0, canvas.min.y + 10.0),
            Self::MINIMAP_SIZE,
        )
    }

    /// Everything on the active level in canvas space.
    fn content_bounds(&self) -> Option<Rect> {
        let level = self.active();
        let mut bounds: Option<Rect> = None;
        let mut add = |r: Rect| bounds = Some(bounds.map_or(r, |b| b.union(r)));
        for (i, node) in level.nodes.iter().enumerate() {
            let size = node_size(node.as_ref(), level.states[i].size_override);
            add(Rect::from_min_size(level.states[i].pos, size));
        }
        for f in &level.frames {
            add(f.rect());
        }
        bounds
    }

    /// An overview of the whole level with the visible part marked. Click or
    /// drag in it to move the view there.
    pub(super) fn show_minimap(&mut self, ui: &mut Ui, canvas: Rect) {
        let rect = self.minimap_rect(canvas);
        let Some(bounds) = self.content_bounds() else { return };
        if rect == Rect::NOTHING {
            return;
        }
        let zoom = self.active().zoom;
        let pan = self.active().pan;
        // The view in canvas space, so the minimap also covers what is
        // currently on screen when that is outside the nodes.
        let view = Rect::from_min_size(
            Pos2::ZERO + (-pan / zoom),
            canvas.size() / zoom,
        );
        let world = bounds.union(view).expand(40.0);
        let inner = rect.shrink(4.0);
        let scale = (inner.width() / world.width()).min(inner.height() / world.height());
        let origin = inner.center() - world.center().to_vec2() * scale;
        let to_map = |p: Pos2| Pos2::new(origin.x + p.x * scale, origin.y + p.y * scale);
        let from_map = |p: Pos2| Pos2::new((p.x - origin.x) / scale, (p.y - origin.y) / scale);

        let response = ui.interact(rect, ui.id().with("minimap"), Sense::click_and_drag());
        if (response.clicked() || response.dragged() || response.is_pointer_button_down_on())
            && let Some(p) = response.interact_pointer_pos()
        {
            let target = from_map(p);
            let level = self.active_mut();
            level.pan = canvas.size() * 0.5 - target.to_vec2() * level.zoom;
        }

        let painter = ui.painter().with_clip_rect(canvas);
        painter.rect_filled(rect, 3.0, theme::BG_DEEP.linear_multiply(0.85));
        painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, theme::STROKE), StrokeKind::Inside);
        let level = self.active();
        for f in &level.frames {
            let r = f.rect();
            painter.rect_filled(Rect::from_min_max(to_map(r.min), to_map(r.max)), 1.0, f.color.linear_multiply(0.25));
        }
        for (i, node) in level.nodes.iter().enumerate() {
            let size = node_size(node.as_ref(), level.states[i].size_override);
            let r = Rect::from_min_max(to_map(level.states[i].pos), to_map(level.states[i].pos + size));
            let r = Rect::from_center_size(r.center(), r.size().max(Vec2::splat(2.0)));
            let color = if self.selected_nodes.contains(&i) { theme::SEM_PRIMARY } else { theme::TEXT_DIM };
            painter.rect_filled(r, 0.5, color);
        }
        let seen = Rect::from_min_max(to_map(view.min), to_map(view.max)).intersect(rect);
        painter.rect_filled(seen, 0.0, theme::SEM_PRIMARY_FILL);
        painter.rect_stroke(seen, 0.0, Stroke::new(1.0_f32, theme::SEM_PRIMARY), StrokeKind::Inside);
    }
}
