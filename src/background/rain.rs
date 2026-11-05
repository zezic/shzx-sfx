use fundsp::hacker32::{AudioNode, Biquad, BiquadCoefs};

use crate::dsp::{HighPass, LowPass, Smooth};

const SURFACES: usize = 6;
const MAX_IMPACT: f32 = 0.55;

pub(crate) struct RainFrame {
    pub exterior: [f32; 2],
    pub cabin: [f32; 2],
}

struct ImpactEnvelope {
    level: f32,
    decay: f32,
    attack: Smooth,
}

impl ImpactEnvelope {
    fn new(rate: f32, attack: f32, decay: f32) -> Self {
        Self {
            level: 0.0,
            decay: (-1.0 / (rate * decay)).exp(),
            attack: Smooth::new(0.0, attack, rate),
        }
    }

    fn accumulate(&mut self, impulse: f32) -> f32 {
        self.level += impulse;
        self.tick()
    }

    fn retrigger(&mut self, peak: f32) -> f32 {
        self.level = self.level.max(peak);
        self.tick()
    }

    fn tick(&mut self) -> f32 {
        self.attack.target(self.level);
        self.level *= self.decay;
        self.attack.tick()
    }
}

/// A fixed material patch accumulates overlapping impacts in its resonators.
/// The two low modes represent a damped panel; the brighter mode adds a small
/// liquid-impact component to the exterior wash.
struct Surface {
    body: [Biquad<f32>; 2],
    contact: LowPass,
    splash: Biquad<f32>,
    splash_envelope: ImpactEnvelope,
    body_envelope: ImpactEnvelope,
    // Normalize discrete impulses separately from the sustained noise burst.
    sample_scale: f32,
    pan: [f32; 2],
}

impl Surface {
    fn new(rate: f32, position: usize, rng: &mut fastrand::Rng) -> Self {
        let low = 210.0 + rng.f32() * 310.0;
        let high = low * (2.1 + rng.f32() * 0.9);
        let liquid = (1_700.0 + rng.f32() * 1_800.0).min(rate * 0.35);
        let pan = (position as f32 + 0.5) / SURFACES as f32;
        let angle = pan * std::f32::consts::FRAC_PI_2;
        Self {
            body: [
                Biquad::with_coefs(BiquadCoefs::resonator(rate, low, 4.5)),
                Biquad::with_coefs(BiquadCoefs::resonator(rate, high, 2.8)),
            ],
            contact: LowPass::new(rate, (low * 3.0).min(1_250.0)),
            splash: Biquad::with_coefs(BiquadCoefs::resonator(rate, liquid, 3.0)),
            splash_envelope: ImpactEnvelope::new(rate, 0.0006, 0.006 + rng.f32() * 0.019),
            body_envelope: ImpactEnvelope::new(rate, 0.002, 0.030 + (low - 210.0) * 0.0001),
            sample_scale: (rate / 48_000.0).sqrt(),
            pan: [angle.cos(), angle.sin()],
        }
    }

    fn tick(&mut self, impulse: f32, noise: f32) -> (f32, f32) {
        let burst = self.splash_envelope.accumulate(impulse.abs() * 0.20);
        // Larger drops excite a longer noisy body. Retriggering preserves its
        // attack and bounds overlapping contacts within each material patch.
        let body_envelope = self.body_envelope.retrigger(impulse * impulse / MAX_IMPACT);
        // Normalize only the discrete contact; the sustained noise drives
        // energy-normalized modes without sample-rate compensation.
        let impact = impulse * self.sample_scale;
        let panel = self
            .contact
            .tick(impact * 0.12 + noise * body_envelope * 0.08);
        let body = self.body[0].tick(&[panel].into())[0] * 2.5
            + self.body[1].tick(&[panel].into())[0] * 1.9;
        let splash = self.splash.tick(&[impact + noise * burst].into())[0] * 0.30
            + noise * burst * self.sample_scale * 0.375;
        (body, splash)
    }
}

/// Procedural rainfall combines an aggregate wash with sparse material impacts.
/// Both listening perspectives advance together, so entering the cabin reveals
/// the same ongoing rainfall rather than restarting an event sequence.
pub(super) struct Rain {
    rng: fastrand::Rng,
    surfaces: [Surface; SURFACES],
    wash_lowpass: [LowPass; 2],
    wash_highpass: [HighPass; 2],
    splash_lowpass: [LowPass; 2],
    cabin_lowpass: [[LowPass; 2]; 2],
    gust: Smooth,
    until_gust: u32,
    sample_rate: f32,
    hit_probability: f32,
    sample_scale: f32,
}

impl Rain {
    pub fn new(rate: u32, seed: u64) -> Self {
        let sample_rate = rate as f32;
        let mut rng = fastrand::Rng::with_seed(seed);
        Self {
            surfaces: std::array::from_fn(|i| Surface::new(sample_rate, i, &mut rng)),
            wash_lowpass: std::array::from_fn(|_| LowPass::new(sample_rate, 6_200.0)),
            wash_highpass: std::array::from_fn(|_| HighPass::new(sample_rate, 230.0)),
            splash_lowpass: std::array::from_fn(|_| LowPass::new(sample_rate, 4_800.0)),
            cabin_lowpass: std::array::from_fn(|_| {
                std::array::from_fn(|_| LowPass::new(sample_rate, 1_100.0))
            }),
            gust: Smooth::new(1.0, 1.3, sample_rate),
            until_gust: rate,
            sample_rate,
            hit_probability: 90.0 / sample_rate,
            // Preserve noise power per Hz when the device rate changes.
            sample_scale: (sample_rate / 48_000.0).sqrt(),
            rng,
        }
    }

    pub fn tick(&mut self) -> RainFrame {
        if self.until_gust == 0 {
            self.gust.target(0.78 + self.rng.f32() * 0.44);
            self.until_gust = (self.sample_rate * (0.7 + self.rng.f32() * 1.3)) as u32;
        }
        self.until_gust -= 1;
        let gust = self.gust.tick();
        let mut impulses = [0.0; SURFACES];
        if self.rng.f32() < self.hit_probability * gust {
            let surface = self.rng.usize(..SURFACES);
            let size = self.rng.f32();
            let polarity = if self.rng.bool() { 1.0 } else { -1.0 };
            impulses[surface] = polarity * (0.24 + size * size * 0.76) * MAX_IMPACT;
        }
        let noise = [self.rng.f32() * 2.0 - 1.0, self.rng.f32() * 2.0 - 1.0];
        let mut exterior = [0.0; 2];
        let mut cabin = [0.0; 2];
        for (surface, impulse) in self.surfaces.iter_mut().zip(impulses) {
            let excitation = self.rng.f32() * 2.0 - 1.0;
            let (body, splash) = surface.tick(impulse, excitation);
            for channel in 0..2 {
                exterior[channel] += splash * surface.pan[channel];
                cabin[channel] += body * surface.pan[channel];
            }
        }
        for channel in 0..2 {
            let wash =
                self.wash_highpass[channel].tick(self.wash_lowpass[channel].tick(noise[channel]));
            exterior[channel] = wash * (0.038 * self.sample_scale * gust)
                + self.splash_lowpass[channel].tick(exterior[channel]);
            let filters = &mut self.cabin_lowpass[channel];
            let filtered = filters[0].tick(cabin[channel]);
            cabin[channel] = filters[1].tick(filtered);
        }
        RainFrame { exterior, cabin }
    }
}

#[cfg(test)]
mod tests {
    use super::Rain;

    #[test]
    fn cabin_strikes_have_a_sustained_body_after_the_contact() {
        for rate in [8_000, 48_000, 192_000] {
            for seed in [123, 456, 789] {
                let mut rain = Rain::new(rate, seed);
                let mut noise = fastrand::Rng::with_seed(seed);
                let energy: Vec<f64> = (0..rate / 5)
                    .map(|sample| {
                        let impulse = if sample == 0 { 0.4 } else { 0.0 };
                        let (body, _) = rain.surfaces[0].tick(impulse, noise.f32() * 2.0 - 1.0);
                        let filters = &mut rain.cabin_lowpass[0];
                        let filtered = filters[0].tick(body);
                        f64::from(filters[1].tick(filtered)).powi(2)
                    })
                    .collect();
                let total = energy.iter().sum::<f64>();
                let body_share = energy[(rate * 15 / 1_000) as usize..(rate * 80 / 1_000) as usize]
                    .iter()
                    .sum::<f64>()
                    / total;
                let quantile = |fraction| {
                    let mut accumulated = 0.0;
                    energy
                        .iter()
                        .position(|value| {
                            accumulated += value;
                            accumulated >= total * fraction
                        })
                        .unwrap() as f32
                        / rate as f32
                };
                let duration = quantile(0.9) - quantile(0.1);
                eprintln!(
                    "{rate}Hz seed{seed}: body share {body_share:.3}, duration {duration:.4}s"
                );
                assert!(body_share > 0.2, "roof strikes need body after their onset");
                assert!(duration > 0.02, "roof strikes must last longer than clicks");
            }
        }
    }

    #[test]
    fn rainfall_is_seeded_stereo_and_bounded_across_device_rates() {
        for rate in [8_000, 48_000, 192_000] {
            let mut rain = Rain::new(rate, 123);
            let mut same = Rain::new(rate, 123);
            let mut energy = [0.0_f64; 2];
            let mut peak = [0.0_f32; 2];
            let mut stereo_difference = 0.0_f64;
            for _ in 0..rate * 3 {
                let frame = rain.tick();
                let repeat = same.tick();
                assert_eq!(frame.exterior, repeat.exterior);
                assert_eq!(frame.cabin, repeat.cabin);
                for (index, signal) in [frame.exterior, frame.cabin].into_iter().enumerate() {
                    for sample in signal {
                        assert!(sample.is_finite());
                        energy[index] += f64::from(sample).powi(2);
                        peak[index] = peak[index].max(sample.abs());
                    }
                }
                stereo_difference += f64::from(frame.exterior[0] - frame.exterior[1]).powi(2);
            }
            let rms = energy.map(|value| (value / f64::from(rate * 3 * 2)).sqrt());
            eprintln!("{rate}Hz rain source RMS {rms:?}, peak {peak:?}");
            assert!(rms[0] > 0.008 && rms[0] < 0.06);
            assert!(rms[1] > 0.001 && rms[1] < 0.02);
            assert!(peak.into_iter().all(|value| value < 0.35));
            assert!(stereo_difference > energy[0] * 0.2);
        }
    }
}
