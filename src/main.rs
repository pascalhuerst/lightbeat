mod beat_clock;
mod config;
mod link_controller;
mod project;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use widgets::nodes::{NodeGraph, NodeId};
use widgets::{ClockNode, PhaseScalerNode, ScopeNode, StepSequencerNode};

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};
use config::AppConfig;

struct LightBeatApp {
    graph: NodeGraph,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
    config: AppConfig,
    project_path: Option<PathBuf>,
    snapshot: Arc<std::sync::Mutex<beat_clock::LinkSnapshot>>,
    quit_requested: bool,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();

        let mut graph = NodeGraph::new();

        // Register node types.
        let snap_for_factory = Arc::clone(&snapshot);
        graph.register_node("Clock", move |id| {
            Box::new(ClockNode::new(id, Arc::clone(&snap_for_factory)))
        });
        graph.register_node("Step Sequencer", |id| {
            Box::new(StepSequencerNode::new(id))
        });
        graph.register_node("Phase Scaler", |id| {
            Box::new(PhaseScalerNode::new(id))
        });
        graph.register_node("Scope", |id| Box::new(ScopeNode::new(id)));

        let mut subs = Vec::new();
        let mut project_path = None;

        // Try to autoload.
        let loaded = if config.autoload_on_open {
            let path = project::default_project_path();
            if path.exists() {
                match project::load_from_file(&path) {
                    Ok(proj) => {
                        let indices = project::load_graph(&mut graph, &proj);
                        for idx in indices {
                            let node = graph.node_mut(idx);
                            if let Some(clock) = node.as_any_mut().downcast_mut::<ClockNode>() {
                                let sub = beat_clock
                                    .subscribe(BeatPattern::every(1), clock.state.clone());
                                subs.push(sub);
                            }
                        }
                        project_path = Some(path);
                        true
                    }
                    Err(e) => {
                        eprintln!("Failed to load project: {}", e);
                        false
                    }
                }
            } else {
                false
            }
        } else {
            false
        };

        if !loaded {
            let clock = ClockNode::new(NodeId(1), Arc::clone(&snapshot));
            let clock_state = clock.state.clone();
            graph.add_node(Box::new(clock), egui::pos2(50.0, 50.0));
            subs.push(beat_clock.subscribe(BeatPattern::every(1), clock_state));
        }

        Self {
            graph,
            beat_clock,
            _subs: subs,
            config,
            project_path,
            snapshot,
            quit_requested: false,
        }
    }

    fn wire_new_nodes(&mut self) {
        for new_node in self.graph.drain_new_nodes() {
            let node = self.graph.node_mut(new_node.index);

            if let Some(clock) = node.as_any_mut().downcast_mut::<ClockNode>() {
                let sub = self
                    .beat_clock
                    .subscribe(BeatPattern::every(1), clock.state.clone());
                self._subs.push(sub);
            }
        }
    }

    fn save_project(&mut self) {
        let path = self
            .project_path
            .clone()
            .unwrap_or_else(project::default_project_path);
        if let Err(e) = project::save_to_file(&self.graph, &path) {
            eprintln!("Failed to save project: {}", e);
        } else {
            self.project_path = Some(path);
        }
    }

    fn save_project_as(&mut self) {
        let dialog = rfd::FileDialog::new()
            .set_title("Save Project As")
            .add_filter("LightBeat Project", &["json"])
            .set_file_name("project.json");

        if let Some(path) = dialog.save_file() {
            if let Err(e) = project::save_to_file(&self.graph, &path) {
                eprintln!("Failed to save project: {}", e);
            } else {
                self.project_path = Some(path);
            }
        }
    }

    fn open_project(&mut self) {
        let dialog = rfd::FileDialog::new()
            .set_title("Open Project")
            .add_filter("LightBeat Project", &["json"]);

        if let Some(path) = dialog.pick_file() {
            match project::load_from_file(&path) {
                Ok(proj) => {
                    // Clear existing graph.
                    self.graph = NodeGraph::new();

                    // Re-register node types.
                    let snap = Arc::clone(&self.snapshot);
                    self.graph.register_node("Clock", move |id| {
                        Box::new(ClockNode::new(id, Arc::clone(&snap)))
                    });
                    self.graph.register_node("Step Sequencer", |id| {
                        Box::new(StepSequencerNode::new(id))
                    });
                    self.graph.register_node("Phase Scaler", |id| {
                        Box::new(PhaseScalerNode::new(id))
                    });
                    self.graph
                        .register_node("Scope", |id| Box::new(ScopeNode::new(id)));

                    let indices = project::load_graph(&mut self.graph, &proj);
                    for idx in indices {
                        let node = self.graph.node_mut(idx);
                        if let Some(clock) = node.as_any_mut().downcast_mut::<ClockNode>() {
                            let sub = self
                                .beat_clock
                                .subscribe(BeatPattern::every(1), clock.state.clone());
                            self._subs.push(sub);
                        }
                    }
                    self.project_path = Some(path);
                }
                Err(e) => {
                    eprintln!("Failed to open project: {}", e);
                }
            }
        }
    }

    fn window_title(&self) -> String {
        let name = self
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled");
        format!("LightBeat - {}", name)
    }
}

impl eframe::App for LightBeatApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint();

        // Update window title.
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // Menu bar.
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        ui.close_menu();
                        self.open_project();
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        ui.close_menu();
                        self.save_project();
                    }
                    if ui.button("Save As...").clicked() {
                        ui.close_menu();
                        self.save_project_as();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ui.close_menu();
                        self.quit_requested = true;
                    }
                });
            });
        });

        // Inspector panel on the right.
        egui::SidePanel::right("inspector")
            .default_width(250.0)
            .show(ctx, |ui| {
                let selected = self.graph.selected_nodes_mut();
                if selected.is_empty() {
                    ui.heading("Inspector");
                    ui.separator();
                    ui.label("Select a node to inspect.");
                } else if selected.len() == 1 {
                    let node = &mut *selected.into_iter().next().unwrap();
                    widgets::inspector::show_inspector(ui, node.as_mut());
                } else {
                    ui.heading(format!("{} nodes selected", selected.len()));
                    ui.separator();
                    widgets::inspector::show_multi_inspector(ui, selected);
                }
            });

        // Node graph fills the rest.
        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui, self.config.snap_to_grid);
        });

        self.wire_new_nodes();

        // Ctrl+S to save.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_project();
        }

        // Ctrl+O to open.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.open_project();
        }

        // Handle quit.
        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.config.autosave_on_close {
            self.save_project();
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LightBeat",
        options,
        Box::new(|cc| Ok(Box::new(LightBeatApp::new(cc)))),
    )
}
