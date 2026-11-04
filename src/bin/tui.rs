mod view;

use std::{
    io::{self, IsTerminal, Write},
    time::{Duration, Instant},
};

use ratatui::crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use shzx_sfx::{
    AudioEngine, AudioStatus, CallbackStats, Command, MarketContext, Mix, Mode, OutputGain,
    OutputInfo, Pan, RenderConfig, ReverbSettings, SfxEvent, SfxKind, Snapshot, SpatialMode,
    Symbol, Trend, Unit,
};
use shzx_sfx::{SfxLayer, SfxLayerSelection};
use strum::IntoEnumIterator;

use super::{CliResult, MixLayer, ReverbBus, ReverbField, invalid};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Scene,
    Market,
    Mixer,
    Sfx,
    Monitor,
    Reverb,
    Inspect,
}

impl Tab {
    const ALL: [Self; 7] = [
        Self::Scene,
        Self::Market,
        Self::Mixer,
        Self::Sfx,
        Self::Monitor,
        Self::Reverb,
        Self::Inspect,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Scene => "Scene",
            Self::Market => "Market",
            Self::Mixer => "Mixer",
            Self::Sfx => "SFX",
            Self::Monitor => "Monitor",
            Self::Reverb => "Reverb",
            Self::Inspect => "Inspect",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarketField {
    Strength,
    Volume,
    Volatility,
    Zoom,
    Activity,
}

impl MarketField {
    const ALL: [Self; 5] = [
        Self::Strength,
        Self::Volume,
        Self::Volatility,
        Self::Zoom,
        Self::Activity,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Strength => "Trend strength",
            Self::Volume => "Volume",
            Self::Volatility => "Volatility",
            Self::Zoom => "Zoom depth",
            Self::Activity => "UI activity",
        }
    }
    fn get(self, market: MarketContext) -> Unit {
        match self {
            Self::Strength => market.trend_strength,
            Self::Volume => market.volume,
            Self::Volatility => market.volatility,
            Self::Zoom => market.zoom,
            Self::Activity => market.activity,
        }
    }
    fn set(self, market: &mut MarketContext, value: Unit) {
        *match self {
            Self::Strength => &mut market.trend_strength,
            Self::Volume => &mut market.volume,
            Self::Volatility => &mut market.volatility,
            Self::Zoom => &mut market.zoom,
            Self::Activity => &mut market.activity,
        } = value;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preset {
    Idle,
    Bullish,
    Bearish,
    Volatile,
    DeepZoom,
}

impl Preset {
    const ALL: [Self; 5] = [
        Self::Idle,
        Self::Bullish,
        Self::Bearish,
        Self::Volatile,
        Self::DeepZoom,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle bunker",
            Self::Bullish => "Bullish",
            Self::Bearish => "Bearish",
            Self::Volatile => "Volatile",
            Self::DeepZoom => "Deep zoom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Inspect,
    AuditionLayer(SfxLayer),
    AuditionAll,
    Symbol,
    CommitSymbol,
    CancelSymbol,
    Tab(Tab),
    SetMode(Mode),
    SetTrend(Trend),
    Market(MarketField),
    Mix(MixLayer),
    OutputGain,
    Reverb(ReverbBus, ReverbField),
    SetSpatial(SpatialMode),
    Preset(Preset),
    SelectEffect(SfxKind),
    EventPan,
    EventIntensity,
    Trigger,
    Fire(SfxKind),
    ToggleMute,
    Reset,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    SymbolEntry,
}

struct App {
    symbol: Symbol,
    initial_symbol: Symbol,
    symbol_input: String,
    tab: Tab,
    focus: Control,
    market: MarketContext,
    mode: Mode,
    mix: Mix,
    reverb: ReverbSettings,
    spatial: SpatialMode,
    effect: SfxKind,
    inspector_layer: SfxLayer,
    event_pan: Pan,
    event_intensity: Unit,
    snapshot: Option<Snapshot>,
    output: OutputInfo,
    callback_stats: CallbackStats,
    status: AudioStatus,
    notice: String,
    max_voices: usize,
    overlay: Overlay,
    initial_mix: Mix,
    initial_reverb: ReverbSettings,
    initial_spatial: SpatialMode,
    muted_level: Option<Unit>,
}

impl App {
    fn new(config: RenderConfig, output: OutputInfo) -> Self {
        Self {
            symbol: config.symbol,
            initial_symbol: config.symbol,
            symbol_input: String::new(),
            tab: Tab::Scene,
            focus: Control::SetMode(Mode::Browse),
            market: MarketContext::default(),
            mode: Mode::Browse,
            mix: config.mix,
            reverb: config.reverb,
            spatial: config.spatial,
            effect: SfxKind::Click,
            inspector_layer: SfxLayer::Texture,
            event_pan: Pan::CENTER,
            event_intensity: Unit::new(0.6).expect("constant intensity is normalized"),
            snapshot: None,
            output,
            callback_stats: CallbackStats::default(),
            status: AudioStatus::Running,
            notice: "Choose a scene, adjust the market, or audition an effect. Press ? for help."
                .into(),
            max_voices: config.max_voices,
            overlay: Overlay::None,
            initial_mix: config.mix,
            initial_reverb: config.reverb,
            initial_spatial: config.spatial,
            muted_level: None,
        }
    }

    fn submit(
        &mut self,
        command: Command,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        match send(command) {
            Ok(()) => {
                match command {
                    Command::Market(market) => self.market = market,
                    Command::Mode(mode) => self.mode = mode,
                    Command::Mix(mix) => self.mix = mix,
                    Command::Symbol(symbol) => self.symbol = symbol,
                    Command::Reverb(reverb) => self.reverb = reverb,
                    Command::Spatial(spatial) => self.spatial = spatial,
                    Command::Trigger(event) => self.effect = event.kind,
                }
                self.notice = match command {
                    Command::Trigger(event) => format!(
                        "Playing {} [{}] | pan {:+.2} | intensity {:.0}%",
                        event.kind,
                        match event.layers {
                            SfxLayerSelection::All => "all layers",
                            SfxLayerSelection::Only(layer) => layer.label(),
                        },
                        event.pan.get(),
                        event.intensity.get() * 100.0
                    ),
                    Command::Mode(mode) => {
                        format!("Scene: {mode}. Environment transitions smoothly.")
                    }
                    Command::Market(_) => {
                        "Visible market updated. Harmony follows its musical clock.".into()
                    }
                    Command::Mix(_) => "Layer levels updated.".into(),
                    Command::Symbol(symbol) => format!(
                        "Symbol {symbol}: harmonic base crossfades within the shared scale."
                    ),
                    Command::Reverb(_) => {
                        "Reverb updated. The existing tail continues through the transition.".into()
                    }
                    Command::Spatial(spatial) => format!("Spatial processing: {spatial}"),
                };
                true
            }
            Err(error) => {
                self.notice = format!("Not applied: {error}");
                false
            }
        }
    }

    fn select_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.focus = self.focus_order()[0];
    }

    fn focus_order(&self) -> Vec<Control> {
        let mut controls = match self.tab {
            Tab::Scene => [Mode::Browse, Mode::Dialog, Mode::Focus]
                .map(Control::SetMode)
                .into_iter()
                .chain(Preset::ALL.map(Control::Preset))
                .chain([SpatialMode::Pan, SpatialMode::Haas].map(Control::SetSpatial))
                .chain(
                    [SfxKind::OrderCreate, SfxKind::OrderFill, SfxKind::Error].map(Control::Fire),
                )
                .collect(),
            Tab::Market => [Trend::Bearish, Trend::Sideways, Trend::Bullish]
                .map(Control::SetTrend)
                .into_iter()
                .chain(MarketField::ALL.map(Control::Market))
                .collect(),
            Tab::Mixer => MixLayer::ALL
                .map(Control::Mix)
                .into_iter()
                .chain([Control::OutputGain])
                .collect(),
            Tab::Sfx => vec![
                Control::SelectEffect(self.effect),
                Control::EventPan,
                Control::EventIntensity,
                Control::Trigger,
                Control::Inspect,
            ],
            Tab::Monitor => Vec::new(),
            Tab::Inspect => [Control::AuditionAll]
                .into_iter()
                .chain(SfxLayer::ALL.map(Control::AuditionLayer))
                .collect(),
            Tab::Reverb => ReverbBus::ALL
                .into_iter()
                .flat_map(|bus| ReverbField::ALL.map(|field| Control::Reverb(bus, field)))
                .collect(),
        };
        controls.extend([
            Control::Symbol,
            Control::ToggleMute,
            Control::Reset,
            Control::Quit,
        ]);
        controls
    }

    fn move_focus(&mut self, offset: isize) {
        let controls = self.focus_order();
        let current = controls
            .iter()
            .position(|&c| c == self.focus)
            .expect("focused control belongs to the selected tab");
        self.focus =
            controls[(current as isize + offset).rem_euclid(controls.len() as isize) as usize];
        if let Control::AuditionLayer(layer) = self.focus {
            self.inspector_layer = layer;
        }
    }

    fn select_effect(&mut self, offset: isize) {
        let current = SfxKind::iter()
            .position(|kind| kind == self.effect)
            .expect("selected effect exists");
        let index =
            (current as isize + offset).rem_euclid(SfxKind::iter().count() as isize) as usize;
        self.effect = SfxKind::iter()
            .nth(index)
            .expect("effect index wraps within the palette");
        self.focus = if self.tab == Tab::Inspect {
            Control::AuditionLayer(self.inspector_layer)
        } else {
            Control::SelectEffect(self.effect)
        };
    }

    fn activate(
        &mut self,
        control: Control,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        match control {
            Control::Inspect => self.select_tab(Tab::Inspect),
            Control::AuditionAll => self.fire(self.effect, send),
            Control::AuditionLayer(layer) => {
                self.inspector_layer = layer;
                let profile = self
                    .effect
                    .recipe()
                    .layers
                    .into_iter()
                    .find(|profile| profile.layer == layer)
                    .expect("every event recipe declares each layer");
                if self.output.sample_rate < profile.minimum_sample_rate {
                    self.notice = format!(
                        "{} requires at least {} Hz; device is {} Hz.",
                        layer.label(),
                        profile.minimum_sample_rate,
                        self.output.sample_rate
                    );
                } else {
                    self.fire_layers(self.effect, SfxLayerSelection::Only(layer), send);
                }
            }
            Control::Symbol => {
                self.symbol_input.clear();
                self.overlay = Overlay::SymbolEntry;
                self.notice = "Enter a ticker. Enter applies; Esc cancels.".into();
            }
            Control::CommitSymbol => match Symbol::new(&self.symbol_input) {
                Ok(symbol) => {
                    if self.submit(Command::Symbol(symbol), send) {
                        self.overlay = Overlay::None;
                    }
                }
                Err(error) => self.notice = error.to_string(),
            },
            Control::CancelSymbol => self.overlay = Overlay::None,
            Control::Tab(tab) => self.select_tab(tab),
            Control::SetMode(mode) => {
                self.submit(Command::Mode(mode), send);
            }
            Control::SetTrend(trend) => {
                self.submit(
                    Command::Market(MarketContext {
                        trend,
                        ..self.market
                    }),
                    send,
                );
            }
            Control::SetSpatial(spatial) => {
                self.submit(Command::Spatial(spatial), send);
            }
            Control::Preset(preset) => {
                self.submit(Command::Market(preset.market()), send);
            }
            Control::SelectEffect(effect) => {
                self.effect = effect;
                self.focus = control;
            }
            Control::Trigger => {
                self.fire(self.effect, send);
            }
            Control::Fire(effect) => {
                self.fire(effect, send);
            }
            Control::ToggleMute => {
                let mut mix = self.mix;
                if mix.master == Unit::ZERO {
                    mix.master = self.muted_level.unwrap_or_else(|| {
                        if self.initial_mix.master == Unit::ZERO {
                            Mix::default().master
                        } else {
                            self.initial_mix.master
                        }
                    });
                    if self.submit(Command::Mix(mix), send) {
                        self.muted_level = None;
                    }
                } else {
                    let previous = mix.master;
                    mix.master = Unit::ZERO;
                    if self.submit(Command::Mix(mix), send) {
                        self.muted_level = Some(previous);
                    }
                }
            }
            Control::Reset => {
                for command in [
                    Command::Market(MarketContext::default()),
                    Command::Mode(Mode::Browse),
                    Command::Mix(self.initial_mix),
                    Command::Spatial(self.initial_spatial),
                    Command::Reverb(self.initial_reverb),
                    Command::Symbol(self.initial_symbol),
                ] {
                    if !self.submit(command, send) {
                        return false;
                    }
                }
                self.muted_level = None;
                self.event_pan = Pan::CENTER;
                self.event_intensity = Unit::new(0.6).expect("constant intensity is normalized");
                self.notice =
                    "Controls reset to startup values. Audio tails continue naturally.".into();
            }
            Control::Quit => return true,
            Control::Market(_)
            | Control::Mix(_)
            | Control::OutputGain
            | Control::Reverb(_, _)
            | Control::EventPan
            | Control::EventIntensity => {}
        }
        false
    }

    fn fire(&mut self, kind: SfxKind, send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>) {
        self.fire_layers(kind, SfxLayerSelection::All, send);
    }

    fn fire_layers(
        &mut self,
        kind: SfxKind,
        layers: SfxLayerSelection,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) {
        self.submit(
            Command::Trigger(SfxEvent {
                layers,
                kind,
                pan: self.event_pan,
                intensity: self.event_intensity,
            }),
            send,
        );
        if matches!(self.focus, Control::SelectEffect(_)) {
            self.focus = Control::SelectEffect(self.effect);
        }
    }

    fn click(
        &mut self,
        control: Control,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        let quit = self.activate(control, send);
        if let Control::SelectEffect(kind) = control {
            self.fire(kind, send);
        }
        quit
    }

    fn numeric(&self, control: Control) -> Option<(f32, f32, f32)> {
        Some(match control {
            Control::Market(field) => (field.get(self.market).get(), 0.0, 1.0),
            Control::Mix(layer) => (layer.get(self.mix).get(), 0.0, 1.0),
            Control::OutputGain => (
                self.mix.output_gain.db(),
                OutputGain::MIN_DB,
                OutputGain::MAX_DB,
            ),
            Control::Reverb(bus, field) => (field.get(bus.get(self.reverb)).get(), 0.0, 1.0),
            Control::EventPan => (self.event_pan.get(), -1.0, 1.0),
            Control::EventIntensity => (self.event_intensity.get(), 0.0, 1.0),
            _ => return None,
        })
    }

    fn adjust(
        &mut self,
        control: Control,
        delta: f32,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) {
        let (current, low, high) = self
            .numeric(control)
            .expect("only numeric controls can be adjusted");
        let next = (current + delta).clamp(low, high);
        match control {
            Control::Market(field) => {
                let mut market = self.market;
                field.set(&mut market, Unit::new(next).expect("slider is normalized"));
                self.submit(Command::Market(market), send);
            }
            Control::Mix(layer) => {
                let mut mix = self.mix;
                layer.set(&mut mix, Unit::new(next).expect("slider is normalized"));
                if self.submit(Command::Mix(mix), send) && layer == MixLayer::Master && next > 0.0 {
                    self.muted_level = None;
                }
            }
            Control::EventPan => self.event_pan = Pan::new(next).expect("pan slider is bounded"),
            Control::OutputGain => {
                let mix = Mix {
                    output_gain: OutputGain::new(next).expect("output gain slider is bounded"),
                    ..self.mix
                };
                self.submit(Command::Mix(mix), send);
            }
            Control::Reverb(bus, field) => {
                let mut settings = self.reverb;
                let mut parameters = bus.get(settings);
                field.set(
                    &mut parameters,
                    Unit::new(next).expect("reverb slider is normalized"),
                );
                bus.set(&mut settings, parameters);
                self.submit(Command::Reverb(settings), send);
            }
            Control::EventIntensity => {
                self.event_intensity = Unit::new(next).expect("intensity slider is normalized")
            }
            _ => unreachable!("numeric controls are exhaustively handled"),
        }
    }

    fn key(
        &mut self,
        key: KeyEvent,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if self.overlay == Overlay::SymbolEntry {
            match key.code {
                KeyCode::Esc => {
                    self.activate(Control::CancelSymbol, send);
                }
                KeyCode::Enter if key.kind == KeyEventKind::Press => {
                    self.activate(Control::CommitSymbol, send);
                }
                KeyCode::Backspace => {
                    self.symbol_input.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.symbol_input.clear()
                }
                KeyCode::Char(character)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    if character.is_ascii_graphic() && self.symbol_input.len() < Symbol::MAX_LEN {
                        self.symbol_input.push(character.to_ascii_uppercase());
                    } else {
                        self.notice = "Ticker must fit within 24 ASCII characters.".into();
                    }
                }
                _ => {}
            }
            return false;
        }
        if self.overlay == Overlay::Help {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => return true,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => {
                        self.overlay = Overlay::None
                    }
                    _ => {}
                }
            }
            return false;
        }
        let repeatable = matches!(
            key.code,
            KeyCode::Tab
                | KeyCode::BackTab
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
        );
        if key.kind == KeyEventKind::Repeat && !repeatable {
            return false;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('1') => self.select_tab(Tab::Scene),
            KeyCode::Char('2') => self.select_tab(Tab::Market),
            KeyCode::Char('3') => self.select_tab(Tab::Mixer),
            KeyCode::Char('4') => self.select_tab(Tab::Sfx),
            KeyCode::Char('5') => self.select_tab(Tab::Monitor),
            KeyCode::Char('6') => self.select_tab(Tab::Reverb),
            KeyCode::Char('7') | KeyCode::Char('i') => {
                return self.activate(Control::Inspect, send);
            }
            KeyCode::Char('t') => return self.activate(Control::Symbol, send),
            KeyCode::Char('m') => {
                return self.activate(Control::ToggleMute, send);
            }
            KeyCode::Char('r') => {
                return self.activate(Control::Reset, send);
            }
            KeyCode::Tab => self.move_focus(if key.modifiers.contains(KeyModifiers::SHIFT) {
                -1
            } else {
                1
            }),
            KeyCode::BackTab => self.move_focus(-1),
            KeyCode::Up | KeyCode::Down if matches!(self.focus, Control::SelectEffect(_)) => {
                self.select_effect(if key.code == KeyCode::Up { -1 } else { 1 })
            }
            KeyCode::PageUp | KeyCode::PageDown if matches!(self.tab, Tab::Sfx | Tab::Inspect) => {
                self.select_effect(if key.code == KeyCode::PageUp { -8 } else { 8 })
            }
            KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::Left | KeyCode::Right if self.numeric(self.focus).is_some() => {
                let unit_step = if self.focus == Control::OutputGain {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        0.25
                    } else {
                        1.0
                    }
                } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                    0.01
                } else {
                    0.05
                };
                let step = if self.focus == Control::EventPan {
                    unit_step * 2.0
                } else {
                    unit_step
                };
                self.adjust(
                    self.focus,
                    if key.code == KeyCode::Left {
                        -step
                    } else {
                        step
                    },
                    send,
                );
            }
            KeyCode::Left => self.move_focus(-1),
            KeyCode::Right => self.move_focus(1),
            KeyCode::Home | KeyCode::End if self.numeric(self.focus).is_some() => {
                let (current, low, high) = self
                    .numeric(self.focus)
                    .expect("numeric control was checked");
                self.adjust(
                    self.focus,
                    if key.code == KeyCode::Home { low } else { high } - current,
                    send,
                );
            }
            KeyCode::Home | KeyCode::End if matches!(self.focus, Control::SelectEffect(_)) => {
                self.effect = if key.code == KeyCode::Home {
                    SfxKind::iter().next()
                } else {
                    SfxKind::iter().next_back()
                }
                .expect("effect palette is populated");
                self.focus = Control::SelectEffect(self.effect);
            }
            KeyCode::Char(' ') if self.tab == Tab::Sfx => {
                self.fire(self.effect, send);
            }
            KeyCode::Char(' ') if self.tab == Tab::Inspect => {
                return self.activate(
                    match self.focus {
                        Control::AuditionLayer(layer) => Control::AuditionLayer(layer),
                        _ => Control::AuditionAll,
                    },
                    send,
                );
            }
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(self.focus, send),
            _ => {}
        }
        false
    }
}

impl Preset {
    fn market(self) -> MarketContext {
        let (trend, values) = match self {
            Self::Idle => (Trend::Sideways, [0.1, 0.15, 0.1, 0.0, 0.05]),
            Self::Bullish => (Trend::Bullish, [0.8, 0.75, 0.3, 0.15, 0.45]),
            Self::Bearish => (Trend::Bearish, [0.85, 0.7, 0.6, 0.2, 0.45]),
            Self::Volatile => (Trend::Sideways, [0.4, 0.95, 0.95, 0.35, 0.8]),
            Self::DeepZoom => (Trend::Bearish, [0.75, 0.65, 0.45, 0.95, 0.65]),
        };
        let [trend_strength, volume, volatility, zoom, activity] =
            values.map(|value| Unit::new(value).expect("preset parameters are normalized"));
        MarketContext {
            trend,
            trend_strength,
            volume,
            volatility,
            zoom,
            activity,
        }
    }
}

/// Require an interactive terminal before opening an audio device.
pub(super) fn require_terminal() -> CliResult<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(invalid("the control panel needs a terminal on stdin and stdout; use play --repl for piped commands").into());
    }
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Each operation gets a chance to run even if a preceding one fails.
        let mut output = io::stdout();
        let _ = execute!(output, DisableMouseCapture);
        let _ = execute!(output, Show);
        let _ = disable_raw_mode();
        let _ = execute!(output, LeaveAlternateScreen);
        let _ = output.flush();
    }
}

pub(super) fn run(mut engine: AudioEngine, config: RenderConfig) -> CliResult<()> {
    let result = control_panel(&mut engine, config);
    let stopped = engine.shutdown();
    result?;
    stopped?;
    Ok(())
}

fn control_panel(engine: &mut AudioEngine, config: RenderConfig) -> CliResult<()> {
    let _guard = TerminalGuard;
    let mut terminal = ratatui::try_init()?;
    execute!(io::stdout(), EnableMouseCapture)?;
    let handle = engine.handle();
    let mut send = |command| handle.send(command);
    let mut app = App::new(config, engine.output_info().clone());
    let mut dragging = None;
    let mut hits = view::HitMap::default();
    let mut next_frame = Instant::now();
    loop {
        if Instant::now() >= next_frame {
            if let Some(snapshot) = engine.latest_snapshot() {
                app.snapshot = Some(snapshot);
            }
            app.callback_stats = engine.callback_stats();
            app.status = engine.status();
            for _ in 0..32 {
                match engine.take_notice() {
                    Some(notice) => app.notice = format!("Audio device notice: {notice}"),
                    None => break,
                }
            }
            if app.status != AudioStatus::Running {
                return Err(invalid(format!("audio output stopped: {:?}", app.status)).into());
            }
            terminal.draw(|frame| {
                hits = view::draw(frame, &app);
            })?;
            next_frame = Instant::now() + Duration::from_millis(80);
        }
        if !event::poll(next_frame.saturating_duration_since(Instant::now()))? {
            continue;
        }
        let previous_tab = app.tab;
        let previous_overlay = app.overlay;
        match event::read()? {
            Event::Key(key) => {
                if app.key(key, &mut send) {
                    break;
                }
            }
            Event::Mouse(mouse) if app.overlay != Overlay::Help => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(control) = hits.at(mouse.column, mouse.row) {
                        if app.overlay == Overlay::None && !matches!(control, Control::Tab(_)) {
                            app.focus = control;
                        }
                        if app.numeric(control).is_some() {
                            dragging = Some(control);
                            slide(&mut app, &hits, control, mouse.column, &mut send);
                        } else if app.click(control, &mut send) {
                            break;
                        }
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) if app.overlay == Overlay::None => {
                    if let Some(control) = dragging {
                        slide(&mut app, &hits, control, mouse.column, &mut send);
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => dragging = None,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    if app.overlay == Overlay::None =>
                {
                    let direction = if mouse.kind == MouseEventKind::ScrollUp {
                        -1
                    } else {
                        1
                    };
                    if let Some(control) = hits
                        .at(mouse.column, mouse.row)
                        .filter(|&control| app.numeric(control).is_some())
                    {
                        app.focus = control;
                        app.adjust(control, -direction as f32 * 0.05, &mut send);
                    } else if app.tab == Tab::Sfx {
                        app.select_effect(direction);
                    } else {
                        app.move_focus(direction);
                    }
                }
                _ => {}
            },
            Event::Resize(_, _) => {
                dragging = None;
                hits = view::HitMap::default();
            }
            _ => {}
        }
        if app.tab != previous_tab || app.overlay != previous_overlay {
            hits = view::HitMap::default();
            dragging = None;
        }
    }
    Ok(())
}

fn slide(
    app: &mut App,
    hits: &view::HitMap,
    control: Control,
    column: u16,
    send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
) {
    if let Some(region) = hits.region(control) {
        let (current, low, high) = app
            .numeric(control)
            .expect("only numeric controls can be dragged");
        let fraction = (column.saturating_sub(region.x) as f32
            / region.width.saturating_sub(1).max(1) as f32)
            .clamp(0.0, 1.0);
        app.adjust(control, low + fraction * (high - low) - current, send);
    }
}

#[cfg(test)]
mod tests;
