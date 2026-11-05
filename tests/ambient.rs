use fundsp::{fft::real_fft, math::Complex32};
use shzx_sfx::{
    AmbientSnapshot, BackgroundSnapshot, Command, MarketContext, Mix, OutputGain, RenderConfig,
    Renderer, ReverbSettings, Symbol, Unit,
};

const RATE: u32 = 48_000;

#[derive(Clone, Copy, Debug)]
enum Source {
    Voices,
    Stutters,
    Coins,
    Lasers,
}

impl Source {
    const ALL: [Self; 4] = [Self::Voices, Self::Stutters, Self::Coins, Self::Lasers];

    fn set(self, mix: &mut Mix, gain: Unit) {
        *match self {
            Self::Voices => &mut mix.demonic_voices,
            Self::Stutters => &mut mix.demonic_stutters,
            Self::Coins => &mut mix.flying_coins,
            Self::Lasers => &mut mix.laser_beams,
        } = gain;
    }

    fn state(self, background: BackgroundSnapshot) -> AmbientSnapshot {
        match self {
            Self::Voices => background.demonic_voices,
            Self::Stutters => background.demonic_stutters,
            Self::Coins => background.flying_coins,
            Self::Lasers => background.laser_beams,
        }
    }
}

fn silent_mix() -> Mix {
    Mix {
        master: Unit::new(0.25).unwrap(),
        output_gain: OutputGain::new(0.0).unwrap(),
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
        reverb: Unit::ZERO,
    }
}

fn config(source: Source, rate: u32) -> RenderConfig {
    let mut mix = silent_mix();
    source.set(&mut mix, Unit::ONE);
    RenderConfig {
        sample_rate: rate,
        seed: 123,
        mix,
        ..RenderConfig::default()
    }
}

fn advance(renderer: &mut Renderer, mut frames: usize) {
    let mut block = [[0.0; 2]; 256];
    while frames > 0 {
        let count = frames.min(block.len());
        renderer.render(&mut block[..count]);
        frames -= count;
    }
}

fn energy(renderer: &mut Renderer, frames: u32) -> f64 {
    (0..frames)
        .flat_map(|_| renderer.next_frame())
        .map(|sample| f64::from(sample).powi(2))
        .sum()
}

#[test]
fn each_ambient_source_is_audible_bounded_and_has_an_independent_linear_gain() {
    for source in Source::ALL {
        let full_config = config(source, RATE);
        let mut half_config = full_config;
        source.set(&mut half_config.mix, Unit::new(0.5).unwrap());
        let mut full = Renderer::new(full_config).unwrap();
        let mut half = Renderer::new(half_config).unwrap();
        let mut total = 0.0_f64;
        let mut scaled_total = 0.0_f64;
        let mut peak = 0.0_f32;
        let mut active_seen = false;
        for frame in 0..RATE * 40 {
            let expected = full.next_frame();
            let scaled = half.next_frame();
            for channel in 0..2 {
                assert!(expected[channel].is_finite());
                assert!((scaled[channel] - expected[channel] * 0.5).abs() < 1e-7);
                total += f64::from(expected[channel]).powi(2);
                scaled_total += f64::from(scaled[channel]).powi(2);
                peak = peak.max(expected[channel].abs());
            }
            if frame.is_multiple_of(256) {
                let background = full.snapshot().background;
                let state = source.state(background);
                active_seen |= state.active > 0;
                assert!(background.demonic_voices.active + background.demonic_stutters.active <= 2);
                assert!(background.flying_coins.active <= 1 && background.laser_beams.active <= 1);
            }
        }
        let rms = (total / f64::from(RATE * 80)).sqrt();
        assert!(active_seen && source.state(full.snapshot().background).generated >= 1);
        assert!((0.000001..0.02).contains(&rms), "{source:?}: RMS {rms}");
        assert!(peak < 0.1, "{source:?}: peak {peak}");
        assert!((scaled_total / total - 0.25).abs() < 1e-5);
    }
}

#[test]
fn irregular_blocks_and_live_tickers_reproduce_while_letters_affect_the_voices() {
    let mut all = config(Source::Voices, RATE);
    for source in Source::ALL {
        source.set(&mut all.mix, Unit::ONE);
    }
    let mut scalar = Renderer::new(all).unwrap();
    let mut blocked = Renderer::new(all).unwrap();
    let mut block = [[0.0; 2]; 1024];
    for (segment, ticker) in ["AAPL", "BRK.B", "TSLA", "123", "IBM"]
        .into_iter()
        .enumerate()
    {
        let command = Command::Symbol(Symbol::new(ticker).unwrap());
        scalar.apply(command);
        blocked.apply(command);
        let mut remaining = RATE as usize * 4;
        let mut block_index = segment;
        while remaining > 0 {
            let count = [1, 7, 137, 1024, 63][block_index % 5].min(remaining);
            blocked.render(&mut block[..count]);
            for &expected in &block[..count] {
                assert_eq!(scalar.next_frame(), expected);
            }
            remaining -= count;
            block_index += 1;
        }
    }
    for source in Source::ALL {
        assert_eq!(
            source.state(scalar.snapshot().background),
            source.state(blocked.snapshot().background)
        );
    }

    let first = Symbol::new("A").unwrap();
    let second = (b'B'..=b'Z')
        .map(|letter| Symbol::new(&char::from(letter).to_string()).unwrap())
        .find(|candidate| candidate.root() == first.root())
        .unwrap();
    let mut first_config = config(Source::Voices, 8_000);
    first_config.symbol = first;
    let mut unchanged = Renderer::new(first_config).unwrap();
    let mut changed = Renderer::new(first_config).unwrap();
    advance(&mut unchanged, 8_000);
    advance(&mut changed, 8_000);
    changed.apply(Command::Symbol(second));
    assert_eq!(unchanged.snapshot().chord, changed.snapshot().chord);
    let mut difference = 0.0_f64;
    let mut reference = 0.0_f64;
    for _ in 0..8_000 * 12 {
        let a = unchanged.next_frame();
        let b = changed.next_frame();
        for channel in 0..2 {
            reference += f64::from(a[channel]).powi(2);
            difference += f64::from(a[channel] - b[channel]).powi(2);
        }
    }
    assert!(
        reference > 0.0 && difference > reference * 0.01,
        "same-root ticker letters must change articulation: {first} -> {second}"
    );
}

#[test]
fn voices_stutters_and_lasers_feed_the_accent_reverb_and_leave_controllable_tails() {
    for source in [Source::Voices, Source::Stutters, Source::Lasers] {
        let mut settings = ReverbSettings::default();
        settings.accents.send = Unit::ONE;
        settings.accents.wet = Unit::ONE;
        settings.accents.room_size = Unit::ONE;
        settings.accents.damping = Unit::ZERO;
        let mut with_reverb = config(source, RATE);
        with_reverb.mix.reverb = Unit::ONE;
        with_reverb.reverb = settings;
        let mut without_send = with_reverb;
        without_send.reverb.accents.send = Unit::ZERO;
        let mut without_wet = with_reverb;
        without_wet.reverb.accents.wet = Unit::ZERO;
        let mut wet = Renderer::new(with_reverb).unwrap();
        let mut no_send = Renderer::new(without_send).unwrap();
        let mut no_wet = Renderer::new(without_wet).unwrap();
        let mut entered = false;
        for _ in 0..RATE * 15 / 256 {
            for renderer in [&mut wet, &mut no_send, &mut no_wet] {
                advance(renderer, 256);
            }
            if source.state(wet.snapshot().background).active > 0 {
                entered = true;
                break;
            }
        }
        assert!(entered);
        for renderer in [&mut wet, &mut no_send, &mut no_wet] {
            advance(renderer, RATE as usize);
            let mut muted = silent_mix();
            muted.reverb = Unit::ONE;
            renderer.apply(Command::Mix(muted));
            advance(renderer, RATE as usize);
        }
        let tail = energy(&mut wet, RATE);
        let no_send_tail = energy(&mut no_send, RATE);
        let no_wet_tail = energy(&mut no_wet, RATE);
        let tail_rms = (tail / f64::from(RATE * 2)).sqrt();
        eprintln!("{source:?} accent tail RMS {tail_rms:e}");
        assert!(
            tail_rms > 5e-7 && tail > no_send_tail * 100.0,
            "{source:?}: reverb tail {tail:e}, zero-send {no_send_tail:e}"
        );
        assert!(
            tail > no_wet_tail * 100.0,
            "{source:?}: zero-wet {no_wet_tail:e}"
        );
        assert!((no_send_tail - no_wet_tail).abs() < 1e-18);
    }
}

#[test]
fn high_register_accents_survive_deep_zoom_and_stay_silent_at_unsupported_rates() {
    // The carrier registers are 1..10 kHz for coins and 10..14 kHz for lasers.
    // Small margins include modulation while rejecting out-of-band products.
    for (source, entrance, band) in [
        (Source::Coins, 5, 900.0..=11_000.0),
        (Source::Lasers, 13, 9_500.0..=15_000.0),
    ] {
        let mut reference = Renderer::new(config(source, RATE)).unwrap();
        let mut zoomed = Renderer::new(config(source, RATE)).unwrap();
        zoomed.apply(Command::Market(MarketContext {
            zoom: Unit::ONE,
            ..MarketContext::default()
        }));
        advance(&mut reference, RATE as usize * entrance);
        advance(&mut zoomed, RATE as usize * entrance);
        assert!(zoomed.snapshot().cutoff_hz < 300.0);
        let mut input = [0.0; 8192];
        let mut normal_energy = 0.0_f64;
        let mut zoom_energy = 0.0_f64;
        for (index, sample) in input.iter_mut().enumerate() {
            let normal = reference.next_frame();
            let deep = zoomed.next_frame();
            normal_energy += normal
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
            zoom_energy += deep
                .iter()
                .map(|value| f64::from(*value).powi(2))
                .sum::<f64>();
            let hann = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / 8191.0).cos();
            *sample = deep[0] * hann;
        }
        assert!(zoom_energy > normal_energy * 0.5 && zoom_energy < normal_energy);
        let mut spectrum = [Complex32::new(0.0, 0.0); 4097];
        real_fft(&input, &mut spectrum);
        let total = spectrum
            .iter()
            .map(|bin| f64::from(bin.norm_sqr()))
            .sum::<f64>();
        let in_band = spectrum
            .iter()
            .enumerate()
            .filter(|(index, _)| band.contains(&(*index as f32 * RATE as f32 / 8192.0)))
            .map(|(_, bin)| f64::from(bin.norm_sqr()))
            .sum::<f64>();
        assert!(
            in_band / total > 0.95,
            "{source:?}: register share {}",
            in_band / total
        );
        let mut unsupported = Renderer::new(config(source, 8_000)).unwrap();
        assert_eq!(energy(&mut unsupported, 8_000 * 40), 0.0);
        assert_eq!(source.state(unsupported.snapshot().background).generated, 0);
    }
}
