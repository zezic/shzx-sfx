use shzx_sfx::{
    Command, Mix, OutputGain, RenderConfig, Renderer, ReverbSettings, SfxEvent, SfxKind, Unit,
};

const SAMPLE_RATE: u32 = 8_000;

fn unit(value: f32) -> Unit {
    Unit::new(value).unwrap()
}

fn settings(room_size: f32, send: f32, wet: f32) -> ReverbSettings {
    let mut settings = ReverbSettings::default();
    settings.events.room_size = unit(room_size);
    settings.events.send = unit(send);
    settings.events.wet = unit(wet);
    settings
}

fn renderer(reverb: ReverbSettings) -> Renderer {
    let mut renderer = Renderer::new(RenderConfig {
        sample_rate: SAMPLE_RATE,
        seed: 123,
        max_voices: 4,
        reverb,
        mix: Mix {
            master: unit(0.1),
            output_gain: OutputGain::new(0.0).unwrap(),
            drone: Unit::ZERO,
            texture: Unit::ZERO,
            harmony: Unit::ZERO,
            melody: Unit::ZERO,
            ui: Unit::ONE,
            reverb: Unit::ONE,
        },
        ..RenderConfig::default()
    })
    .unwrap();
    render(&mut renderer, 0.25);
    renderer
}

fn render(renderer: &mut Renderer, seconds: f32) -> Vec<[f32; 2]> {
    let mut frames = vec![[0.0; 2]; (seconds * SAMPLE_RATE as f32).ceil() as usize];
    renderer.render(&mut frames);
    frames
}

fn energy(frames: &[[f32; 2]]) -> f64 {
    frames
        .iter()
        .flatten()
        .map(|&sample| f64::from(sample).powi(2))
        .sum()
}

/// End after all source and Haas/filter samples, before observing reverb alone.
fn excite(renderer: &mut Renderer) {
    renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
    let duration = renderer.snapshot().last_event.unwrap().duration_seconds;
    render(renderer, duration + 0.1);
    assert_eq!(renderer.snapshot().active_voices, 0);
}

#[test]
fn larger_rooms_keep_audible_event_energy_later_in_the_tail() {
    let mut short = renderer(settings(0.2, 1.0, 1.0));
    let mut long = renderer(settings(0.95, 1.0, 1.0));
    excite(&mut short);
    excite(&mut long);
    render(&mut short, 1.0);
    render(&mut long, 1.0);
    let short_energy = energy(&render(&mut short, 1.0));
    let long_energy = energy(&render(&mut long, 1.0));
    eprintln!(
        "event reverb energy 1-2s after source end: short {short_energy:.8e}, long {long_energy:.8e}, ratio {:.2e}",
        long_energy / short_energy.max(f64::MIN_POSITIVE)
    );
    assert!(
        long_energy > 0.000001,
        "the long tail must remain measurable"
    );
    assert!(long_energy > short_energy * 50.0);
}

#[test]
fn independent_sends_preserve_dry_events_and_keep_the_other_bus_audible() {
    let mut dry_by_send = renderer(settings(0.95, 0.0, 1.0));
    let mut dry_by_return = renderer(settings(0.95, 1.0, 0.0));
    let mut event_settings = settings(0.95, 1.0, 1.0);
    event_settings.environment.send = Unit::ZERO;
    let mut events_without_environment_send = renderer(event_settings);
    let mut events_with_environment_send = renderer(settings(0.95, 1.0, 1.0));
    for renderer in [
        &mut dry_by_send,
        &mut dry_by_return,
        &mut events_without_environment_send,
        &mut events_with_environment_send,
    ] {
        renderer.apply(Command::Trigger(SfxEvent::new(SfxKind::OrderFill)));
    }
    let dry = render(&mut dry_by_send, 1.5);
    assert_eq!(dry, render(&mut dry_by_return, 1.5));
    assert!(energy(&dry) > 0.0001, "zero send must keep the dry event");
    let wet = render(&mut events_without_environment_send, 1.5);
    assert_eq!(wet, render(&mut events_with_environment_send, 1.5));
    assert!(energy(&wet) > energy(&dry));
    let dry_tail = energy(&render(&mut dry_by_send, 1.0));
    let event_tail = energy(&render(&mut events_without_environment_send, 1.0));
    eprintln!("zero event send tail {dry_tail:.8e}; independent event bus tail {event_tail:.8e}");
    assert!(dry_tail < 0.000000000001);
    assert!(
        event_tail > 0.000001,
        "environment send must not mute event reverb"
    );
}

#[test]
fn automating_send_to_zero_keeps_the_existing_tail_intact() {
    let initial = settings(0.95, 1.0, 1.0);
    let mut reference = renderer(initial);
    let mut stopped_send = renderer(initial);
    excite(&mut reference);
    excite(&mut stopped_send);
    let mut next = initial;
    next.events.send = Unit::ZERO;
    stopped_send.apply(Command::Reverb(next));
    let reference_tail = render(&mut reference, 2.0);
    let stopped_send_tail = render(&mut stopped_send, 2.0);
    assert_eq!(reference_tail, stopped_send_tail);
    assert!(energy(&stopped_send_tail) > 0.000001);
    assert_eq!(stopped_send.snapshot().reverb.events.send, Unit::ZERO);
}

#[test]
fn wet_return_can_mute_and_restore_a_tail_without_resetting_the_room() {
    let initial = settings(0.95, 1.0, 1.0);
    let mut reference = renderer(initial);
    let mut automated = renderer(initial);
    excite(&mut reference);
    excite(&mut automated);
    let mut silent_return = initial;
    silent_return.events.wet = Unit::ZERO;
    automated.apply(Command::Reverb(silent_return));
    render(&mut reference, 0.8);
    render(&mut automated, 0.8);
    let reference_muted_window = energy(&render(&mut reference, 0.3));
    let muted_window = energy(&render(&mut automated, 0.3));
    let muted_ratio = muted_window / reference_muted_window;
    assert!(
        muted_ratio < 0.001,
        "wet return did not settle to silence: {muted_ratio}"
    );
    automated.apply(Command::Reverb(initial));
    render(&mut reference, 0.8);
    render(&mut automated, 0.8);
    let reference_restored_window = energy(&render(&mut reference, 0.3));
    let restored_window = energy(&render(&mut automated, 0.3));
    let restored_ratio = restored_window / reference_restored_window;
    eprintln!(
        "event wet automation energy ratios: muted {muted_ratio:.8}, restored {restored_ratio:.8}"
    );
    assert!(restored_window > 0.00000001);
    assert!(
        (0.98..=1.02).contains(&restored_ratio),
        "the original room tail was not restored"
    );
}
