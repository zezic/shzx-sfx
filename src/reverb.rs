//! A diffuse, slowly modulated native Rust reverb with live loop controls.

use fundsp::hacker32::{An, AudioNode, AudioUnit, Frame, Shared, U1, reverb2_stereo};

const ROOM_METERS: f32 = 20.0;
const BASE_REVERB_SECONDS: f32 = 60.0;
// Mean of the 32 delay lengths used by FunDSP's 20-meter hybrid FDN. The
// allpass diffusers add dispersion, so this controls an approximate RT60.
const MEAN_LOOP_SECONDS: f32 = 0.103_906_25;

#[derive(Clone)]
struct LoopFilter {
    gain: Shared,
    pole: Shared,
    state: f32,
}

impl AudioNode for LoopFilter {
    const ID: u64 = 0x5346_5852;
    type Inputs = U1;
    type Outputs = U1;

    #[inline]
    fn tick(&mut self, input: &Frame<f32, U1>) -> Frame<f32, U1> {
        self.state += (input[0] - self.state) * self.pole.value();
        [self.state * self.gain.value()].into()
    }

    fn reset(&mut self) {
        self.state = 0.0;
    }
}

/// A 32-channel hybrid FDN with pre-diffusion and independently modulated delay
/// lines. Parameter changes update loop loss and damping without replacing the
/// processor, clearing memory, or reallocating its delay lines.
pub(crate) struct SpaceReverb {
    node: Box<dyn AudioUnit>,
    loop_gain: Shared,
    loop_pole: Shared,
    sample_rate: f32,
    room_size: f32,
    damping: f32,
    width: f32,
    wet: f32,
    dry: f32,
}

impl SpaceReverb {
    pub fn new(sample_rate: u32) -> Self {
        let loop_gain = Shared::new(0.9);
        let loop_pole = Shared::new(0.4);
        let filter = An(LoopFilter {
            gain: loop_gain.clone(),
            pole: loop_pole.clone(),
            state: 0.0,
        });
        let mut node: Box<dyn AudioUnit> = Box::new(reverb2_stereo(
            ROOM_METERS,
            BASE_REVERB_SECONDS,
            0.78,
            0.2,
            filter,
        ));
        node.set_sample_rate(sample_rate as f64);
        node.allocate();
        let mut reverb = Self {
            node,
            loop_gain,
            loop_pole,
            sample_rate: sample_rate as f32,
            room_size: 0.0,
            damping: 0.0,
            width: 1.0,
            wet: 0.0,
            dry: 0.0,
        };
        reverb.update_decay(0.5);
        reverb.update_damping(0.5);
        reverb
    }

    pub fn set_room_size(&mut self, size: f32) {
        if size != self.room_size {
            self.update_decay(size);
        }
    }

    fn update_decay(&mut self, size: f32) {
        self.room_size = size;
        let seconds = 0.7 + size * size * 9.3;
        let desired = 10.0_f32.powf(-3.0 * MEAN_LOOP_SECONDS / seconds);
        let baked = 10.0_f32.powf(-3.0 * 0.035 / BASE_REVERB_SECONDS);
        self.loop_gain.set(desired / baked);
    }

    pub fn set_dampening(&mut self, damping: f32) {
        if damping != self.damping {
            self.update_damping(damping);
        }
    }

    fn update_damping(&mut self, damping: f32) {
        self.damping = damping;
        let cutoff = (600.0 + (1.0 - damping).powi(2) * 12_000.0).min(self.sample_rate * 0.45);
        self.loop_pole
            .set(1.0 - (-std::f32::consts::TAU * cutoff / self.sample_rate).exp());
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width;
    }
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = wet;
    }
    pub fn set_dry(&mut self, dry: f32) {
        self.dry = dry;
    }

    #[inline]
    pub fn tick(&mut self, input: (f32, f32)) -> (f32, f32) {
        let (left, right) = self.node.filter_stereo(input.0, input.1);
        let mid = (left + right) * 0.5;
        let side = (left - right) * 0.5 * self.width;
        (
            (mid + side) * self.wet + input.0 * self.dry,
            (mid - side) * self.wet + input.1 * self.dry,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SpaceReverb;

    #[test]
    fn diffuse_tail_is_stereo_dense_and_survives_live_parameter_changes() {
        let mut reverb = SpaceReverb::new(8_000);
        reverb.set_room_size(0.9);
        reverb.set_dampening(0.5);
        reverb.set_wet(1.0);
        let mut energy = [0.0_f64; 2];
        let mut occupied = 0;
        let mut observed = 0;
        for frame in 0..32_000 {
            if frame == 12_000 {
                reverb.set_room_size(0.8);
                reverb.set_dampening(0.6);
                reverb.set_width(0.8);
            }
            let input = if frame == 0 { (1.0, 0.0) } else { (0.0, 0.0) };
            let (left, right) = reverb.tick(input);
            assert!(left.is_finite() && right.is_finite());
            if (4_000..8_000).contains(&frame) {
                observed += 1;
                occupied += usize::from(left.abs().max(right.abs()) > 0.000001);
            }
            if frame > 16_000 {
                energy[0] += f64::from(left).powi(2);
                energy[1] += f64::from(right).powi(2);
            }
        }
        assert!(
            occupied as f32 / observed as f32 > 0.95,
            "late reflections should form a dense tail"
        );
        assert!(
            energy[0] > 0.00001 && energy[1] > 0.00001,
            "tail must survive live edits"
        );
    }
}
