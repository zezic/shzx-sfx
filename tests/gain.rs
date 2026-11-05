use shzx_sfx::{Command, Mix, OutputGain, RenderConfig, Renderer, SfxEvent, SfxKind, Unit};
use strum::IntoEnumIterator;

const SAMPLE_RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug)]
struct Levels {
    peak: f32,
    rms: f64,
}

fn levels(frames: &[[f32; 2]]) -> Levels {
    let peak = frames
        .iter()
        .flatten()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max);
    let mean_square = frames
        .iter()
        .flatten()
        .map(|&sample| f64::from(sample).powi(2))
        .sum::<f64>()
        / (frames.len() * 2) as f64;
    Levels {
        peak,
        rms: mean_square.sqrt(),
    }
}

fn renderer(mix: Mix) -> Renderer {
    Renderer::new(RenderConfig {
        sample_rate: SAMPLE_RATE,
        seed: 123,
        mix,
        ..RenderConfig::default()
    })
    .unwrap()
}

#[test]
fn default_ui_levels_are_audible_without_maximizing_the_mixer() {
    let mix = Mix {
        drone: Unit::ZERO,
        demonic_voices: Unit::ZERO,
        demonic_stutters: Unit::ZERO,
        flying_coins: Unit::ZERO,
        laser_beams: Unit::ZERO,
        rain: Unit::ZERO,
        sub: Unit::ZERO,
        transmission: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        ..Mix::default()
    };
    assert!((20.0 * mix.ui.get().log10() + 10.0).abs() < 0.00001);
    let mut measurements = Vec::new();
    // Listening references include the -10 dB UI bus and broad 3 kHz comfort
    // cut; avoid makeup gain that would restore the removed ear pressure.
    for (kind, minimum_peak, minimum_rms) in [
        (SfxKind::Click, 0.110, 0.0158),
        (SfxKind::Confirm, 0.095, 0.0158),
        (SfxKind::OrderFill, 0.095, 0.0123),
    ] {
        let mut renderer = renderer(mix);
        let mut warmup = vec![[0.0; 2]; SAMPLE_RATE as usize * 5 / 4];
        renderer.render(&mut warmup);
        renderer.apply(Command::Trigger(SfxEvent::new(kind)));
        let duration = renderer.snapshot().last_event.unwrap().duration_seconds;
        let mut event = vec![[0.0; 2]; (duration * SAMPLE_RATE as f32).ceil() as usize];
        renderer.render(&mut event);
        let measured = levels(&event);
        eprintln!(
            "{kind}: peak {:.6} ({:.2} dBFS), active-window RMS {:.6} ({:.2} dBFS)",
            measured.peak,
            measured.peak.log10() * 20.0,
            measured.rms,
            measured.rms.log10() * 20.0
        );
        measurements.push((kind, minimum_peak, minimum_rms, measured));
    }
    for (kind, minimum_peak, minimum_rms, measured) in measurements {
        assert!(
            measured.peak >= minimum_peak,
            "default {kind} peak {:.6} is below calibrated floor {minimum_peak}; active-window RMS {:.6}",
            measured.peak,
            measured.rms
        );
        assert!(
            measured.rms >= minimum_rms,
            "default {kind} active-window RMS {:.6} is below the listening reference {minimum_rms}; peak alone does not establish audibility",
            measured.rms
        );
        assert!(
            measured.peak <= 0.95,
            "default {kind} should stay inside the output sample ceiling: {measured:?}"
        );
    }
}

#[test]
fn default_environment_is_restrained_but_audible_at_midrange_controls() {
    let mut renderer = renderer(Mix::default());
    let mut environment = vec![[0.0; 2]; SAMPLE_RATE as usize * 3];
    renderer.render(&mut environment);
    let measured = levels(&environment[SAMPLE_RATE as usize..]);
    eprintln!(
        "default environment: peak {:.6} ({:.2} dBFS), RMS {:.6} ({:.2} dBFS)",
        measured.peak,
        measured.peak.log10() * 20.0,
        measured.rms,
        measured.rms.log10() * 20.0
    );
    assert!(
        (0.05..=0.25).contains(&measured.rms),
        "default environment should have audible, restrained RMS without maxed controls: {measured:?}"
    );
    assert!(
        measured.peak < 0.65,
        "ambient default should retain event headroom: {measured:?}"
    );
}

#[test]
fn default_non_drone_layers_keep_an_audible_presence() {
    let defaults = Mix::default();
    for (name, mix, window_start, window_end, minimum_rms) in [
        (
            "texture",
            Mix {
                drone: Unit::ZERO,
                demonic_voices: Unit::ZERO,
                demonic_stutters: Unit::ZERO,
                flying_coins: Unit::ZERO,
                laser_beams: Unit::ZERO,
                rain: Unit::ZERO,
                sub: Unit::ZERO,
                transmission: Unit::ZERO,
                harmony: Unit::ZERO,
                melody: Unit::ZERO,
                ..defaults
            },
            1.0,
            3.0,
            0.012,
        ),
        (
            "harmony",
            Mix {
                drone: Unit::ZERO,
                demonic_voices: Unit::ZERO,
                demonic_stutters: Unit::ZERO,
                flying_coins: Unit::ZERO,
                laser_beams: Unit::ZERO,
                rain: Unit::ZERO,
                sub: Unit::ZERO,
                transmission: Unit::ZERO,
                texture: Unit::ZERO,
                melody: Unit::ZERO,
                ..defaults
            },
            1.0,
            3.0,
            0.008,
        ),
        (
            "melody",
            Mix {
                drone: Unit::ZERO,
                demonic_voices: Unit::ZERO,
                demonic_stutters: Unit::ZERO,
                flying_coins: Unit::ZERO,
                laser_beams: Unit::ZERO,
                rain: Unit::ZERO,
                sub: Unit::ZERO,
                transmission: Unit::ZERO,
                texture: Unit::ZERO,
                harmony: Unit::ZERO,
                ..defaults
            },
            10.0,
            12.0,
            0.0006,
        ),
    ] {
        let mut renderer = renderer(mix);
        let mut frames = vec![[0.0; 2]; (window_end * SAMPLE_RATE as f32) as usize];
        renderer.render(&mut frames);
        let measured = levels(&frames[(window_start * SAMPLE_RATE as f32) as usize..]);
        eprintln!(
            "default {name}: peak {:.6} ({:.2} dBFS), RMS {:.6} ({:.2} dBFS)",
            measured.peak,
            measured.peak.log10() * 20.0,
            measured.rms,
            measured.rms.log10() * 20.0
        );
        assert!(
            measured.rms >= minimum_rms,
            "default {name} is buried despite a healthy low-drone meter: {measured:?}"
        );
        assert!(
            measured.peak < 0.65,
            "default {name} should retain headroom"
        );
    }
}

#[test]
fn ordinary_event_levels_follow_the_master_without_continuous_saturation() {
    let mix = Mix {
        master: Unit::new(0.2).unwrap(),
        drone: Unit::ZERO,
        demonic_voices: Unit::ZERO,
        demonic_stutters: Unit::ZERO,
        flying_coins: Unit::ZERO,
        laser_beams: Unit::ZERO,
        rain: Unit::ZERO,
        sub: Unit::ZERO,
        transmission: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        reverb: Unit::ZERO,
        ..Mix::default()
    };
    let mut quiet = renderer(mix);
    let mut louder = renderer(Mix {
        master: Unit::new(0.4).unwrap(),
        ..mix
    });
    let mut warmup = [[0.0; 2]; 2_000];
    quiet.render(&mut warmup);
    louder.render(&mut warmup);
    for renderer in [&mut quiet, &mut louder] {
        renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
    }
    for _ in 0..48_000 {
        let a = quiet.next_frame();
        let b = louder.next_frame();
        for (quiet, louder) in a.into_iter().zip(b) {
            assert!(
                (louder - quiet * 2.0).abs() < 0.000002,
                "ordinary event levels should retain linear master control"
            );
        }
    }
}

#[test]
fn every_default_effect_stays_inside_the_output_ceiling_after_boost() {
    let mix = Mix {
        drone: Unit::ZERO,
        demonic_voices: Unit::ZERO,
        demonic_stutters: Unit::ZERO,
        flying_coins: Unit::ZERO,
        laser_beams: Unit::ZERO,
        rain: Unit::ZERO,
        sub: Unit::ZERO,
        transmission: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        ..Mix::default()
    };
    let (loudest_kind, peak) = SfxKind::iter()
        .map(|kind| {
            let mut renderer = renderer(mix);
            let mut warmup = vec![[0.0; 2]; SAMPLE_RATE as usize * 5 / 4];
            renderer.render(&mut warmup);
            renderer.apply(Command::Trigger(SfxEvent::new(kind)));
            let duration = renderer.snapshot().last_event.unwrap().duration_seconds;
            let mut output = vec![[0.0; 2]; (duration * SAMPLE_RATE as f32).ceil() as usize];
            renderer.render(&mut output);
            assert!(
                output.iter().flatten().all(|sample| sample.is_finite()),
                "default {kind} must remain finite after output calibration"
            );
            (kind, levels(&output).peak)
        })
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .expect("the effect palette is populated");
    eprintln!(
        "loudest default effect: {loudest_kind}, peak {peak:.6} ({:.2} dBFS)",
        peak.log10() * 20.0
    );
    assert!(
        peak <= 0.95,
        "default {loudest_kind} must stay inside the output sample ceiling: peak {peak:.6}"
    );
}

#[test]
fn output_gain_rejects_invalid_decibels_and_serialized_values() {
    for db in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -60.1, 24.1] {
        assert!(OutputGain::new(db).is_err());
    }
    assert_eq!(OutputGain::new(0.0).unwrap().amplitude(), 1.0);
    assert!((OutputGain::new(12.0).unwrap().amplitude() - 3.981_071_7).abs() < 0.000001);
    assert!(OutputGain::new(OutputGain::MIN_DB).is_ok());
    assert!(OutputGain::new(OutputGain::MAX_DB).is_ok());
    assert!(serde_json::from_str::<OutputGain>("24.1").is_err());
    assert!(serde_json::from_str::<Mix>(r#"{"output_gain":-61}"#).is_err());
    let encoded = serde_json::to_string(&OutputGain::new(6.0).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_str::<OutputGain>(&encoded).unwrap().db(),
        6.0
    );
}

#[test]
fn output_gain_is_an_independent_linear_boost_below_the_output_knee() {
    let neutral_gain = OutputGain::new(0.0).unwrap();
    let boosted_gain = OutputGain::new(6.0).unwrap();
    let mix = Mix {
        master: Unit::new(0.15).unwrap(),
        output_gain: neutral_gain,
        drone: Unit::ZERO,
        demonic_voices: Unit::ZERO,
        demonic_stutters: Unit::ZERO,
        flying_coins: Unit::ZERO,
        laser_beams: Unit::ZERO,
        rain: Unit::ZERO,
        sub: Unit::ZERO,
        transmission: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        reverb: Unit::ZERO,
        ..Mix::default()
    };
    let mut neutral = renderer(mix);
    let mut boosted = renderer(Mix {
        output_gain: boosted_gain,
        ..mix
    });
    let mut warmup = [[0.0; 2]; 2_000];
    neutral.render(&mut warmup);
    boosted.render(&mut warmup);
    for renderer in [&mut neutral, &mut boosted] {
        renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
    }
    let ratio = boosted_gain.amplitude();
    let mut difference_energy = 0.0_f64;
    let mut expected_energy = 0.0_f64;
    let mut maximum_difference = 0.0_f32;
    for _ in 0..SAMPLE_RATE {
        let a = neutral.next_frame();
        let b = boosted.next_frame();
        for (neutral, boosted) in a.into_iter().zip(b) {
            let expected = neutral * ratio;
            let difference = boosted - expected;
            maximum_difference = maximum_difference.max(difference.abs());
            difference_energy += f64::from(difference).powi(2);
            expected_energy += f64::from(expected).powi(2);
        }
    }
    let relative_error = (difference_energy / expected_energy).sqrt();
    eprintln!(
        "output trim waveform: maximum difference {maximum_difference:.8}, relative RMS error {relative_error:.8}"
    );
    assert!(
        maximum_difference < 0.00005 && relative_error < 0.001,
        "output dB trim must preserve the waveform below the output knee; relative RMS error {relative_error}"
    );
    let target = OutputGain::new(24.0).unwrap();
    neutral.apply(Command::Mix(Mix {
        output_gain: target,
        ..mix
    }));
    assert_eq!(neutral.snapshot().mix.output_gain, target);
    assert!(neutral.snapshot().output_gain_db < target.db());
    for _ in 0..SAMPLE_RATE / 2 {
        neutral.next_frame();
    }
    assert!((neutral.snapshot().output_gain_db - target.db()).abs() < 0.01);
}

#[test]
fn maximum_output_boost_stays_finite_and_inside_the_sample_ceiling() {
    let mut renderer = renderer(Mix {
        master: Unit::ONE,
        output_gain: OutputGain::new(OutputGain::MAX_DB).unwrap(),
        ..Mix::default()
    });
    let mut warmup = [[0.0; 2]; 2_000];
    renderer.render(&mut warmup);
    for kind in SfxKind::iter() {
        renderer.apply(Command::Trigger(SfxEvent::new(kind)));
    }
    let mut peak = 0.0_f32;
    for _ in 0..SAMPLE_RATE {
        let frame = renderer.next_frame();
        for sample in frame {
            assert!(sample.is_finite() && sample.abs() <= 0.95);
            peak = peak.max(sample.abs());
        }
    }
    assert!(peak > 0.75, "the test must exercise the output knee");
}
