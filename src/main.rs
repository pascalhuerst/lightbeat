mod beat_clock;
mod color;
mod config;
mod engine;
// mod interfaces; // TODO: fix artnet_protocol/sacn API version mismatch
mod link_controller;
mod objects;
mod project;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;

use beat_clock::{BeatClock, BeatPattern, SubscriptionHandle};
use config::AppConfig;
use engine::types::{new_shared_state, EngineCommand, NodeId, PortType};
use engine::EngineHandle;
use engine::nodes::display::color_display::ColorDisplayProcessNode;
use engine::nodes::display::scope::ScopeProcessNode;
use engine::nodes::io::clock::ClockProcessNode;
use engine::nodes::math::compare::{CompareOp, CompareProcessNode};
use engine::nodes::math::constant::ConstantProcessNode;
use engine::nodes::math::logic_gate::{LogicOp, LogicGateProcessNode};
use engine::nodes::math::math_op::{MathOp, MathProcessNode};
use engine::nodes::math::oscillator::{OscFunc, OscillatorProcessNode};
use engine::nodes::transport::delay::TriggerDelayProcessNode;
use engine::nodes::transport::envelope::EnvelopeProcessNode;
use engine::nodes::transport::phase_scaler::PhaseScalerProcessNode;
use engine::nodes::transport::step_sequencer::StepSequencerProcessNode;
use widgets::nodes::display::color_display::ColorDisplayWidget;
use widgets::nodes::display::scope::ScopeWidget;
use widgets::nodes::io::clock::ClockWidget;
use widgets::nodes::math::compare::CompareWidget;
use widgets::nodes::math::constant::ConstantWidget;
use widgets::nodes::math::logic_gate::LogicGateWidget;
use widgets::nodes::math::math_op::MathWidget;
use widgets::nodes::math::oscillator::OscillatorWidget;
use widgets::nodes::transport::delay::TriggerDelayWidget;
use widgets::nodes::transport::envelope::EnvelopeWidget;
use widgets::nodes::transport::phase_scaler::PhaseScalerWidget;
use widgets::nodes::transport::step_sequencer::StepSequencerWidget;
use widgets::nodes::NodeGraph;

struct LightBeatApp {
    graph: NodeGraph,
    engine: EngineHandle,
    beat_clock: BeatClock,
    _subs: Vec<SubscriptionHandle>,
    config: AppConfig,
    project_path: Option<PathBuf>,
    snapshot: Arc<std::sync::Mutex<beat_clock::LinkSnapshot>>,
    quit_requested: bool,
    show_dmx_monitor: bool,
    show_fixture_list: bool,
    show_interface_list: bool,
    dmx_monitor: widgets::dmx_monitor::DmxMonitor,
    fixture_manager: widgets::fixture_list::FixtureManager,
    interface_manager: widgets::interface_list::InterfaceManager,
}

impl LightBeatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let beat_clock = BeatClock::new(4.0);
        let snapshot = beat_clock.snapshot();
        let engine = EngineHandle::start();

        let mut app = Self {
            graph: NodeGraph::new(),
            engine,
            beat_clock,
            _subs: Vec::new(),
            config,
            project_path: None,
            snapshot,
            quit_requested: false,
            show_dmx_monitor: false,
            show_fixture_list: false,
            show_interface_list: false,
            dmx_monitor: widgets::dmx_monitor::DmxMonitor::new(),
            fixture_manager: widgets::fixture_list::FixtureManager::new(),
            interface_manager: widgets::interface_list::InterfaceManager::new(),
        };

        app.register_node_factories();

        // Try autoload, or create default clock.
        let loaded = if app.config.autoload_on_open {
            let path = project::default_project_path();
            if path.exists() {
                app.load_project_from(&path);
                true
            } else {
                false
            }
        } else {
            false
        };

        if !loaded {
            app.create_default_clock();
            // Drain since create_default_clock already handled the engine side.
            app.graph.drain_new_nodes();
        }

        app
    }

    fn register_node_factories(&mut self) {
        // IO
        self.graph.register_node("IO", "Clock", |id| {
            Box::new(ClockWidget::new(id, new_shared_state(0, 3)))
        });

        // Transport
        self.graph.register_node("Transport", "Phase Scaler", |id| {
            Box::new(PhaseScalerWidget::new(id, new_shared_state(1, 1)))
        });
        self.graph.register_node("Transport", "Step Sequencer", |id| {
            Box::new(StepSequencerWidget::new(id, new_shared_state(1, 2)))
        });
        self.graph.register_node("Transport", "ADSR", |id| {
            Box::new(EnvelopeWidget::new(id, new_shared_state(2, 2)))
        });
        self.graph.register_node("Transport", "Trigger Delay", |id| {
            Box::new(TriggerDelayWidget::new(id, new_shared_state(2, 1)))
        });

        // Math
        self.graph.register_node("Math", "Add", |id| {
            Box::new(MathWidget::new(id, MathOp::Add, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Sub", |id| {
            Box::new(MathWidget::new(id, MathOp::Sub, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Mul", |id| {
            Box::new(MathWidget::new(id, MathOp::Mul, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Div", |id| {
            Box::new(MathWidget::new(id, MathOp::Div, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Sin", |id| {
            Box::new(OscillatorWidget::new(id, OscFunc::Sin, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Cos", |id| {
            Box::new(OscillatorWidget::new(id, OscFunc::Cos, new_shared_state(2, 1)))
        });
        self.graph.register_node("Math", "Const Value", |id| {
            Box::new(ConstantWidget::new(id, PortType::Untyped, new_shared_state(0, 1)))
        });
        self.graph.register_node("Math", "Const Logic", |id| {
            Box::new(ConstantWidget::new(id, PortType::Logic, new_shared_state(0, 1)))
        });
        self.graph.register_node("Math", "Const Phase", |id| {
            Box::new(ConstantWidget::new(id, PortType::Phase, new_shared_state(0, 1)))
        });

        // Compare
        self.graph.register_node("Compare", ">=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Gte, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "<=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Lte, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "==", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Eq, new_shared_state(2, 1)))
        });
        self.graph.register_node("Compare", "!=", |id| {
            Box::new(CompareWidget::new(id, CompareOp::Neq, new_shared_state(2, 1)))
        });

        // Logic
        self.graph.register_node("Logic", "AND", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::And, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "OR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Or, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "XOR", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Xor, new_shared_state(2, 1)))
        });
        self.graph.register_node("Logic", "NOT", |id| {
            Box::new(LogicGateWidget::new(id, LogicOp::Not, new_shared_state(1, 1)))
        });

        // Display
        self.graph.register_node("Display", "Scope", |id| {
            Box::new(ScopeWidget::new(id, new_shared_state(2, 0)))
        });
        self.graph.register_node("Display", "Color Display", |id| {
            Box::new(ColorDisplayWidget::new(id, new_shared_state(3, 0)))
        });
    }

    fn create_default_clock(&mut self) {
        let id = NodeId(1);
        let shared = new_shared_state(0, 3);
        let engine_node = ClockProcessNode::new(id, Arc::clone(&self.snapshot));
        let beat_state = engine_node.beat_state.clone();
        let widget = ClockWidget::new(id, Arc::clone(&shared));

        self.graph.add_node(Box::new(widget), egui::pos2(50.0, 50.0));
        self.engine.send(EngineCommand::AddNode {
            node: Box::new(engine_node),
            shared,
        });
        self._subs.push(
            self.beat_clock
                .subscribe(BeatPattern::every(1), beat_state),
        );
    }

    fn wire_new_nodes(&mut self) {
        // Handle nodes created by context menu / paste / duplicate.
        for new_node in self.graph.drain_new_nodes() {
            let node = self.graph.node_mut(new_node.index);
            let type_name = node.type_name();
            let id = node.node_id();
            let shared = Arc::clone(node.shared_state());

            // Create corresponding engine node.
            match type_name {
                "Clock" => {
                    let engine_node = ClockProcessNode::new(id, Arc::clone(&self.snapshot));
                    let beat_state = engine_node.beat_state.clone();
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(engine_node),
                        shared,
                    });
                    self._subs.push(
                        self.beat_clock
                            .subscribe(BeatPattern::every(1), beat_state),
                    );
                }
                "Phase Scaler" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(PhaseScalerProcessNode::new(id)),
                        shared,
                    });
                }
                "Step Sequencer" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(StepSequencerProcessNode::new(id)),
                        shared,
                    });
                }
                "Scope" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(ScopeProcessNode::new(id)),
                        shared,
                    });
                }
                "ADSR" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(EnvelopeProcessNode::new(id)),
                        shared,
                    });
                }
                "Trigger Delay" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(TriggerDelayProcessNode::new(id)),
                        shared,
                    });
                }
                "Add" | "Sub" | "Mul" | "Div" => {
                    let op = match type_name {
                        "Add" => MathOp::Add,
                        "Sub" => MathOp::Sub,
                        "Mul" => MathOp::Mul,
                        "Div" => MathOp::Div,
                        _ => unreachable!(),
                    };
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(MathProcessNode::new(id, op)),
                        shared,
                    });
                }
                "Color Display" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(ColorDisplayProcessNode::new(id)),
                        shared,
                    });
                }
                ">=" | "<=" | "==" | "!=" => {
                    let op = match type_name {
                        ">=" => CompareOp::Gte,
                        "<=" => CompareOp::Lte,
                        "==" => CompareOp::Eq,
                        "!=" => CompareOp::Neq,
                        _ => unreachable!(),
                    };
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(CompareProcessNode::new(id, op)),
                        shared,
                    });
                }
                "AND" | "OR" | "XOR" | "NOT" => {
                    let op = match type_name {
                        "AND" => LogicOp::And,
                        "OR" => LogicOp::Or,
                        "XOR" => LogicOp::Xor,
                        "NOT" => LogicOp::Not,
                        _ => unreachable!(),
                    };
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(LogicGateProcessNode::new(id, op)),
                        shared,
                    });
                }
                "Const Value" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(ConstantProcessNode::new(id, PortType::Untyped, 0.0)),
                        shared,
                    });
                }
                "Const Logic" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(ConstantProcessNode::new(id, PortType::Logic, 0.0)),
                        shared,
                    });
                }
                "Const Phase" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(ConstantProcessNode::new(id, PortType::Phase, 0.0)),
                        shared,
                    });
                }
                "Sin" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(OscillatorProcessNode::new(id, OscFunc::Sin)),
                        shared,
                    });
                }
                "Cos" => {
                    self.engine.send(EngineCommand::AddNode {
                        node: Box::new(OscillatorProcessNode::new(id, OscFunc::Cos)),
                        shared,
                    });
                }
                _ => {
                    eprintln!("Unknown node type for engine: {}", type_name);
                }
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
            self.load_project_from(&path);
        }
    }

    fn load_project_from(&mut self, path: &std::path::Path) {
        match project::load_from_file(&path.to_path_buf()) {
            Ok(proj) => {
                // Clear engine and UI graph.
                self.engine.send(EngineCommand::RemoveAllNodes);
                self.graph = NodeGraph::new();
                self.register_node_factories();

                // Load nodes and connections into UI graph.
                let indices = project::load_graph(&mut self.graph, &proj);

                // Create engine nodes (via wire_new_nodes mechanism).
                // drain_new_nodes will pick them up.
                // But we also need to send connections and load_data to the engine.

                // Create engine nodes.
                self.wire_new_nodes();

                // Drain engine commands queued by add_connection during load.
                for cmd in self.graph.drain_engine_commands() {
                    self.engine.send(cmd);
                }

                // Send load_data for nodes that have custom data.
                for (i, saved) in proj.nodes.iter().enumerate() {
                    if let Some(data) = &saved.data {
                        if i < indices.len() {
                            let node = self.graph.node_mut(indices[i]);
                            self.engine.send(EngineCommand::LoadData {
                                node_id: node.node_id(),
                                data: data.clone(),
                            });
                        }
                    }
                }

                self.project_path = Some(path.to_path_buf());
            }
            Err(e) => {
                eprintln!("Failed to open project: {}", e);
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
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_fixture_list, "Fixtures").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_interface_list, "Interfaces").changed() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_dmx_monitor, "DMX Monitor").changed() {
                        ui.close_menu();
                    }
                });
            });
        });

        // Fixture list window (toggled via View menu).
        if self.show_fixture_list {
            egui::Window::new("Fixtures")
                .open(&mut self.show_fixture_list)
                .default_size([350.0, 400.0])
                .show(ctx, |ui| {
                    self.fixture_manager.show(ui);
                });
        }

        // Interface list window (toggled via View menu).
        if self.show_interface_list {
            egui::Window::new("Interfaces")
                .open(&mut self.show_interface_list)
                .default_size([350.0, 300.0])
                .show(ctx, |ui| {
                    self.interface_manager.show(ui);
                });
        }

        // Inspector panel.
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

        // DMX Monitor (floating window, toggled via View menu).
        if self.show_dmx_monitor {
            egui::Window::new("DMX Monitor")
                .open(&mut self.show_dmx_monitor)
                .default_size([600.0, 200.0])
                .show(ctx, |ui| {
                    let empty = [0u8; 512];
                    self.dmx_monitor.show(ui, "No output connected", &empty);
                });
        }

        // Node graph.
        egui::CentralPanel::default().show(ctx, |ui| {
            self.graph.show(ui, self.config.snap_to_grid);
        });

        self.wire_new_nodes();

        // Send any pending engine commands from the graph.
        for cmd in self.graph.drain_engine_commands() {
            self.engine.send(cmd);
        }

        // Keyboard shortcuts.
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_project();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.open_project();
        }

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
