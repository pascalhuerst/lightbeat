use crate::engine::types::*;

/// Mode values stored in `ColorDisplayData::mode`:
/// 0 = Auto/Neutral (input is `Any`, nothing rendered yet)
/// 1 = Color   (single RGB swatch)
/// 2 = Palette (4-color set)
/// 3 = Gradient (8-stop gradient preview)
pub const MODE_NEUTRAL: usize = 0;
pub const MODE_COLOR: usize = 1;
pub const MODE_PALETTE: usize = 2;
pub const MODE_GRADIENT: usize = 3;

/// Max input channels across modes (Gradient = 40).
const MAX_CHANNELS: usize = GRADIENT_STOP_COUNT * GRADIENT_STOP_FLOATS;

pub struct ColorDisplayData {
    pub mode: usize,
    pub channels: [f32; MAX_CHANNELS],
}

pub struct ColorDisplayProcessNode {
    id: NodeId,
    mode: usize,
    channels: [f32; MAX_CHANNELS],
    inputs: Vec<PortDef>,
}

/// The ports, stated once: the widget builds its own from this, so the two
/// halves cannot disagree.
pub fn ports_for(mode: usize) -> (Vec<PortDef>, Vec<PortDef>) {
    let input = match mode {
        MODE_PALETTE => PortDef::new("palette", PortType::Palette),
        MODE_COLOR => PortDef::new("color", PortType::Color),
        MODE_GRADIENT => PortDef::new("gradient", PortType::Gradient),
        _ => PortDef::new("?", PortType::Any),
    };
    (vec![input], vec![])
}

impl ColorDisplayProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            mode: MODE_NEUTRAL,
            channels: [0.0; MAX_CHANNELS],
            inputs: ports_for(MODE_NEUTRAL).0,
        }
    }

    fn rebuild_inputs(&mut self) {
        self.inputs = ports_for(self.mode).0;
    }
}

impl ProcessNode for ColorDisplayProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Color Display" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &[] }

    fn write_input(&mut self, channel: usize, value: f32) {
        if channel < MAX_CHANNELS { self.channels[channel] = value; }
    }

    fn read_input(&self, channel: usize) -> f32 {
        if channel < MAX_CHANNELS { self.channels[channel] } else { 0.0 }
    }

    fn process(&mut self) {}

    fn non_promotable_params(&self) -> &'static [&'static str] {
        &["Type"]
    }

    fn params(&self) -> Vec<ParamDef> {
        vec![ParamDef::Choice {
            name: "Type".into(),
            value: self.mode,
            options: vec!["Auto".into(), "Color".into(), "Palette".into(), "Gradient".into()],
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
        // Don't clamp Gradient channels — alpha = -1.0 is the "unused" marker.
        let channels = if self.mode == MODE_GRADIENT {
            self.channels
        } else {
            let mut clamped = [0.0f32; MAX_CHANNELS];
            for (dst, &c) in clamped.iter_mut().zip(&self.channels) {
                *dst = c.clamp(0.0, 1.0);
            }
            clamped
        };
        shared.display = Some(Box::new(ColorDisplayData {
            mode: self.mode,
            channels,
        }));
    }
}
