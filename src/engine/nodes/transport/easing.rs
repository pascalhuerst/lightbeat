/// Easing curve types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingCurve {
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseOutBounce,
    EaseInBounce,
    EaseOutElastic,
    EaseInElastic,
}

impl EasingCurve {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::EaseInQuad => "Ease In Quad",
            Self::EaseOutQuad => "Ease Out Quad",
            Self::EaseInOutQuad => "Ease In/Out Quad",
            Self::EaseInCubic => "Ease In Cubic",
            Self::EaseOutCubic => "Ease Out Cubic",
            Self::EaseInOutCubic => "Ease In/Out Cubic",
            Self::EaseInExpo => "Ease In Expo",
            Self::EaseOutExpo => "Ease Out Expo",
            Self::EaseInOutExpo => "Ease In/Out Expo",
            Self::EaseOutBounce => "Ease Out Bounce",
            Self::EaseInBounce => "Ease In Bounce",
            Self::EaseOutElastic => "Ease Out Elastic",
            Self::EaseInElastic => "Ease In Elastic",
        }
    }

    pub fn all() -> &'static [EasingCurve] {
        &[
            Self::Linear, Self::EaseInQuad, Self::EaseOutQuad, Self::EaseInOutQuad,
            Self::EaseInCubic, Self::EaseOutCubic, Self::EaseInOutCubic,
            Self::EaseInExpo, Self::EaseOutExpo, Self::EaseInOutExpo,
            Self::EaseOutBounce, Self::EaseInBounce,
            Self::EaseOutElastic, Self::EaseInElastic,
        ]
    }

    pub fn from_index(i: usize) -> Self {
        Self::all().get(i).copied().unwrap_or(Self::Linear)
    }

    pub fn to_index(&self) -> usize {
        Self::all().iter().position(|c| c == self).unwrap_or(0)
    }

    /// Apply the easing curve. Input t in 0..1, output in 0..1.
    pub fn apply(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseInQuad => t * t,
            Self::EaseOutQuad => t * (2.0 - t),
            Self::EaseInOutQuad => {
                if t < 0.5 { 2.0 * t * t } else { -1.0 + (4.0 - 2.0 * t) * t }
            }
            Self::EaseInCubic => t * t * t,
            Self::EaseOutCubic => { let t1 = t - 1.0; t1 * t1 * t1 + 1.0 }
            Self::EaseInOutCubic => {
                if t < 0.5 { 4.0 * t * t * t }
                else { let t1 = 2.0 * t - 2.0; 0.5 * t1 * t1 * t1 + 1.0 }
            }
            Self::EaseInExpo => {
                if t == 0.0 { 0.0 } else { 2.0_f32.powf(10.0 * (t - 1.0)) }
            }
            Self::EaseOutExpo => {
                if t == 1.0 { 1.0 } else { 1.0 - 2.0_f32.powf(-10.0 * t) }
            }
            Self::EaseInOutExpo => {
                if t == 0.0 { return 0.0; }
                if t == 1.0 { return 1.0; }
                if t < 0.5 { 0.5 * 2.0_f32.powf(20.0 * t - 10.0) }
                else { 1.0 - 0.5 * 2.0_f32.powf(-20.0 * t + 10.0) }
            }
            Self::EaseOutBounce => bounce_out(t),
            Self::EaseInBounce => 1.0 - bounce_out(1.0 - t),
            Self::EaseOutElastic => {
                if t == 0.0 || t == 1.0 { return t; }
                2.0_f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * std::f32::consts::TAU / 3.0).sin() + 1.0
            }
            Self::EaseInElastic => {
                if t == 0.0 || t == 1.0 { return t; }
                -(2.0_f32.powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * std::f32::consts::TAU / 3.0).sin())
            }
        }
    }
}

fn bounce_out(t: f32) -> f32 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}
