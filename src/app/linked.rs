//! Linked subgraphs, the app's side (`crate::linked` has the rules): noticing
//! that an instance was edited and bringing the others to the same content.

use std::time::{Duration, Instant};

use crate::LightBeatApp;
use crate::engine::types::NodeId;
use crate::linked::{self, SyncStep};
use crate::project::{self, ProjectFile};

/// After a project is applied (undo, redo, load) every node is new and the
/// level the user is in is half-mirrored for a few ticks; as the source of a
/// sync it would copy params it does not have yet into the other instances.
const QUIET_AFTER_APPLY: Duration = Duration::from_millis(400);

/// How long after a load the instances are compared: the nodes need to have
/// told their parameter names, which the front-panel rule depends on.
const CHECK_AFTER_LOAD: Duration = Duration::from_millis(1200);

/// Rounds of planning after rebuilds that invalidate nested targets.
const MAX_ROUNDS: usize = 8;

impl LightBeatApp {
    pub(crate) fn quiet_links_for_a_while(&mut self) {
        self.link_quiet_until = Some(Instant::now() + QUIET_AFTER_APPLY);
    }

    pub(crate) fn check_links_after_load(&mut self) {
        self.link_check_at = Some(Instant::now() + CHECK_AFTER_LOAD);
    }

    fn links_are_quiet(&self) -> bool {
        self.link_quiet_until.is_some_and(|t| Instant::now() < t)
    }

    /// Called with the project as just saved from the graph, when it differs
    /// from the baseline. The source is never guessed from what changed: it is
    /// the instance the user can be editing (`editing`, see
    /// `NodeGraph::editing_path`), or the selected instance when its own name or
    /// ports were edited from the level it sits on. The others follow it, and
    /// the returned project shows the result, so that the undo history sees the
    /// edit and its sync as one step.
    pub(crate) fn sync_links(
        &mut self,
        baseline: &ProjectFile,
        mut current: ProjectFile,
        editing: Option<Vec<u64>>,
    ) -> ProjectFile {
        if self.links_are_quiet() {
            return current;
        }
        let mut sources: Vec<Vec<u64>> = editing.into_iter().collect();
        if let Some(path) = self.graph.selected_instance_path() {
            let before = linked::node_at(baseline, &path).map(|n| &n.data);
            let now = linked::node_at(&current, &path).map(|n| &n.data);
            if before.is_some() && before != now {
                sources.push(path);
            }
        }
        if sources.is_empty() {
            self.graph.forget_port_removals();
            return current;
        }
        if self.graph.assign_def_nodes() {
            current = project::save_graph(&self.graph);
        }
        self.run_sync(current, &sources, false)
    }

    fn run_sync(&mut self, mut current: ProjectFile, sources: &[Vec<u64>], report_load: bool) -> ProjectFile {
        let known = self.graph.known();
        let mut reported = false;
        for _ in 0..MAX_ROUNDS {
            let graph = &mut self.graph;
            let mut steps = linked::plan_sync(&current, sources, &known, &mut || graph.alloc_id().0);
            if steps.is_empty() {
                break;
            }
            if report_load && !reported {
                reported = true;
                eprintln!("Linked subgraphs differ after loading; the first instance of each definition wins.");
            }
            self.graph.attach_port_removals(&mut steps);
            self.apply_link_steps(&steps);
            linked::apply_steps(&mut current, &steps);
        }
        current
    }

    /// Compare the instances of a freshly loaded project (see `CHECK_AFTER_LOAD`).
    pub(crate) fn run_link_check(&mut self) {
        let Some(at) = self.link_check_at else { return };
        if Instant::now() < at {
            return;
        }
        self.link_check_at = None;
        self.graph.assign_def_nodes();
        let current = project::save_graph(&self.graph);
        let sources = linked::first_instances(&current);
        if sources.is_empty() {
            return;
        }
        let synced = self.run_sync(current.clone(), &sources, true);
        if synced != current {
            self.project_baseline = Some(synced);
        }
    }

    /// Carry out steps on the live graph and the engine.
    pub(crate) fn apply_link_steps(&mut self, steps: &[SyncStep]) {
        for step in steps {
            let rebuilt = self.graph.apply_sync_step(step);
            if rebuilt && step.rebuild {
                self.wire_new_nodes();
                for cmd in self.graph.drain_engine_commands() {
                    self.engine.send(cmd);
                }
                if let Some(inner) = &step.result.inner_graph {
                    let indices: Vec<usize> = (0..inner.nodes.len()).collect();
                    let path: Vec<NodeId> = step.path.iter().map(|&id| NodeId(id)).collect();
                    self.send_load_data_recursive(&inner.nodes, &indices, path);
                }
            }
        }
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }
    }

    /// Edit, "Link identical subgraphs".
    pub(crate) fn link_identical_subgraphs(&mut self) {
        let report = self.graph.link_identical();
        let message = if report.groups == 0 {
            "No identical subgraphs to link.".to_string()
        } else {
            format!(
                "Linked {} instances in {} group{}.",
                report.instances,
                report.groups,
                if report.groups == 1 { "" } else { "s" }
            )
        };
        self.file_notes.push(message);
    }

    /// Messages that came up inside the graph (a refused paste).
    pub(crate) fn drain_graph_notices(&mut self) {
        for message in self.graph.take_notices() {
            self.report_error(message);
        }
    }
}
