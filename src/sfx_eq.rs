use fundsp::hacker32::{AudioNode, Biquad, BiquadCoefs};
use serde::Serialize;

/// Shared SFX-bus equalization, applied to dry audio and the reverb send.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SfxEq {
    pub center_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

pub const SFX_EQ: SfxEq = SfxEq {
    center_hz: 3_000.0,
    gain_db: -9.0,
    q: 0.65,
};

fn coefficients(sample_rate: u32) -> BiquadCoefs<f32> {
    BiquadCoefs::bell(
        sample_rate as f32,
        SFX_EQ.center_hz,
        SFX_EQ.q,
        10.0_f32.powf(SFX_EQ.gain_db / 20.0),
    )
}

pub(crate) struct SfxEqFilter(Biquad<f32>);

impl SfxEqFilter {
    pub fn new(sample_rate: u32) -> Self {
        Self(Biquad::with_coefs(coefficients(sample_rate)))
    }
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        self.0.tick(&[input].into())[0]
    }
}

#[cfg(test)]
mod tests {
    use super::{SFX_EQ, SfxEqFilter};

    fn sine_gain_db(sample_rate: u32, frequency: f64) -> f64 {
        let mut filter = SfxEqFilter::new(sample_rate);
        let warmup = sample_rate / 20;
        let measured = sample_rate / 5;
        let mut input_energy = 0.0_f64;
        let mut output_energy = 0.0_f64;
        for frame in 0..warmup + measured {
            let phase =
                std::f64::consts::TAU * frequency * f64::from(frame) / f64::from(sample_rate);
            let input = phase.sin() as f32 * 0.1;
            let output = filter.tick(input);
            assert!(output.is_finite());
            if frame >= warmup {
                input_energy += f64::from(input).powi(2);
                output_energy += f64::from(output).powi(2);
            }
        }
        10.0 * (output_energy / input_energy).log10()
    }

    #[test]
    fn actual_filter_cuts_three_kilohertz_by_nine_decibels_at_supported_rates() {
        for sample_rate in [8_000, 44_100, 48_000, 96_000, 192_000] {
            let measured = sine_gain_db(sample_rate, 3_000.0);
            assert!(
                (measured + 9.0).abs() < 0.1,
                "{sample_rate} Hz: center gain {measured:.4} dB"
            );
            assert!((measured - f64::from(SFX_EQ.gain_db)).abs() < 0.1);
        }
    }

    #[test]
    fn dip_is_broad_while_low_bass_and_upper_air_are_preserved() {
        for frequency in [1_500.0, 6_000.0] {
            let measured = sine_gain_db(48_000, frequency);
            assert!(
                (-5.2..-3.7).contains(&measured),
                "{frequency} Hz should remain inside the broad dip: {measured:.3} dB"
            );
        }
        for (frequency, maximum_loss) in [(300.0, 0.4), (12_000.0, 1.2), (18_000.0, 0.4)] {
            let measured = sine_gain_db(48_000, frequency);
            assert!(
                (-maximum_loss..=0.02).contains(&measured),
                "{frequency} Hz should be outside the main dip: {measured:.3} dB"
            );
        }
    }

    #[test]
    fn channel_states_are_independent_and_decay_to_finite_silence() {
        for sample_rate in [8_000, 48_000, 192_000] {
            let mut left = SfxEqFilter::new(sample_rate);
            let mut right = SfxEqFilter::new(sample_rate);
            let mut left_energy = 0.0_f64;
            let mut late_peak = 0.0_f32;
            for frame in 0..sample_rate / 2 {
                let input = if frame == 0 { 1.0 } else { 0.0 };
                let output = left.tick(input);
                assert!(output.is_finite());
                assert_eq!(
                    right.tick(0.0),
                    0.0,
                    "left energy crossed into the right filter"
                );
                left_energy += f64::from(output).powi(2);
                if frame > sample_rate / 4 {
                    late_peak = late_peak.max(output.abs());
                }
            }
            assert!(left_energy > 0.1 && left_energy < 1.0);
            assert!(
                late_peak < 0.000001,
                "a stable bell must not ring indefinitely"
            );
        }
    }
}
