use fundsp::{fft::real_fft, hacker32::Complex32};
use shzx_sfx::{
    Command, GeneratedEvent, Mix, OutputGain, Pan, RenderConfig, Renderer, SfxEdits, SfxEvent,
    SfxFilterBand, SfxFmIndex, SfxFmRatio, SfxGenerator, SfxKind, SfxLayer, SfxLayerEdits,
    SfxLayerSelection, SfxLayerState, SfxMilliseconds, SfxModulation, SfxScale, SfxStepRate,
    SfxTonalEdits, SfxTonalRate, SfxTonalSequence, SfxTonalStep, SpatialMode, Unit,
};
use strum::IntoEnumIterator;

fn dry_renderer(sample_rate: u32) -> Renderer {
    Renderer::new(RenderConfig {
        sample_rate,
        seed: 123,
        max_voices: 1,
        spatial: SpatialMode::Pan,
        mix: Mix {
            master: Unit::new(0.1).unwrap(),
            output_gain: OutputGain::new(0.0).unwrap(),
            drone: Unit::ZERO,
            texture: Unit::ZERO,
            harmony: Unit::ZERO,
            melody: Unit::ZERO,
            ui: Unit::ONE,
            reverb: Unit::ZERO,
        },
        ..RenderConfig::default()
    })
    .unwrap()
}

fn event_audio(kind: SfxKind, layers: SfxLayerSelection, sample_rate: u32) -> Vec<[f32; 2]> {
    render_event(
        SfxEvent {
            layers,
            ..SfxEvent::new(kind)
        },
        sample_rate,
    )
    .1
}

fn render_event(event: SfxEvent, sample_rate: u32) -> (GeneratedEvent, Vec<[f32; 2]>) {
    render_spatial_event(event, sample_rate, SpatialMode::Pan)
}

fn render_spatial_event(
    event: SfxEvent,
    sample_rate: u32,
    spatial: SpatialMode,
) -> (GeneratedEvent, Vec<[f32; 2]>) {
    let mut renderer = dry_renderer(sample_rate);
    renderer.apply(Command::Spatial(spatial));
    let mut warmup = vec![[0.0; 2]; sample_rate as usize / 4];
    renderer.render(&mut warmup);
    renderer.apply(Command::Trigger(event));
    let generated = renderer.snapshot().last_event.unwrap();
    assert_eq!(generated.kind, event.kind);
    assert_eq!(generated.layers, event.layers);
    assert_eq!(generated.edits, event.edits);
    let mut frames =
        vec![[0.0; 2]; (generated.duration_seconds * sample_rate as f32).ceil() as usize];
    renderer.render(&mut frames);
    (generated, frames)
}

fn energy(frames: &[[f32; 2]]) -> f64 {
    frames
        .iter()
        .flatten()
        .map(|&sample| f64::from(sample).powi(2))
        .sum()
}

fn spectral_share_above(frames: &[[f32; 2]], sample_rate: u32, cutoff_hz: f32) -> f64 {
    const FFT_LENGTH: usize = 1024;
    let mut input = [0.0; FFT_LENGTH];
    let mut output = [Complex32::new(0.0, 0.0); FFT_LENGTH / 2 + 1];
    let mut total = 0.0;
    let mut high = 0.0;
    for window in frames.windows(FFT_LENGTH).step_by(FFT_LENGTH / 4) {
        for (index, (&frame, sample)) in window.iter().zip(&mut input).enumerate() {
            let hann =
                0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (FFT_LENGTH - 1) as f32).cos();
            *sample = frame[0] * hann;
        }
        real_fft(&input, &mut output);
        for (index, bin) in output.iter().enumerate() {
            let power = f64::from(bin.norm_sqr());
            total += power;
            if index as f32 * sample_rate as f32 / FFT_LENGTH as f32 >= cutoff_hz {
                high += power;
            }
        }
    }
    assert!(
        total > 0.0,
        "the isolated component must produce measurable audio"
    );
    high / total
}

#[test]
fn isolated_components_reconstruct_the_same_seeded_full_voice() {
    for (kind, pan, spatial) in [
        (SfxKind::Focus, -0.7, SpatialMode::Pan),
        (SfxKind::Confirm, 0.0, SpatialMode::Haas),
        (SfxKind::OrderFill, 0.7, SpatialMode::Haas),
    ] {
        let mut event = SfxEvent::new(kind);
        event.pan = Pan::new(pan).unwrap();
        event.edits.tonal.step_rate = Some(SfxTonalRate::Sweep {
            start: SfxStepRate::new(40.0).unwrap(),
            end: SfxStepRate::new(1_200.0).unwrap(),
        });
        let full = render_spatial_event(event, 48_000, spatial).1;
        let mut sum = vec![[0.0; 2]; full.len()];
        for layer in SfxLayer::ALL {
            let solo = render_spatial_event(
                SfxEvent {
                    layers: SfxLayerSelection::Only(layer),
                    ..event
                },
                48_000,
                spatial,
            )
            .1;
            assert_eq!(solo.len(), full.len());
            for (frame, component) in sum.iter_mut().zip(solo) {
                frame[0] += component[0];
                frame[1] += component[1];
            }
        }
        let residual: f64 = sum
            .iter()
            .zip(&full)
            .map(|(sum, full)| {
                f64::from(sum[0] - full[0]).powi(2) + f64::from(sum[1] - full[1]).powi(2)
            })
            .sum();
        let relative_rms_error = (residual / energy(&full)).sqrt();
        assert!(
            relative_rms_error < 0.002,
            "{kind}, pan {pan}, {spatial}: solos must preserve stereo, random choices and swept timing; relative RMS error {relative_rms_error}"
        );
    }
}

#[test]
fn dry_layer_width_survives_mono_fold_down_across_sample_rates() {
    for sample_rate in [8_000, 48_000, 192_000] {
        for layer in SfxLayer::ALL {
            let frames = event_audio(
                SfxKind::OrderFill,
                SfxLayerSelection::Only(layer),
                sample_rate,
            );
            assert!(frames.iter().flatten().all(|sample| sample.is_finite()));
            let total = energy(&frames);
            if sample_rate == 8_000 && matches!(layer, SfxLayer::Shimmer | SfxLayer::Crisp) {
                assert_eq!(
                    total, 0.0,
                    "{layer}: unsupported high bands must remain silent"
                );
                continue;
            }
            assert!(total > 0.0, "{layer} at {sample_rate} Hz must be audible");
            let (mid, side) = frames
                .iter()
                .fold((0.0, 0.0), |(mid, side), &[left, right]| {
                    (
                        mid + ((f64::from(left) + f64::from(right)) * 0.5).powi(2),
                        side + ((f64::from(left) - f64::from(right)) * 0.5).powi(2),
                    )
                });
            let side_share = 2.0 * side / total;
            let mono_share = 2.0 * mid / total;
            if matches!(layer, SfxLayer::Body | SfxLayer::Texture) {
                assert!(
                    side_share < 1e-10,
                    "{layer} at {sample_rate} Hz must stay centered: {side_share}"
                );
            } else {
                assert!(
                    side_share > 0.001,
                    "{layer} at {sample_rate} Hz needs intrinsic width without positional Haas: {side_share}"
                );
            }
            assert!(
                mono_share > 0.25,
                "{layer} at {sample_rate} Hz loses too much energy in mono: {mono_share}"
            );
        }
    }
}

#[test]
fn numeric_tonal_rate_json_stays_fixed_and_sweeps_change_seeded_timing() {
    let mut fixed: SfxEvent = serde_json::from_str(
        r#"{"kind":"confirm","pan":0,"intensity":0.6,"edits":{"tonal":{"step_rate":325}}}"#,
    )
    .unwrap();
    fixed.layers = SfxLayerSelection::Only(SfxLayer::Tonal);
    let fixed_rate = SfxTonalRate::Fixed(SfxStepRate::new(325.0).unwrap());
    assert_eq!(fixed.edits.tonal.step_rate, Some(fixed_rate));
    assert_eq!(fixed.recipe().tonal_rate, fixed_rate);
    let encoded = serde_json::to_value(fixed).unwrap();
    assert_eq!(encoded["edits"]["tonal"]["step_rate"].as_f64(), Some(325.0));

    let mut swept = fixed;
    let rate = SfxTonalRate::Sweep {
        start: SfxStepRate::new(50.0).unwrap(),
        end: SfxStepRate::new(600.0).unwrap(),
    };
    swept.edits.tonal.step_rate = Some(rate);
    assert_eq!(swept.recipe().tonal_rate, rate);
    let encoded = serde_json::to_value(swept).unwrap();
    assert_eq!(
        encoded["edits"]["tonal"]["step_rate"],
        serde_json::json!({"start":50.0,"end":600.0})
    );
    assert_eq!(serde_json::from_value::<SfxEvent>(encoded).unwrap(), swept);
    let (fixed_snapshot, fixed_frames) = render_event(fixed, 48_000);
    let (swept_snapshot, swept_frames) = render_event(swept, 48_000);
    assert_eq!(fixed_snapshot.seed, swept_snapshot.seed);
    assert_eq!(fixed_snapshot.pitch_hz, swept_snapshot.pitch_hz);
    assert_eq!(
        fixed_snapshot.duration_seconds,
        swept_snapshot.duration_seconds
    );
    assert_ne!(
        fixed_frames, swept_frames,
        "equal average clock rates must still produce different timing when one sweeps"
    );
    assert_eq!(render_event(swept, 48_000).1, swept_frames);
}

#[test]
fn event_selection_round_trips_and_omitted_selection_means_all_layers() {
    let ordinary: SfxEvent =
        serde_json::from_str(r#"{"kind":"click","pan":0,"intensity":0.6}"#).unwrap();
    assert_eq!(ordinary.layers, SfxLayerSelection::All);
    assert_eq!(ordinary.edits, SfxEdits::DEFAULT);
    let solo = SfxEvent {
        layers: SfxLayerSelection::Only(SfxLayer::Texture),
        ..ordinary
    };
    let encoded = serde_json::to_string(&solo).unwrap();
    assert_eq!(serde_json::from_str::<SfxEvent>(&encoded).unwrap(), solo);
    assert!(
        serde_json::from_str::<SfxEvent>(
            r#"{"kind":"click","pan":0,"intensity":0.6,"layers":{"mode":"only","layer":"unknown"}}"#
        )
        .is_err()
    );
}

#[test]
fn edited_events_round_trip_and_reject_invalid_numeric_input() {
    let mut event = SfxEvent::new(SfxKind::Confirm);
    event.edits.pitch = SfxScale::new(1.5).unwrap();
    event.edits.duration = SfxScale::new(2.0).unwrap();
    event.edits.set_layer(
        SfxLayer::Texture,
        SfxLayerEdits {
            state: SfxLayerState::Muted,
            level: Some(Unit::new(0.12).unwrap()),
            filter: Some(SfxFilterBand::new(800.0, 2_000.0).unwrap()),
        },
    );
    let encoded = serde_json::to_value(event).unwrap();
    assert_eq!(
        serde_json::from_value::<SfxEvent>(encoded.clone()).unwrap(),
        event
    );
    for scale in [f32::NAN, f32::INFINITY, -1.0, 0.0, 4.01] {
        assert!(SfxScale::new(scale).is_err());
    }
    for (highpass, lowpass) in [(0.0, 1_000.0), (2_000.0, 1_000.0), (100.0, f32::NAN)] {
        assert!(SfxFilterBand::new(highpass, lowpass).is_err());
    }
    for invalid in [0.0, 4.01] {
        let mut input = encoded.clone();
        input["edits"]["pitch"] = serde_json::json!(invalid);
        assert!(serde_json::from_value::<SfxEvent>(input).is_err());
    }
    let mut input = encoded;
    input["edits"]["layers"]["texture"]["filter"]["highpass_hz"] = serde_json::json!(3_000.0);
    assert!(serde_json::from_value::<SfxEvent>(input).is_err());
    let partial: SfxEvent = serde_json::from_str(
        r#"{"kind":"click","pan":0,"intensity":0.6,"edits":{"layers":{"texture":{"state":"muted"}}}}"#,
    )
    .unwrap();
    assert_eq!(
        partial.edits.layer(SfxLayer::Texture).state,
        SfxLayerState::Muted
    );
    assert_eq!(
        partial.edits.layer(SfxLayer::Body),
        SfxLayerEdits::default()
    );
}

#[test]
fn layer_mutes_preserve_settings_and_remove_only_the_chosen_path() {
    let event = SfxEvent::new(SfxKind::OrderFill);
    let (_, full) = render_event(event, 48_000);
    for layer in SfxLayer::ALL {
        let mut muted = event;
        muted.edits.set_layer(
            layer,
            SfxLayerEdits {
                state: SfxLayerState::Muted,
                ..SfxLayerEdits::default()
            },
        );
        let (_, remaining) = render_event(muted, 48_000);
        let solo = event_audio(event.kind, SfxLayerSelection::Only(layer), 48_000);
        let residual: f64 = full
            .iter()
            .zip(&remaining)
            .zip(&solo)
            .flat_map(|((full, remaining), solo)| {
                [
                    full[0] - remaining[0] - solo[0],
                    full[1] - remaining[1] - solo[1],
                ]
            })
            .map(|sample| f64::from(sample).powi(2))
            .sum();
        assert!((residual / energy(&full)).sqrt() < 0.002, "{layer}");
        muted.layers = SfxLayerSelection::Only(layer);
        assert_eq!(energy(&render_event(muted, 48_000).1), 0.0);
        muted.edits.set_layer(layer, SfxLayerEdits::default());
        assert_eq!(render_event(muted, 48_000).1, solo);
    }
}

#[test]
fn every_layer_uses_edited_levels_and_filter_bands() {
    for sample_rate in [8_000, 48_000] {
        for layer in SfxLayer::ALL {
            let event = SfxEvent {
                layers: SfxLayerSelection::Only(layer),
                ..SfxEvent::new(SfxKind::OrderFill)
            };
            let profile = event
                .recipe()
                .layers
                .into_iter()
                .find(|p| p.layer == layer)
                .unwrap();
            let (_, original) = render_event(event, sample_rate);
            let mut edited = event;
            edited.edits.set_layer(
                layer,
                SfxLayerEdits {
                    level: Some(Unit::new(profile.level * 0.5).unwrap()),
                    ..SfxLayerEdits::default()
                },
            );
            let (_, quieter) = render_event(edited, sample_rate);
            if energy(&original) == 0.0 {
                assert_eq!(energy(&quieter), 0.0);
                continue;
            }
            assert!(
                (energy(&quieter) / energy(&original) - 0.25).abs() < 0.001,
                "{layer}"
            );
            let band = if layer == SfxLayer::Tonal && sample_rate == 48_000 {
                SfxFilterBand::new(profile.highpass_hz, 1_500.0).unwrap()
            } else {
                SfxFilterBand::new(200.0, 1_000.0).unwrap()
            };
            edited.edits.set_layer(
                layer,
                SfxLayerEdits {
                    filter: Some(band),
                    ..SfxLayerEdits::default()
                },
            );
            let adjusted = edited
                .recipe()
                .layers
                .into_iter()
                .find(|p| p.layer == layer)
                .unwrap();
            assert_eq!(adjusted.highpass_hz, band.highpass_hz());
            assert_eq!(adjusted.lowpass_hz, band.lowpass_hz());
            let (_, filtered) = render_event(edited, sample_rate);
            assert!(filtered.iter().flatten().all(|sample| sample.is_finite()));
            assert_ne!(
                filtered, original,
                "{layer}: editing the band must reach the renderer"
            );
            if layer == SfxLayer::Tonal && sample_rate == 48_000 {
                assert!(
                    energy(&filtered) < energy(&original) * 0.5,
                    "lowering only the tonal low-pass should attenuate its energy"
                );
            }
            assert_eq!(render_event(edited, sample_rate).1, filtered);
        }
    }
}

#[test]
fn pitch_edits_shift_tonal_shimmer_and_crisp_out_of_their_nominal_registers() {
    for (layer, split_hz) in [
        (SfxLayer::Tonal, 3_000.0),
        (SfxLayer::Shimmer, 10_000.0),
        (SfxLayer::Crisp, 11_000.0),
    ] {
        let event = SfxEvent {
            layers: SfxLayerSelection::Only(layer),
            ..SfxEvent::new(SfxKind::Click)
        };
        let mut shifted = event;
        shifted.edits.pitch = SfxScale::new(1.5).unwrap();
        let original_profile = event
            .recipe()
            .layers
            .into_iter()
            .find(|p| p.layer == layer)
            .unwrap();
        let shifted_profile = shifted
            .recipe()
            .layers
            .into_iter()
            .find(|p| p.layer == layer)
            .unwrap();
        assert_eq!(
            shifted_profile.carrier_hz.unwrap(),
            original_profile.carrier_hz.unwrap() * 1.5
        );
        let original = render_event(event, 48_000).1;
        let shifted = render_event(shifted, 48_000).1;
        let original_high = spectral_share_above(&original, 48_000, split_hz);
        let shifted_high = spectral_share_above(&shifted, 48_000, split_hz);
        assert!(
            shifted_high > original_high + 0.05,
            "{layer}: pitch should move spectral energy upward ({original_high} -> {shifted_high})"
        );
    }
}

#[test]
fn event_pitch_and_duration_edits_reach_the_recipe_and_seeded_voice() {
    let event = SfxEvent::new(SfxKind::Confirm);
    let mut edited = event;
    edited.edits.pitch = SfxScale::new(1.5).unwrap();
    edited.edits.duration = SfxScale::new(2.0).unwrap();
    let recipe = edited.recipe();
    assert_eq!(recipe.pitch_hz, event.recipe().pitch_hz * 1.5);
    assert_eq!(
        recipe.duration_seconds,
        event.recipe().duration_seconds * 2.0
    );
    let (original, _) = render_event(event, 48_000);
    let (generated, frames) = render_event(edited, 48_000);
    assert!((generated.pitch_hz / original.pitch_hz - 1.5).abs() < 0.0001);
    assert!(generated.duration_seconds > original.duration_seconds * 1.9);
    assert_eq!(render_event(edited, 48_000).1, frames);
    let mut peak = 0.0_f32;
    let mut rms = 0.0_f64;
    for sample in frames.iter().flatten() {
        peak = peak.max(sample.abs());
        rms += f64::from(*sample).powi(2);
    }
    rms = (rms / (frames.len() * 2) as f64).sqrt();
    assert!(peak.is_finite() && peak > 0.0 && peak < 1.0);
    assert!(rms.is_finite() && rms > 0.0);
}

#[test]
fn tonal_generator_edits_change_each_control_without_changing_other_layers() {
    let event = SfxEvent::new(SfxKind::Confirm);
    let original = event_audio(event.kind, SfxLayerSelection::Only(SfxLayer::Tonal), 48_000);
    let other_layers: Vec<_> = SfxLayer::ALL
        .into_iter()
        .filter(|&layer| layer != SfxLayer::Tonal)
        .map(|layer| {
            (
                layer,
                event_audio(event.kind, SfxLayerSelection::Only(layer), 48_000),
            )
        })
        .collect();
    let mut variants: Vec<_> = [
        (
            "carrier",
            SfxTonalEdits {
                carrier: SfxScale::new(1.5).unwrap(),
                ..SfxTonalEdits::DEFAULT
            },
        ),
        (
            "FM depth",
            SfxTonalEdits {
                fm_index: SfxFmIndex::new(0.6).unwrap(),
                ..SfxTonalEdits::DEFAULT
            },
        ),
        (
            "FM ratio",
            SfxTonalEdits {
                fm_ratio: SfxFmRatio::new(3.0).unwrap(),
                ..SfxTonalEdits::DEFAULT
            },
        ),
        (
            "step rate",
            SfxTonalEdits {
                step_rate: Some(SfxTonalRate::Fixed(SfxStepRate::new(80.0).unwrap())),
                ..SfxTonalEdits::DEFAULT
            },
        ),
        (
            "attack",
            SfxTonalEdits {
                attack: SfxMilliseconds::new(2.0).unwrap(),
                ..SfxTonalEdits::DEFAULT
            },
        ),
        (
            "release",
            SfxTonalEdits {
                release: SfxMilliseconds::new(2.0).unwrap(),
                ..SfxTonalEdits::DEFAULT
            },
        ),
    ]
    .into_iter()
    .map(|(name, edits)| (name.to_owned(), edits))
    .collect();
    let ratios = SfxTonalSequence::new(
        event
            .recipe()
            .tonal_pitch_ratios
            .map(|ratio| SfxScale::new(ratio).unwrap()),
    );
    for step in SfxTonalStep::ALL {
        let mut sequence = ratios;
        sequence.set(step, SfxScale::new(1.125).unwrap());
        variants.push((
            step.label().to_owned(),
            SfxTonalEdits {
                pitch_ratios: Some(sequence),
                ..SfxTonalEdits::DEFAULT
            },
        ));
    }
    for (name, tonal) in variants {
        let mut edited = event;
        edited.edits.tonal = tonal;
        edited.layers = SfxLayerSelection::Only(SfxLayer::Tonal);
        let frames = render_event(edited, 48_000).1;
        assert_ne!(
            frames, original,
            "{name}: the control must reach the generator"
        );
        assert_eq!(render_event(edited, 48_000).1, frames, "{name}");
        for (layer, original) in &other_layers {
            edited.layers = SfxLayerSelection::Only(*layer);
            assert_eq!(
                &render_event(edited, 48_000).1,
                original,
                "{name}: {layer} changed"
            );
        }
    }
}

#[test]
fn tonal_recipe_and_serialization_share_the_effective_generator_controls() {
    let mut event = SfxEvent::new(SfxKind::Confirm);
    event.edits.tonal = SfxTonalEdits {
        carrier: SfxScale::new(1.5).unwrap(),
        fm_index: SfxFmIndex::new(0.7).unwrap(),
        fm_ratio: SfxFmRatio::new(3.0).unwrap(),
        step_rate: Some(SfxTonalRate::Fixed(SfxStepRate::new(80.0).unwrap())),
        attack: SfxMilliseconds::new(2.0).unwrap(),
        release: SfxMilliseconds::new(3.0).unwrap(),
        pitch_ratios: Some(SfxTonalSequence::new([SfxScale::new(1.25).unwrap(); 6])),
    };
    let recipe = event.recipe();
    let profile = recipe
        .layers
        .into_iter()
        .find(|profile| profile.layer == SfxLayer::Tonal)
        .unwrap();
    let original_carrier = SfxKind::Confirm.recipe().tonal_base_hz;
    let expected = original_carrier * (1.5 / SfxTonalEdits::DEFAULT.carrier.get());
    assert!((profile.carrier_hz.unwrap() - expected).abs() < 0.001);
    assert!(
        matches!(profile.modulation, Some(SfxModulation::Fm {index,ratio}) if index == 0.7 && ratio == 3.0)
    );
    assert_eq!(profile.attack_ms, 2.0);
    assert_eq!(profile.release_ms, 3.0);
    assert_eq!(profile.duration_ms, 12.5);
    assert_eq!(
        recipe.tonal_rate,
        SfxTonalRate::Fixed(SfxStepRate::new(80.0).unwrap())
    );
    assert_eq!(
        recipe.detail_rate_hz,
        SfxKind::Confirm.recipe().detail_rate_hz
    );
    for step in SfxTonalStep::ALL {
        assert_eq!(recipe.tonal_pitch_ratio(step), 1.25);
    }
    let encoded = serde_json::to_value(event).unwrap();
    assert_eq!(
        serde_json::from_value::<SfxEvent>(encoded.clone()).unwrap(),
        event
    );
    assert_eq!(encoded["edits"]["tonal"]["pitch_ratios"]["one"], 1.25);
    for (field, invalid) in [
        ("fm_index", 8.01),
        ("fm_ratio", 0.0),
        ("step_rate", 2001.0),
        ("attack", 0.0),
        ("release", -1.0),
    ] {
        let mut input = encoded.clone();
        input["edits"]["tonal"][field] = serde_json::json!(invalid);
        assert!(
            serde_json::from_value::<SfxEvent>(input).is_err(),
            "{field}"
        );
    }
    let mut invalid = encoded;
    invalid["edits"]["tonal"]["pitch_ratios"]["six"] = serde_json::json!(0.0);
    assert!(serde_json::from_value::<SfxEvent>(invalid).is_err());
    for invalid in [f32::NAN, f32::INFINITY, 0.0, 2001.0] {
        assert!(SfxStepRate::new(invalid).is_err());
        assert!(SfxMilliseconds::new(invalid).is_err());
    }
}

#[test]
fn extreme_tonal_edits_stay_finite_across_supported_sample_rates() {
    for sample_rate in [8_000, 48_000, 192_000] {
        for (scale, rate, milliseconds) in [
            (4.0, SfxStepRate::MAX_HZ, SfxMilliseconds::MIN),
            (0.25, SfxStepRate::MIN_HZ, SfxMilliseconds::MAX),
        ] {
            let mut event = SfxEvent::new(SfxKind::Confirm);
            event.layers = SfxLayerSelection::Only(SfxLayer::Tonal);
            event.edits.pitch = SfxScale::new(scale).unwrap();
            event.edits.tonal = SfxTonalEdits {
                carrier: SfxScale::new(scale).unwrap(),
                fm_index: SfxFmIndex::new(SfxFmIndex::MAX).unwrap(),
                fm_ratio: SfxFmRatio::new(scale).unwrap(),
                step_rate: Some(SfxTonalRate::Fixed(SfxStepRate::new(rate).unwrap())),
                attack: SfxMilliseconds::new(milliseconds).unwrap(),
                release: SfxMilliseconds::new(milliseconds).unwrap(),
                pitch_ratios: Some(SfxTonalSequence::new([SfxScale::new(scale).unwrap(); 6])),
            };
            let frames = render_event(event, sample_rate).1;
            assert!(
                frames
                    .iter()
                    .flatten()
                    .all(|sample| sample.is_finite() && sample.abs() < 1.0)
            );
            assert!(energy(&frames) > 0.0);
        }
    }
}

#[test]
fn every_palette_recipe_exposes_the_components_used_for_audition() {
    let nominal_tonal = SfxTonalEdits::DEFAULT;
    assert!(nominal_tonal.carrier.get() < 1.0);
    assert!(nominal_tonal.fm_index.get() > 0.08);
    assert!(nominal_tonal.fm_ratio.get() > 4.0);
    for kind in SfxKind::iter() {
        let recipe = kind.recipe();
        assert_eq!(recipe.kind, kind);
        assert!(recipe.pitch_hz.is_finite() && recipe.pitch_hz > 0.0);
        assert!(recipe.duration_seconds.is_finite() && recipe.duration_seconds > 0.0);
        assert!(!recipe.phrase.is_empty());
        assert!(!recipe.micro_pattern.is_empty());
        assert!(!recipe.tonal_pitch_ratios.is_empty());
        let mut pitches = recipe.tonal_pitch_ratios;
        pitches.sort_by(f32::total_cmp);
        assert!(
            pitches.windows(2).all(|pair| pair[0] < pair[1]),
            "{kind}: all six pitch steps should differ"
        );
        assert!(
            pitches[5] / pitches[0] >= 4.0,
            "{kind}: the phrase should cover at least two octaves"
        );
        for layer in SfxLayer::ALL {
            assert_eq!(
                recipe
                    .layers
                    .iter()
                    .filter(|profile| profile.layer == layer)
                    .count(),
                1,
                "{kind}: every auditable component must appear exactly once"
            );
        }
        for profile in recipe.layers {
            assert!(profile.level.is_finite() && profile.level >= 0.0);
            assert!(profile.highpass_hz.is_finite() && profile.highpass_hz > 0.0);
            assert!(profile.lowpass_hz.is_finite() && profile.lowpass_hz > profile.highpass_hz);
            assert!(profile.duration_ms.is_finite() && profile.duration_ms > 0.0);
        }
        let texture = recipe
            .layers
            .iter()
            .find(|profile| profile.layer == SfxLayer::Texture)
            .unwrap();
        assert!(matches!(texture.generator, SfxGenerator::Intermodulation));
        assert!(texture.modulation.is_some());
        let tonal = recipe
            .layers
            .iter()
            .find(|profile| profile.layer == SfxLayer::Tonal)
            .unwrap();
        assert!(
            matches!(tonal.modulation, Some(SfxModulation::Fm {index,ratio}) if index > 0.08 && ratio > 4.0),
            "{kind}: the nominal voice should use strong inharmonic FM"
        );
        let flecks = recipe
            .layers
            .iter()
            .find(|profile| profile.layer == SfxLayer::Flecks)
            .unwrap();
        assert!(
            (12_000.0..=17_000.0).contains(&flecks.lowpass_hz),
            "{kind}: flecks should keep their high-frequency band"
        );
        assert!(
            (0.07..=0.15).contains(&flecks.level),
            "{kind}: flecks should retain their intended audible level"
        );
        let serialized = serde_json::to_value(recipe).unwrap();
        assert_eq!(
            serialized["layers"].as_array().unwrap().len(),
            SfxLayer::ALL.len()
        );
    }
}

#[test]
fn tonal_details_and_quiet_shimmer_occupy_their_high_pass_bands() {
    let tonal = event_audio(
        SfxKind::OrderFill,
        SfxLayerSelection::Only(SfxLayer::Tonal),
        48_000,
    );
    let shimmer = event_audio(
        SfxKind::OrderFill,
        SfxLayerSelection::Only(SfxLayer::Shimmer),
        48_000,
    );
    let tonal_share = spectral_share_above(&tonal, 48_000, 1_200.0);
    let shimmer_share = spectral_share_above(&shimmer, 48_000, 7_000.0);
    assert!(
        tonal_share > 0.80,
        "tonal detail is intended above the body band: {tonal_share}"
    );
    assert!(
        shimmer_share > 0.90,
        "the longer detail must stay in its high-pass band: {shimmer_share}"
    );
    assert!(
        energy(&shimmer) < energy(&tonal) * 0.1,
        "shimmer should remain a quiet additional layer rather than dominate the tonal sequence"
    );
    let unavailable = event_audio(
        SfxKind::OrderFill,
        SfxLayerSelection::Only(SfxLayer::Shimmer),
        8_000,
    );
    assert_eq!(
        energy(&unavailable),
        0.0,
        "the 7 kHz shimmer must be absent when the sample rate cannot represent it"
    );
}
