use shzx_sfx::{
    Chord, ChordShape, ChordTransition, Command, MarketContext, Mix, Mode, Pan, RenderConfig,
    Renderer, RootTransition, ScaleDegree, SfxEvent, SfxKind, Symbol, Trend, Unit,
};

const RATE: u32 = 8_000;

fn renderer() -> Renderer {
    Renderer::new(RenderConfig {
        sample_rate: RATE,
        seed: 123,
        ..RenderConfig::default()
    })
    .unwrap()
}

fn advance(renderer: &mut Renderer, mut samples: usize) {
    let mut block = [[0.0; 2]; 128];
    while samples > 0 {
        let length = samples.min(block.len());
        renderer.render(&mut block[..length]);
        samples -= length;
    }
}

#[test]
fn symbols_are_inline_normalized_validated_and_map_stably_to_scale_roots() {
    assert_eq!(Symbol::new("aapl").unwrap(), Symbol::new("AAPL").unwrap());
    assert_eq!(Symbol::new("brk.b").unwrap().as_str(), "BRK.B");
    assert_eq!(Symbol::new("^gspc").unwrap().as_str(), "^GSPC");
    assert_eq!(Symbol::new("AAPL").unwrap().root(), ScaleDegree::III);
    assert_eq!(Symbol::new("TSLA").unwrap().root(), ScaleDegree::IV);
    assert_eq!(Symbol::new("IBM").unwrap().root(), ScaleDegree::I);
    for invalid in [
        "",
        " ",
        "A B",
        "A/B",
        "A@B",
        "...",
        "é",
        "ABCDEFGHIJKLMNOPQRSTUVWXY",
    ] {
        assert!(Symbol::new(invalid).is_err(), "{invalid:?}");
    }
    assert!(Symbol::new("ABCDEFGHIJKLMNOPQRSTUVWX").is_ok());
    let symbol = Symbol::new("msft").unwrap();
    let copied = symbol;
    assert_eq!(symbol, copied);
    assert_eq!(serde_json::to_string(&symbol).unwrap(), "\"MSFT\"");
    assert_eq!(serde_json::from_str::<Symbol>("\"mSfT\"").unwrap(), symbol);
    assert!(serde_json::from_str::<Symbol>("\"A B\"").is_err());
}

#[test]
fn every_root_and_voicing_stays_in_the_shared_collection_below_420_hz() {
    let allowed_pitch_classes = [0, 2, 4, 5, 7, 9, 11];
    for root in ScaleDegree::ALL {
        assert!(allowed_pitch_classes.contains(&(root.pedal_note() % 12)));
        for shape in ChordShape::ALL {
            let chord = Chord::new(root, shape);
            let notes = chord.notes();
            assert!(notes.windows(2).all(|pair| pair[0] < pair[1]), "{chord}");
            assert!(root.pedal_note() < notes[0], "{chord}");
            assert!(
                notes
                    .iter()
                    .all(|note| allowed_pitch_classes.contains(&(note % 12))),
                "{chord}"
            );
            assert!(
                notes.iter().all(|note| {
                    440.0 * 2.0_f32.powf((f32::from(*note) - 69.0) / 12.0) <= 420.0
                }),
                "{chord}: {notes:?}"
            );
        }
    }
    let tension = Chord::new(ScaleDegree::I, ChordShape::Tension).notes();
    assert!(tension.iter().any(|note| note % 12 == 5));
    assert!(tension.iter().any(|note| note % 12 == 11));
    assert_eq!(
        Chord::new(ScaleDegree::II, ChordShape::Seventh).to_string(),
        "Bm7b5"
    );
    assert_eq!(
        Chord::new(ScaleDegree::III, ChordShape::Seventh).to_string(),
        "Cmaj7"
    );
}

#[test]
fn symbol_changes_crossfade_roots_while_market_updates_keep_the_selected_base() {
    let mut renderer = renderer();
    advance(&mut renderer, 128);
    let from = renderer.snapshot().background.harmony.root;
    let symbol = Symbol::new("IBM").unwrap();
    renderer.apply(Command::Symbol(symbol));
    let requested = renderer.snapshot();
    assert_eq!(requested.symbol, symbol);
    assert_eq!(
        requested.background.harmony.root,
        symbol.root().pitch_class()
    );
    assert!(matches!(requested.background.harmony.root_transition,
        RootTransition::Crossfading { from: source, to, progress, remaining_seconds }
        if source == from && to == symbol.root().pitch_class() && progress == Unit::ZERO && remaining_seconds == 3.0));
    advance(&mut renderer, 800);
    let ongoing = renderer.snapshot().background.harmony.root_transition;
    renderer.apply(Command::Symbol(symbol));
    assert_eq!(
        renderer.snapshot().background.harmony.root_transition,
        ongoing
    );
    for (index, trend) in [Trend::Bearish, Trend::Bullish, Trend::Sideways]
        .into_iter()
        .enumerate()
    {
        renderer.apply(Command::Market(MarketContext {
            trend,
            trend_strength: Unit::ONE,
            volume: Unit::ONE,
            volatility: Unit::ONE,
            activity: Unit::ONE,
            ..MarketContext::default()
        }));
        advance(
            &mut renderer,
            if index == 0 { RATE as usize * 12 } else { 128 },
        );
        assert_eq!(
            renderer.snapshot().background.harmony.root,
            symbol.root().pitch_class()
        );
        assert_eq!(
            renderer.snapshot().chord.root(),
            symbol.root().pitch_class()
        );
    }
}

#[test]
fn rapid_symbol_changes_finish_the_current_fade_before_the_latest_pending_root() {
    let mut renderer = renderer();
    let first = Symbol::new("IBM").unwrap();
    let latest = Symbol::new("TSLA").unwrap();
    renderer.apply(Command::Symbol(first));
    advance(&mut renderer, RATE as usize);
    let first_fade = renderer.snapshot().background.harmony.root_transition;
    renderer.apply(Command::Symbol(latest));
    assert_eq!(
        renderer.snapshot().background.harmony.root,
        latest.root().pitch_class()
    );
    assert_eq!(
        renderer.snapshot().background.harmony.root_transition,
        first_fade
    );
    advance(&mut renderer, RATE as usize * 2 + 128);
    assert!(
        matches!(renderer.snapshot().background.harmony.root_transition,
        RootTransition::Crossfading { from, to, .. } if from == first.root().pitch_class() && to == latest.root().pitch_class())
    );
    advance(&mut renderer, RATE as usize * 3 + 128);
    assert_eq!(
        renderer.snapshot().background.harmony.root_transition,
        RootTransition::Steady {
            root: latest.root().pitch_class()
        }
    );
    assert_eq!(
        renderer.snapshot().background.harmony.transition,
        ChordTransition::Steady {
            chord: renderer.snapshot().chord
        }
    );
}

#[test]
fn focus_width_moves_to_a_narrow_return_then_restores_the_requested_base() {
    let mut renderer = renderer();
    let base = renderer.snapshot().reverb;
    renderer.apply(Command::Mode(Mode::Dialog));
    let requested = renderer.snapshot().background.reverb_width;
    assert_eq!(requested.current, base.environment.width.get());
    assert!((requested.target - base.environment.width.get() * 0.2).abs() < 0.00001);
    advance(&mut renderer, RATE as usize * 4);
    let narrowed = renderer.snapshot();
    assert!((narrowed.background.reverb_width.current - requested.target).abs() < 0.001);
    assert_eq!(narrowed.reverb, base);
    renderer.apply(Command::Mode(Mode::Focus));
    assert!(
        (renderer.snapshot().background.reverb_width.target - base.environment.width.get() * 0.48)
            .abs()
            < 0.00001
    );
    renderer.apply(Command::Mode(Mode::Browse));
    advance(&mut renderer, RATE as usize * 4);
    assert!(
        (renderer.snapshot().background.reverb_width.current - base.environment.width.get()).abs()
            < 0.001
    );
    assert_eq!(renderer.snapshot().reverb, base);
}

#[test]
fn mode_width_does_not_change_the_dry_event_pan() {
    let mix = Mix {
        master: Unit::new(0.1).unwrap(),
        drone: Unit::ZERO,
        demonic_voices: Unit::ZERO,
        demonic_stutters: Unit::ZERO,
        flying_coins: Unit::ZERO,
        laser_beams: Unit::ZERO,
        sub: Unit::ZERO,
        texture: Unit::ZERO,
        harmony: Unit::ZERO,
        melody: Unit::ZERO,
        reverb: Unit::ZERO,
        ..Mix::default()
    };
    let config = RenderConfig {
        sample_rate: RATE,
        mix,
        seed: 123,
        ..RenderConfig::default()
    };
    let mut browse = Renderer::new(config).unwrap();
    let mut dialog = Renderer::new(config).unwrap();
    dialog.apply(Command::Mode(Mode::Dialog));
    advance(&mut browse, RATE as usize * 4);
    advance(&mut dialog, RATE as usize * 4);
    let event = SfxEvent {
        pan: Pan::new(-0.8).unwrap(),
        ..SfxEvent::new(SfxKind::Click)
    };
    browse.apply(Command::Trigger(event));
    dialog.apply(Command::Trigger(event));
    for _ in 0..RATE {
        assert_eq!(browse.next_frame(), dialog.next_frame());
    }
}
