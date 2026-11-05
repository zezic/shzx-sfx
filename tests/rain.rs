use fundsp::{fft::real_fft, math::Complex32};
use shzx_sfx::{
    Command, MarketContext, Mix, Mode, OutputGain, RenderConfig, Renderer, ReverbSettings, Unit,
};

const RATE: u32 = 16_000;

fn config() -> RenderConfig {
    RenderConfig {
        sample_rate: RATE,
        seed: 123,
        mix: Mix {
            master: Unit::new(0.25).unwrap(),
            output_gain: OutputGain::new(0.0).unwrap(),
            rain: Unit::ONE,
            drone: Unit::ZERO,
            sub: Unit::ZERO,
            texture: Unit::ZERO,
            harmony: Unit::ZERO,
            melody: Unit::ZERO,
            demonic_voices: Unit::ZERO,
            demonic_stutters: Unit::ZERO,
            flying_coins: Unit::ZERO,
            laser_beams: Unit::ZERO,
            ui: Unit::ZERO,
            transmission: Unit::ZERO,
            reverb: Unit::ONE,
        },
        ..RenderConfig::default()
    }
}

fn advance(renderer: &mut Renderer, seconds: u32) {
    for _ in 0..RATE * seconds {
        renderer.next_frame();
    }
}

fn energy(frame: [f32; 2]) -> f64 {
    frame.into_iter().map(|x| f64::from(x).powi(2)).sum()
}

#[test]
fn focus_modes_reduce_direct_rain_and_increase_its_relative_reverb() {
    let [browse, focus, dialog] = [Mode::Browse, Mode::Focus, Mode::Dialog].map(|mode| {
        let config = config();
        let mut dry_config = config;
        dry_config.reverb.rain.wet = Unit::ZERO;
        let mut full = Renderer::new(config).unwrap();
        let mut direct = Renderer::new(dry_config).unwrap();
        for renderer in [&mut full, &mut direct] {
            renderer.apply(Command::Mode(mode));
            advance(renderer, 5);
        }
        let mut dry_energy = 0.0;
        let mut wet_energy = 0.0;
        for _ in 0..RATE * 2 {
            let output = full.next_frame();
            let dry = direct.next_frame();
            dry_energy += energy(dry);
            wet_energy += energy(std::array::from_fn(|i| output[i] - dry[i]));
        }
        let state = full.snapshot().background.rain;
        eprintln!("{mode}: dry {dry_energy:.6}, wet {wet_energy:.6}, state {state:?}");
        assert!(dry_energy > 1e-5 && wet_energy > 1e-7);
        (dry_energy, wet_energy / dry_energy)
    });
    assert!(focus.0 < browse.0 * 0.35 && dialog.0 < focus.0 * 0.4);
    assert!(focus.1 > browse.1 * 5.0 && dialog.1 > focus.1 * 2.0);
}

#[test]
fn exterior_rain_retains_audible_short_term_detail_against_the_wash() {
    let rate = 48_000;
    let mut config = config();
    config.sample_rate = rate;
    config.mix.reverb = Unit::ZERO;
    let mut renderer = Renderer::new(config).unwrap();
    for _ in 0..rate {
        renderer.next_frame();
    }
    let mut windows = [0.0_f64; 600];
    for power in &mut windows {
        for _ in 0..rate / 50 {
            let frame = renderer.next_frame();
            let mono = f64::from((frame[0] + frame[1]) * 0.5);
            *power += mono * mono;
        }
    }
    windows.sort_by(f64::total_cmp);
    let contrast = (windows[540] / windows[60]).sqrt();
    eprintln!("exterior rain 20ms RMS p90/p10: {contrast:.3}");
    assert!(
        contrast > 1.5,
        "drop texture must emerge above a steady wash"
    );
}

#[test]
fn deep_zoom_darkens_rain_and_blends_in_the_car_roof_smoothly() {
    let mut dry_config = config();
    dry_config.mix.reverb = Unit::ZERO;
    let mut exterior = Renderer::new(dry_config).unwrap();
    let mut cabin = Renderer::new(dry_config).unwrap();
    cabin.apply(Command::Market(MarketContext {
        zoom: Unit::ONE,
        ..MarketContext::default()
    }));
    for renderer in [&mut exterior, &mut cabin] {
        advance(renderer, 4);
    }
    let state = cabin.snapshot().background.rain;
    assert!(state.cabin_mix.current > 0.999);
    assert!(state.exterior_lowpass_hz.current < 560.0);
    let high_share = |renderer: &mut Renderer| {
        let mut input = [0.0; 8192];
        for (index, sample) in input.iter_mut().enumerate() {
            let frame = renderer.next_frame();
            let hann = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / 8191.0).cos();
            *sample = frame[0] * hann;
        }
        let mut spectrum = [Complex32::new(0.0, 0.0); 4097];
        real_fft(&input, &mut spectrum);
        let mut total = 0.0;
        let mut high = 0.0;
        for (index, bin) in spectrum.iter().enumerate() {
            let power = f64::from(bin.norm_sqr());
            total += power;
            if index as f32 * RATE as f32 / 8192.0 >= 2_000.0 {
                high += power;
            }
        }
        assert!(total > 1e-5);
        high / total
    };
    let broad_high = high_share(&mut exterior);
    let cabin_high = high_share(&mut cabin);
    eprintln!("rain >2kHz spectral share: exterior {broad_high:.6}, cabin {cabin_high:.6}");
    assert!(broad_high > 0.05 && cabin_high < broad_high * 0.2);

    // Compare identical live sources just after the command: a perspective
    // change must begin gradually even though the rain contains sharp impacts.
    let mut reference = Renderer::new(dry_config).unwrap();
    let mut moving = Renderer::new(dry_config).unwrap();
    advance(&mut reference, 1);
    advance(&mut moving, 1);
    moving.apply(Command::Market(MarketContext {
        zoom: Unit::ONE,
        ..MarketContext::default()
    }));
    moving.apply(Command::Mode(Mode::Dialog));
    let mut initial_error = 0.0_f64;
    let mut initial_energy = 0.0_f64;
    let mut maximum_step = 0.0_f32;
    let mut previous = [0.0; 2];
    for sample in 0..RATE * 3 {
        let a = reference.next_frame();
        let b = moving.next_frame();
        for i in 0..2 {
            assert!(b[i].is_finite() && b[i].abs() < 0.2);
            if sample < RATE / 100 {
                initial_error += f64::from(b[i] - a[i]).powi(2);
                initial_energy += f64::from(a[i]).powi(2);
            }
            if sample > 0 {
                maximum_step = maximum_step.max((b[i] - previous[i]).abs());
            }
            previous[i] = b[i];
        }
    }
    let initial_relative_error = (initial_error / initial_energy).sqrt();
    eprintln!(
        "rain transition first 10ms relative error {initial_relative_error:.6}, maximum step {maximum_step:.6}"
    );
    assert!(initial_relative_error < 0.025);
    assert!(maximum_step < 0.06);
}

#[test]
fn rain_has_its_own_gain_send_return_and_preserves_tails_when_muted() {
    let mut config = config();
    config.reverb.rain.room_size = Unit::ONE;
    config.reverb.rain.send = Unit::ONE;
    config.reverb.rain.wet = Unit::ONE;
    let mut unrelated = config;
    for bus in [
        &mut unrelated.reverb.environment,
        &mut unrelated.reverb.events,
        &mut unrelated.reverb.transmission,
        &mut unrelated.reverb.melody,
        &mut unrelated.reverb.accents,
    ] {
        bus.send = Unit::ZERO;
        bus.wet = Unit::ZERO;
    }
    let mut dry_config = config;
    dry_config.reverb.rain.send = Unit::ZERO;
    let mut no_return_config = config;
    no_return_config.reverb.rain.wet = Unit::ZERO;
    let mut half_config = config;
    half_config.mix.rain = Unit::new(0.5).unwrap();
    let mut full = Renderer::new(config).unwrap();
    let mut isolated = Renderer::new(unrelated).unwrap();
    let mut no_send = Renderer::new(dry_config).unwrap();
    let mut no_return = Renderer::new(no_return_config).unwrap();
    let mut half = Renderer::new(half_config).unwrap();
    let mut total = 0.0;
    for _ in 0..RATE * 3 {
        let frame = full.next_frame();
        assert_eq!(frame, isolated.next_frame());
        assert_eq!(no_send.next_frame(), no_return.next_frame());
        let scaled = half.next_frame();
        for i in 0..2 {
            assert!((scaled[i] - frame[i] * 0.5).abs() < 1e-7);
        }
        total += energy(frame);
    }
    assert!(total > 0.1);
    let mut muted = config.mix;
    muted.rain = Unit::ZERO;
    for renderer in [&mut full, &mut no_send, &mut no_return] {
        renderer.apply(Command::Mix(muted));
        advance(renderer, 1);
    }
    let mut tail = 0.0;
    let mut dry_tail = 0.0;
    let mut no_return_tail = 0.0;
    for _ in 0..RATE {
        tail += energy(full.next_frame());
        dry_tail += energy(no_send.next_frame());
        no_return_tail += energy(no_return.next_frame());
    }
    eprintln!(
        "rain muted-source tail {tail:e}, zero send {dry_tail:e}, zero wet {no_return_tail:e}"
    );
    assert!(tail > 1e-6 && tail > dry_tail * 100.0 && tail > no_return_tail * 100.0);
}

#[test]
fn rain_controls_validate_and_old_settings_receive_defaults() {
    assert!(serde_json::from_str::<Mix>(r#"{"rain":1.1}"#).is_err());
    let defaults = ReverbSettings::default();
    let mut encoded = serde_json::to_value(defaults).unwrap();
    encoded["rain"]["send"] = serde_json::json!(-0.1);
    assert!(serde_json::from_value::<ReverbSettings>(encoded.clone()).is_err());
    encoded.as_object_mut().unwrap().remove("rain");
    assert_eq!(
        serde_json::from_value::<ReverbSettings>(encoded).unwrap(),
        defaults
    );
    assert_eq!(
        serde_json::from_str::<Mix>("{}").unwrap().rain,
        Mix::default().rain
    );
    let renderer = Renderer::new(RenderConfig {
        sample_rate: 8_000,
        ..config()
    })
    .unwrap();
    let cutoff = renderer.snapshot().background.rain.exterior_lowpass_hz;
    assert_eq!(cutoff.current, 3_600.0);
    assert_eq!(cutoff.target, 3_600.0);
}
