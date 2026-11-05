use shzx_sfx::{
    Chord, ChordShape, ChordTransition, ClockState, Command, HarmonyMode, MarketContext, Mode,
    RenderConfig, Renderer, RootTransition, Symbol, Trend, Unit,
};

const RATE: u32 = 8_000;

fn config(symbol: Symbol, mode: HarmonyMode) -> RenderConfig {
    RenderConfig {
        sample_rate: RATE,
        symbol,
        harmony_mode: mode,
        seed: 71,
        ..RenderConfig::default()
    }
}

fn advance(renderer: &mut Renderer, mut frames: usize) {
    let mut block = [[0.0; 2]; 256];
    while frames > 0 {
        let count = frames.min(block.len());
        renderer.render(&mut block[..count]);
        assert!(
            block[..count]
                .iter()
                .flatten()
                .all(|sample| sample.is_finite() && sample.abs() <= 0.95)
        );
        frames -= count;
    }
}

#[test]
fn dissonant_harmony_is_the_default_and_diatonic_voicings_remain_selectable() {
    let defaults = RenderConfig::default();
    assert_eq!(defaults.harmony_mode, HarmonyMode::Dissonant);
    for (ticker, notes) in [
        ("AAPL", [50, 53, 60, 64]),
        ("TSLA", [50, 53, 60, 64]),
        ("IBM", [45, 48, 55, 59]),
    ] {
        let symbol = Symbol::new(ticker).unwrap();
        let implicit = RenderConfig {
            sample_rate: RATE,
            symbol,
            ..defaults
        };
        let mut default = Renderer::new(implicit).unwrap();
        let mut explicit = Renderer::new(RenderConfig {
            harmony_mode: HarmonyMode::Dissonant,
            ..implicit
        })
        .unwrap();
        let snapshot = default.snapshot();
        assert_eq!(
            snapshot.chord,
            Chord::from_symbol(symbol, HarmonyMode::Dissonant)
        );
        assert_eq!(snapshot.background.harmony.mode, HarmonyMode::Dissonant);
        let diatonic = Renderer::new(RenderConfig {
            harmony_mode: HarmonyMode::Diatonic,
            ..implicit
        })
        .unwrap()
        .snapshot();
        assert_eq!(
            diatonic.chord,
            Chord::new(symbol.root(), ChordShape::MinorNinth)
        );
        assert_eq!(diatonic.chord.notes(), notes);
        assert_eq!(diatonic.background.harmony.mode, HarmonyMode::Diatonic);
        for _ in 0..RATE {
            assert_eq!(default.next_frame(), explicit.next_frame());
        }
    }
}

#[test]
fn dissonant_harmony_is_fixed_by_the_ticker_across_seeds_markets_and_focus() {
    let symbol = Symbol::new("BRK.B").unwrap();
    let expected = Chord::from_symbol(symbol, HarmonyMode::Dissonant);
    for seed in [0, u64::MAX] {
        for trend in [Trend::Sideways, Trend::Bullish, Trend::Bearish] {
            let mut renderer = Renderer::new(RenderConfig {
                seed,
                ..config(symbol, HarmonyMode::Dissonant)
            })
            .unwrap();
            renderer.apply(Command::Market(MarketContext {
                trend,
                trend_strength: Unit::ONE,
                volume: Unit::ONE,
                volatility: Unit::ONE,
                zoom: Unit::ONE,
                activity: Unit::ONE,
            }));
            for mode in [Mode::Browse, Mode::Dialog, Mode::Focus] {
                renderer.apply(Command::Mode(mode));
                advance(&mut renderer, RATE as usize * 6);
                let snapshot = renderer.snapshot();
                assert_eq!(snapshot.chord, expected);
                assert_eq!(snapshot.background.harmony.next, expected);
                assert_eq!(snapshot.background.harmony.root, expected.root());
                assert_eq!(snapshot.background.harmony.mode, HarmonyMode::Dissonant);
                assert_eq!(snapshot.background.harmony.clock, ClockState::Fixed);
            }
        }
    }
}

#[test]
fn whole_tickers_and_mode_changes_replace_the_pending_chord_even_when_roots_match() {
    let first = Symbol::new("AAPL").unwrap();
    let initial = Chord::from_symbol(first, HarmonyMode::Dissonant);
    let peer = (0..4096)
        .map(|index| Symbol::new(&format!("T{index}")).unwrap())
        .find(|symbol| {
            let chord = Chord::from_symbol(*symbol, HarmonyMode::Dissonant);
            symbol.root() == first.root()
                && chord.root() == initial.root()
                && chord.notes() != initial.notes()
        })
        .expect("a ticker peer shares both root mappings while changing its upper notes");
    let changed = Chord::from_symbol(peer, HarmonyMode::Dissonant);
    let latest = (0..128)
        .map(|index| Symbol::new(&format!("^L-{index}")).unwrap())
        .find(|symbol| Chord::from_symbol(*symbol, HarmonyMode::Dissonant).root() != initial.root())
        .unwrap();
    let final_chord = Chord::from_symbol(latest, HarmonyMode::Dissonant);
    let mut renderer = Renderer::new(config(first, HarmonyMode::Dissonant)).unwrap();
    renderer.apply(Command::Symbol(peer));
    let snapshot = renderer.snapshot();
    assert_eq!(snapshot.chord, changed);
    assert!(matches!(snapshot.background.harmony.transition,
        ChordTransition::Crossfading { from, to, .. } if from == initial && to == changed));
    advance(&mut renderer, RATE as usize / 20);
    let running = renderer.snapshot().background.harmony.transition;
    renderer.apply(Command::Symbol(latest));
    assert_eq!(renderer.snapshot().chord, final_chord);
    assert_eq!(renderer.snapshot().background.harmony.transition, running);

    renderer.apply(Command::Harmony(HarmonyMode::Diatonic));
    assert_eq!(
        renderer.snapshot().background.harmony.mode,
        HarmonyMode::Diatonic
    );
    assert_eq!(
        renderer.snapshot().chord,
        Chord::from_symbol(latest, HarmonyMode::Diatonic)
    );
    renderer.apply(Command::Harmony(HarmonyMode::Dissonant));
    assert_eq!(renderer.snapshot().chord, final_chord);
    assert_eq!(renderer.snapshot().background.harmony.transition, running);
    advance(&mut renderer, RATE as usize * 8);
    let settled = renderer.snapshot().background.harmony;
    assert_eq!(settled.mode, HarmonyMode::Dissonant);
    assert_eq!(settled.root, final_chord.root());
    assert_eq!(
        settled.root_transition,
        RootTransition::Steady {
            root: final_chord.root()
        }
    );
    assert_eq!(
        settled.transition,
        ChordTransition::Steady { chord: final_chord }
    );
    assert_eq!(settled.clock, ClockState::Fixed);
}

#[test]
fn harmony_commands_render_reproducibly_in_irregular_blocks_at_extreme_rates() {
    for rate in [8_000, 48_000, 192_000] {
        let settings = RenderConfig {
            sample_rate: rate,
            seed: 71,
            ..RenderConfig::default()
        };
        let mut scalar = Renderer::new(settings).unwrap();
        let mut blocked = Renderer::new(settings).unwrap();
        let timeline = [
            (0, Command::Harmony(HarmonyMode::Dissonant)),
            (129, Command::Symbol(Symbol::new("BRK.B").unwrap())),
            (rate / 2 + 19, Command::Harmony(HarmonyMode::Diatonic)),
            (rate + 7, Command::Symbol(Symbol::new("^GSPC").unwrap())),
            (rate * 2 + 13, Command::Harmony(HarmonyMode::Dissonant)),
            (rate * 3 + 1, Command::Mode(Mode::Dialog)),
            (rate * 4 + 31, Command::Symbol(Symbol::new("IBM").unwrap())),
            (rate * 5 + 17, Command::Harmony(HarmonyMode::Diatonic)),
            (rate * 6 + 15, Command::Harmony(HarmonyMode::Dissonant)),
        ];
        let mut block = [[0.0; 2]; 1024];
        let mut position = 0;
        let mut block_index = 0;
        let mut energy = 0.0_f64;
        for (until, command) in timeline
            .into_iter()
            .map(|(frame, command)| (frame, Some(command)))
            .chain([(rate * 8, None)])
        {
            while position < until {
                let count = [1, 7, 137, 1024, 63][block_index % 5].min((until - position) as usize);
                blocked.render(&mut block[..count]);
                for &frame in &block[..count] {
                    assert_eq!(scalar.next_frame(), frame);
                    assert!(
                        frame
                            .iter()
                            .all(|value| value.is_finite() && value.abs() <= 0.95)
                    );
                    energy += frame
                        .iter()
                        .map(|value| f64::from(*value).powi(2))
                        .sum::<f64>();
                }
                position += count as u32;
                block_index += 1;
            }
            if let Some(command) = command {
                scalar.apply(command);
                blocked.apply(command);
            }
        }
        assert!(energy > 0.01);
        let a = scalar.snapshot();
        let b = blocked.snapshot();
        assert_eq!(a.chord, b.chord);
        assert_eq!(
            a.background.harmony.transition,
            b.background.harmony.transition
        );
        assert_eq!(a.background.harmony.mode, HarmonyMode::Dissonant);
        assert_eq!(a.background.harmony.clock, ClockState::Fixed);
        assert_eq!(
            a.chord,
            Chord::from_symbol(Symbol::new("IBM").unwrap(), HarmonyMode::Dissonant)
        );
    }
}
