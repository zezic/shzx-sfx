use fundsp::hacker32::{AudioNode, Biquad, BiquadCoefs};

use crate::{DelaySnapshot, Pan, ParameterSnapshot, SpatialMode};

/// Precomputed amplitude panning for the two positional spatial modes.
#[derive(Clone, Copy)]
pub(crate) struct PanGains {
    pan: [f32; 2],
    haas: [f32; 2],
}

impl PanGains {
    pub fn new(position: Pan) -> Self {
        let gains = |pan: f32| {
            let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
            [angle.cos(), angle.sin()]
        };
        Self {
            pan: gains(position.get()),
            haas: gains(position.get() * 0.33),
        }
    }

    #[inline]
    pub fn blend(self, haas: f32) -> [f32; 2] {
        std::array::from_fn(|i| self.pan[i] + (self.haas[i] - self.pan[i]) * haas)
    }

    pub fn for_mode(self, spatial: SpatialMode) -> [f32; 2] {
        match spatial {
            SpatialMode::Pan => self.pan,
            SpatialMode::Haas => self.haas,
        }
    }
}

/// Preserve ordinary signal levels and stereo balance; reserve the soft knee
/// for overlapping events that exceed the output stage's headroom.
pub(crate) fn limit_stereo(frame: [f32; 2]) -> [f32; 2] {
    let peak = frame[0].abs().max(frame[1].abs());
    if peak <= 0.75 {
        return frame;
    }
    let target = 0.75 + 0.20 * fundsp::math::softsign((peak - 0.75) / 0.20);
    let gain = target / peak;
    [frame[0] * gain, frame[1] * gain]
}

#[cfg(test)]
mod tests {
    use super::{Echo, EchoParameters, Osc, SineTable, limit_stereo};

    #[test]
    fn startup_pitch_integrates_frequency_while_preserving_the_target() {
        let rate = 8_000.0;
        let hz = 173.0;
        let table = SineTable::new();
        let mut oscillator = Osc::new(hz, rate, 0.0);
        for sample in 0_u32..=8_000 {
            let time = sample as f32 / rate;
            let pitch_ratio = time * time * (3.0 - 2.0 * time);
            let value = oscillator.tick_scaled(&table, pitch_ratio);
            if sample.is_multiple_of(1_000) {
                let integrated_cycles = hz * (time.powi(3) - 0.5 * time.powi(4));
                // A left Riemann sum trails the continuous integral by half a step.
                let cycles = integrated_cycles - hz * pitch_ratio / (2.0 * rate);
                let expected = (cycles * std::f32::consts::TAU).sin();
                assert!(
                    (value - expected).abs() < 0.001,
                    "at {time}s: {value} vs {expected}"
                );
            }
        }
        assert_eq!(oscillator.frequency_hz(rate), hz);
        let mut saw = Osc::new(hz, rate, 0.0);
        for _ in 0..10 {
            assert!(saw.saw_scaled(0.0).is_finite());
        }
    }

    #[test]
    fn output_knee_preserves_normal_signals_and_stereo_balance_under_overload() {
        for frame in [[0.0, 0.0], [0.3, -0.15], [0.75, -0.5]] {
            assert_eq!(limit_stereo(frame), frame);
        }
        for frame in [[0.76, 0.38], [2.0, -0.5], [-100.0, 50.0]] {
            let out = limit_stereo(frame);
            assert!(out.iter().all(|sample| sample.abs() <= 0.95));
            assert!((out[0] * frame[1] - out[1] * frame[0]).abs() < 0.00001);
        }
    }

    #[test]
    fn ping_pong_echoes_alternate_channels_without_leaking_into_another_send() {
        let parameters = EchoParameters {
            seconds: 0.1,
            feedback: 0.5,
            send: 1.0,
        };
        let mut active = Echo::new(8_000, parameters, 3_000.0);
        let mut independent = Echo::new(8_000, parameters, 3_000.0);
        let mut returns = [[0.0_f64; 2]; 3];
        let mut tail_peak = 0.0_f32;
        for frame in 0..40_000 {
            let input = if frame == 0 { [1.0, 0.0] } else { [0.0; 2] };
            let out = active.tick(input);
            assert!(out.iter().all(|sample| sample.is_finite()));
            assert_eq!(independent.tick([0.0; 2]), [0.0; 2]);
            for (index, energy) in returns.iter_mut().enumerate() {
                if ((index + 1) * 800..(index + 1) * 800 + 40).contains(&frame) {
                    for channel in 0..2 {
                        energy[channel] += f64::from(out[channel]).powi(2);
                    }
                }
            }
            if frame > 32_000 {
                tail_peak = tail_peak.max(out[0].abs()).max(out[1].abs());
            }
        }
        assert!(returns[0][0] > 0.1 && returns[0][1] < 1.0e-12);
        assert!(returns[1][1] > 0.02 && returns[1][0] < 1.0e-12);
        assert!(returns[2][0] > 0.001 && returns[2][1] < 1.0e-12);
        assert!(returns[1][1] < returns[0][0]);
        assert!(returns[2][0] < returns[1][1]);
        assert!(tail_peak < 0.000001);
    }

    #[test]
    fn changing_echo_time_crossfades_taps_without_a_doppler_sweep() {
        let parameters = EchoParameters {
            seconds: 0.1,
            feedback: 0.0,
            send: 1.0,
        };
        let mut echo = Echo::new(8_000, parameters, 3_000.0);
        let destination = EchoParameters {
            seconds: 0.7,
            ..parameters
        };
        let mut previous = 0.0;
        let mut largest_step = 0.0_f32;
        for frame in 0..20_000 {
            if frame == 16_000 {
                echo.target(destination);
            }
            let input = (frame as f32 * std::f32::consts::TAU * 200.0 / 8_000.0).sin() * 0.1;
            let out = echo.tick([input, 0.0]);
            if (16_000..17_500).contains(&frame) {
                largest_step = largest_step.max((out[0] - previous).abs());
            }
            previous = out[0];
        }
        assert!(
            largest_step < 0.022,
            "tap changes introduced a fast phase sweep: {largest_step}"
        );
        assert!((echo.snapshot(destination).time_ms.current - 700.0).abs() < 0.1);
    }
}

#[derive(Clone)]
pub(crate) struct LowPass {
    node: Biquad<f32>,
    sample_rate: f32,
    q: f32,
}

impl LowPass {
    pub fn new(sample_rate: f32, cutoff: f32) -> Self {
        Self::resonant(sample_rate, cutoff, std::f32::consts::FRAC_1_SQRT_2)
    }

    pub fn resonant(sample_rate: f32, cutoff: f32, q: f32) -> Self {
        Self {
            node: Biquad::with_coefs(BiquadCoefs::lowpass(
                sample_rate,
                cutoff.min(sample_rate * 0.45),
                q,
            )),
            sample_rate,
            q,
        }
    }
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.node.set_coefs(BiquadCoefs::lowpass(
            self.sample_rate,
            cutoff.min(self.sample_rate * 0.45),
            self.q,
        ));
    }
    pub fn magnitude_at(&self, hz: f32) -> f32 {
        self.node
            .coefs()
            .response(f64::from(hz / self.sample_rate))
            .norm() as f32
    }
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        self.node.tick(&[input].into())[0]
    }
}

pub(crate) struct HighPass {
    node: Biquad<f32>,
    sample_rate: f32,
}

impl HighPass {
    pub fn new(sample_rate: f32, cutoff: f32) -> Self {
        Self {
            node: Biquad::with_coefs(BiquadCoefs::highpass(
                sample_rate,
                cutoff,
                std::f32::consts::FRAC_1_SQRT_2,
            )),
            sample_rate,
        }
    }
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.node.set_coefs(BiquadCoefs::highpass(
            self.sample_rate,
            cutoff,
            std::f32::consts::FRAC_1_SQRT_2,
        ));
    }
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        self.node.tick(&[input].into())[0]
    }
}

pub(crate) struct Smooth {
    value: f32,
    target: f32,
    coefficient: f32,
}

impl Smooth {
    pub fn new(value: f32, seconds: f32, rate: f32) -> Self {
        Self {
            value,
            target: value,
            coefficient: 1.0 - (-1.0 / (seconds * rate)).exp(),
        }
    }
    pub fn target(&mut self, value: f32) {
        self.target = value;
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    #[inline]
    pub fn tick(&mut self) -> f32 {
        self.value += (self.target - self.value) * self.coefficient;
        self.value
    }
}

pub(crate) const TABLE_LEN: usize = 4096;

/// Linear table lookup keeps the unison oscillator bank inexpensive.
pub(crate) struct SineTable([f32; TABLE_LEN]);

impl SineTable {
    pub fn new() -> Self {
        Self(std::array::from_fn(|i| {
            (i as f32 * std::f32::consts::TAU / TABLE_LEN as f32).sin()
        }))
    }
    #[inline]
    pub fn at(&self, phase: f32) -> f32 {
        let position = phase * TABLE_LEN as f32;
        let index = position as usize;
        let fraction = position - index as f32;
        let a = self.0[index & (TABLE_LEN - 1)];
        let b = self.0[(index + 1) & (TABLE_LEN - 1)];
        a + (b - a) * fraction
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Osc {
    phase: f32,
    step: f32,
}

impl Osc {
    pub fn new(hz: f32, sample_rate: f32, phase: f32) -> Self {
        Self {
            phase,
            step: hz / sample_rate,
        }
    }
    pub fn frequency(&mut self, hz: f32, sample_rate: f32) {
        self.step = hz / sample_rate;
    }
    pub fn frequency_hz(&self, sample_rate: f32) -> f32 {
        self.step * sample_rate
    }
    #[inline]
    pub fn tick(&mut self, table: &SineTable) -> f32 {
        self.tick_scaled(table, 1.0)
    }

    #[inline]
    pub fn tick_scaled(&mut self, table: &SineTable, pitch_ratio: f32) -> f32 {
        let value = table.at(self.phase);
        self.advance(self.step * pitch_ratio);
        value
    }

    #[inline]
    pub fn tick_modulated(
        &mut self,
        table: &SineTable,
        phase_cycles: f32,
        pitch_ratio: f32,
    ) -> f32 {
        let value = table.at((self.phase + phase_cycles).rem_euclid(1.0));
        self.advance(self.step * pitch_ratio);
        value
    }

    /// PolyBLEP removes the saw discontinuity's high-frequency aliasing.
    #[inline]
    pub fn saw_scaled(&mut self, pitch_ratio: f32) -> f32 {
        let step = self.step * pitch_ratio;
        let mut value = self.phase * 2.0 - 1.0;
        if self.phase < step {
            let t = self.phase / step;
            value -= t + t - t * t - 1.0;
        } else if self.phase > 1.0 - step {
            let t = (self.phase - 1.0) / step;
            value -= t * t + t + t + 1.0;
        }
        self.advance(step);
        value
    }

    #[inline]
    fn advance(&mut self, step: f32) {
        self.phase += step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct EchoParameters {
    pub seconds: f32,
    pub feedback: f32,
    pub send: f32,
}

#[derive(Clone, Copy)]
enum DelayTap {
    Steady { samples: f32 },
    Crossfading { from: f32, to: f32, progress: f32 },
}

/// Wet-only ping-pong delay. Fixed read taps crossfade when the requested time
/// changes, keeping a time adjustment from becoming an oscillator pitch sweep.
pub(crate) struct Echo {
    line: Vec<[f32; 2]>,
    cursor: usize,
    rate: f32,
    target: EchoParameters,
    tap: DelayTap,
    fade_step: f32,
    feedback: Smooth,
    send: Smooth,
    damp: [LowPass; 2],
}

impl Echo {
    pub fn new(rate: u32, initial: EchoParameters, cutoff: f32) -> Self {
        let sample_rate = rate as f32;
        Self {
            line: vec![[0.0; 2]; (sample_rate * 1.2).ceil() as usize + 2],
            cursor: 0,
            rate: sample_rate,
            target: initial,
            tap: DelayTap::Steady {
                samples: initial.seconds * sample_rate,
            },
            fade_step: 1.0 / (sample_rate * 0.12),
            feedback: Smooth::new(initial.feedback, 0.09, sample_rate),
            send: Smooth::new(initial.send, 0.06, sample_rate),
            damp: std::array::from_fn(|_| LowPass::new(sample_rate, cutoff)),
        }
    }

    pub fn target(&mut self, target: EchoParameters) {
        self.target = target;
        self.feedback.target(target.feedback);
        self.send.target(target.send);
    }

    fn read(&self, samples: f32) -> [f32; 2] {
        let mut position = self.cursor as f32 - samples;
        if position < 0.0 {
            position += self.line.len() as f32;
        }
        let whole = position as usize;
        let fraction = position - whole as f32;
        // A tiny negative position can round to exactly len when wrapped in
        // f32. That point is sample zero on the circular line.
        let index = if whole == self.line.len() { 0 } else { whole };
        let next = if index + 1 == self.line.len() {
            0
        } else {
            index + 1
        };
        std::array::from_fn(|channel| {
            self.line[index][channel]
                + (self.line[next][channel] - self.line[index][channel]) * fraction
        })
    }

    pub fn tick(&mut self, input: [f32; 2]) -> [f32; 2] {
        let delayed = match self.tap {
            DelayTap::Steady { samples } => {
                let target = self.target.seconds * self.rate;
                if (samples - target).abs() >= self.rate * 0.003 {
                    self.tap = DelayTap::Crossfading {
                        from: samples,
                        to: target,
                        progress: 0.0,
                    };
                }
                self.read(samples)
            }
            DelayTap::Crossfading { from, to, progress } => {
                let a = self.read(from);
                let b = self.read(to);
                let blend = progress * progress * (3.0 - 2.0 * progress);
                let next = progress + self.fade_step;
                self.tap = if next >= 1.0 {
                    DelayTap::Steady { samples: to }
                } else {
                    DelayTap::Crossfading {
                        from,
                        to,
                        progress: next,
                    }
                };
                std::array::from_fn(|channel| a[channel] * (1.0 - blend) + b[channel] * blend)
            }
        };
        let wet = [self.damp[0].tick(delayed[0]), self.damp[1].tick(delayed[1])];
        let feedback = self.feedback.tick();
        let send = self.send.tick();
        self.line[self.cursor] = [
            input[0] * send + wet[1] * feedback,
            input[1] * send + wet[0] * feedback,
        ];
        self.cursor = (self.cursor + 1) % self.line.len();
        wet
    }

    pub fn snapshot(&self, requested: EchoParameters) -> DelaySnapshot {
        let samples = match self.tap {
            DelayTap::Steady { samples } => samples,
            DelayTap::Crossfading { from, to, progress } => {
                let blend = progress * progress * (3.0 - 2.0 * progress);
                from * (1.0 - blend) + to * blend
            }
        };
        DelaySnapshot {
            time_ms: ParameterSnapshot {
                current: samples * 1000.0 / self.rate,
                target: requested.seconds * 1000.0,
            },
            feedback: ParameterSnapshot {
                current: self.feedback.value(),
                target: requested.feedback,
            },
            send: ParameterSnapshot {
                current: self.send.value(),
                target: requested.send,
            },
        }
    }
}
