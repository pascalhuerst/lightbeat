use crate::engine::types::*;
use crate::objects::output::OutputConfig;

/// Display state for the UI.
pub struct InterfaceDisplay {
    pub config: OutputConfig,
}

pub struct InterfaceProcessNode {
    id: NodeId,
    config: OutputConfig,
}

impl InterfaceProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            config: OutputConfig::ArtNet {
                host: "255.255.255.255".to_string(),
                port: 6454,
            },
        }
    }
}

impl ProcessNode for InterfaceProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Interface" }
    fn inputs(&self) -> &[PortDef] { &[] }
    fn outputs(&self) -> &[PortDef] { &[] }
    fn process(&mut self) {}

    fn save_data(&self) -> Option<serde_json::Value> {
        serde_json::to_value(&self.config).ok()
    }

    fn load_data(&mut self, data: &serde_json::Value) {
        if let Ok(c) = serde_json::from_value(data.clone()) {
            self.config = c;
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(InterfaceDisplay {
            config: self.config.clone(),
        }));
    }
}
