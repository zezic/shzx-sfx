use crate::{
    ParameterSnapshot, RainSnapshot, ReverbParameters,
    background::rain::RainFrame,
    dsp::{LowPass, Smooth},
    reverb::SpaceReverb,
};

use super::{configure_reverb, new_reverb, reverb_width_scale};

#[derive(Clone, Copy)]
struct Perspective {
    exterior_lowpass_hz: f32,
    dry_gain: f32,
    reverb_gain: f32,
    cabin_mix: f32,
}

impl Perspective {
    fn new(zoom: f32, focus: f32, rate: f32) -> Self {
        let depth = ((zoom - 0.45) / 0.55).clamp(0.0, 1.0);
        let cabin = depth * depth * (3.0 - 2.0 * depth);
        Self {
            exterior_lowpass_hz: ((3_800.0 - 3_250.0 * cabin) * (1.0 - 0.35 * focus))
                .min(rate * 0.45),
            dry_gain: 0.85 - 0.68 * focus,
            reverb_gain: (0.65 + 1.8 * focus) * (1.0 - 0.65 * cabin),
            cabin_mix: cabin,
        }
    }
}

/// A separate rain send preserves its tails while the listener moves between
/// exterior rain and damped roof impacts heard from inside the car.
pub(super) struct RainSpace {
    rate: f32,
    reverb: SpaceReverb,
    exterior_lowpass: [[LowPass; 2]; 2],
    wet_lowpass: [LowPass; 2],
    dry: Smooth,
    wet: Smooth,
    cabin: Smooth,
}

impl RainSpace {
    pub fn new(rate: u32, parameters: ReverbParameters) -> Self {
        let rate_f = rate as f32;
        let initial = Perspective::new(0.0, 0.0, rate_f);
        Self {
            rate: rate_f,
            reverb: new_reverb(rate, parameters),
            exterior_lowpass: std::array::from_fn(|_| {
                std::array::from_fn(|_| LowPass::new(rate_f, initial.exterior_lowpass_hz))
            }),
            wet_lowpass: std::array::from_fn(|_| LowPass::new(rate_f, initial.exterior_lowpass_hz)),
            dry: Smooth::new(initial.dry_gain, 0.02, rate_f),
            wet: Smooth::new(initial.reverb_gain, 0.02, rate_f),
            cabin: Smooth::new(initial.cabin_mix, 0.02, rate_f),
        }
    }

    pub fn control(&mut self, zoom: f32, focus: f32, mut parameters: ReverbParameters) {
        let perspective = Perspective::new(zoom, focus, self.rate);
        for channel in &mut self.exterior_lowpass {
            for filter in channel {
                filter.set_cutoff(perspective.exterior_lowpass_hz);
            }
        }
        for filter in &mut self.wet_lowpass {
            filter.set_cutoff(perspective.exterior_lowpass_hz);
        }
        self.dry.target(perspective.dry_gain);
        self.wet.target(perspective.reverb_gain);
        self.cabin.target(perspective.cabin_mix);
        parameters.width = crate::Unit::clamped(parameters.width.get() * reverb_width_scale(focus));
        configure_reverb(&mut self.reverb, parameters.room_size.get(), parameters);
    }

    pub fn tick(&mut self, source: RainFrame, gain: f32, send: f32, wet: f32) -> [f32; 2] {
        let dry_gain = self.dry.tick();
        let wet_gain = self.wet.tick() * wet;
        let cabin = self.cabin.tick();
        let exterior: [f32; 2] = std::array::from_fn(|channel| {
            let filters = &mut self.exterior_lowpass[channel];
            let filtered = filters[0].tick(source.exterior[channel]);
            filters[1].tick(filtered) * (1.0 - 0.40 * cabin)
        });
        let direct: [f32; 2] =
            std::array::from_fn(|i| (exterior[i] + source.cabin[i] * cabin) * gain);
        let tail = self.reverb.tick((direct[0] * send, direct[1] * send));
        let tail = [tail.0, tail.1];
        std::array::from_fn(|i| direct[i] * dry_gain + self.wet_lowpass[i].tick(tail[i]) * wet_gain)
    }

    pub fn snapshot(
        &self,
        zoom: f32,
        focus: f32,
        target_zoom: f32,
        target_focus: f32,
    ) -> RainSnapshot {
        let current = Perspective::new(zoom, focus, self.rate);
        let target = Perspective::new(target_zoom, target_focus, self.rate);
        let parameter = |current, target| ParameterSnapshot { current, target };
        RainSnapshot {
            exterior_lowpass_hz: parameter(current.exterior_lowpass_hz, target.exterior_lowpass_hz),
            dry_gain: parameter(self.dry.value(), target.dry_gain),
            reverb_gain: parameter(self.wet.value(), target.reverb_gain),
            cabin_mix: parameter(self.cabin.value(), target.cabin_mix),
        }
    }
}
