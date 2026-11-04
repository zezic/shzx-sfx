use fundsp::{fft::real_fft, hacker32::Complex32};
use shzx_sfx::{
    Command, Mix, OutputGain, RenderConfig, Renderer, SfxEvent, SfxGenerator, SfxKind, SfxLayer,
    SfxLayerSelection, SpatialMode, Unit,
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
    let mut renderer = dry_renderer(sample_rate);
    let mut warmup = vec![[0.0; 2]; sample_rate as usize / 4];
    renderer.render(&mut warmup);
    renderer.apply(Command::Trigger(SfxEvent {
        layers,
        ..SfxEvent::new(kind)
    }));
    let generated = renderer.snapshot().last_event.unwrap();
    assert_eq!(generated.kind, kind);
    assert_eq!(generated.layers, layers);
    let mut frames =
        vec![[0.0; 2]; (generated.duration_seconds * sample_rate as f32).ceil() as usize];
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
    for kind in [
        SfxKind::Focus,
        SfxKind::Confirm,
        SfxKind::Click,
        SfxKind::OrderFill,
    ] {
        let full = event_audio(kind, SfxLayerSelection::All, 48_000);
        let mut sum = vec![[0.0; 2]; full.len()];
        for layer in SfxLayer::ALL {
            let solo = event_audio(kind, SfxLayerSelection::Only(layer), 48_000);
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
            "{kind}: soloing must preserve random choices and timing; relative RMS error {relative_rms_error}"
        );
    }
}

#[test]
fn event_selection_round_trips_and_omitted_selection_means_all_layers() {
    let ordinary: SfxEvent =
        serde_json::from_str(r#"{"kind":"click","pan":0,"intensity":0.6}"#).unwrap();
    assert_eq!(ordinary.layers, SfxLayerSelection::All);
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
fn every_palette_recipe_exposes_the_components_used_for_audition() {
    for kind in SfxKind::iter() {
        let recipe = kind.recipe();
        assert_eq!(recipe.kind, kind);
        assert!(recipe.pitch_hz.is_finite() && recipe.pitch_hz > 0.0);
        assert!(recipe.duration_seconds.is_finite() && recipe.duration_seconds > 0.0);
        assert!(!recipe.phrase.is_empty());
        assert!(!recipe.micro_pattern.is_empty());
        assert!(!recipe.tonal_pitch_ratios.is_empty());
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
