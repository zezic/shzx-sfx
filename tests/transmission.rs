use shzx_sfx::{
    Command, DataTransmission, DataTransmissionCommand, Mix, OutputGain, Pan, RenderConfig,
    Renderer, ReverbSettings, SpatialMode, TransmissionCount, TransmissionPhase, TransmissionRate,
    Unit,
};

fn config(rate: u32, wet: f32) -> RenderConfig {
    let mut reverb = ReverbSettings::default();
    reverb.transmission.wet = Unit::new(wet).unwrap();
    RenderConfig {
        sample_rate: rate,
        seed: 90210,
        mix: Mix {
            master: Unit::ONE,
            output_gain: OutputGain::new(0.0).unwrap(),
            drone: Unit::ZERO,
            demonic_voices: Unit::ZERO,
            demonic_stutters: Unit::ZERO,
            flying_coins: Unit::ZERO,
            laser_beams: Unit::ZERO,
            sub: Unit::ZERO,
            texture: Unit::ZERO,
            harmony: Unit::ZERO,
            melody: Unit::ZERO,
            ui: Unit::ZERO,
            transmission: Unit::ONE,
            reverb: Unit::ONE,
        },
        reverb,
        spatial: SpatialMode::Pan,
        ..RenderConfig::default()
    }
}

fn start(renderer: &mut Renderer, settings: DataTransmission) {
    renderer.apply(Command::DataTransmission(DataTransmissionCommand::Start(
        settings,
    )));
}

fn advance(renderer: &mut Renderer, frames: usize) -> f64 {
    (0..frames)
        .flat_map(|_| renderer.next_frame())
        .map(|sample| f64::from(sample).powi(2))
        .sum()
}

#[test]
fn caller_delimits_a_capped_sequence_and_a_single_closing_signature() {
    let mut renderer = Renderer::new(config(48_000, 0.0)).unwrap();
    advance(&mut renderer, 48_000);
    start(
        &mut renderer,
        DataTransmission {
            max_triggers: TransmissionCount::new(3).unwrap(),
            rate: TransmissionRate::new(30.0).unwrap(),
            ..DataTransmission::default()
        },
    );
    assert_eq!(
        renderer.snapshot().transmission.phase,
        TransmissionPhase::Opening
    );
    assert!(advance(&mut renderer, 48_000) > 0.0);
    let capped = renderer.snapshot().transmission;
    assert_eq!(capped.phase, TransmissionPhase::AwaitingEnd);
    assert_eq!(capped.repeated, 3);
    assert_eq!(capped.generated_events, 4);
    advance(&mut renderer, 48_000);
    assert_eq!(renderer.snapshot().transmission.generated_events, 4);
    renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
    assert_eq!(
        renderer.snapshot().transmission.phase,
        TransmissionPhase::Closing
    );
    assert!(advance(&mut renderer, 48_000) > 0.0);
    assert_eq!(
        renderer.snapshot().transmission.phase,
        TransmissionPhase::Idle
    );
    assert_eq!(renderer.snapshot().transmission.generated_events, 5);
    renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
    assert_eq!(renderer.snapshot().transmission.generated_events, 5);
}

#[test]
fn early_end_finishes_opening_then_closes_without_pulses() {
    let mut renderer = Renderer::new(config(48_000, 0.0)).unwrap();
    start(&mut renderer, DataTransmission::default());
    renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
    advance(&mut renderer, 48_000);
    let snapshot = renderer.snapshot().transmission;
    assert_eq!(snapshot.phase, TransmissionPhase::Idle);
    assert_eq!(snapshot.repeated, 0);
    assert_eq!(snapshot.generated_events, 2);
}

#[test]
fn pulse_timing_has_at_most_three_changes_then_holds_and_is_deterministic() {
    let settings = DataTransmission {
        max_triggers: TransmissionCount::new(64).unwrap(),
        rate: TransmissionRate::new(40.0).unwrap(),
        ..DataTransmission::default()
    };
    let mut a = Renderer::new(config(48_000, 0.0)).unwrap();
    let mut b = Renderer::new(config(48_000, 0.0)).unwrap();
    start(&mut a, settings);
    start(&mut b, settings);
    let mut event_frames = Vec::new();
    let mut previous = 0;
    let mut block = [[0.0; 2]; 137];
    for block_index in 0..900 {
        b.render(&mut block);
        for (index, expected) in block.into_iter().enumerate() {
            assert_eq!(a.next_frame(), expected);
            let snapshot = a.snapshot().transmission;
            if snapshot.repeated > previous {
                event_frames.push((block_index * block.len() + index, snapshot.rate_hz));
                previous = snapshot.repeated;
                assert!((30.0..=50.0).contains(&snapshot.rate_hz));
            }
        }
    }
    assert_eq!(event_frames.len(), 64);
    let mut changes = Vec::new();
    for (index, pair) in event_frames.windows(2).enumerate() {
        let [(previous_frame, previous_rate), (frame, rate)] = pair else {
            unreachable!()
        };
        assert_eq!(
            frame - previous_frame,
            (48_000.0 / previous_rate).round() as usize
        );
        if rate != previous_rate {
            changes.push(index + 2);
        }
    }
    assert_eq!(changes, [4, 8, 12]);
    let final_rate = event_frames[11].1;
    assert!(
        event_frames[11..]
            .iter()
            .all(|(_, rate)| *rate == final_rate)
    );
}

#[test]
fn transmission_pan_moves_the_high_band_and_sub_nyquist_rates_stay_silent() {
    for sample_rate in [8_000, 16_000, 48_000] {
        let mut renderer = Renderer::new(config(sample_rate, 0.0)).unwrap();
        advance(&mut renderer, sample_rate as usize);
        start(
            &mut renderer,
            DataTransmission {
                pan: Pan::new(-1.0).unwrap(),
                ..DataTransmission::default()
            },
        );
        advance(&mut renderer, sample_rate as usize / 5);
        let mut energy = [0.0_f64; 2];
        for _ in 0..sample_rate / 2 {
            let frame = renderer.next_frame();
            for channel in 0..2 {
                assert!(frame[channel].is_finite());
                energy[channel] += f64::from(frame[channel]).powi(2);
            }
        }
        if sample_rate <= 16_000 {
            assert_eq!(energy, [0.0; 2]);
        } else {
            assert!(energy[0] > 1e-9);
            assert!(energy[1] < energy[0] * 0.0001);
        }
    }
}

#[test]
fn haas_transmission_pan_uses_thirty_three_percent_range_through_live_edits() {
    let mut haas_config = config(48_000, 0.0);
    haas_config.spatial = SpatialMode::Haas;
    let mut haas = Renderer::new(haas_config).unwrap();
    let mut mapped = Renderer::new(config(48_000, 0.0)).unwrap();
    let mut full = Renderer::new(config(48_000, 0.0)).unwrap();
    for renderer in [&mut haas, &mut mapped, &mut full] {
        advance(renderer, 48_000);
    }
    for (renderer, pan) in [(&mut haas, -1.0), (&mut mapped, -0.33), (&mut full, -1.0)] {
        start(
            renderer,
            DataTransmission {
                pan: Pan::new(pan).unwrap(),
                rate: TransmissionRate::new(40.0).unwrap(),
                max_triggers: TransmissionCount::new(64).unwrap(),
                ..DataTransmission::default()
            },
        );
    }
    let mut haas_energy = [0.0_f64; 2];
    let mut full_energy = [0.0_f64; 2];
    for frame in 0..60_000 {
        if frame == 24_000 {
            for (renderer, pan) in [(&mut haas, 1.0), (&mut mapped, 0.33), (&mut full, 1.0)] {
                renderer.apply(Command::DataTransmission(DataTransmissionCommand::Pan(
                    Pan::new(pan).unwrap(),
                )));
            }
            assert_eq!(haas.snapshot().transmission.pan, Pan::new(1.0).unwrap());
        }
        if frame == 48_000 {
            haas.apply(Command::Spatial(SpatialMode::Pan));
            mapped.apply(Command::DataTransmission(DataTransmissionCommand::Pan(
                Pan::new(1.0).unwrap(),
            )));
        }
        let a = haas.next_frame();
        let b = mapped.next_frame();
        let c = full.next_frame();
        assert_eq!(a, b, "Haas amplitude pan must match 33% of pan-only range");
        if (12_000..24_000).contains(&frame) {
            for channel in 0..2 {
                haas_energy[channel] += f64::from(a[channel]).powi(2);
                full_energy[channel] += f64::from(c[channel]).powi(2);
            }
        }
    }
    assert!(full_energy[1] < full_energy[0] * 0.0001);
    assert!(haas_energy[1] > haas_energy[0] * 0.05);

    let mut center_pan = Renderer::new(config(48_000, 0.0)).unwrap();
    let mut center_haas = Renderer::new(haas_config).unwrap();
    for renderer in [&mut center_pan, &mut center_haas] {
        advance(renderer, 48_000);
        start(renderer, DataTransmission::default());
    }
    for _ in 0..24_000 {
        assert_eq!(center_pan.next_frame(), center_haas.next_frame());
    }
}

#[test]
fn third_reverb_retains_tail_independently_of_the_ui_bus() {
    let mut wet = Renderer::new(config(48_000, 1.0)).unwrap();
    let mut dry = Renderer::new(config(48_000, 0.0)).unwrap();
    for renderer in [&mut wet, &mut dry] {
        advance(renderer, 48_000);
        start(renderer, DataTransmission::default());
        advance(renderer, 24_000);
        renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
        advance(renderer, 24_000);
        assert_eq!(
            renderer.snapshot().transmission.phase,
            TransmissionPhase::Idle
        );
    }
    let dry_energy = advance(&mut dry, 24_000);
    let wet_energy = advance(&mut wet, 24_000);
    assert!(wet_energy > 1e-9);
    assert!(wet_energy > dry_energy * 100.0);
}

#[test]
fn default_transmission_level_is_subtle_but_retains_audible_high_band_energy() {
    let mut renderer = Renderer::new(RenderConfig {
        seed: 123,
        mix: Mix {
            drone: Unit::ZERO,
            demonic_voices: Unit::ZERO,
            demonic_stutters: Unit::ZERO,
            flying_coins: Unit::ZERO,
            laser_beams: Unit::ZERO,
            sub: Unit::ZERO,
            texture: Unit::ZERO,
            harmony: Unit::ZERO,
            melody: Unit::ZERO,
            ui: Unit::ZERO,
            ..Mix::default()
        },
        ..RenderConfig::default()
    })
    .unwrap();
    advance(&mut renderer, 48_000);
    start(&mut renderer, DataTransmission::default());
    let mut energy = 0.0;
    let mut peak = 0.0_f32;
    for frame in 0..96_000 {
        if frame == 48_000 {
            renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
        }
        for sample in renderer.next_frame() {
            peak = peak.max(sample.abs());
            energy += f64::from(sample).powi(2);
        }
    }
    let rms = (energy / 192_000.0).sqrt();
    assert!((0.001..0.02).contains(&rms), "transmission RMS {rms}");
    assert!(peak < 0.18, "transmission peak {peak}");
}
