use shzx_sfx::{
    Candle, Chord, ChordShape, Command, MarketContext, Mix, Mode, Pan, RenderConfig, Renderer,
    SfxEvent, SfxKind, SpatialMode, Symbol, Trend, Unit,
};
use strum::IntoEnumIterator;

fn config(sample_rate: u32) -> RenderConfig {
    RenderConfig {
        sample_rate,
        max_voices: 8,
        seed: 71,
        ..RenderConfig::default()
    }
}

fn unit(value: f32) -> Unit {
    Unit::new(value).unwrap()
}

fn energy(frames: &[[f32; 2]]) -> f64 {
    frames
        .iter()
        .flatten()
        .map(|&sample| f64::from(sample).powi(2))
        .sum()
}

#[test]
fn external_numeric_and_serialized_inputs_preserve_their_domains() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
        assert!(Unit::new(value).is_err());
    }
    for value in [f32::NAN, f32::INFINITY, -1.01, 1.01] {
        assert!(Pan::new(value).is_err());
    }
    assert_eq!(Unit::new(0.0).unwrap(), Unit::ZERO);
    assert_eq!(Unit::new(1.0).unwrap(), Unit::ONE);
    assert!(Pan::new(-1.0).is_ok());
    assert!(Pan::new(1.0).is_ok());
    assert!(serde_json::from_str::<Unit>("1.01").is_err());
    assert!(serde_json::from_str::<Pan>("-1.01").is_err());
    assert!(serde_json::from_str::<Mix>(r#"{"unexpected":0.5}"#).is_err());
    assert!(
        serde_json::from_str::<Command>(
            r#"{"type":"trigger","value":{"kind":"click","pan":0,"intensity":2}}"#
        )
        .is_err()
    );
    let command = Command::Trigger(SfxEvent {
        layers: shzx_sfx::SfxLayerSelection::All,
        edits: shzx_sfx::SfxEdits::DEFAULT,
        kind: SfxKind::OrderFill,
        pan: Pan::new(-0.7).unwrap(),
        intensity: unit(0.4),
    });
    let encoded = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&encoded).unwrap(), command);

    for invalid in [
        RenderConfig {
            sample_rate: 7_999,
            ..config(48_000)
        },
        RenderConfig {
            sample_rate: 192_001,
            ..config(48_000)
        },
        RenderConfig {
            max_voices: 0,
            ..config(48_000)
        },
        RenderConfig {
            max_voices: 129,
            ..config(48_000)
        },
    ] {
        assert!(Renderer::new(invalid).is_err());
    }
}

#[test]
fn visible_candle_metrics_are_scale_independent_and_use_mean_volume() {
    let candles = [
        Candle::new(100.0, 100.5, 99.8, 100.3, 0.0).unwrap(),
        Candle::new(100.3, 101.0, 100.1, 100.8, 200.0).unwrap(),
    ];
    let scaled = [
        Candle::new(100_000.0, 100_500.0, 99_800.0, 100_300.0, 0.0).unwrap(),
        Candle::new(100_300.0, 101_000.0, 100_100.0, 100_800.0, 20_000.0).unwrap(),
    ];
    let market = MarketContext::from_candles(&candles, 100.0, unit(0.6), unit(0.3)).unwrap();
    let other = MarketContext::from_candles(&scaled, 10_000.0, unit(0.6), unit(0.3)).unwrap();
    assert_eq!(market.trend, Trend::Bullish);
    assert!((market.trend_strength.get() - other.trend_strength.get()).abs() < 0.00001);
    assert!((market.volatility.get() - other.volatility.get()).abs() < 0.00001);
    assert_eq!(market.volume, Unit::ONE);
    assert_eq!(other.volume, market.volume);
    assert_eq!(market.zoom, unit(0.6));
    assert_eq!(market.activity, unit(0.3));

    let extreme = [Candle::new(
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MAX,
    )
    .unwrap()];
    let result =
        MarketContext::from_candles(&extreme, f64::MIN_POSITIVE, Unit::ZERO, Unit::ZERO).unwrap();
    assert!(result.trend_strength.get().is_finite());
    assert!(result.volume.get().is_finite());
    assert!(result.volatility.get().is_finite());
    assert!(MarketContext::from_candles(&[], 1.0, Unit::ZERO, Unit::ZERO).is_err());
    assert!(MarketContext::from_candles(&candles, 0.0, Unit::ZERO, Unit::ZERO).is_err());
    assert!(Candle::new(100.0, 99.0, 98.0, 100.0, 1.0).is_err());
    assert!(Candle::new(100.0, 101.0, 99.0, 100.0, -1.0).is_err());
    assert!(Candle::new(f64::NAN, 101.0, 99.0, 100.0, 1.0).is_err());
}

fn scheduled_render(block_size: usize) -> (Vec<[f32; 2]>, shzx_sfx::Snapshot) {
    let mut renderer = Renderer::new(config(8_000)).unwrap();
    let market = MarketContext {
        trend: Trend::Bearish,
        trend_strength: Unit::ONE,
        volume: unit(0.7),
        volatility: unit(0.6),
        zoom: unit(0.4),
        activity: unit(0.5),
    };
    let timeline = [
        (0, Command::Market(market)),
        (1_024, Command::Trigger(SfxEvent::new(SfxKind::OrderFill))),
        (4_096, Command::Mode(Mode::Dialog)),
        (8_192, Command::Mode(Mode::Browse)),
        (10_240, Command::Spatial(SpatialMode::Pan)),
        (
            12_000,
            Command::Mix(Mix {
                melody: unit(0.7),
                ..Mix::default()
            }),
        ),
        (17_000, Command::Trigger(SfxEvent::new(SfxKind::Confirm))),
    ];
    // Include the first melodic entrance, incidental details, and chord change.
    let mut output = vec![[0.0; 2]; 84_000];
    let mut position = 0;
    for (frame, command) in timeline {
        for block in output[position..frame].chunks_mut(block_size) {
            renderer.render(block);
        }
        renderer.apply(command);
        position = frame;
    }
    for block in output[position..].chunks_mut(block_size) {
        renderer.render(block);
    }
    (output, renderer.snapshot())
}

#[test]
fn sample_timeline_and_seed_are_reproducible_across_irregular_blocks() {
    let (small, a) = scheduled_render(31);
    let (large, b) = scheduled_render(256);
    assert_eq!(small, large);
    assert_eq!(a.generated_events, b.generated_events);
    assert_eq!(a.chord, b.chord);
    assert_ne!(
        a.chord,
        Chord::new(Symbol::default().root(), ChordShape::Sus2)
    );
    assert_eq!(a.last_event, b.last_event);

    let mut different_seed = Renderer::new(RenderConfig {
        seed: 72,
        ..config(8_000)
    })
    .unwrap();
    different_seed.apply(Command::Market(a.market));
    let mut different = [[0.0; 2]; 128];
    different_seed.render(&mut different);
    assert_ne!(different.as_slice(), &small[..128]);
}

#[test]
fn bursts_and_live_parameter_changes_stay_finite_and_bounded() {
    let mut renderer = Renderer::new(config(8_000)).unwrap();
    renderer.apply(Command::Mix(Mix {
        master: Unit::ONE,
        drone: Unit::ONE,
        texture: Unit::ONE,
        harmony: Unit::ONE,
        melody: Unit::ONE,
        ui: Unit::ONE,
        reverb: Unit::ONE,
        output_gain: shzx_sfx::OutputGain::default(),
    }));
    let mut block = [[0.0_f32; 2]; 128];
    let mut peak = 0.0_f32;
    for cycle in 0..80 {
        if cycle % 10 == 0 {
            for kind in SfxKind::iter() {
                renderer.apply(Command::Trigger(SfxEvent {
                    layers: shzx_sfx::SfxLayerSelection::All,
                    edits: shzx_sfx::SfxEdits::DEFAULT,
                    kind,
                    intensity: Unit::ONE,
                    pan: Pan::new(if cycle % 20 == 0 { -1.0 } else { 1.0 }).unwrap(),
                }));
            }
            renderer.apply(Command::Mode(if cycle % 20 == 0 {
                Mode::Dialog
            } else {
                Mode::Browse
            }));
            renderer.apply(Command::Spatial(if cycle % 20 == 0 {
                SpatialMode::Pan
            } else {
                SpatialMode::Haas
            }));
            renderer.apply(Command::Market(MarketContext {
                trend: Trend::Bearish,
                trend_strength: Unit::ONE,
                volume: Unit::ONE,
                volatility: Unit::ONE,
                zoom: Unit::ONE,
                activity: Unit::ONE,
            }));
        }
        renderer.render(&mut block);
        for &sample in block.iter().flatten() {
            assert!(sample.is_finite());
            assert!(sample.abs() <= 0.95);
            peak = peak.max(sample.abs());
        }
    }
    let snapshot = renderer.snapshot();
    assert!(peak > 0.001);
    assert!(snapshot.stolen_voices > 0);
    assert!(snapshot.coalesced_events > 0);
    assert!(snapshot.active_voices <= 8);
    assert!(snapshot.rms.is_finite());
}

#[test]
fn muting_a_playing_effect_uses_a_smooth_gain_transition() {
    let mut renderer = Renderer::new(config(8_000)).unwrap();
    let mut reference = Renderer::new(config(8_000)).unwrap();
    let mut block = [[0.0; 2]; 1_024];
    for engine in [&mut renderer, &mut reference] {
        engine.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
        engine.render(&mut block);
    }
    renderer.apply(Command::Mix(Mix {
        master: Unit::ZERO,
        ..Mix::default()
    }));
    let first = renderer.next_frame();
    let uninterrupted = reference.next_frame();
    for (actual, expected) in first.into_iter().zip(uninterrupted) {
        assert!((actual - expected).abs() < reference.snapshot().peak * 0.02);
    }
    for _ in 0..8 {
        renderer.render(&mut block);
    }
    assert!(energy(&block) < 1.0e-12);
}

#[test]
fn deep_zoom_darkens_the_noise_texture_after_smoothing() {
    let mix = Mix {
        master: Unit::ONE,
        drone: Unit::ZERO,
        texture: Unit::ONE,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        ui: Unit::ZERO,
        reverb: Unit::ZERO,
        output_gain: shzx_sfx::OutputGain::default(),
    };
    let mut overview = Renderer::new(RenderConfig {
        mix,
        ..config(8_000)
    })
    .unwrap();
    let mut detail = Renderer::new(RenderConfig {
        mix,
        ..config(8_000)
    })
    .unwrap();
    detail.apply(Command::Market(MarketContext {
        zoom: Unit::ONE,
        ..MarketContext::default()
    }));
    let mut broad = vec![[0.0; 2]; 15_000];
    let mut deep = vec![[0.0; 2]; broad.len()];
    overview.render(&mut broad);
    detail.render(&mut deep);
    let broad = &broad[8_000..];
    let deep = &deep[8_000..];
    // First-difference energy relative to signal energy is a simple high-band
    // proxy. This window precedes the first random incidental detail.
    let difference_energy = |frames: &[[f32; 2]]| -> f64 {
        frames
            .windows(2)
            .map(|pair| {
                f64::from(pair[1][0] - pair[0][0]).powi(2)
                    + f64::from(pair[1][1] - pair[0][1]).powi(2)
            })
            .sum()
    };
    let broad_energy = energy(broad);
    let deep_energy = energy(deep);
    assert!(deep_energy < broad_energy * 0.95);
    assert!(difference_energy(deep) / deep_energy < difference_energy(broad) / broad_energy * 0.9);
    assert!(detail.snapshot().cutoff_hz < overview.snapshot().cutoff_hz * 0.4);
}

#[test]
fn dialog_focus_changes_the_environment_but_keeps_ui_effects_audible() {
    let mix = Mix {
        master: Unit::ONE,
        drone: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        ui: Unit::ONE,
        reverb: Unit::ZERO,
        output_gain: shzx_sfx::OutputGain::default(),
    };
    let mut renderer = Renderer::new(RenderConfig {
        mix,
        ..config(8_000)
    })
    .unwrap();
    let overview_cutoff = renderer.snapshot().cutoff_hz;
    renderer.apply(Command::Mode(Mode::Dialog));
    let mut silence = vec![[0.0; 2]; 24_000];
    renderer.render(&mut silence);
    let focused = renderer.snapshot();
    assert!(focused.focus_amount > 0.99);
    assert!(focused.cutoff_hz < overview_cutoff * 0.6);
    assert!(energy(&silence) < 1.0e-12);
    renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
    let mut effect = [[0.0; 2]; 4_000];
    renderer.render(&mut effect);
    assert!(energy(&effect) > 0.0001);
    assert_eq!(renderer.snapshot().mode, Mode::Dialog);
}

#[test]
fn supported_sample_rates_produce_finite_audio_with_correct_sample_clocks() {
    for rate in [8_000, 44_100, 48_000, 96_000, 192_000] {
        let mut renderer = Renderer::new(config(rate)).unwrap();
        renderer.apply(Command::Mode(Mode::Dialog));
        renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::Click)));
        renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
        let frames = (rate / 40) as usize;
        let mut output = vec![[0.0; 2]; frames];
        renderer.render(&mut output);
        assert!(
            output
                .iter()
                .flatten()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.95)
        );
        assert!(energy(&output) > 0.0000001);
        let snapshot = renderer.snapshot();
        assert_eq!(snapshot.sample_rate, rate);
        assert!((snapshot.time_seconds - frames as f64 / f64::from(rate)).abs() < 1.0e-10);
    }
}
