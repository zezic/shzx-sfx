use shzx_sfx::{
    BackgroundSnapshot, BackgroundVoice, Chord, ChordShape, ChordTransition, ClockState, Command,
    MarketContext, Mode, RenderConfig, Renderer, Symbol, Trend, Unit,
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

fn advance(renderer: &mut Renderer, mut frames: usize) {
    let mut block = [[0.0; 2]; 64];
    while frames > 0 {
        let count = frames.min(block.len());
        renderer.render(&mut block[..count]);
        frames -= count;
    }
}

fn scheduled_seconds(clock: ClockState) -> f32 {
    match clock {
        ClockState::Scheduled { in_seconds } => in_seconds,
        ClockState::Held { .. } => panic!("expected an active musical clock"),
    }
}

#[test]
fn requested_targets_update_before_the_smoothed_background_follows() {
    let mut renderer = renderer();
    let initial = renderer.snapshot();
    renderer.apply(Command::Market(MarketContext {
        zoom: Unit::ONE,
        ..initial.market
    }));
    renderer.apply(Command::Mode(Mode::Dialog));
    let requested = renderer.snapshot();
    assert_eq!(requested.market.zoom, Unit::ONE);
    assert_eq!(requested.background.market.zoom, Unit::ZERO);
    assert_eq!(requested.background.focus.current, 0.0);
    assert_eq!(requested.background.focus.target, 1.0);
    assert_eq!(requested.background.lowpass_hz.current, initial.cutoff_hz);
    assert!((requested.background.lowpass_hz.target - 650.0).abs() < 0.001);
    assert!((requested.background.highpass_hz.target - 320.0).abs() < 0.001);
    assert!((requested.background.reverb_room.target - 1.0).abs() < 0.00001);

    advance(&mut renderer, 512);
    let following = renderer.snapshot().background;
    assert!(following.market.zoom.get() > 0.0 && following.market.zoom.get() < 1.0);
    assert!(following.focus.current > 0.0 && following.focus.current < following.focus.target);
    assert!(following.highpass_hz.current > requested.background.highpass_hz.current);
    assert!(following.reverb_room.current > requested.background.reverb_room.current);
    advance(&mut renderer, RATE as usize * 3);
    let settled = renderer.snapshot().background;
    assert!(settled.focus.current > 0.99);
    assert!(settled.market.zoom.get() > 0.999);
    assert!((settled.lowpass_hz.current - settled.lowpass_hz.target).abs() < 2.0);
    assert!((settled.highpass_hz.current - settled.highpass_hz.target).abs() < 2.0);
    assert!((settled.reverb_room.current - settled.reverb_room.target).abs() < 0.001);

    // Public telemetry stays a copyable value and remains available to JSONL
    // consumers independently of the interactive terminal UI.
    let _: BackgroundSnapshot = settled;
    let serialized = serde_json::to_value(renderer.snapshot()).unwrap();
    assert_eq!(serialized["background"]["focus"]["target"], 1.0);
}

#[test]
fn pending_harmony_uses_live_context_and_crossfade_progress_tracks_rendering() {
    let mut renderer = renderer();
    assert_eq!(
        renderer.snapshot().background.harmony.next,
        Chord::new(Symbol::default().root(), ChordShape::MinorNinth)
    );
    renderer.apply(Command::Market(MarketContext {
        trend: Trend::Bullish,
        trend_strength: Unit::ONE,
        volume: Unit::ONE,
        activity: Unit::ONE,
        ..MarketContext::default()
    }));
    advance(&mut renderer, 512);
    let pending = renderer.snapshot();
    assert_eq!(
        pending.chord,
        Chord::new(Symbol::default().root(), ChordShape::MinorNinth)
    );
    assert_eq!(
        pending.background.harmony.next,
        Chord::new(Symbol::default().root(), ChordShape::MinorEleventh)
    );
    assert!(scheduled_seconds(pending.background.harmony.clock) > 9.0);

    advance(&mut renderer, RATE as usize * 10 + 64 - 512);
    let first = renderer.snapshot().background.harmony.transition;
    let (first_progress, first_remaining) = match first {
        ChordTransition::Crossfading {
            from,
            to,
            progress,
            remaining_seconds,
        } => {
            assert_eq!(
                from,
                Chord::new(Symbol::default().root(), ChordShape::MinorNinth)
            );
            assert_eq!(
                to,
                Chord::new(Symbol::default().root(), ChordShape::MinorEleventh)
            );
            (progress.get(), remaining_seconds)
        }
        ChordTransition::Steady { .. } => panic!("the musical decision should start a crossfade"),
    };
    advance(&mut renderer, RATE as usize);
    match renderer.snapshot().background.harmony.transition {
        ChordTransition::Crossfading {
            progress,
            remaining_seconds,
            ..
        } => {
            assert!(progress.get() > first_progress);
            assert!(remaining_seconds < first_remaining);
            assert!((remaining_seconds - 2.0).abs() < 0.02);
        }
        ChordTransition::Steady { .. } => panic!("a three-second chord crossfade is still active"),
    }
    advance(&mut renderer, RATE as usize * 2 + 128);
    assert_eq!(
        renderer.snapshot().background.harmony.transition,
        ChordTransition::Steady {
            chord: Chord::new(Symbol::default().root(), ChordShape::MinorEleventh)
        }
    );
}

#[test]
fn focus_explains_held_harmony_and_melody_clocks_without_losing_their_schedule() {
    let mut renderer = renderer();
    renderer.apply(Command::Mode(Mode::Dialog));
    advance(&mut renderer, RATE as usize * 11);
    let held = renderer.snapshot();
    assert_eq!(
        held.chord,
        Chord::new(Symbol::default().root(), ChordShape::MinorNinth)
    );
    assert!(matches!(held.background.harmony.clock,
        ClockState::Held { recheck_in_seconds } if recheck_in_seconds > 0.0));
    assert!(matches!(held.background.melody.clock,
        ClockState::Held { recheck_in_seconds } if recheck_in_seconds > 0.0));
    assert_eq!(held.background.melody.generated, 0);
    assert_eq!(held.background.melody.voice, BackgroundVoice::Silent);
    assert!(held.background.details.generated > 0);

    renderer.apply(Command::Mode(Mode::Browse));
    assert_eq!(renderer.snapshot().background.focus.target, 0.0);
    advance(&mut renderer, RATE as usize);
    let resumed = renderer.snapshot();
    assert!(scheduled_seconds(resumed.background.harmony.clock) > 0.0);
    assert!(scheduled_seconds(resumed.background.melody.clock) > 0.0);
    assert_eq!(
        resumed.chord,
        Chord::new(Symbol::default().root(), ChordShape::MinorNinth)
    );
}

#[test]
fn autonomous_details_and_melody_report_source_activity_and_upcoming_events() {
    let mut renderer = renderer();
    let initial = renderer.snapshot().background;
    assert_eq!(initial.details.generated, 0);
    assert_eq!(initial.details.voice, BackgroundVoice::Silent);
    assert_eq!(initial.details.next_in_seconds, 2.0);
    assert_eq!(initial.melody.generated, 0);
    assert_eq!(scheduled_seconds(initial.melody.clock), 7.0);

    advance(&mut renderer, RATE as usize * 2 + 64);
    let detail = renderer.snapshot().background.details;
    assert_eq!(detail.generated, 1);
    assert!(detail.next_in_seconds > 0.0);
    assert!(
        matches!(detail.voice, BackgroundVoice::Playing { frequency_hz, remaining_seconds }
        if frequency_hz > 0.0 && remaining_seconds > 0.0 && remaining_seconds < 0.1)
    );
    advance(&mut renderer, 1_024);
    let after = renderer.snapshot().background.details;
    assert_eq!(after.generated, detail.generated);
    assert_eq!(after.voice, BackgroundVoice::Silent);
    assert!(after.next_in_seconds < detail.next_in_seconds);

    advance(&mut renderer, RATE as usize * 5 - 1_024);
    let melodic = renderer.snapshot().background.melody;
    assert_eq!(melodic.generated, 1);
    assert!(scheduled_seconds(melodic.clock) > 10.0);
    assert!(
        matches!(melodic.voice, BackgroundVoice::Playing { frequency_hz, remaining_seconds }
        if frequency_hz > 0.0 && (10.9..11.0).contains(&remaining_seconds))
    );
    advance(&mut renderer, (RATE as f32 * 2.7) as usize);
    let tail = renderer.snapshot().background.melody;
    assert_eq!(tail.generated, 1);
    assert!(
        matches!(tail.voice, BackgroundVoice::Playing { remaining_seconds, .. }
        if (8.2..8.3).contains(&remaining_seconds))
    );
    advance(
        &mut renderer,
        RATE as usize * 11 - 64 - (RATE as f32 * 2.7) as usize,
    );
    let retired = renderer.snapshot().background.melody;
    assert_eq!(retired.generated, 1);
    assert_eq!(retired.voice, BackgroundVoice::Silent);
    assert!(scheduled_seconds(retired.clock) > 0.0);
}

#[test]
fn melody_monitor_reports_its_current_vibrato_pitch() {
    let mut renderer = renderer();
    advance(&mut renderer, RATE as usize * 7 + 1);
    let frequency = |renderer: &Renderer| match renderer.snapshot().background.melody.voice {
        BackgroundVoice::Playing { frequency_hz, .. } => frequency_hz,
        BackgroundVoice::Silent => panic!("the melody should still be playing"),
    };
    let onset = frequency(&renderer);
    let quarter_cycle = (RATE as f32 / (4.3 * 4.0)).round() as usize;
    advance(&mut renderer, quarter_cycle - 1);
    let upper = frequency(&renderer);
    advance(&mut renderer, quarter_cycle * 2);
    let lower = frequency(&renderer);
    assert!((1.024..1.026).contains(&(upper / onset)));
    assert!((0.974..0.976).contains(&(lower / onset)));
    assert_eq!(renderer.snapshot().background.melody.generated, 1);
}
