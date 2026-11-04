use super::*;

fn app() -> App {
    configured_app(RenderConfig::default())
}

fn configured_app(config: RenderConfig) -> App {
    App::new(
        config,
        OutputInfo {
            device: "Test output".into(),
            sample_rate: 48_000,
            channels: 2,
            sample_format: "f32".into(),
            buffer_size: shzx_sfx::BufferSize::SystemDefault,
        },
    )
}

fn key(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
    KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)
}

#[test]
fn accepted_controls_submit_typed_commands_and_failed_controls_preserve_state() {
    let mut app = app();
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    assert!(!app.activate(Control::SetMode(Mode::Dialog), &mut send));
    assert_eq!(app.mode, Mode::Dialog);
    assert!(!app.activate(Control::SetTrend(Trend::Bearish), &mut send));
    assert_eq!(app.market.trend, Trend::Bearish);
    assert!(!app.activate(Control::SetSpatial(SpatialMode::Pan), &mut send));
    assert_eq!(app.spatial, SpatialMode::Pan);
    assert_eq!(
        commands,
        [
            Command::Mode(Mode::Dialog),
            Command::Market(app.market),
            Command::Spatial(SpatialMode::Pan),
        ]
    );

    let before = (app.mode, app.market, app.spatial, app.mix);
    let mut reject = |_| Err(shzx_sfx::Error::QueueFull);
    app.activate(Control::SetMode(Mode::Focus), &mut reject);
    app.activate(Control::SetTrend(Trend::Bullish), &mut reject);
    app.activate(Control::SetSpatial(SpatialMode::Haas), &mut reject);
    app.activate(Control::Preset(Preset::DeepZoom), &mut reject);
    app.adjust(Control::Market(MarketField::Zoom), 0.2, &mut reject);
    app.adjust(Control::Mix(MixLayer::Master), 0.2, &mut reject);
    assert_eq!((app.mode, app.market, app.spatial, app.mix), before);
    assert!(app.notice.to_ascii_lowercase().contains("full"));
}

#[test]
fn effect_selection_is_silent_and_firing_uses_the_selected_pan_and_intensity() {
    let mut app = app();
    let mut commands = Vec::new();
    app.activate(Control::SelectEffect(SfxKind::OrderFill), &mut |command| {
        commands.push(command);
        Ok(())
    });
    assert_eq!(app.effect, SfxKind::OrderFill);
    assert!(commands.is_empty());

    app.event_pan = Pan::new(-0.7).unwrap();
    app.event_intensity = Unit::new(0.35).unwrap();
    app.activate(Control::Trigger, &mut |command| {
        commands.push(command);
        Ok(())
    });
    app.activate(Control::Fire(SfxKind::Click), &mut |command| {
        commands.push(command);
        Ok(())
    });
    assert_eq!(
        commands,
        [
            Command::Trigger(SfxEvent {
                edits: SfxEdits::default(),
                layers: shzx_sfx::SfxLayerSelection::All,
                kind: SfxKind::OrderFill,
                pan: app.event_pan,
                intensity: app.event_intensity,
            }),
            Command::Trigger(SfxEvent {
                edits: SfxEdits::default(),
                layers: shzx_sfx::SfxLayerSelection::All,
                kind: SfxKind::Click,
                pan: app.event_pan,
                intensity: app.event_intensity,
            }),
        ]
    );
}

#[test]
fn sliders_clamp_their_domains_and_local_audition_controls_send_no_audio_updates() {
    let mut app = app();
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    app.adjust(Control::Market(MarketField::Zoom), 10.0, &mut send);
    assert_eq!(app.market.zoom, Unit::ONE);
    app.adjust(Control::Market(MarketField::Zoom), -10.0, &mut send);
    assert_eq!(app.market.zoom, Unit::ZERO);
    app.adjust(Control::Mix(MixLayer::Harmony), 10.0, &mut send);
    assert_eq!(app.mix.harmony, Unit::ONE);
    app.adjust(Control::Mix(MixLayer::Harmony), -10.0, &mut send);
    assert_eq!(app.mix.harmony, Unit::ZERO);
    let submitted = commands.len();

    let mut local_send = |command| {
        commands.push(command);
        Ok(())
    };
    app.adjust(Control::EventPan, -10.0, &mut local_send);
    assert_eq!(app.event_pan.get(), -1.0);
    app.adjust(Control::EventPan, 10.0, &mut local_send);
    assert_eq!(app.event_pan.get(), 1.0);
    app.adjust(Control::EventIntensity, 10.0, &mut local_send);
    assert_eq!(app.event_intensity, Unit::ONE);
    app.adjust(Control::EventIntensity, -10.0, &mut local_send);
    assert_eq!(app.event_intensity, Unit::ZERO);
    assert_eq!(commands.len(), submitted);
}

#[test]
fn mute_restores_the_previous_level_and_is_recoverable_from_an_initial_zero() {
    let mut app = app();
    let initial = app.mix.master;
    let mut accept = |_| Ok(());
    app.activate(Control::ToggleMute, &mut accept);
    assert_eq!(app.mix.master, Unit::ZERO);
    app.activate(Control::ToggleMute, &mut accept);
    assert_eq!(app.mix.master, initial);

    let before = (app.mix.master, app.muted_level);
    app.activate(Control::ToggleMute, &mut |_| {
        Err(shzx_sfx::Error::QueueFull)
    });
    assert_eq!((app.mix.master, app.muted_level), before);

    let mut silent = configured_app(RenderConfig {
        mix: Mix {
            master: Unit::ZERO,
            ..Mix::default()
        },
        ..RenderConfig::default()
    });
    silent.activate(Control::ToggleMute, &mut accept);
    assert!(silent.mix.master.get() > 0.0);
    silent.activate(Control::ToggleMute, &mut accept);
    assert_eq!(silent.mix.master, Unit::ZERO);
}

#[test]
fn reset_restores_initial_controls_and_submits_each_relevant_state() {
    let mut app = configured_app(RenderConfig {
        mix: Mix {
            harmony: Unit::new(0.22).unwrap(),
            ..Mix::default()
        },
        spatial: SpatialMode::Pan,
        ..RenderConfig::default()
    });
    app.mix.master = Unit::ZERO;
    app.mix.harmony = Unit::ONE;
    app.market.zoom = Unit::ONE;
    app.market.trend = Trend::Bearish;
    app.mode = Mode::Dialog;
    app.spatial = SpatialMode::Haas;
    let mut commands = Vec::new();
    app.activate(Control::Reset, &mut |command| {
        commands.push(command);
        Ok(())
    });
    assert_eq!(app.mix, app.initial_mix);
    assert_eq!(app.spatial, app.initial_spatial);
    assert_eq!(app.market, MarketContext::default());
    assert_eq!(app.mode, Mode::Browse);
    for expected in [
        Command::Mix(app.initial_mix),
        Command::Spatial(app.initial_spatial),
        Command::Market(MarketContext::default()),
        Command::Mode(Mode::Browse),
        Command::Reverb(app.initial_reverb),
    ] {
        assert!(commands.contains(&expected), "missing command {expected:?}");
    }
}

#[test]
fn output_boost_and_independent_reverb_sends_obey_input_and_queue_contracts() {
    let mut app = app();
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    app.adjust(Control::OutputGain, 3.0, &mut send);
    assert_eq!(app.mix.output_gain.db(), 15.0);
    app.adjust(
        Control::Reverb(ReverbBus::Events, ReverbField::Send),
        0.1,
        &mut send,
    );
    assert!((app.reverb.events.send.get() - 0.22).abs() < 0.00001);
    assert_eq!(
        app.reverb.environment.send,
        app.initial_reverb.environment.send
    );
    assert!(
        commands
            .iter()
            .any(|command| matches!(command, Command::Reverb(_)))
    );
    let previous = (app.mix, app.reverb);
    let mut reject = |_| Err(shzx_sfx::Error::QueueFull);
    app.adjust(Control::OutputGain, 6.0, &mut reject);
    app.adjust(
        Control::Reverb(ReverbBus::Environment, ReverbField::Room),
        -0.2,
        &mut reject,
    );
    assert_eq!((app.mix, app.reverb), previous);
    app.select_tab(Tab::Mixer);
    app.focus = Control::OutputGain;
    app.key(key(KeyCode::Right, KeyEventKind::Press), &mut |_| Ok(()));
    assert_eq!(app.mix.output_gain.db(), 16.0);
    app.key(
        KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT),
        &mut |_| Ok(()),
    );
    assert_eq!(app.mix.output_gain.db(), 15.75);
    app.key(
        key(KeyCode::Char('6'), KeyEventKind::Press),
        &mut |_| Ok(()),
    );
    assert_eq!(app.tab, Tab::Reverb);
}

#[test]
fn a_partially_accepted_reset_keeps_the_state_of_each_accepted_command() {
    let mut app = configured_app(RenderConfig {
        spatial: SpatialMode::Pan,
        ..RenderConfig::default()
    });
    app.market.zoom = Unit::ONE;
    app.mode = Mode::Dialog;
    app.mix.master = Unit::ZERO;
    app.spatial = SpatialMode::Haas;
    app.muted_level = Some(Unit::new(0.3).unwrap());
    let mut accepted = 0;
    app.activate(Control::Reset, &mut |_| {
        if accepted < 2 {
            accepted += 1;
            Ok(())
        } else {
            Err(shzx_sfx::Error::QueueFull)
        }
    });
    assert_eq!(app.market, MarketContext::default());
    assert_eq!(app.mode, Mode::Browse);
    assert_eq!(app.mix.master, Unit::ZERO);
    assert_eq!(app.spatial, SpatialMode::Haas);
    assert_eq!(app.muted_level, Some(Unit::new(0.3).unwrap()));
    assert!(app.notice.to_ascii_lowercase().contains("full"));
}

#[test]
fn keyboard_release_and_repeat_never_duplicate_a_trigger_and_navigation_stays_silent() {
    let mut app = app();
    app.tab = Tab::Sfx;
    app.focus = Control::Trigger;
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    assert!(!app.key(key(KeyCode::Enter, KeyEventKind::Release), &mut send));
    assert!(!app.key(key(KeyCode::Enter, KeyEventKind::Repeat), &mut send));
    assert!(!app.key(key(KeyCode::Char('q'), KeyEventKind::Release), &mut send));
    assert!(!app.key(key(KeyCode::Enter, KeyEventKind::Press), &mut send));
    assert_eq!(commands.len(), 1);

    let mut navigate = |command| {
        commands.push(command);
        Ok(())
    };
    for (digit, expected) in [
        ('1', Tab::Scene),
        ('2', Tab::Market),
        ('3', Tab::Mixer),
        ('4', Tab::Sfx),
        ('5', Tab::Monitor),
    ] {
        assert!(!app.key(
            key(KeyCode::Char(digit), KeyEventKind::Press),
            &mut navigate
        ));
        assert_eq!(app.tab, expected);
    }
    let initial_focus = app.focus;
    app.key(key(KeyCode::Tab, KeyEventKind::Press), &mut navigate);
    assert_ne!(app.focus, initial_focus);
    app.key(key(KeyCode::BackTab, KeyEventKind::Press), &mut navigate);
    assert_eq!(app.focus, initial_focus);
    assert_eq!(commands.len(), 1);
}

#[test]
fn help_can_close_without_quitting_and_ctrl_c_always_quits() {
    let mut app = app();
    let mut accept = |_| Ok(());
    assert!(!app.key(key(KeyCode::Char('?'), KeyEventKind::Press), &mut accept));
    assert_eq!(app.overlay, Overlay::Help);
    assert!(!app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut accept));
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut accept));
    app.overlay = Overlay::Help;
    assert!(app.key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &mut accept
    ));
    assert!(app.key(key(KeyCode::Char('q'), KeyEventKind::Press), &mut accept));
}

#[test]
fn ticker_editor_validates_drafts_and_commits_only_after_queue_acceptance() {
    let mut app = app();
    let initial = app.symbol;
    let mut accepted = Vec::new();
    app.activate(Control::Symbol, &mut |_| Ok(()));
    assert_eq!(app.overlay, Overlay::SymbolEntry);
    app.key(key(KeyCode::Enter, KeyEventKind::Press), &mut |_| Ok(()));
    assert_eq!(app.overlay, Overlay::SymbolEntry);
    assert_eq!(app.symbol, initial);
    for character in "qqq".chars() {
        assert!(!app.key(
            key(KeyCode::Char(character), KeyEventKind::Press),
            &mut |_| Ok(())
        ));
    }
    assert_eq!(app.symbol_input, "QQQ");
    app.activate(Control::CommitSymbol, &mut |_| {
        Err(shzx_sfx::Error::QueueFull)
    });
    assert_eq!(app.overlay, Overlay::SymbolEntry);
    assert_eq!(app.symbol, initial);
    app.activate(Control::CommitSymbol, &mut |command| {
        accepted.push(command);
        Ok(())
    });
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(app.symbol.as_str(), "QQQ");
    assert_eq!(accepted, [Command::Symbol(Symbol::new("QQQ").unwrap())]);
    app.activate(Control::Symbol, &mut |_| Ok(()));
    app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut |_| Ok(()));
    assert_eq!(app.symbol.as_str(), "QQQ");
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn clicking_an_effect_auditions_each_click_with_the_selected_parameters() {
    let mut app = app();
    app.event_pan = Pan::new(0.7).unwrap();
    app.event_intensity = Unit::new(0.4).unwrap();
    let mut commands = Vec::new();
    for _ in 0..2 {
        assert!(
            !app.click(Control::SelectEffect(SfxKind::OrderFill), &mut |command| {
                commands.push(command);
                Ok(())
            })
        );
    }
    let event = SfxEvent {
        edits: SfxEdits::default(),
        layers: shzx_sfx::SfxLayerSelection::All,
        kind: SfxKind::OrderFill,
        pan: app.event_pan,
        intensity: app.event_intensity,
    };
    assert_eq!(commands, [Command::Trigger(event); 2]);
    assert_eq!(app.effect, SfxKind::OrderFill);
}

#[test]
fn inspector_navigation_is_silent_and_solo_buttons_keep_original_event_parameters() {
    let mut app = app();
    app.event_pan = Pan::new(-0.4).unwrap();
    app.event_intensity = Unit::new(0.7).unwrap();
    let mut commands = Vec::new();
    app.key(
        key(KeyCode::Char('i'), KeyEventKind::Press),
        &mut |command| {
            commands.push(command);
            Ok(())
        },
    );
    assert_eq!(app.tab, Tab::Inspect);
    app.key(key(KeyCode::Tab, KeyEventKind::Press), &mut |command| {
        commands.push(command);
        Ok(())
    });
    assert!(commands.is_empty());
    assert_eq!(app.inspector_layer, SfxLayer::Body);
    app.key(key(KeyCode::Enter, KeyEventKind::Press), &mut |command| {
        commands.push(command);
        Ok(())
    });
    let Command::Trigger(event) = commands[0] else {
        panic!("solo should enqueue an event")
    };
    assert_eq!(event.layers, SfxLayerSelection::Only(SfxLayer::Body));
    assert_eq!(event.pan, app.event_pan);
    assert_eq!(event.intensity, app.event_intensity);
    app.click(Control::AuditionLayer(SfxLayer::Texture), &mut |command| {
        commands.push(command);
        Ok(())
    });
    let Command::Trigger(event) = commands[1] else {
        panic!("layer click should enqueue an event")
    };
    assert_eq!(event.layers, SfxLayerSelection::Only(SfxLayer::Texture));
    app.key(
        key(KeyCode::PageDown, KeyEventKind::Press),
        &mut |command| {
            commands.push(command);
            Ok(())
        },
    );
    assert_eq!(commands.len(), 2);
    assert_eq!(app.focus, Control::AuditionLayer(SfxLayer::Texture));
    app.click(Control::AuditionAll, &mut |command| {
        commands.push(command);
        Ok(())
    });
    let Command::Trigger(event) = commands[2] else {
        panic!("play all should enqueue an event")
    };
    assert_eq!(event.layers, SfxLayerSelection::All);
}

#[test]
fn inspector_edits_and_mutes_follow_each_effect_through_mix_and_solo_auditions() {
    let mut app = app();
    app.select_tab(Tab::Sfx);
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    app.activate(Control::EditLayer(SfxLayer::Texture), &mut send);
    app.adjust(Control::SfxParameter(SfxParameter::Level), 0.1, &mut send);
    app.adjust(
        Control::SfxParameter(SfxParameter::Highpass),
        100.0,
        &mut send,
    );
    app.adjust(Control::SfxParameter(SfxParameter::Pitch), 0.5, &mut send);
    app.adjust(
        Control::SfxParameter(SfxParameter::Duration),
        0.25,
        &mut send,
    );
    let original_edits = app.current_edits();
    let level = app
        .numeric(Control::SfxParameter(SfxParameter::Level))
        .unwrap()
        .0;
    app.activate(Control::MuteLayer(SfxLayer::Texture), &mut send);
    assert_eq!(
        app.current_edits().layer(SfxLayer::Texture).state,
        SfxLayerState::Muted
    );
    assert_eq!(
        app.numeric(Control::SfxParameter(SfxParameter::Level))
            .unwrap()
            .0,
        level
    );
    let muted_edits = app.current_edits();
    assert!(!app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut send));
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(app.focus, Control::EditLayer(SfxLayer::Texture));
    app.select_effect(1);
    assert_eq!(app.current_edits(), SfxEdits::default());
    app.select_effect(-1);
    assert_eq!(app.current_edits(), muted_edits);
    assert!(commands.is_empty());

    app.activate(Control::AuditionAll, &mut |command| {
        commands.push(command);
        Ok(())
    });
    app.activate(Control::AuditionLayer(SfxLayer::Tonal), &mut |command| {
        commands.push(command);
        Ok(())
    });
    for (command, selection) in commands.into_iter().zip([
        SfxLayerSelection::All,
        SfxLayerSelection::Only(SfxLayer::Tonal),
    ]) {
        let Command::Trigger(event) = command else {
            panic!("audition must trigger an event")
        };
        assert_eq!(event.edits, muted_edits);
        assert_eq!(event.layers, selection);
    }
    app.activate(Control::MuteLayer(SfxLayer::Texture), &mut |_| Ok(()));
    assert_eq!(app.current_edits(), original_edits);
}

#[test]
fn editor_traps_focus_bounds_filter_bands_and_resets_only_the_current_effect() {
    let mut app = app();
    let mut send =
        |_| -> shzx_sfx::Result<()> { panic!("editing and navigation must stay silent") };
    app.select_tab(Tab::Inspect);
    app.activate(Control::EditLayer(SfxLayer::Shimmer), &mut send);
    app.key(key(KeyCode::BackTab, KeyEventKind::Press), &mut send);
    assert_eq!(app.focus, Control::CloseSfxEditor);
    app.key(key(KeyCode::Tab, KeyEventKind::Press), &mut send);
    assert_eq!(app.focus, Control::SfxParameter(SfxParameter::Level));
    app.key(key(KeyCode::Char('1'), KeyEventKind::Press), &mut send);
    assert_eq!(app.tab, Tab::Inspect);
    app.focus = Control::SfxParameter(SfxParameter::Highpass);
    app.key(key(KeyCode::End, KeyEventKind::Press), &mut send);
    app.adjust(
        Control::SfxParameter(SfxParameter::Lowpass),
        -100_000.0,
        &mut send,
    );
    let band = app.current_edits().layer(SfxLayer::Shimmer).filter.unwrap();
    assert!(band.highpass_hz() < band.lowpass_hz());
    app.adjust(Control::SfxParameter(SfxParameter::Pitch), -10.0, &mut send);
    assert_eq!(app.current_edits().pitch.get(), SfxScale::MIN);
    app.adjust(
        Control::SfxParameter(SfxParameter::Duration),
        10.0,
        &mut send,
    );
    assert_eq!(app.current_edits().duration.get(), SfxScale::MAX);
    app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut send);
    let first_effect = app.effect;
    let first_edits = app.current_edits();
    app.select_effect(1);
    app.activate(Control::MuteLayer(SfxLayer::Body), &mut send);
    app.activate(Control::ResetEffect, &mut send);
    assert_eq!(app.current_edits(), SfxEdits::default());
    assert_eq!(app.effect_edits[&first_effect], first_edits);
}

#[test]
fn every_recipe_can_be_edited_to_slider_limits_without_inverting_filter_bands() {
    let mut app = app();
    for effect in SfxKind::iter() {
        app.effect = effect;
        for layer in SfxLayer::ALL {
            app.inspector_layer = layer;
            let parameters = EditorPage::ALL
                .into_iter()
                .filter(|&page| page == EditorPage::Mix || layer == SfxLayer::Tonal)
                .flat_map(EditorPage::parameters);
            for parameter in parameters {
                let control = Control::SfxParameter(parameter);
                for delta in [100_000.0, -100_000.0] {
                    app.adjust(control, delta, &mut |_| panic!("edits must stay local"));
                    let (value, low, high) = app.numeric(control).unwrap();
                    assert!((low..=high).contains(&value));
                }
            }
        }
    }
}

#[test]
fn tonal_editor_keeps_the_event_sequence_and_auditions_its_construction_settings() {
    let mut app = app();
    app.select_tab(Tab::Sfx);
    let mut commands = Vec::new();
    let mut send = |command| {
        commands.push(command);
        Ok(())
    };
    app.activate(Control::EditLayer(SfxLayer::Tonal), &mut send);
    assert_eq!(app.overlay, Overlay::SfxEditor(EditorPage::Tone));
    assert_eq!(
        app.focus,
        Control::SfxParameter(SfxParameter::Tonal(TonalParameter::Carrier))
    );
    assert!(app.current_edits().tonal.step_rate.is_none());
    app.adjust(
        Control::SfxParameter(SfxParameter::Tonal(TonalParameter::FmIndex)),
        0.2,
        &mut send,
    );
    app.adjust(
        Control::SfxParameter(SfxParameter::Tonal(TonalParameter::StepRate)),
        50.0,
        &mut send,
    );
    let original_ratios = app.edited_recipe().tonal_pitch_ratios;
    app.key(key(KeyCode::PageDown, KeyEventKind::Press), &mut send);
    assert_eq!(app.overlay, Overlay::SfxEditor(EditorPage::Sequence));
    app.adjust(
        Control::SfxParameter(SfxParameter::Tonal(TonalParameter::StepPitch(
            SfxTonalStep::Three,
        ))),
        0.25,
        &mut send,
    );
    let edited_ratios = app.edited_recipe().tonal_pitch_ratios;
    for (step, (original, edited)) in SfxTonalStep::ALL
        .into_iter()
        .zip(original_ratios.into_iter().zip(edited_ratios))
    {
        assert_eq!(
            edited,
            if step == SfxTonalStep::Three {
                original + 0.25
            } else {
                original
            }
        );
    }
    let edits = app.current_edits();
    app.key(key(KeyCode::Esc, KeyEventKind::Press), &mut send);
    app.select_effect(1);
    assert_eq!(app.current_edits().tonal, SfxTonalEdits::DEFAULT);
    app.select_effect(-1);
    app.activate(Control::EditLayer(SfxLayer::Tonal), &mut send);
    assert_eq!(app.current_edits(), edits);
    assert!(commands.is_empty());
    app.key(
        key(KeyCode::Char(' '), KeyEventKind::Press),
        &mut |command| {
            commands.push(command);
            Ok(())
        },
    );
    app.activate(Control::AuditionAll, &mut |command| {
        commands.push(command);
        Ok(())
    });
    for (command, layers) in commands.into_iter().zip([
        SfxLayerSelection::Only(SfxLayer::Tonal),
        SfxLayerSelection::All,
    ]) {
        let Command::Trigger(event) = command else {
            panic!("audition must trigger a sound")
        };
        assert_eq!(event.edits, edits);
        assert_eq!(event.layers, layers);
    }
}

#[test]
fn resetting_tonal_construction_keeps_layer_mix_and_other_event_settings() {
    let mut app = app();
    let mut send = |_| -> shzx_sfx::Result<()> { panic!("editing must stay silent") };
    app.select_tab(Tab::Inspect);
    app.activate(Control::EditLayer(SfxLayer::Tonal), &mut send);
    app.adjust(Control::SfxParameter(SfxParameter::Level), 0.1, &mut send);
    app.adjust(
        Control::SfxParameter(SfxParameter::Highpass),
        100.0,
        &mut send,
    );
    app.adjust(Control::SfxParameter(SfxParameter::Pitch), 0.5, &mut send);
    app.adjust(
        Control::SfxParameter(SfxParameter::Tonal(TonalParameter::FmIndex)),
        0.2,
        &mut send,
    );
    app.activate(Control::MuteLayer(SfxLayer::Tonal), &mut send);
    let before = app.current_edits();
    app.key(key(KeyCode::Char('r'), KeyEventKind::Press), &mut send);
    assert_eq!(app.current_edits().tonal, SfxTonalEdits::DEFAULT);
    assert_eq!(
        app.current_edits().layer(SfxLayer::Tonal),
        before.layer(SfxLayer::Tonal)
    );
    assert_eq!(app.current_edits().pitch, before.pitch);
    app.key(key(KeyCode::PageUp, KeyEventKind::Press), &mut send);
    assert_eq!(app.overlay, Overlay::SfxEditor(EditorPage::Mix));
    app.key(key(KeyCode::Char('r'), KeyEventKind::Press), &mut send);
    assert_eq!(app.current_edits(), SfxEdits::DEFAULT);
}
