mod activity;
mod events;
mod view;

use std::{
    collections::BTreeMap,
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
    AudioEngine, AudioStatus, CallbackStats, Command, DataTransmissionCommand, HarmonyMode,
    MarketContext, Mix, Mode, OutputGain, OutputInfo, Pan, RenderConfig, ReverbSettings, SfxEvent,
    SfxKind, Snapshot, SpatialMode, Symbol, Trend, Unit,
};
use shzx_sfx::{
    SfxEdits, SfxFilterBand, SfxFmIndex, SfxFmRatio, SfxLayer, SfxLayerSelection, SfxLayerState,
    SfxMilliseconds, SfxRecipe, SfxScale, SfxStepRate, SfxTonalEdits, SfxTonalRate,
    SfxTonalSequence, SfxTonalStep,
};
use strum::IntoEnumIterator;

use super::{CliResult, MixLayer, ReverbBus, ReverbField, TransmissionDemoBurst, invalid};

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

fn ticker_choices() -> impl Iterator<Item = Symbol> {
    [
        "AAPL", "TSLA", "IBM", "MSFT", "NVDA", "AMZN", "GOOGL", "META", "AMD", "INTC", "ORCL",
        "ADBE", "NFLX", "CRM", "UBER", "SHOP", "COIN", "PLTR", "SOFI", "BRK.B", "^GSPC", "BTC-USD",
        "ETH-USD", "SPY",
    ]
    .into_iter()
    .map(|value| Symbol::new(value).expect("ticker choices are valid symbols"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScheduledTicker {
    at: Duration,
    symbol: Symbol,
}

struct TickerNavigation {
    rng: fastrand::Rng,
    elapsed: Duration,
    next: Option<ScheduledTicker>,
}

impl TickerNavigation {
    fn new(seed: u64, symbol: Symbol) -> Self {
        let mut navigation = Self {
            rng: fastrand::Rng::with_seed(seed ^ 0x5449_434b_4552),
            elapsed: Duration::ZERO,
            next: None,
        };
        navigation.enable(symbol);
        navigation
    }

    fn schedule(&mut self, current: Symbol) {
        let candidates = || ticker_choices().filter(|symbol| *symbol != current);
        let index = self.rng.usize(..candidates().count());
        self.next = Some(ScheduledTicker {
            at: self.elapsed + Duration::from_millis(self.rng.u64(25_000..=180_000)),
            symbol: candidates()
                .nth(index)
                .expect("random ticker index is within the grid"),
        });
    }

    fn enable(&mut self, current: Symbol) {
        if self.next.is_none() {
            self.schedule(current);
        }
    }

    fn toggle(&mut self, current: Symbol) {
        if self.next.is_some() {
            self.next = None;
        } else {
            self.enable(current);
        }
    }

    fn symbol_changed(&mut self, current: Symbol) {
        if self.next.is_some() {
            self.schedule(current);
        }
    }

    fn remaining(&self) -> Option<Duration> {
        self.next.map(|next| next.at.saturating_sub(self.elapsed))
    }

    fn due(&self) -> Option<Symbol> {
        self.next
            .filter(|next| next.at <= self.elapsed)
            .map(|next| next.symbol)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Control {
    Inspect,
    AuditionLayer(SfxLayer),
    AuditionAll,
    MuteLayer(SfxLayer),
    EditLayer(SfxLayer),
    SfxParameter(SfxParameter),
    SfxEditorPage(EditorPage),
    ResetEffect,
    ResetTonal,
    CloseSfxEditor,
    Symbol,
    CommitSymbol,
    PickSymbol(Symbol),
    ToggleTickerNavigation,
    ToggleUserActivity,
    CancelSymbol,
    Tab(Tab),
    SetMode(Mode),
    SetHarmony(HarmonyMode),
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
enum SfxParameter {
    Level,
    Highpass,
    Lowpass,
    Pitch,
    Duration,
    Tonal(TonalParameter),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TonalParameter {
    Carrier,
    FmIndex,
    FmRatio,
    RateStart,
    RateEnd,
    Attack,
    Release,
    StepPitch(SfxTonalStep),
}

impl TonalParameter {
    const SYNTHESIS: [Self; 5] = [
        Self::Carrier,
        Self::FmIndex,
        Self::FmRatio,
        Self::Attack,
        Self::Release,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Carrier => "Carrier tuning",
            Self::FmIndex => "FM index",
            Self::FmRatio => "FM ratio",
            Self::RateStart => "Rate start",
            Self::RateEnd => "Rate end",
            Self::Attack => "Slot attack",
            Self::Release => "Slot release",
            Self::StepPitch(step) => step.label(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorPage {
    Mix,
    Tone,
    Rate,
    Sequence,
}

impl EditorPage {
    const ALL: [Self; 4] = [Self::Mix, Self::Tone, Self::Rate, Self::Sequence];

    fn label(self) -> &'static str {
        match self {
            Self::Mix => "Mix",
            Self::Tone => "Synthesis",
            Self::Rate => "Rate",
            Self::Sequence => "Sequence",
        }
    }

    fn parameters(self) -> impl Iterator<Item = SfxParameter> {
        SfxParameter::ALL
            .into_iter()
            .chain(TonalParameter::SYNTHESIS.map(SfxParameter::Tonal))
            .chain([TonalParameter::RateStart, TonalParameter::RateEnd].map(SfxParameter::Tonal))
            .chain(
                SfxTonalStep::ALL.map(|step| SfxParameter::Tonal(TonalParameter::StepPitch(step))),
            )
            .filter(move |parameter| match parameter {
                SfxParameter::Tonal(TonalParameter::StepPitch(_)) => self == Self::Sequence,
                SfxParameter::Tonal(TonalParameter::RateStart | TonalParameter::RateEnd) => {
                    self == Self::Rate
                }
                SfxParameter::Tonal(_) => self == Self::Tone,
                _ => self == Self::Mix,
            })
    }
}

impl SfxParameter {
    const ALL: [Self; 5] = [
        Self::Level,
        Self::Highpass,
        Self::Lowpass,
        Self::Pitch,
        Self::Duration,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Level => "Layer level",
            Self::Highpass => "High-pass Hz",
            Self::Lowpass => "Low-pass Hz",
            Self::Pitch => "Event pitch",
            Self::Duration => "Event duration",
            Self::Tonal(parameter) => parameter.label(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    SymbolEntry,
    SfxEditor(EditorPage),
}

struct App {
    symbol: Symbol,
    ticker_navigation: TickerNavigation,
    activity: activity::UserActivity,
    event_log: events::EventLog,
    initial_symbol: Symbol,
    initial_harmony_mode: HarmonyMode,
    symbol_input: String,
    tab: Tab,
    focus: Control,
    market: MarketContext,
    mode: Mode,
    harmony_mode: HarmonyMode,
    mix: Mix,
    reverb: ReverbSettings,
    spatial: SpatialMode,
    effect: SfxKind,
    inspector_layer: SfxLayer,
    effect_edits: BTreeMap<SfxKind, SfxEdits>,
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
            ticker_navigation: TickerNavigation::new(config.seed, config.symbol),
            activity: activity::UserActivity::new(config.seed),
            event_log: events::EventLog::default(),
            initial_symbol: config.symbol,
            initial_harmony_mode: config.harmony_mode,
            symbol_input: String::new(),
            tab: Tab::Scene,
            focus: Control::SetMode(Mode::Browse),
            market: MarketContext::default(),
            mode: Mode::Browse,
            harmony_mode: config.harmony_mode,
            mix: config.mix,
            reverb: config.reverb,
            spatial: config.spatial,
            effect: SfxKind::Click,
            inspector_layer: SfxLayer::Texture,
            effect_edits: BTreeMap::new(),
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

    fn apply_accepted(&mut self, command: Command, source: events::EventSource) {
        match command {
            Command::Market(market) => self.market = market,
            Command::Mode(mode) => self.mode = mode,
            Command::Harmony(mode) => self.harmony_mode = mode,
            Command::Mix(mix) => self.mix = mix,
            Command::Symbol(symbol) => {
                self.symbol = symbol;
                self.ticker_navigation.symbol_changed(symbol);
            }
            Command::Reverb(reverb) => self.reverb = reverb,
            Command::Spatial(spatial) => self.spatial = spatial,
            Command::Trigger(event) => {
                if source == events::EventSource::User {
                    self.effect = event.kind;
                }
            }
            Command::DataTransmission(_) => {}
        }
        self.event_log.record(source, command);
    }

    fn submit(
        &mut self,
        command: Command,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        self.submit_from(command, events::EventSource::User, send)
    }

    fn submit_from(
        &mut self,
        command: Command,
        source: events::EventSource,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> bool {
        match send(command) {
            Ok(()) => {
                self.apply_accepted(command, source);
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
                    Command::Market(_) => match self.harmony_mode {
                        HarmonyMode::Diatonic => {
                            "Visible market updated. Harmony follows its musical clock.".into()
                        }
                        HarmonyMode::Dissonant => {
                            "Visible market updated. Ticker harmony stays fixed.".into()
                        }
                    },
                    Command::Mix(_) => "Layer levels updated.".into(),
                    Command::Symbol(symbol) => {
                        format!("Symbol {symbol}: {} harmony selected.", self.harmony_mode)
                    }
                    Command::Harmony(mode) => {
                        format!("Harmony: {mode}. The chord changes smoothly.")
                    }
                    Command::Reverb(_) => {
                        "Reverb updated. The existing tail continues through the transition.".into()
                    }
                    Command::Spatial(spatial) => format!("Spatial processing: {spatial}"),
                    Command::DataTransmission(_) => "Data transmission updated.".into(),
                };
                true
            }
            Err(error) => {
                self.notice = format!("Not applied: {error}");
                false
            }
        }
    }

    fn advance(
        &mut self,
        elapsed: Duration,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) {
        self.event_log.advance(elapsed);
        self.ticker_navigation.elapsed = elapsed;
        if self.overlay != Overlay::SymbolEntry
            && let Some(symbol) = self.ticker_navigation.due()
        {
            self.submit_from(
                Command::Symbol(symbol),
                events::EventSource::Simulator,
                send,
            );
        }
        let edits = &self.effect_edits;
        let market = self.market;
        let mut accepted = None;
        let result = self.activity.advance(elapsed, &mut |action| {
            let trigger = |kind, pan, intensity| {
                Command::Trigger(SfxEvent {
                    pan,
                    intensity,
                    edits: edits.get(&kind).copied().unwrap_or_default(),
                    ..SfxEvent::new(kind)
                })
            };
            let command = match action {
                activity::Action::Trigger {
                    kind,
                    pan,
                    intensity,
                } => trigger(kind, pan, intensity),
                activity::Action::Mode(mode) => Command::Mode(mode),
                activity::Action::Market(preset) => Command::Market(MarketContext {
                    zoom: market.zoom,
                    ..preset.market()
                }),
                activity::Action::Zoom(zoom) => Command::Market(MarketContext { zoom, ..market }),
                activity::Action::Ui {
                    event,
                    pan,
                    intensity,
                } => {
                    if let Some(kind) = event.sound() {
                        send(trigger(kind, pan, intensity))?;
                    }
                    accepted = Some(events::LoggedEventKind::Activity(event));
                    return Ok(());
                }
                activity::Action::Thought(thought) => {
                    accepted = Some(events::LoggedEventKind::Thought(thought));
                    return Ok(());
                }
            };
            send(command)?;
            accepted = Some(events::LoggedEventKind::Command(command));
            Ok(())
        });
        match accepted {
            Some(events::LoggedEventKind::Command(command)) => {
                self.apply_accepted(command, events::EventSource::Simulator);
            }
            Some(events::LoggedEventKind::Activity(event)) => self.event_log.record_activity(event),
            Some(events::LoggedEventKind::Thought(thought)) => {
                self.event_log.record_thought(thought)
            }
            None => {}
        }
        if let Err(error) = result {
            self.notice = format!("User activity pending: {error}");
        }
    }

    fn select_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.focus = self.focus_order()[0];
    }

    fn focus_order(&self) -> Vec<Control> {
        if matches!(self.overlay, Overlay::SfxEditor(_)) {
            return EditorPage::ALL
                .into_iter()
                .filter(|_| self.inspector_layer == SfxLayer::Tonal)
                .map(Control::SfxEditorPage)
                .chain(self.editor_parameters().map(Control::SfxParameter))
                .chain(self.editor_actions().map(|(control, _)| control))
                .collect();
        }
        let mut controls = match self.tab {
            Tab::Scene => [Mode::Browse, Mode::Dialog, Mode::Focus]
                .map(Control::SetMode)
                .into_iter()
                .chain(Preset::ALL.map(Control::Preset))
                .chain([SpatialMode::Pan, SpatialMode::Haas].map(Control::SetSpatial))
                .chain(
                    [SfxKind::OrderCreate, SfxKind::OrderFill, SfxKind::Error].map(Control::Fire),
                )
                .chain([HarmonyMode::Diatonic, HarmonyMode::Dissonant].map(Control::SetHarmony))
                .chain([Control::ToggleTickerNavigation])
                .chain(ticker_choices().map(Control::PickSymbol))
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
                Control::AuditionAll,
            ]
            .into_iter()
            .chain(SfxLayer::ALL.into_iter().flat_map(Self::layer_controls))
            .collect(),
            Tab::Monitor => Vec::new(),
            Tab::Inspect => [Control::AuditionAll]
                .into_iter()
                .chain(SfxLayer::ALL.into_iter().flat_map(Self::layer_controls))
                .collect(),
            Tab::Reverb => ReverbBus::ALL
                .into_iter()
                .flat_map(|bus| ReverbField::ALL.map(|field| Control::Reverb(bus, field)))
                .collect(),
        };
        controls.extend([
            Control::Symbol,
            Control::ToggleUserActivity,
            Control::ToggleMute,
            Control::Reset,
            Control::Quit,
        ]);
        controls
    }

    fn layer_controls(layer: SfxLayer) -> [Control; 3] {
        [
            Control::AuditionLayer(layer),
            Control::MuteLayer(layer),
            Control::EditLayer(layer),
        ]
    }

    fn move_focus(&mut self, offset: isize) {
        let controls = self.focus_order();
        let current = controls
            .iter()
            .position(|&c| c == self.focus)
            .expect("focused control belongs to the selected tab");
        self.focus =
            controls[(current as isize + offset).rem_euclid(controls.len() as isize) as usize];
        if let Control::AuditionLayer(layer)
        | Control::MuteLayer(layer)
        | Control::EditLayer(layer) = self.focus
        {
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
            Control::MuteLayer(layer) => {
                self.inspector_layer = layer;
                let edits = self.effect_edits.entry(self.effect).or_default();
                let mut settings = edits.layer(layer);
                settings.state = match settings.state {
                    SfxLayerState::Audible => SfxLayerState::Muted,
                    SfxLayerState::Muted => SfxLayerState::Audible,
                };
                edits.set_layer(layer, settings);
                self.notice = format!(
                    "{} {}. Play mix auditions the current settings.",
                    layer.label(),
                    if settings.state == SfxLayerState::Muted {
                        "muted"
                    } else {
                        "unmuted"
                    }
                );
            }
            Control::EditLayer(layer) => {
                self.inspector_layer = layer;
                let page = if layer == SfxLayer::Tonal {
                    EditorPage::Tone
                } else {
                    EditorPage::Mix
                };
                self.select_editor_page(page);
            }
            Control::SfxEditorPage(page) => self.select_editor_page(page),
            Control::CloseSfxEditor => {
                self.overlay = Overlay::None;
                self.focus = Control::EditLayer(self.inspector_layer);
            }
            Control::ResetEffect => {
                self.effect_edits.remove(&self.effect);
                self.notice = format!("{} restored to its original sound settings.", self.effect);
            }
            Control::ResetTonal => {
                assert_eq!(self.inspector_layer, SfxLayer::Tonal);
                self.effect_edits.entry(self.effect).or_default().tonal = SfxTonalEdits::DEFAULT;
                self.notice = format!(
                    "{} tonal synthesis restored. Layer mix settings retained.",
                    self.effect
                );
            }
            Control::AuditionLayer(layer) => {
                self.inspector_layer = layer;
                let profile = self
                    .edited_recipe()
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
            Control::PickSymbol(symbol) => {
                self.submit(Command::Symbol(symbol), send);
            }
            Control::ToggleTickerNavigation => {
                self.ticker_navigation.toggle(self.symbol);
                self.notice = if self.ticker_navigation.remaining().is_some() {
                    "Automatic tickers enabled. Changes are 25 to 180 seconds apart."
                } else {
                    "Automatic tickers paused. Select a ticker from the grid or enter one."
                }
                .into();
            }
            Control::ToggleUserActivity => {
                self.activity.toggle();
                self.notice = if self.activity.enabled() {
                    "User activity enabled. Spatial terminal interactions with occasional orders."
                } else {
                    "User activity paused."
                }
                .into();
            }
            Control::Tab(tab) => self.select_tab(tab),
            Control::SetMode(mode) => {
                self.submit(Command::Mode(mode), send);
            }
            Control::SetHarmony(mode) => {
                self.submit(Command::Harmony(mode), send);
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
                    Command::Harmony(self.initial_harmony_mode),
                ] {
                    if !self.submit(command, send) {
                        return false;
                    }
                }
                self.muted_level = None;
                self.effect_edits.clear();
                self.event_pan = Pan::CENTER;
                self.event_intensity = Unit::new(0.6).expect("constant intensity is normalized");
                self.ticker_navigation.enable(self.symbol);
                self.activity.reset();
                self.notice =
                    "Controls reset to startup values. Audio tails continue naturally.".into();
            }
            Control::Quit => return true,
            Control::Market(_)
            | Control::Mix(_)
            | Control::OutputGain
            | Control::Reverb(_, _)
            | Control::EventPan
            | Control::EventIntensity
            | Control::SfxParameter(_) => {}
        }
        false
    }

    fn fire(&mut self, kind: SfxKind, send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>) {
        self.fire_layers(kind, SfxLayerSelection::All, send);
    }

    fn current_edits(&self) -> SfxEdits {
        self.effect_edits
            .get(&self.effect)
            .copied()
            .unwrap_or_default()
    }

    fn event(&self, kind: SfxKind, layers: SfxLayerSelection) -> SfxEvent {
        SfxEvent {
            kind,
            layers,
            pan: self.event_pan,
            intensity: self.event_intensity,
            edits: self.effect_edits.get(&kind).copied().unwrap_or_default(),
        }
    }

    fn edited_recipe(&self) -> SfxRecipe {
        self.event(self.effect, SfxLayerSelection::All).recipe()
    }

    fn select_editor_page(&mut self, page: EditorPage) {
        assert!(page == EditorPage::Mix || self.inspector_layer == SfxLayer::Tonal);
        self.overlay = Overlay::SfxEditor(page);
        self.focus = Control::SfxParameter(
            page.parameters()
                .next()
                .expect("editor pages contain parameters"),
        );
    }

    fn editor_parameters(&self) -> impl Iterator<Item = SfxParameter> {
        let Overlay::SfxEditor(page) = self.overlay else {
            panic!("editor parameters require an open editor")
        };
        page.parameters()
    }

    fn editor_actions(&self) -> impl Iterator<Item = (Control, &'static str)> {
        let Overlay::SfxEditor(page) = self.overlay else {
            panic!("editor actions require an open editor")
        };
        let reset = if page == EditorPage::Mix {
            (
                Control::ResetEffect,
                if self.inspector_layer == SfxLayer::Tonal {
                    "Reset event"
                } else {
                    "Reset event / R"
                },
            )
        } else {
            (Control::ResetTonal, "Reset tone")
        };
        (self.inspector_layer == SfxLayer::Tonal)
            .then_some((Control::AuditionLayer(SfxLayer::Tonal), "Play tone"))
            .into_iter()
            .chain([
                (
                    Control::AuditionAll,
                    if self.inspector_layer == SfxLayer::Tonal {
                        "Play mix"
                    } else {
                        "Play mix / Space"
                    },
                ),
                reset,
                (Control::CloseSfxEditor, "Close / Esc"),
            ])
    }

    fn fire_layers(
        &mut self,
        kind: SfxKind,
        layers: SfxLayerSelection,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) {
        self.submit(Command::Trigger(self.event(kind, layers)), send);
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

    /// The edit coordinate and bounds shared by keyboard, wheel, and dragging.
    fn numeric(&self, control: Control) -> Option<(f32, f32, f32)> {
        Some(match control {
            Control::Market(field) => (field.get(self.market).get(), 0.0, 1.0),
            Control::Mix(layer) => (layer.get(self.mix).get().cbrt(), 0.0, 1.0),
            Control::OutputGain => (
                self.mix.output_gain.db(),
                OutputGain::MIN_DB,
                OutputGain::MAX_DB,
            ),
            Control::Reverb(bus, field) => (field.get(bus.get(self.reverb)).get(), 0.0, 1.0),
            Control::EventPan => (self.event_pan.get(), -1.0, 1.0),
            Control::EventIntensity => (self.event_intensity.get(), 0.0, 1.0),
            Control::SfxParameter(parameter) => {
                let recipe = self.edited_recipe();
                let profile = recipe
                    .layers
                    .iter()
                    .find(|profile| profile.layer == self.inspector_layer)
                    .expect("every selectable layer has a recipe");
                match parameter {
                    SfxParameter::Level => (profile.level, 0.0, 1.0),
                    SfxParameter::Highpass => (
                        profile.highpass_hz,
                        SfxFilterBand::MIN_HZ,
                        (profile.lowpass_hz - 1.0).min(SfxFilterBand::MAX_HZ),
                    ),
                    SfxParameter::Lowpass => (
                        profile.lowpass_hz,
                        (profile.highpass_hz + 1.0).max(SfxFilterBand::MIN_HZ),
                        SfxFilterBand::MAX_HZ,
                    ),
                    SfxParameter::Pitch => (
                        self.current_edits().pitch.get(),
                        SfxScale::MIN,
                        SfxScale::MAX,
                    ),
                    SfxParameter::Duration => (
                        self.current_edits().duration.get(),
                        SfxScale::MIN,
                        SfxScale::MAX,
                    ),
                    SfxParameter::Tonal(parameter) => {
                        assert_eq!(self.inspector_layer, SfxLayer::Tonal);
                        let tonal = self.current_edits().tonal;
                        match parameter {
                            TonalParameter::Carrier => {
                                (tonal.carrier.get(), SfxScale::MIN, SfxScale::MAX)
                            }
                            TonalParameter::FmIndex => {
                                (tonal.fm_index.get(), SfxFmIndex::MIN, SfxFmIndex::MAX)
                            }
                            TonalParameter::FmRatio => {
                                (tonal.fm_ratio.get(), SfxFmRatio::MIN, SfxFmRatio::MAX)
                            }
                            TonalParameter::RateStart => (
                                recipe.tonal_rate.start().get(),
                                SfxStepRate::MIN_HZ,
                                SfxStepRate::MAX_HZ,
                            ),
                            TonalParameter::RateEnd => (
                                recipe.tonal_rate.end().get(),
                                SfxStepRate::MIN_HZ,
                                SfxStepRate::MAX_HZ,
                            ),
                            TonalParameter::Attack => (
                                tonal.attack.get(),
                                SfxMilliseconds::MIN,
                                SfxMilliseconds::MAX,
                            ),
                            TonalParameter::Release => (
                                tonal.release.get(),
                                SfxMilliseconds::MIN,
                                SfxMilliseconds::MAX,
                            ),
                            TonalParameter::StepPitch(step) => {
                                (recipe.tonal_pitch_ratio(step), SfxScale::MIN, SfxScale::MAX)
                            }
                        }
                    }
                }
            }
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
                layer.set(
                    &mut mix,
                    Unit::new(next.powi(3)).expect("mixer position is normalized"),
                );
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
            Control::SfxParameter(parameter) => {
                let mut edits = self.current_edits();
                let mut layer = edits.layer(self.inspector_layer);
                match parameter {
                    SfxParameter::Level => {
                        layer.level = Some(Unit::new(next).expect("layer level is normalized"))
                    }
                    SfxParameter::Highpass | SfxParameter::Lowpass => {
                        let profile = self
                            .edited_recipe()
                            .layers
                            .into_iter()
                            .find(|profile| profile.layer == self.inspector_layer)
                            .expect("every selectable layer has a recipe");
                        let (highpass, lowpass) = if parameter == SfxParameter::Highpass {
                            (next, profile.lowpass_hz)
                        } else {
                            (profile.highpass_hz, next)
                        };
                        layer.filter = Some(
                            SfxFilterBand::new(highpass, lowpass)
                                .expect("filter sliders preserve an ordered band"),
                        );
                    }
                    SfxParameter::Pitch => {
                        edits.pitch = SfxScale::new(next).expect("pitch slider is bounded")
                    }
                    SfxParameter::Duration => {
                        edits.duration = SfxScale::new(next).expect("duration slider is bounded")
                    }
                    SfxParameter::Tonal(parameter) => match parameter {
                        TonalParameter::Carrier => {
                            edits.tonal.carrier =
                                SfxScale::new(next).expect("carrier tuning slider is bounded")
                        }
                        TonalParameter::FmIndex => {
                            edits.tonal.fm_index =
                                SfxFmIndex::new(next).expect("FM index slider is bounded")
                        }
                        TonalParameter::FmRatio => {
                            edits.tonal.fm_ratio =
                                SfxFmRatio::new(next).expect("FM ratio slider is bounded")
                        }
                        TonalParameter::RateStart | TonalParameter::RateEnd => {
                            let rate = self.edited_recipe().tonal_rate;
                            let next = SfxStepRate::new(next).expect("step rate slider is bounded");
                            let (start, end) = if parameter == TonalParameter::RateStart {
                                (next, rate.end())
                            } else {
                                (rate.start(), next)
                            };
                            edits.tonal.step_rate = Some(SfxTonalRate::Sweep { start, end });
                        }
                        TonalParameter::Attack => {
                            edits.tonal.attack =
                                SfxMilliseconds::new(next).expect("attack slider is bounded")
                        }
                        TonalParameter::Release => {
                            edits.tonal.release =
                                SfxMilliseconds::new(next).expect("release slider is bounded")
                        }
                        TonalParameter::StepPitch(step) => {
                            let mut sequence = edits.tonal.pitch_ratios.unwrap_or_else(|| {
                                SfxTonalSequence::new(self.edited_recipe().tonal_pitch_ratios.map(
                                    |ratio| {
                                        SfxScale::new(ratio)
                                            .expect("recipe pitch ratios are bounded")
                                    },
                                ))
                            });
                            sequence.set(
                                step,
                                SfxScale::new(next).expect("step pitch slider is bounded"),
                            );
                            edits.tonal.pitch_ratios = Some(sequence);
                        }
                    },
                }
                edits.set_layer(self.inspector_layer, layer);
                self.effect_edits.insert(self.effect, edits);
                self.notice = format!(
                    "{} settings updated. Space auditions {}; Esc closes.",
                    self.effect,
                    if self.inspector_layer == SfxLayer::Tonal {
                        "Tonal"
                    } else {
                        "the mix"
                    }
                );
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
        if let Overlay::SfxEditor(page) = self.overlay {
            match key.code {
                KeyCode::Esc => {
                    self.activate(Control::CloseSfxEditor, send);
                }
                KeyCode::Char('q') => return true,
                KeyCode::Char(' ') if self.inspector_layer == SfxLayer::Tonal => {
                    self.activate(Control::AuditionLayer(SfxLayer::Tonal), send);
                }
                KeyCode::Char(' ') => self.fire(self.effect, send),
                KeyCode::Char('m') => {
                    self.activate(Control::MuteLayer(self.inspector_layer), send);
                }
                KeyCode::Char('r') => {
                    self.activate(
                        if page == EditorPage::Mix {
                            Control::ResetEffect
                        } else {
                            Control::ResetTonal
                        },
                        send,
                    );
                }
                KeyCode::PageUp | KeyCode::PageDown if self.inspector_layer == SfxLayer::Tonal => {
                    let index = EditorPage::ALL
                        .into_iter()
                        .position(|candidate| candidate == page)
                        .expect("editor page exists");
                    let offset = if key.code == KeyCode::PageDown {
                        1
                    } else {
                        EditorPage::ALL.len() - 1
                    };
                    self.select_editor_page(
                        EditorPage::ALL[(index + offset) % EditorPage::ALL.len()],
                    );
                }
                KeyCode::Enter => return self.activate(self.focus, send),
                _ => self.focus_key(key, send),
            }
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
            KeyCode::Char('u') => return self.activate(Control::ToggleUserActivity, send),
            KeyCode::Char('m') => {
                return self.activate(Control::ToggleMute, send);
            }
            KeyCode::Char('r') => {
                return self.activate(Control::Reset, send);
            }
            KeyCode::Up | KeyCode::Down if matches!(self.focus, Control::SelectEffect(_)) => {
                self.select_effect(if key.code == KeyCode::Up { -1 } else { 1 })
            }
            KeyCode::PageUp | KeyCode::PageDown if matches!(self.tab, Tab::Sfx | Tab::Inspect) => {
                self.select_effect(if key.code == KeyCode::PageUp { -8 } else { 8 })
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
            KeyCode::Char(' ') if matches!(self.tab, Tab::Sfx | Tab::Inspect) => {
                return self.activate(
                    match self.focus {
                        Control::AuditionLayer(layer) => Control::AuditionLayer(layer),
                        _ => Control::AuditionAll,
                    },
                    send,
                );
            }
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(self.focus, send),
            _ => self.focus_key(key, send),
        }
        false
    }

    fn focus_key(&mut self, key: KeyEvent, send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>) {
        match key.code {
            KeyCode::Tab => self.move_focus(if key.modifiers.contains(KeyModifiers::SHIFT) {
                -1
            } else {
                1
            }),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(-1),
            KeyCode::Down => self.move_focus(1),
            KeyCode::Left | KeyCode::Right if self.numeric(self.focus).is_some() => {
                let step =
                    Self::numeric_step(self.focus, key.modifiers.contains(KeyModifiers::SHIFT));
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
            KeyCode::Left => self.move_focus(-1),
            KeyCode::Right => self.move_focus(1),
            _ => {}
        }
    }

    fn numeric_step(control: Control, fine: bool) -> f32 {
        let (normal_step, fine_step) = match control {
            Control::OutputGain => (1.0, 0.25),
            Control::SfxParameter(SfxParameter::Highpass | SfxParameter::Lowpass) => (100.0, 10.0),
            Control::SfxParameter(SfxParameter::Level) => (0.01, 0.001),
            Control::SfxParameter(SfxParameter::Tonal(TonalParameter::FmIndex)) => (0.1, 0.01),
            Control::SfxParameter(SfxParameter::Tonal(
                TonalParameter::RateStart | TonalParameter::RateEnd,
            )) => (10.0, 1.0),
            Control::SfxParameter(SfxParameter::Tonal(
                TonalParameter::Attack | TonalParameter::Release,
            )) => (0.05, 0.01),
            Control::EventPan => (0.1, 0.02),
            _ => (0.05, 0.01),
        };
        if fine { fine_step } else { normal_step }
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

#[derive(Clone, Copy)]
enum TransmissionDemoStep {
    Waiting(TransmissionDemoBurst),
    Started { end: Duration },
}

struct LiveTransmissionDemo<I> {
    schedule: I,
    step: Option<TransmissionDemoStep>,
}

impl<I: Iterator<Item = TransmissionDemoBurst>> LiveTransmissionDemo<I> {
    fn new(mut schedule: I) -> Self {
        let step = Self::next_burst(&mut schedule);
        Self { schedule, step }
    }

    fn next_burst(schedule: &mut I) -> Option<TransmissionDemoStep> {
        schedule.next().map(TransmissionDemoStep::Waiting)
    }

    fn advance(
        &mut self,
        elapsed: Duration,
        send: &mut impl FnMut(Command) -> shzx_sfx::Result<()>,
    ) -> shzx_sfx::Result<()> {
        while let Some(step) = self.step {
            match step {
                // Skip unsent bursts that expired while the host was suspended.
                TransmissionDemoStep::Waiting(burst)
                    if Duration::from_secs_f64(burst.end_seconds) <= elapsed =>
                {
                    self.step = Self::next_burst(&mut self.schedule);
                }
                TransmissionDemoStep::Waiting(burst)
                    if Duration::from_secs_f64(burst.start_seconds) <= elapsed =>
                {
                    send(Command::DataTransmission(DataTransmissionCommand::Start(
                        burst.settings,
                    )))?;
                    self.step = Some(TransmissionDemoStep::Started {
                        end: Duration::from_secs_f64(burst.end_seconds),
                    });
                }
                TransmissionDemoStep::Started { end } if end <= elapsed => {
                    send(Command::DataTransmission(DataTransmissionCommand::End))?;
                    self.step = Self::next_burst(&mut self.schedule);
                }
                _ => break,
            }
        }
        Ok(())
    }
}

pub(super) fn run(
    mut engine: AudioEngine,
    config: RenderConfig,
    demo: Option<impl Iterator<Item = TransmissionDemoBurst>>,
) -> CliResult<()> {
    let result = control_panel(&mut engine, config, demo);
    let stopped = engine.shutdown();
    result?;
    stopped?;
    Ok(())
}

fn control_panel(
    engine: &mut AudioEngine,
    config: RenderConfig,
    demo: Option<impl Iterator<Item = TransmissionDemoBurst>>,
) -> CliResult<()> {
    let _guard = TerminalGuard;
    let mut terminal = ratatui::try_init()?;
    execute!(io::stdout(), EnableMouseCapture)?;
    let handle = engine.handle();
    let mut send = |command| handle.send(command);
    let mut app = App::new(config, engine.output_info().clone());
    let mut demo = demo.map(LiveTransmissionDemo::new);
    let started = Instant::now();
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
            if let Some(demo) = &mut demo
                && let Err(error) = demo.advance(started.elapsed(), &mut send)
            {
                app.notice = format!("Transmission demo pending: {error}");
            }
            app.advance(started.elapsed(), &mut send);
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
                        if matches!(app.overlay, Overlay::None | Overlay::SfxEditor(_))
                            && !matches!(control, Control::Tab(_))
                        {
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
                MouseEventKind::Drag(MouseButton::Left)
                    if matches!(app.overlay, Overlay::None | Overlay::SfxEditor(_)) =>
                {
                    if let Some(control) = dragging {
                        slide(&mut app, &hits, control, mouse.column, &mut send);
                    }
                }
                MouseEventKind::Up(MouseButton::Left) => dragging = None,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                    if matches!(app.overlay, Overlay::None | Overlay::SfxEditor(_)) =>
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
                        app.adjust(
                            control,
                            -direction as f32 * App::numeric_step(control, false),
                            &mut send,
                        );
                    } else if app.tab == Tab::Sfx && app.overlay == Overlay::None {
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
