use crate::engine::types::*;

/// Mode values stored in `ColorDisplayData::mode`:
/// 0 = Auto/Neutral (input is `Any`, nothing rendered yet)
/// 1 = Color  (single RGB swatch)
/// 2 = Palette (4-color set)
pub const MODE_NEUTRAL: usize = 0;
pub const MODE_COLOR: usize = 1;
pub const MODE_PALETTE: usize = 2;

pub struct ColorDisplayData {
    pub mode: usize,
    pub channels: [f32; 12], // up to 4×RGB
}

pub struct ColorDisplayProcessNode {
    id: NodeId,
    mode: usize,
    channels: [f32; 12],
    inputs: Vec<PortDef>,
}

impl ColorDisplayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            mode: MODE_NEUTRAL,
            channels: [0.0; 12],
            inputs: vec![PortDef::new("?", PortType::Any)],
        }
    }

    fn rebuild_inputs(&mut self) {
        self.inputs = match self.mode {
            MODE_PALETTE => vec![PortDef::new("palette", PortType::Palette)],
            MODE_COLOR => vec![PortDef::new("color", PortType::Color)],
            _ => vec![PortDef::new("?", PortType::Any)],
        };
    }
}

impl ProcessNode for ColorDisplayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Display" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < 12 { self.channels[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < 12 { self.channels[channel] } else { 0.0 }
    }

    fn process(&mut self) {}

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Mode".into(),
            value: self.mode,
            options: vec!["Auto".into(), "Color".into(), "Palette".into()],
        }]
    }

    fn set_param(&mut self, index: usize, value: ParamValue) {
        if index == 0 {
            let new_mode = value.as_usize();
            if new_mode != self.mode {
                self.mode = new_mode;
                self.rebuild_inputs();
            }
        }
    }

    fn update_display(&self, shared: &mut NodeSharedState) {
        let mut clamped = [0.0f32; 12];
        for i in 0..12 {
            clamped[i] = self.channels[i].clamp(0.0, 1.0);
        }
        shared.display = Some(Box::new(ColorDisplayData {
            mode: self.mode,
            channels: clamped,
        }));
    }
}
