use crate::engine::types::*;

/// Display state — RGB values for the UI to render.
pub struct ColorDisplayData {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

pub struct ColorDisplayProcessNode {
    id: NodeId,
    r: f32,
    g: f32,
    b: f32,
    inputs: Vec<PortDef>,
}

impl ColorDisplayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            inputs: vec![
                PortDef::new("R", PortType::Untyped),
                PortDef::new("G", PortType::Untyped),
                PortDef::new("B", PortType::Untyped),
            ],
        }
    }
}

impl ProcessNode for ColorDisplayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Display" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, port_index: usize, value: f32) {
        match port_index {
            0 => self.r = value,
            1 => self.g = value,
            2 => self.b = value,
            _ => {}
        }
    }

    fn read_input(&self, port_index: usize) -> f32 {
        match port_index { 0 => self.r, 1 => self.g, 2 => self.b, _ => 0.0 }
    }

    fn process(&mut self) {}

    fn update_display(&self, shared: &mut NodeSharedState) {
        shared.display = Some(Box::new(ColorDisplayData {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
        }));
    }
}
