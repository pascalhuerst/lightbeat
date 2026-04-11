use crate::engine::types::*;
use crate::objects::color_palette::STACK_SIZE;

// ---------------------------------------------------------------------------
// Stack Split: ColorStack → 4 Color outputs
// ---------------------------------------------------------------------------

pub struct StackSplitProcessNode {
    id: NodeId,
    channels: [f32; 12], // 4 × RGB
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StackSplitProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            channels: [0.0; 12],
            inputs: vec![PortDef::new("palette", PortType::ColorStack)],
            outputs: vec![
                PortDef::new("primary", PortType::Color),
                PortDef::new("secondary", PortType::Color),
                PortDef::new("third", PortType::Color),
                PortDef::new("fourth", PortType::Color),
            ],
        }
    }
}

impl ProcessNode for StackSplitProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Stack Split" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < 12 { self.channels[ch] = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        if ch < 12 { self.channels[ch] } else { 0.0 }
    }
    fn process(&mut self) {}
    fn read_output(&self, ch: usize) -> f32 {
        // Output layout: 4 Color ports × 3 channels = 12 channels.
        // Directly maps: output channel N = input channel N.
        if ch < 12 { self.channels[ch] } else { 0.0 }
    }
}

// ---------------------------------------------------------------------------
// Stack Merge: 4 Color inputs → ColorStack output
// ---------------------------------------------------------------------------

pub struct StackMergeProcessNode {
    id: NodeId,
    channels: [f32; 12],
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
}

impl StackMergeProcessNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            channels: [0.0; 12],
            inputs: vec![
                PortDef::new("primary", PortType::Color),
                PortDef::new("secondary", PortType::Color),
                PortDef::new("third", PortType::Color),
                PortDef::new("fourth", PortType::Color),
            ],
            outputs: vec![PortDef::new("palette", PortType::ColorStack)],
        }
    }
}

impl ProcessNode for StackMergeProcessNode {
    fn node_id(&self) -> NodeId { self.id }
    fn type_name(&self) -> &'static str { "Stack Merge" }
    fn inputs(&self) -> &[PortDef] { &self.inputs }
    fn outputs(&self) -> &[PortDef] { &self.outputs }

    fn write_input(&mut self, ch: usize, v: f32) {
        if ch < 12 { self.channels[ch] = v; }
    }
    fn read_input(&self, ch: usize) -> f32 {
        if ch < 12 { self.channels[ch] } else { 0.0 }
    }
    fn process(&mut self) {}
    fn read_output(&self, ch: usize) -> f32 {
        if ch < 12 { self.channels[ch] } else { 0.0 }
    }
}
