use std::any::Any;

use egui::{self, Ui};

use crate::engine::nodes::math::formula::FormulaDisplay;
use crate::engine::types::*;
use crate::theme;
use crate::widgets::nodes::node::NodeWidget;
use crate::widgets::nodes::types::UiPortDef;

/// Where the operators and functions the formula understands are listed.
const SYNTAX_URL: &str = "https://docs.rs/meval/latest/meval/#supported-expressions";

pub struct FormulaWidget {
    id: NodeId,
    shared: SharedState,
    formula: String,
    /// The engine's copy has been read once (it holds what a loaded project
    /// saved); after that this widget is the source of truth while typing.
    synced: bool,
    error: Option<String>,
}

impl FormulaWidget {
    pub fn new(id: NodeId, shared: SharedState) -> Self {
        Self { id, shared, formula: "x".to_string(), synced: false, error: None }
    }

    fn sync_from_display(&mut self) {
        let shared = self.shared.lock().unwrap();
        if let Some(d) = shared.display.as_ref().and_then(|d| d.downcast_ref::<FormulaDisplay>()) {
            if !self.synced {
                self.formula = d.formula.clone();
                self.synced = true;
            }
            self.error = d.error.clone();
        }
    }

    fn push(&self) {
        self.shared.lock().unwrap().pending_config = Some(serde_json::json!({ "formula": self.formula }));
    }
}

impl NodeWidget for FormulaWidget {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Formula" }
    fn description(&self) -> &'static str {
        "Writes a function of its input to its output. The input is x (or i); \
         write the result as 2*x+7 or y = sin(x*2*PI). Operators and functions: \
         https://docs.rs/meval/latest/meval/#supported-expressions"
    }

    fn ui_inputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("x", PortType::Untyped))]
    }
    fn ui_outputs(&self) -> Vec<UiPortDef> {
        vec![UiPortDef::from_def(&PortDef::new("y", PortType::Untyped))]
    }

    fn min_width(&self) -> f32 { 130.0 }
    fn min_content_height(&self) -> f32 { 20.0 }
    fn shared_state(&self) -> &SharedState { &self.shared }

    fn show_content(&mut self, ui: &mut Ui, _zoom: f32) {
        self.sync_from_display();
        let color = if self.error.is_some() { theme::SEM_WARNING } else { theme::TEXT };
        ui.add(egui::Label::new(egui::RichText::new(&self.formula).color(color).monospace()).truncate());
    }

    fn show_inspector(&mut self, ui: &mut Ui) {
        self.sync_from_display();
        ui.label("Formula:");
        let edit = egui::TextEdit::singleline(&mut self.formula)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY)
            .hint_text("y = 2*x + 7");
        if ui.add(edit).changed() {
            self.push();
        }
        match &self.error {
            Some(e) => {
                ui.colored_label(theme::SEM_WARNING, e);
            }
            None => {
                ui.colored_label(theme::TEXT_DIM, "x or i is the input; y = or o = in front is optional.");
            }
        }
        ui.add_space(4.0);
        ui.colored_label(theme::TEXT_DIM, "Constants: PI, TAU, E");
        ui.colored_label(
            theme::TEXT_DIM,
            "Operators: + - * / % ^   Functions: sin cos tan asin acos atan atan2 sinh cosh tanh \
             sqrt cbrt exp ln log2 log10 pow abs signum floor ceil round trunc fract clamp min max",
        );
        ui.hyperlink_to("Syntax reference (meval)", SYNTAX_URL);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
