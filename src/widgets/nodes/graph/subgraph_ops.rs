use super::*;

impl NodeGraph {
    /// Public entry point for navigating into a subgraph (used by load_graph).
    pub fn navigate_into_by_index(&mut self, subgraph_node_index: usize) {
        self.navigate_into(subgraph_node_index);
    }

    /// Navigate back to the parent level.
    pub fn navigate_up(&mut self) {
        if let Some(parent) = self.levels[self.active_level].parent_level_idx {
            self.active_level = parent;
            self.selected_nodes.clear();
            self.drag = DragState::default();
        }
    }

    pub(super) fn navigate_into(&mut self, subgraph_node_index: usize) {
        let level = self.active();
        let subgraph_id = level.states[subgraph_node_index].id;
        let label = level.nodes[subgraph_node_index].display_title().to_string();

        // Get port defs by reading the SubgraphWidget's display from shared state.
        // The widget's ports are also reflected in its ui_inputs/ui_outputs.
        let (input_defs, output_defs) = {
            let inputs = level.nodes[subgraph_node_index].ui_inputs_full();
            let outputs = level.nodes[subgraph_node_index].ui_outputs();
            let in_defs: Vec<SubgraphPortDef> = inputs.iter()
                .map(|p| SubgraphPortDef {
                    name: p.def.name.clone(),
                    port_type_idx: port_type_to_idx(p.def.port_type),
                })
                .collect();
            let out_defs: Vec<SubgraphPortDef> = outputs.iter()
                .map(|p| SubgraphPortDef {
                    name: p.def.name.clone(),
                    port_type_idx: port_type_to_idx(p.def.port_type),
                })
                .collect();
            (in_defs, out_defs)
        };

        // Check if we already have this level on the stack (re-entering after navigating up).
        let existing = self.levels.iter().position(|l| l.subgraph_id == Some(subgraph_id));
        if let Some(idx) = existing {
            self.active_level = idx;
            // Update bridge node ports in case they changed.
            let level = &mut self.levels[idx];
            for node in &mut level.nodes {
                if node.node_id() == BRIDGE_IN_NODE_ID {
                    if let Some(gi) = node.as_any_mut().downcast_mut::<GraphInputWidget>() {
                        gi.update_ports(input_defs.clone());
                    }
                } else if node.node_id() == BRIDGE_OUT_NODE_ID
                    && let Some(go) = node.as_any_mut().downcast_mut::<GraphOutputWidget>() {
                        go.update_ports(output_defs.clone());
                    }
            }
        } else {
            // Create new level with bridge nodes.
            let mut nodes: Vec<Box<dyn NodeWidget>> = Vec::new();
            let mut states: Vec<NodeState> = Vec::new();

            nodes.push(Box::new(GraphInputWidget::new(input_defs)));
            states.push(NodeState::new(BRIDGE_IN_NODE_ID, Pos2::new(20.0, 100.0)));
            nodes.push(Box::new(GraphOutputWidget::new(output_defs)));
            states.push(NodeState::new(BRIDGE_OUT_NODE_ID, Pos2::new(500.0, 100.0)));

            let parent = self.active_level;
            self.levels.push(GraphLevel {
                nodes,
                states,
                connections: Vec::new(),
                pan: Vec2::ZERO,
                zoom: 1.0,
                subgraph_id: Some(subgraph_id),
                parent_level_idx: Some(parent),
                label,
                frames: Vec::new(),
            });
            self.active_level = self.levels.len() - 1;
        }
        self.selected_nodes.clear();
        self.drag = DragState::default();
    }

    pub fn navigate_to_level(&mut self, level: usize) {
        if level < self.levels.len() {
            self.active_level = level;
            self.selected_nodes.clear();
            self.drag = DragState::default();
        }
    }

    // -----------------------------------------------------------------------
    // Move selection into subgraph
    // -----------------------------------------------------------------------

    pub(super) fn move_selection_to_subgraph(&mut self) {
        let selected = self.selected_nodes.clone();
        if selected.is_empty() { return; }

        let level = self.active();
        let selected_ids: Vec<NodeId> = selected.iter()
            .map(|&i| level.states[i].id)
            .collect();

        // 1. Classify connections.
        let mut internal_conns = Vec::new();
        let mut incoming_conns = Vec::new();
        let mut outgoing_conns = Vec::new();

        for conn in &level.connections {
            let from_inside = selected_ids.contains(&conn.from.node);
            let to_inside = selected_ids.contains(&conn.to.node);
            match (from_inside, to_inside) {
                (true, true) => internal_conns.push(conn.clone()),
                (false, true) => incoming_conns.push(conn.clone()),
                (true, false) => outgoing_conns.push(conn.clone()),
                (false, false) => {}
            }
        }

        // 2. Deduplicate inputs: group incoming connections by source port.
        //    Multiple wires from the same output become one subgraph input.
        let mut unique_input_sources: Vec<PortId> = Vec::new();
        let mut incoming_port_map: Vec<usize> = Vec::new(); // incoming_conn index -> subgraph input port index
        for conn in &incoming_conns {
            let idx = unique_input_sources.iter().position(|s| *s == conn.from);
            if let Some(idx) = idx {
                incoming_port_map.push(idx);
            } else {
                incoming_port_map.push(unique_input_sources.len());
                unique_input_sources.push(conn.from);
            }
        }

        let input_defs: Vec<SubgraphPortDef> = unique_input_sources.iter().enumerate()
            .map(|(i, src_port)| {
                let pt = port_type_for(&level.nodes, &level.states, *src_port)
                    .unwrap_or(PortType::Untyped);
                SubgraphPortDef {
                    name: format!("in {}", i),
                    port_type_idx: port_type_to_idx(pt),
                }
            }).collect();

        // 3. Deduplicate outputs: group outgoing connections by source port.
        //    Multiple wires from the same inner output become one subgraph output.
        let mut unique_output_sources: Vec<PortId> = Vec::new();
        let mut outgoing_port_map: Vec<usize> = Vec::new(); // outgoing_conn index -> subgraph output port index
        for conn in &outgoing_conns {
            let idx = unique_output_sources.iter().position(|s| *s == conn.from);
            if let Some(idx) = idx {
                outgoing_port_map.push(idx);
            } else {
                outgoing_port_map.push(unique_output_sources.len());
                unique_output_sources.push(conn.from);
            }
        }

        let output_defs: Vec<SubgraphPortDef> = unique_output_sources.iter().enumerate()
            .map(|(i, src_port)| {
                let pt = port_type_for(&level.nodes, &level.states, *src_port)
                    .unwrap_or(PortType::Untyped);
                SubgraphPortDef {
                    name: format!("out {}", i),
                    port_type_idx: port_type_to_idx(pt),
                }
            }).collect();

        // 4. Compute centroid of selected nodes.
        let centroid = {
            let sum: Vec2 = selected.iter()
                .map(|&i| level.states[i].pos.to_vec2())
                .fold(Vec2::ZERO, |a, b| a + b);
            Pos2::new(0.0, 0.0) + sum / selected.len() as f32
        };

        // 5. Create SubgraphWidget.
        let sub_id = self.alloc_id();
        let shared = crate::engine::types::new_shared_state(12, 12);
        let mut sub_widget = SubgraphWidget::new(sub_id, shared);
        sub_widget.input_defs = input_defs.clone();
        sub_widget.output_defs = output_defs.clone();
        sub_widget.push_config();

        // 6. Extract selected nodes from the active level.
        //    Capture current params and save_data so we can restore them on the new engine nodes.
        let mut moved_nodes: Vec<(Box<dyn NodeWidget>, NodeState, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        let mut sorted_selected = selected.clone();
        sorted_selected.sort_unstable();
        sorted_selected.dedup();

        let level = self.active_mut();
        for &i in sorted_selected.iter().rev() {
            let node = level.nodes.remove(i);
            let state = level.states.remove(i);
            // Snapshot params and save_data from shared state.
            let (params, save_data) = {
                let shared = node.shared_state().lock().unwrap();
                let params: Vec<(usize, ParamValue)> = shared.current_params.iter().enumerate()
                    .map(|(pi, p)| {
                        let val = match p {
                            ParamDef::Float { value, .. } => ParamValue::Float(*value),
                            ParamDef::Int { value, .. } => ParamValue::Int(*value),
                            ParamDef::Bool { value, .. } => ParamValue::Bool(*value),
                            ParamDef::Choice { value, .. } => ParamValue::Choice(*value),
                        };
                        (pi, val)
                    })
                    .collect();
                (params, shared.save_data.clone())
            };
            moved_nodes.push((node, state, params, save_data));
        }
        moved_nodes.reverse();

        // Remove all connections involving selected nodes from outer graph.
        level.connections.retain(|c| {
            !selected_ids.contains(&c.from.node) && !selected_ids.contains(&c.to.node)
        });

        // 7. Notify engine to remove moved nodes from outer graph.
        for &id in &selected_ids {
            self.push_engine_cmd(EngineCommand::RemoveNode(id));
        }

        // 8. Add subgraph node to outer graph.
        let sub_idx = self.add_node(Box::new(sub_widget), centroid);
        let sub_node_id = self.active().states[sub_idx].id;

        // 9. Wire outer connections to the subgraph's ports (deduplicated).
        // Incoming: each unique source -> subgraph input port
        for (i, src_port) in unique_input_sources.iter().enumerate() {
            let to = PortId { node: sub_node_id, index: i, dir: PortDir::Input };
            self.add_connection(*src_port, to);
        }
        // Outgoing: subgraph output port -> each external destination
        for (conn_idx, conn) in outgoing_conns.iter().enumerate() {
            let port_idx = outgoing_port_map[conn_idx];
            let from = PortId { node: sub_node_id, index: port_idx, dir: PortDir::Output };
            self.add_connection(from, conn.to);
        }

        // 10. Build the new subgraph's inner level directly — do NOT use
        // `navigate_into`, which has a cache path that would reuse a
        // pre-existing level with the same `subgraph_id`. For a freshly
        // created subgraph we want an unambiguously fresh level and we
        // stay on the root level throughout, so no navigation happens.
        let mut inner_nodes: Vec<Box<dyn NodeWidget>> = Vec::new();
        let mut inner_states: Vec<NodeState> = Vec::new();
        inner_nodes.push(Box::new(GraphInputWidget::new(input_defs.clone())));
        inner_states.push(NodeState::new(BRIDGE_IN_NODE_ID, Pos2::new(20.0, 100.0)));
        inner_nodes.push(Box::new(GraphOutputWidget::new(output_defs.clone())));
        inner_states.push(NodeState::new(BRIDGE_OUT_NODE_ID, Pos2::new(500.0, 100.0)));

        let mut restore_info: Vec<(NodeId, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        for (mut node, state, params, save_data) in moved_nodes {
            let node_id = node.node_id();
            let new_pos = Pos2::new(
                state.pos.x - centroid.x + 250.0,
                state.pos.y - centroid.y + 150.0,
            );
            let mut ns = NodeState::new(node_id, new_pos);
            if let Some(size) = state.size_override {
                ns.size_override = Some(size);
            }

            // Mux / Demux / Port Hub widgets compute their port layout
            // from `port_type` + `slots` (+ `select_mode` for Mux/Demux).
            // When the engine node behind them is re-spawned inside the
            // subgraph it boots at defaults, then pending_params catches
            // up on the next tick. Meanwhile the OLD engine's shared
            // `display` was left behind by `RemoveNode` — so there's a
            // window where the widget's `sync_from_display` could snap
            // back to defaults if anything writes to `display` in the
            // interim. Force-restore the widget's fields from the
            // captured params right now so `ui_inputs` / `ui_outputs`
            // are guaranteed correct for this frame's
            // `cleanup_stale_connections`.
            {
                use crate::engine::nodes::math::multiplex::{
                    SelectMode, type_from_index,
                };
                let type_idx = params.iter().find(|(pi, _)| *pi == 0).map(|(_, v)| v.as_usize());
                let slots = params.iter().find(|(pi, _)| *pi == 1).map(|(_, v)| v.as_usize());
                let mode_idx = params.iter().find(|(pi, _)| *pi == 2).map(|(_, v)| v.as_usize());
                let n: &mut dyn NodeWidget = node.as_mut();
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::multiplex::MultiplexerWidget>()
                {
                    let pt = type_idx.map(type_from_index).unwrap_or(PortType::Any);
                    let s = slots.unwrap_or(
                        crate::engine::nodes::math::multiplex::MUX_DEFAULT_SLOTS,
                    );
                    let m = mode_idx.map(SelectMode::from_index).unwrap_or(SelectMode::Index);
                    w.set_state_from_load(pt, s, m);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::multiplex::DemultiplexerWidget>()
                {
                    let pt = type_idx.map(type_from_index).unwrap_or(PortType::Any);
                    let s = slots.unwrap_or(
                        crate::engine::nodes::math::multiplex::MUX_DEFAULT_SLOTS,
                    );
                    let m = mode_idx.map(SelectMode::from_index).unwrap_or(SelectMode::Index);
                    w.set_state_from_load(pt, s, m);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::port_hub::PortHubWidget>()
                {
                    let pt = type_idx.map(type_from_index).unwrap_or(PortType::Untyped);
                    let s = slots.unwrap_or(
                        crate::engine::nodes::math::port_hub::HUB_DEFAULT_SLOTS,
                    );
                    w.set_state_from_load(pt, s);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::counter::CounterWidget>()
                {
                    // Counter's Select Mode is at param index 2.
                    let m = params.iter().find(|(pi, _)| *pi == 2)
                        .map(|(_, v)| SelectMode::from_index(v.as_usize()))
                        .unwrap_or(SelectMode::Index);
                    w.set_state_from_load(m);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::math::palette_select::PaletteSelectWidget>()
                {
                    // Palette Select's Select Mode is at param index 0.
                    let m = params.iter().find(|(pi, _)| *pi == 0)
                        .map(|(_, v)| SelectMode::from_index(v.as_usize()))
                        .unwrap_or(SelectMode::Index);
                    w.set_state_from_load(m);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<crate::widgets::nodes::transport::envelope::EnvelopeWidget>()
                {
                    use crate::engine::nodes::transport::envelope::EnvelopeKind;
                    let k = params.iter().find(|(pi, _)| *pi == 4)
                        .map(|(_, v)| EnvelopeKind::from_index(v.as_usize()))
                        .unwrap_or(EnvelopeKind::Adsr);
                    w.set_state_from_load(k);
                }
            }

            // Portal widgets derive their entire port layout from
            // `save_data` (name / bound_name + `port_defs`). On move,
            // the engine node is re-spawned at defaults and the widget
            // could briefly report an empty port list to the first
            // frame, dropping any wired `bound_name` target or internal
            // wires. Push the captured save_data through the widget's
            // own `restore_from_save_data` so `ui_inputs` / `ui_outputs`
            // already match this frame.
            if let Some(ref data) = save_data {
                let n: &mut dyn NodeWidget = node.as_mut();
                if let Some(w) = n.as_any_mut()
                    .downcast_mut::<PortalSendWidget>()
                {
                    w.restore_from_save_data(data);
                } else if let Some(w) = n.as_any_mut()
                    .downcast_mut::<PortalReceiveWidget>()
                {
                    w.restore_from_save_data(data);
                }
            }

            inner_states.push(ns);
            inner_nodes.push(node);
            restore_info.push((node_id, params, save_data));
        }

        // Assemble inner connections: preserved internal edges + bridge
        // stubs for incoming/outgoing crossings.
        let mut inner_connections: Vec<Connection> = internal_conns.clone();
        for (conn_idx, conn) in incoming_conns.iter().enumerate() {
            let port_idx = incoming_port_map[conn_idx];
            let bridge_conn = Connection {
                from: PortId { node: BRIDGE_IN_NODE_ID, index: port_idx, dir: PortDir::Output },
                to: conn.to,
            };
            if !inner_connections.contains(&bridge_conn) {
                inner_connections.push(bridge_conn);
            }
        }
        for (conn_idx, conn) in outgoing_conns.iter().enumerate() {
            let port_idx = outgoing_port_map[conn_idx];
            let bridge_conn = Connection {
                from: conn.from,
                to: PortId { node: BRIDGE_OUT_NODE_ID, index: port_idx, dir: PortDir::Input },
            };
            if !inner_connections.contains(&bridge_conn) {
                inner_connections.push(bridge_conn);
            }
        }

        // Stage params and save_data on each moved node's shared state so
        // the engine picks them up when wire_new_nodes spawns the new
        // ProcessNode counterpart.
        for (node_id, params, save_data) in restore_info {
            if let Some(idx) = inner_states.iter().position(|s| s.id == node_id) {
                let shared = inner_nodes[idx].shared_state();
                let mut state = shared.lock().unwrap();
                for (pi, val) in params {
                    state.pending_params.push((pi, val));
                }
                if let Some(data) = save_data {
                    state.pending_config = Some(data);
                }
            }
        }

        let parent_idx = self.active_level;
        let label = "Subgraph".to_string();
        let new_level_idx = self.levels.len();
        self.levels.push(GraphLevel {
            nodes: inner_nodes,
            states: inner_states,
            connections: inner_connections.clone(),
            pan: Vec2::ZERO,
            zoom: 1.0,
            subgraph_id: Some(sub_node_id),
            parent_level_idx: Some(parent_idx),
            label,
            frames: Vec::new(),
        });

        // Engine-side path to the new subgraph's inner graph. If the user
        // triggered move-to-subgraph from inside another subgraph, the
        // ancestors must be included so the engine can route inner
        // commands to the right nested inner_graph.
        let mut path = self.current_subgraph_path();
        path.push(sub_node_id);

        // Register moved nodes with the engine. Bridges (indices 0, 1)
        // don't need wiring.
        let moved_count = self.levels[new_level_idx].nodes.len();
        for i in 2..moved_count {
            self.new_nodes.push(NewNode {
                index: i,
                subgraph_path: path.clone(),
            });
        }

        // Emit AddConnection for every inner connection, wrapped in the
        // new subgraph's path. If the subgraph itself lives at root, path
        // is just [sub_node_id]; if it's nested, all ancestors are
        // included so the engine's SubgraphInnerCommand router walks the
        // right chain.
        for conn in inner_connections {
            if path.is_empty() {
                // Can't happen — we always pushed at least sub_node_id.
                self.pending_engine_cmds.push(EngineCommand::AddConnection(conn));
            } else {
                self.pending_engine_cmds.push(EngineCommand::SubgraphInnerCommand {
                    subgraph_path: path.clone(),
                    command: Box::new(SubgraphInnerCmd::AddConnection(conn)),
                });
            }
        }

        self.selected_nodes.clear();
    }

    /// Inverse of `move_selection_to_subgraph`: replace the Subgraph at
    /// `sub_idx` (in the active level) with its inner nodes and re-route its
    /// external wires through the bridges' inner connections.
    pub(super) fn unpack_subgraph(&mut self, sub_idx: usize) {
        // Validate selection and capture the subgraph's id/pos up front.
        let (sub_id, sub_pos) = {
            let level = self.active();
            if sub_idx >= level.nodes.len() { return; }
            if level.nodes[sub_idx].type_name() != "Subgraph" { return; }
            (level.states[sub_idx].id, level.states[sub_idx].pos)
        };

        let inner_level_idx = match self.levels.iter().position(|l| l.subgraph_id == Some(sub_id)) {
            Some(i) => i,
            None => return,
        };

        // External connections touching the subgraph (in the parent level).
        let (external_incoming, external_outgoing): (Vec<(PortId, usize)>, Vec<(usize, PortId)>) = {
            let level = self.active();
            let mut inc = Vec::new();
            let mut out = Vec::new();
            for c in &level.connections {
                if c.to.node == sub_id { inc.push((c.from, c.to.index)); }
                if c.from.node == sub_id { out.push((c.from.index, c.to)); }
            }
            (inc, out)
        };

        // Partition inner connections into bridge-in / bridge-out / internal.
        let (bridge_in_conns, bridge_out_conns, internal_conns): (Vec<Connection>, Vec<Connection>, Vec<Connection>) = {
            let inner = &self.levels[inner_level_idx];
            let mut bi = Vec::new();
            let mut bo = Vec::new();
            let mut it = Vec::new();
            for c in &inner.connections {
                let from_br = c.from.node == BRIDGE_IN_NODE_ID;
                let to_br = c.to.node == BRIDGE_OUT_NODE_ID;
                if from_br && !to_br { bi.push(c.clone()); }
                else if !from_br && to_br { bo.push(c.clone()); }
                else if !from_br && !to_br { it.push(c.clone()); }
            }
            (bi, bo, it)
        };

        // Drain the inner widgets (skip bridges). Snapshot params + save_data
        // so the new engine nodes can be primed on their next tick.
        let mut taken: Vec<(Box<dyn NodeWidget>, NodeState, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        {
            let inner = &mut self.levels[inner_level_idx];
            let mut i = inner.nodes.len();
            while i > 0 {
                i -= 1;
                let id = inner.states[i].id;
                if id == BRIDGE_IN_NODE_ID || id == BRIDGE_OUT_NODE_ID { continue; }
                let node = inner.nodes.remove(i);
                let state = inner.states.remove(i);
                let (params, save_data) = {
                    let shared = node.shared_state().lock().unwrap();
                    let params: Vec<(usize, ParamValue)> = shared.current_params.iter().enumerate()
                        .map(|(pi, p)| {
                            let v = match p {
                                ParamDef::Float { value, .. } => ParamValue::Float(*value),
                                ParamDef::Int { value, .. } => ParamValue::Int(*value),
                                ParamDef::Bool { value, .. } => ParamValue::Bool(*value),
                                ParamDef::Choice { value, .. } => ParamValue::Choice(*value),
                            };
                            (pi, v)
                        })
                        .collect();
                    (params, shared.save_data.clone())
                };
                taken.push((node, state, params, save_data));
            }
            taken.reverse();
            // Clear the orphaned inner level's connections too; the inner
            // engine gets torn down as soon as we remove the subgraph.
            inner.connections.clear();
        }

        // Remove the subgraph node and its connections from the parent level.
        {
            let level = self.active_mut();
            level.connections.retain(|c| c.from.node != sub_id && c.to.node != sub_id);
            level.states.remove(sub_idx);
            level.nodes.remove(sub_idx);
        }
        self.push_engine_cmd(EngineCommand::RemoveNode(sub_id));

        // Spread the taken nodes around sub_pos, preserving their relative layout.
        let centroid: Vec2 = if taken.is_empty() {
            Vec2::ZERO
        } else {
            let sum: Vec2 = taken.iter()
                .map(|(_, s, _, _)| s.pos.to_vec2())
                .fold(Vec2::ZERO, |a, b| a + b);
            sum / (taken.len() as f32)
        };

        let parent_path = self.current_subgraph_path();
        let mut restore_info: Vec<(NodeId, Vec<(usize, ParamValue)>, Option<serde_json::Value>)> = Vec::new();
        let start_idx = self.active().nodes.len();
        {
            let level = self.active_mut();
            for (node, state, params, save_data) in taken {
                let node_id = node.node_id();
                let new_pos = Pos2::new(
                    state.pos.x - centroid.x + sub_pos.x,
                    state.pos.y - centroid.y + sub_pos.y,
                );
                level.states.push(NodeState::new(node_id, new_pos));
                if let Some(sz) = state.size_override {
                    level.states.last_mut().unwrap().size_override = Some(sz);
                }
                level.nodes.push(node);
                restore_info.push((node_id, params, save_data));
            }
        }
        let end_idx = self.active().nodes.len();
        for i in start_idx..end_idx {
            self.new_nodes.push(NewNode { index: i, subgraph_path: parent_path.clone() });
        }

        // Restore params/save_data on the relocated widgets.
        for (node_id, params, save_data) in restore_info {
            let level = self.active();
            if let Some(idx) = level.states.iter().position(|s| s.id == node_id) {
                let shared = level.nodes[idx].shared_state();
                let mut state = shared.lock().unwrap();
                for (pi, val) in params {
                    state.pending_params.push((pi, val));
                }
                if let Some(data) = save_data {
                    state.pending_config = Some(data);
                }
            }
        }

        // Internal inner connections carry over as-is.
        for conn in &internal_conns {
            self.active_mut().connections.push(conn.clone());
            self.push_engine_cmd(EngineCommand::AddConnection(conn.clone()));
        }

        // External src -> subgraph.input[k], combined with BRIDGE_IN[k] -> inner_to,
        // becomes src -> inner_to.
        for bin in &bridge_in_conns {
            let k = bin.from.index;
            let inner_to = bin.to;
            for (src, ext_k) in &external_incoming {
                if *ext_k == k {
                    let c = Connection { from: *src, to: inner_to };
                    self.active_mut().connections.push(c.clone());
                    self.push_engine_cmd(EngineCommand::AddConnection(c));
                }
            }
        }
        // inner_from -> BRIDGE_OUT[k], combined with subgraph.output[k] -> dst,
        // becomes inner_from -> dst.
        for bout in &bridge_out_conns {
            let k = bout.to.index;
            let inner_from = bout.from;
            for (ext_k, dst) in &external_outgoing {
                if *ext_k == k {
                    let c = Connection { from: inner_from, to: *dst };
                    self.active_mut().connections.push(c.clone());
                    self.push_engine_cmd(EngineCommand::AddConnection(c));
                }
            }
        }

        self.selected_nodes.clear();
    }
}
