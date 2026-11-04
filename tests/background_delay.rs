use shzx_sfx::{Command, MarketContext, Mode, RenderConfig, Renderer, Unit};

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
        assert!(
            block[..count]
                .iter()
                .flatten()
                .all(|sample| sample.is_finite())
        );
        frames -= count;
    }
}

#[test]
fn requested_delay_destinations_update_before_the_two_sends_follow() {
    let mut renderer = renderer();
    let initial = renderer.snapshot().background;
    renderer.apply(Command::Market(MarketContext {
        zoom: Unit::ONE,
        volume: Unit::ONE,
        volatility: Unit::ONE,
        ..MarketContext::default()
    }));
    renderer.apply(Command::Mode(Mode::Dialog));
    let requested = renderer.snapshot().background;
    assert_eq!(
        requested.detail_delay.time_ms.current,
        initial.detail_delay.time_ms.current
    );
    assert_eq!(
        requested.melody_delay.time_ms.current,
        initial.melody_delay.time_ms.current
    );
    assert!(requested.detail_delay.time_ms.target > initial.detail_delay.time_ms.target + 200.0);
    assert!(requested.melody_delay.time_ms.target > initial.melody_delay.time_ms.target + 500.0);
    assert!(requested.detail_delay.feedback.target > initial.detail_delay.feedback.target);
    assert!(requested.melody_delay.feedback.target > initial.melody_delay.feedback.target);
    advance(&mut renderer, 512);
    let moving = renderer.snapshot().background;
    assert!(moving.detail_delay.time_ms.current > initial.detail_delay.time_ms.current);
    assert!(moving.melody_delay.time_ms.current > initial.melody_delay.time_ms.current);
    assert!(moving.detail_delay.time_ms.current < moving.detail_delay.time_ms.target);
    assert!(moving.melody_delay.feedback.current < moving.melody_delay.feedback.target);
    advance(&mut renderer, RATE as usize * 4);
    let settled = renderer.snapshot().background;
    for delay in [settled.detail_delay, settled.melody_delay] {
        assert!((delay.time_ms.current - delay.time_ms.target).abs() < 4.0);
        assert!((delay.feedback.current - delay.feedback.target).abs() < 0.002);
        assert!((delay.send.current - delay.send.target).abs() < 0.002);
    }
    assert_ne!(settled.detail_delay, settled.melody_delay);
}

#[test]
fn delay_time_feedback_and_send_respond_to_conditions_with_bounded_targets() {
    let mut renderer = renderer();
    let quiet = renderer.snapshot().background;
    renderer.apply(Command::Market(MarketContext {
        activity: Unit::ONE,
        volume: Unit::ONE,
        volatility: Unit::ONE,
        ..MarketContext::default()
    }));
    let active = renderer.snapshot().background;
    assert!(active.detail_delay.send.target > quiet.detail_delay.send.target);
    assert!(active.detail_delay.feedback.target > quiet.detail_delay.feedback.target);
    assert_ne!(
        active.melody_delay.feedback.target,
        quiet.melody_delay.feedback.target
    );
    for mode in [Mode::Browse, Mode::Focus, Mode::Dialog] {
        renderer.apply(Command::Mode(mode));
        for value in [Unit::ZERO, Unit::ONE] {
            renderer.apply(Command::Market(MarketContext {
                zoom: value,
                activity: value,
                volume: value,
                volatility: value,
                ..MarketContext::default()
            }));
            let state = renderer.snapshot().background;
            for delay in [state.detail_delay, state.melody_delay] {
                assert!((100.0..=1_150.0).contains(&delay.time_ms.target));
                assert!((0.15..=0.65).contains(&delay.feedback.target));
                assert!((0.06..=0.6).contains(&delay.send.target));
            }
            advance(&mut renderer, RATE as usize / 4);
        }
    }
    let trace = serde_json::to_value(renderer.snapshot()).unwrap();
    assert!(trace["background"]["detail_delay"]["time_ms"]["target"].is_number());
    assert!(trace["background"]["melody_delay"]["feedback"]["current"].is_number());
}
