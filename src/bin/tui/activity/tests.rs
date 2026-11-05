use std::collections::{BTreeMap, BTreeSet, VecDeque};

use shzx_sfx::SfxEvent;
use strum::IntoEnumIterator;

use super::*;

fn accept_next(activity: &mut UserActivity) -> (Duration, Action, Workflow) {
    let at = activity.running.as_ref().unwrap().next_at;
    let mut events = Vec::new();
    activity
        .advance(at, &mut |event| {
            events.push(event);
            Ok(())
        })
        .unwrap();
    assert_eq!(events.len(), 1);
    let running = activity.running.as_ref().unwrap();
    assert!(running.next_at > at);
    (at, events[0], running.workflow)
}

fn action_trace(seed: u64, count: usize) -> Vec<(Duration, Action, Workflow)> {
    let mut activity = UserActivity::new(seed);
    (0..count).map(|_| accept_next(&mut activity)).collect()
}

fn trace(seed: u64, count: usize) -> Vec<(Duration, SfxEvent, Workflow)> {
    action_trace(seed, count)
        .into_iter()
        .filter_map(|(at, action, workflow)| {
            let (kind, pan, intensity) = match action {
                Action::Trigger {
                    kind,
                    pan,
                    intensity,
                } => (kind, pan, intensity),
                Action::Ui {
                    event,
                    pan,
                    intensity,
                } => (event.sound()?, pan, intensity),
                _ => return None,
            };
            Some((
                at,
                SfxEvent {
                    pan,
                    intensity,
                    ..SfxEvent::new(kind)
                },
                workflow,
            ))
        })
        .collect()
}

#[test]
fn seeded_workflows_cover_the_palette_with_frequent_hover_and_rare_coherent_orders() {
    let events = trace(1234, 5_000);
    assert_eq!(events, trace(1234, 5_000));
    assert_ne!(events, trace(5678, 5_000));
    assert_eq!(
        events
            .iter()
            .map(|(_, event, _)| event.kind)
            .collect::<BTreeSet<_>>(),
        SfxKind::iter().collect()
    );

    let mut pending_order = None;
    let mut last_resolution = Duration::ZERO;
    let mut outcomes = BTreeSet::new();
    let mut order_events = 0;
    let mut hover_events = 0;
    let mut scroll_events = 0;
    for (at, event, _) in &events {
        match event.kind {
            SfxKind::OrderCreate => {
                assert!(pending_order.replace(*at).is_none());
                assert!(*at - last_resolution >= Duration::from_secs(45));
                order_events += 1;
            }
            SfxKind::OrderFill | SfxKind::OrderCancel => {
                let created = pending_order.take().expect("orders must be created first");
                assert!(*at > created);
                last_resolution = *at;
                outcomes.insert(event.kind);
                order_events += 1;
            }
            SfxKind::HoverEnter | SfxKind::HoverLeave => hover_events += 1,
            SfxKind::Scroll => scroll_events += 1,
            _ => {}
        }
    }
    assert_eq!(
        outcomes,
        BTreeSet::from([SfxKind::OrderFill, SfxKind::OrderCancel])
    );
    assert!(order_events > 10);
    assert!(
        order_events * 20 < events.len(),
        "orders exceed 5% of events"
    );
    assert!(
        hover_events * 2 > events.len(),
        "hover should dominate pointer activity"
    );
    assert!(
        scroll_events * 10 > events.len(),
        "wheel activity should exceed 10% of events"
    );

    let intervals = events
        .windows(2)
        .map(|pair| pair[1].0 - pair[0].0)
        .collect::<BTreeSet<_>>();
    assert!(intervals.len() > 100);
    assert!(intervals.first().unwrap() >= &Duration::from_millis(100));
    assert!(intervals.last().unwrap() > &Duration::from_secs(2));
    assert!(
        events
            .iter()
            .map(|(_, event, _)| event.intensity.get().to_bits())
            .collect::<BTreeSet<_>>()
            .len()
            > 100
    );
}

#[test]
fn virtual_targets_place_pointer_actions_and_drag_motion_across_the_stereo_field() {
    let events = trace(1234, 5_000);
    let drag = events
        .iter()
        .filter(|(_, event, workflow)| {
            *workflow == Workflow::Routine(Routine::Rearrange)
                && matches!(
                    event.kind,
                    SfxKind::DragStart | SfxKind::DragOver | SfxKind::Drop
                )
        })
        .take(4)
        .map(|(_, event, _)| event.pan.get())
        .collect::<Vec<_>>();
    assert_eq!(drag.len(), 4);
    assert!(drag[0] < -0.55, "drag starts on the left watchlist");
    assert!(drag[1].abs() < 0.11, "drag crosses the center chart");
    assert!(drag[2] > 0.55, "drag crosses the right order ticket");
    assert!(
        drag[3].abs() < 0.11,
        "drop lands in the center positions panel"
    );

    let mut anchored_actions = 0;
    let mut scrollbar_actions = 0;
    for pair in events.windows(2) {
        let previous = pair[0].1;
        let event = pair[1].1;
        if previous.kind == SfxKind::HoverEnter
            && !matches!(
                event.kind,
                SfxKind::HoverLeave
                    | SfxKind::Notification
                    | SfxKind::Warning
                    | SfxKind::Error
                    | SfxKind::Connect
                    | SfxKind::Disconnect
                    | SfxKind::DataRefresh
                    | SfxKind::OrderFill
            )
        {
            assert!(
                (previous.pan.get() - event.pan.get()).abs() < 0.201,
                "the first action stays within its hovered target"
            );
            anchored_actions += 1;
        }
        if matches!(
            event.kind,
            SfxKind::ScrollbarGrab | SfxKind::ScrollbarMove | SfxKind::ScrollbarRelease
        ) {
            assert!(
                event.pan.get() < -0.55,
                "scrollbar stays in the left watchlist"
            );
            scrollbar_actions += 1;
        }
        if event.kind == SfxKind::Scroll {
            assert!(
                event.pan.get() < -0.55 || event.pan.get().abs() < 0.11,
                "wheel activity stays in the watchlist, chart, positions, or order book"
            );
        }
        if previous.kind == SfxKind::HoverLeave {
            assert_eq!(
                event.kind,
                SfxKind::HoverEnter,
                "pointer travel leaves before entering"
            );
        }
    }
    assert!(anchored_actions > 50 && scrollbar_actions > 10);
}

#[test]
fn seeded_activity_uses_every_thought_between_related_actions_without_repetition() {
    let actions = action_trace(1234, 40_000);
    let mut events = BTreeSet::new();
    let mut thoughts = BTreeSet::new();
    let mut preceding = None;
    let mut pending = Vec::new();
    let mut last_used = BTreeMap::new();
    let mut recent = VecDeque::new();
    let mut real_events = 0;
    let mut consecutive = false;
    for (at, action, _) in actions {
        match action {
            Action::Ui { event, .. } => {
                events.insert(event);
                if let Some((previous_at, previous)) = preceding {
                    consecutive |= pending.len() > 1;
                    for (thought_at, thought) in pending.drain(..) {
                        let thought: &Thought = thought;
                        assert!(
                            thought.fits(previous, event, at - previous_at),
                            "{}",
                            thought.id
                        );
                        assert!(
                            thought_at > previous_at && thought_at < at,
                            "{}",
                            thought.id
                        );
                    }
                }
                preceding = Some((at, event));
                real_events += 1;
            }
            Action::Thought(thought) => {
                assert!(preceding.is_some(), "thoughts need a preceding UI action");
                assert!(!recent.contains(&thought.id.as_str()), "{}", thought.id);
                if let Some(last) = last_used.insert(thought.id.as_str(), real_events) {
                    assert!(
                        real_events - last >= thought.cooldown_events(),
                        "{}",
                        thought.id
                    );
                }
                recent.push_back(thought.id.as_str());
                if recent.len() > thoughts::library().dedupe_window() {
                    recent.pop_front();
                }
                thoughts.insert(thought.id.as_str());
                pending.push((at, thought));
            }
            _ => {}
        }
    }
    assert_eq!(events, UiEvent::iter().collect());
    let expected = thoughts::library()
        .thoughts
        .iter()
        .map(|thought| thought.id.as_str())
        .collect();
    assert_eq!(thoughts, expected);
    assert!(
        consecutive,
        "some pauses should contain consecutive thoughts"
    );
}

#[test]
fn activity_starts_enabled_and_can_pause_resume_and_reset() {
    let mut activity = UserActivity::new(1234);
    let later = Duration::from_secs(36_000);
    assert!(activity.enabled());
    activity.toggle();
    assert!(!activity.enabled());
    assert_eq!(activity.description(), "Paused");
    activity
        .advance(later, &mut |_| panic!("disabled activity is silent"))
        .unwrap();
    activity.toggle();
    assert!(activity.enabled());
    let first_at = activity.running.as_ref().unwrap().next_at;
    assert!(first_at > later);
    activity
        .advance(first_at - Duration::from_nanos(1), &mut |_| {
            panic!("deadline has not arrived")
        })
        .unwrap();
    accept_next(&mut activity);
    activity.toggle();
    assert!(!activity.enabled());
    activity
        .advance(later * 2, &mut |_| {
            panic!("toggling off stops queued activity")
        })
        .unwrap();
    activity.toggle();
    assert!(activity.running.as_ref().unwrap().next_at > later * 2);
    activity.toggle();
    activity
        .advance(later * 3, &mut |_| panic!("paused activity stays silent"))
        .unwrap();
    activity.reset();
    assert!(activity.enabled());
    assert!(activity.running.as_ref().unwrap().next_at > later * 3);
    accept_next(&mut activity);
}

#[test]
fn queue_rejection_retries_the_same_action_without_replaying_an_accepted_mode() {
    let mut activity = UserActivity::new(1234);
    let (_, accepted_mode, _) = accept_next(&mut activity);
    assert_eq!(accepted_mode, Action::Mode(Mode::Browse));
    let due = activity.running.as_ref().unwrap().next_at;
    let mut rejected = None;
    let result = activity.advance(due, &mut |event| {
        rejected = Some(event);
        Err(shzx_sfx::Error::QueueFull)
    });
    assert!(matches!(result, Err(shzx_sfx::Error::QueueFull)));
    let queued = activity.running.as_ref().unwrap().events.len();
    let retry_at = due + Duration::from_secs(1);
    let mut accepted = None;
    activity
        .advance(retry_at, &mut |event| {
            accepted = Some(event);
            Ok(())
        })
        .unwrap();
    assert_eq!(accepted, rejected);
    assert!(matches!(accepted, Some(Action::Trigger { .. })));
    let running = activity.running.as_ref().unwrap();
    assert_eq!(running.events.len(), queued - 1);
    assert!(running.next_at > retry_at);
    activity
        .advance(retry_at, &mut |_| {
            panic!("accepted events are not duplicated")
        })
        .unwrap();
}

#[test]
fn suspension_advances_one_gesture_and_rebases_the_next_deadline() {
    let mut activity = UserActivity::new(1234);
    accept_next(&mut activity);
    let resumed = Duration::from_secs(86_400);
    let mut sent = Vec::new();
    activity
        .advance(resumed, &mut |event| {
            sent.push(event);
            Ok(())
        })
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert!(activity.running.as_ref().unwrap().next_at > resumed);
    activity
        .advance(resumed, &mut |_| {
            panic!("missed events must not catch up in a burst")
        })
        .unwrap();
}

#[test]
fn workflow_state_actions_match_dialog_focus_and_chart_lifecycles() {
    let actions = action_trace(1234, 5_000);
    assert_eq!(actions, action_trace(1234, 5_000));
    let mut mode = Mode::Browse;
    let mut entered_at = Duration::ZERO;
    let mut modes = Vec::new();
    let mut deep_zoom_at = None;
    let mut zoom_transitions = 0;
    let mut dismissed_at = None;
    for (at, action, workflow) in actions {
        if let Some(dismissed_at) = dismissed_at.take() {
            assert_eq!(action, Action::Mode(Mode::Browse));
            assert!(
                at - dismissed_at <= Duration::from_millis(100),
                "returning to the bunker took {:?} after popup dismissal",
                at - dismissed_at
            );
        }
        match action {
            Action::Mode(next) => {
                if next != mode {
                    if matches!(mode, Mode::Dialog | Mode::Focus) {
                        assert!(at - entered_at >= Duration::from_secs(4));
                    }
                    entered_at = at;
                    mode = next;
                }
                if !modes.contains(&mode) {
                    modes.push(mode);
                }
            }
            Action::Trigger { kind, .. } => match kind {
                SfxKind::DialogOpen | SfxKind::DialogClose => assert_eq!(mode, Mode::Dialog),
                SfxKind::Focus | SfxKind::Blur => assert_eq!(
                    mode,
                    if workflow == Workflow::Routine(Routine::Dialog) {
                        Mode::Dialog
                    } else {
                        Mode::Focus
                    }
                ),
                SfxKind::ZoomIn | SfxKind::ZoomOut => assert_eq!(mode, Mode::Focus),
                _ => {}
            },
            Action::Ui { event, .. } => match event {
                UiEvent::PopupOpen | UiEvent::PopupRead | UiEvent::PopupDismiss => {
                    assert_eq!(mode, Mode::Dialog);
                    if event == UiEvent::PopupDismiss {
                        dismissed_at = Some(at);
                    }
                }
                UiEvent::ChartZoomIn | UiEvent::ChartZoomOut => {
                    assert_eq!(mode, Mode::Focus);
                }
                _ => {}
            },
            Action::Zoom(zoom) => {
                assert_eq!(workflow, Workflow::Routine(Routine::Chart));
                if zoom.get() >= 0.9 {
                    assert!((0.9..=0.98).contains(&zoom.get()));
                    assert!(deep_zoom_at.replace(at).is_none());
                } else if zoom.get() <= 0.2 {
                    let entered = deep_zoom_at.take().expect("overview follows deep zoom");
                    assert!(at - entered >= Duration::from_secs(4));
                    zoom_transitions += 1;
                } else {
                    assert!((0.4..=0.6).contains(&zoom.get()));
                    assert!(deep_zoom_at.is_none());
                }
            }
            Action::Market(_) | Action::Thought(_) => {}
        }
    }
    assert_eq!(modes.len(), 3);
    assert!(zoom_transitions > 5);
}

#[test]
fn market_changes_cover_every_regime_once_per_shuffled_cycle_and_rebase_after_acceptance() {
    let actions = action_trace(1234, 5_000);
    let changes: Vec<_> = actions
        .iter()
        .filter_map(|(at, action, _)| match action {
            Action::Market(preset) => Some((*at, *preset)),
            _ => None,
        })
        .collect();
    assert!(changes.len() >= 12);
    for cycle in changes.as_chunks::<4>().0 {
        for preset in [
            Preset::Idle,
            Preset::Bullish,
            Preset::Bearish,
            Preset::Volatile,
        ] {
            assert_eq!(
                cycle.iter().filter(|(_, value)| *value == preset).count(),
                1
            );
        }
    }
    for pair in changes.windows(2) {
        assert_ne!(pair[0].1, pair[1].1);
        assert!(pair[1].0 - pair[0].0 >= Duration::from_secs(25));
    }

    let mut activity = UserActivity::new(1234);
    let due = activity.running.as_ref().unwrap().next_at;
    activity.running.as_mut().unwrap().next_market_at = due;
    assert_eq!(accept_next(&mut activity).1, Action::Mode(Mode::Browse));
    let next_at = activity.running.as_ref().unwrap().next_at;
    let mut rejected = None;
    activity
        .advance(next_at, &mut |action| {
            rejected = Some(action);
            Err(shzx_sfx::Error::QueueFull)
        })
        .unwrap_err();
    assert!(matches!(rejected, Some(Action::Market(_))));
    assert_eq!(activity.running.as_ref().unwrap().next_market_at, due);
    let retried_at = next_at + Duration::from_secs(100);
    let mut accepted = None;
    activity
        .advance(retried_at, &mut |action| {
            accepted = Some(action);
            Ok(())
        })
        .unwrap();
    assert_eq!(accepted, rejected);
    assert!(
        (Duration::from_secs(25)..=Duration::from_secs(60))
            .contains(&(activity.running.as_ref().unwrap().next_market_at - retried_at))
    );
}
