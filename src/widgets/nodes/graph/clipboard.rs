use super::*;

impl NodeGraph {
    pub(super) fn delete_selected(&mut self) {
        self.stack_sig = None;
        // Frames have no engine-side counterpart, so just drop them.
        if !self.selected_frames.is_empty() {
            let to_drop = self.selected_frames.clone();
            self.active_mut().frames.retain(|f| !to_drop.contains(&f.id));
            self.selected_frames.clear();
        }

        // Selected connections — route through `remove_connection_to` so
        // widgets get their disconnect callbacks (mode-autodetect nodes
        // reset to Neutral, etc.), same as any other wire removal.
        if !self.selected_connections.is_empty() {
            let to_drop: Vec<Connection> = self.selected_connections.drain().collect();
            for conn in to_drop {
                self.remove_connection_to(conn.to);
            }
        }

        let mut to_remove = self.selected_nodes.clone();
        to_remove.sort_unstable();
        to_remove.dedup();

        // Don't delete bridge pseudo-nodes.
        let level = self.active();
        to_remove.retain(|&i| {
            let id = level.states[i].id;
            id != BRIDGE_IN_NODE_ID && id != BRIDGE_OUT_NODE_ID
        });
        if to_remove.is_empty() {
            return;
        }

        let removed_ids: Vec<NodeId> = to_remove.iter()
            .map(|&i| self.active().states[i].id).collect();

        // Drop every connection touching a removed node *through* the regular
        // remove_connection_to path, so endpoint widgets get disconnect
        // callbacks (e.g. Scope resets its port type back to Any). This keeps
        // disconnect-on-delete behavior generic — widgets don't have to
        // implement anything extra.
        let affected_tos: Vec<PortId> = self.active().connections.iter()
            .filter(|c| removed_ids.contains(&c.from.node) || removed_ids.contains(&c.to.node))
            .map(|c| c.to)
            .collect();
        for to in affected_tos {
            self.remove_connection_to(to);
        }

        // Remove nodes in reverse order.
        let level = self.active_mut();
        for &i in to_remove.iter().rev() {
            level.nodes.remove(i);
            level.states.remove(i);
        }

        for &id in &removed_ids {
            self.push_engine_cmd(EngineCommand::RemoveNode(id));
        }

        self.selected_nodes.clear();
    }

    pub(super) fn copy_selected(&mut self) {
        self.clipboard.clear();
        self.clipboard_connections.clear();
        if self.selected_nodes.is_empty() {
            return;
        }

        let origin = self.active().states[self.selected_nodes[0]].pos;
        let selected = self.selected_nodes.clone();

        let mut new_clip = Vec::new();
        for &i in &selected {
            let level = self.active();
            let node = &level.nodes[i];
            let state = &level.states[i];

            // Read params from shared state.
            let shared = node.shared_state().lock().unwrap();
            let params: Vec<(usize, ParamValue)> = shared
                .current_params
                .iter()
                .enumerate()
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

            let data = shared.save_data.clone();
            let promoted = shared.promoted.clone();
            let mut snapshots = shared.snapshots.clone();
            snapshots.active = shared.snapshot_active.or(snapshots.active);
            let hide_unused_ports = shared.hide_unused_ports;
            drop(shared);

            // For Subgraph nodes, snapshot the inner level so duplicate
            // recreates the nested content instead of leaving an empty
            // shell behind.
            let inner_graph = if node.type_name() == "Subgraph" {
                self.find_level_for_subgraph(state.id)
                    .map(|lvl| crate::project::save_level(lvl, self))
            } else {
                None
            };

            new_clip.push(ClipboardNode {
                type_name: node.type_name().to_string(),
                size: state.size_override,
                params,
                promoted,
                snapshots,
                hide_unused_ports,
                data,
                offset: state.pos - origin,
                original_id: state.id,
                inner_graph,
            });
        }

        // Capture connections whose endpoints are both inside the selection
        // so duplicate preserves the internal wiring.
        let selected_ids: std::collections::HashSet<NodeId> = selected
            .iter()
            .map(|&i| self.active().states[i].id)
            .collect();
        let mut conns = Vec::new();
        for conn in &self.active().connections {
            if selected_ids.contains(&conn.from.node)
                && selected_ids.contains(&conn.to.node)
            {
                conns.push(ClipboardConn {
                    from_node: conn.from.node,
                    from_index: conn.from.index,
                    to_node: conn.to.node,
                    to_index: conn.to.index,
                });
            }
        }

        self.clipboard = new_clip;
        self.clipboard_connections = conns;
    }

    /// Pick a name for a duplicated node with a name-like identifier
    /// (portals, etc.). If the source name ends with a space followed by
    /// a number, that number is incremented; otherwise ` 2` is appended.
    /// Intentionally simple — if the generated name still clashes with
    /// another node in the graph, the user renames via the inspector.
    fn next_name_suffix(name: &str) -> String {
        if let Some(space_idx) = name.rfind(' ') {
            let (base, suffix) = name.split_at(space_idx);
            if let Ok(n) = suffix.trim_start().parse::<u32>() {
                return format!("{} {}", base, n + 1);
            }
        }
        format!("{} 2", name)
    }

    pub(super) fn paste(&mut self, base_pos: Pos2) {
        if self.clipboard.is_empty() {
            return;
        }

        self.selected_nodes.clear();
        let clip = std::mem::take(&mut self.clipboard);
        let conns = std::mem::take(&mut self.clipboard_connections);

        // original_id → new_id. Populated as we create each duplicate so
        // we can rewrite the captured connections.
        let mut id_map: std::collections::HashMap<NodeId, NodeId> =
            std::collections::HashMap::new();

        for cn in &clip {
            let id = self.alloc_id();
            if let Some(node) = self.create_from_registry(&cn.type_name, id) {
                let pos = base_pos + cn.offset;
                let idx = self.add_node(node, pos);

                // Apply params by pushing to shared state pending_params.
                {
                    let shared = self.active().nodes[idx].shared_state();
                    let mut state = shared.lock().unwrap();
                    for (pi, val) in &cn.params {
                        state.pending_params.push((*pi, val.clone()));
                    }
                    state.promoted = cn.promoted.clone();
                    state.promoted_applied = cn.promoted.clone();
                    if !cn.snapshots.is_empty() {
                        state.snapshots = cn.snapshots.clone();
                        state.pending_snapshots = Some(cn.snapshots.clone());
                    }
                    state.hide_unused_ports = cn.hide_unused_ports;
                }

                if let Some(size) = cn.size {
                    self.active_mut().states[idx].size_override = Some(size);
                }

                let mut data = cn.data.clone();
                // A Portal Send publishes under a unique name, so the copy
                // gets the next number. A Portal Receive keeps its
                // `bound_name` so the copy stays bound to the same Send.
                if cn.type_name == "Portal Send"
                    && let Some(data) = data.as_mut()
                    && let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                        let next = Self::next_name_suffix(name);
                        data["name"] = serde_json::Value::String(next);
                    }

                // The same restore as on load, so the copy has its real
                // ports before `navigate_into` (deep-copying a subgraph) and
                // `cleanup_stale_connections` look at it.
                self.active_mut().nodes[idx].restore_from_save(data.as_ref(), &cn.params);

                // Dispatch the source node's save_data to the duplicate's
                // process node so per-instance settings (fader-group rows/cols
                // and enable flags, controller bindings, layer stacks, etc.)
                // survive copy-paste.
                if let Some(data) = data {
                    let node_id = self.active().nodes[idx].node_id();
                    self.push_engine_cmd(EngineCommand::LoadData {
                        node_id,
                        data,
                    });
                }

                id_map.insert(cn.original_id, id);
                self.selected_nodes.push(idx);

                // Subgraph deep-clone: the clipboard carries a snapshot
                // of the original's inner level. Remap its NodeIds to
                // fresh ones (the originals are still live), navigate
                // into the fresh duplicate, and load the remapped inner
                // graph — same path as project-file load, so nested
                // subgraphs recurse through the same machinery.
                if cn.type_name == "Subgraph"
                    && let Some(inner) = cn.inner_graph.clone() {
                        let mut inner = inner;
                        crate::project::remap_ids_inplace(&mut inner, &mut || self.alloc_id().0);
                        let parent_level = self.active_level;
                        self.navigate_into(idx);
                        let _ = crate::project::load_graph(self, &inner);
                        self.navigate_to_level(parent_level);
                    }
            }
        }

        // Re-create internal connections on the duplicate. For each
        // captured connection, look up both endpoints in the id map and
        // call add_connection with the rewritten PortIds — this also
        // queues the engine's AddConnection command.
        for c in &conns {
            if let (Some(&new_from), Some(&new_to)) =
                (id_map.get(&c.from_node), id_map.get(&c.to_node))
            {
                let from = PortId { node: new_from, index: c.from_index, dir: PortDir::Output };
                let to = PortId { node: new_to, index: c.to_index, dir: PortDir::Input };
                self.add_connection(from, to);
            }
        }

        self.clipboard = clip;
        self.clipboard_connections = conns;
    }

    pub(super) fn duplicate_selected(&mut self) {
        self.copy_selected();
        // Paste offset from the first selected node.
        let base = if let Some(&i) = self.selected_nodes.first() {
            self.active().states[i].pos + Vec2::new(GRID_SPACING * 2.0, GRID_SPACING * 2.0)
        } else {
            Pos2::new(100.0, 100.0)
        };
        // Clear selection before paste (paste will set new selection).
        self.paste(base);
    }
}

#[cfg(test)]
mod paste_tests {
    use super::*;
    use crate::engine::nodes::math::multiplex::{MUX_TYPE_NAMES, type_to_index};
    use crate::engine::types::new_shared_state;
    use crate::midi_io::MidiHub;
    use crate::widgets::nodes::io::midi_common::new_shared_learned;
    use crate::widgets::nodes::io::midi_input::MidiInputWidget;
    use crate::widgets::nodes::math::gradient_source::new_shared_gradient_library;
    use crate::widgets::nodes::math::lookup::LookupWidget;
    use crate::widgets::nodes::math::multiplex::MultiplexerWidget;
    use crate::widgets::nodes::meta::subgraph::SubgraphWidget;
    use crate::widgets::nodes::ui::button_group::ButtonGroupWidget;
    use crate::widgets::nodes::ui::fader_group::FaderGroupWidget;

    fn registry() -> NodeGraph {
        let mut g = NodeGraph::new();
        g.register_node("Route", "Lookup Table", |id| {
            Box::new(LookupWidget::new(id, new_shared_state(1, 321), new_shared_gradient_library()))
        });
        g.register_node("Input", "MIDI Input", |id| {
            Box::new(MidiInputWidget::new(id, new_shared_state(0, 33), MidiHub::idle(), new_shared_learned()))
        });
        g.register_node("Input", "Fader Group", |id| Box::new(FaderGroupWidget::new(id, new_shared_state(256, 256))));
        g.register_node("Input", "Button Group", |id| Box::new(ButtonGroupWidget::new(id, new_shared_state(256, 256))));
        g.register_node("Route", "Multiplexer", |id| Box::new(MultiplexerWidget::new(id, new_shared_state(321, 40))));
        g.register_node("Route", "Subgraph", |id| Box::new(SubgraphWidget::new(id, new_shared_state(12, 12))));
        g
    }

    fn port_counts(g: &NodeGraph, i: usize) -> (usize, usize) {
        let n = g.active().nodes[i].as_ref();
        (n.ui_inputs().len(), n.ui_outputs().len())
    }

    /// Set a node up the way the user left it (the widget restored from
    /// `data` and `params`, the shared state holding what the engine would
    /// have mirrored), copy and paste it, and compare the port counts of the
    /// original and the copy. The ports of a node just made must differ from
    /// the original's, or the test would prove nothing.
    fn keeps_ports(
        type_name: &str,
        data: Option<serde_json::Value>,
        params: Vec<(usize, ParamValue)>,
        declared: Vec<ParamDef>,
    ) {
        let mut g = registry();
        let fresh = g.create_from_registry(type_name, NodeId(1)).unwrap();
        let fresh_ports = (fresh.ui_inputs().len(), fresh.ui_outputs().len());
        let i = g.add_node(fresh, Pos2::ZERO);
        g.node_mut(i).restore_from_save(data.as_ref(), &params);
        {
            let mut shared = g.node_mut(i).shared_state().lock().unwrap();
            shared.save_data = data;
            shared.current_params = declared;
        }
        let original = port_counts(&g, i);
        assert_ne!(original, fresh_ports, "{type_name}: the test setup must change the ports");

        g.selected_nodes = vec![i];
        g.copy_selected();
        g.paste(Pos2::new(300.0, 300.0));
        let pasted = g.selected_nodes[0];
        assert_ne!(pasted, i);
        assert_eq!(port_counts(&g, pasted), original, "{type_name}: the copy lost ports");
    }

    #[test]
    fn a_pasted_lookup_table_keeps_its_columns() {
        keeps_ports(
            "Lookup Table",
            Some(serde_json::json!({ "columns": [{ "name": "a" }, { "name": "b" }, { "name": "c" }] })),
            vec![],
            vec![],
        );
    }

    #[test]
    fn a_pasted_midi_input_keeps_its_entries() {
        let entry = |id: u64, n: u64| serde_json::json!(
            { "id": id, "name": format!("e{id}"), "binding": {"kind": "cc", "channel": 1, "number": n}, "mode": "value" });
        keeps_ports(
            "MIDI Input",
            Some(serde_json::json!({ "port": "dev", "entries": [entry(1, 1), entry(2, 2)] })),
            vec![],
            vec![],
        );
    }

    #[test]
    fn a_pasted_fader_group_keeps_its_size() {
        keeps_ports(
            "Fader Group",
            Some(serde_json::json!({
                "rows": 3, "cols": 4,
                "inputs_enabled": vec![true; 12], "outputs_enabled": vec![true; 12],
            })),
            vec![],
            vec![],
        );
    }

    #[test]
    fn a_pasted_button_group_keeps_its_size() {
        keeps_ports(
            "Button Group",
            Some(serde_json::json!({ "rows": 3, "cols": 3, "inputs_enabled": true })),
            vec![],
            vec![],
        );
    }

    #[test]
    fn a_pasted_multiplexer_keeps_its_type_and_slots() {
        let color = type_to_index(PortType::Color);
        let declared = vec![
            ParamDef::Choice {
                name: "Type".into(),
                value: color,
                options: MUX_TYPE_NAMES.iter().map(|s| (*s).into()).collect(),
            },
            ParamDef::Int { name: "Slots".into(), value: 10, min: 2, max: 64 },
        ];
        keeps_ports(
            "Multiplexer",
            None,
            vec![(0, ParamValue::Choice(color)), (1, ParamValue::Int(10))],
            declared,
        );
    }

    #[test]
    fn a_pasted_subgraph_keeps_its_ports() {
        keeps_ports(
            "Subgraph",
            Some(serde_json::json!({
                "name": "S",
                "inputs": [{ "name": "a", "port_type_idx": 2 }, { "name": "b", "port_type_idx": 2 }],
                "outputs": [{ "name": "x", "port_type_idx": 2 }],
            })),
            vec![],
            vec![],
        );
    }
}
