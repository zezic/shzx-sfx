use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use shzx_sfx::{
    AmbientSnapshot, AudioStatus, BackgroundVoice, BufferSize, ChordTransition, ClockState,
    Command, DataTransmissionCommand, DelaySnapshot, HarmonyMode, Mode, Pan, ParameterSnapshot,
    RootTransition, SfxKind, SfxLayer, SfxLayerRecipe, SfxLayerSelection, SfxLayerState,
    SfxLayerStereo, SfxModulation, SfxRecipe, Snapshot, SpatialMode, Trend, Unit,
};
use strum::IntoEnumIterator;

use super::{
    App, Control, EditorPage, MarketField, Overlay, Preset, ReverbBus, ReverbField, SfxParameter,
    Tab, TonalParameter, ticker_choices,
};
use crate::MixLayer;

const BACKGROUND: Color = Color::Rgb(8, 15, 21);
const PANEL: Color = Color::Rgb(12, 23, 31);
const FOCUSED: Color = Color::Rgb(25, 65, 77);
const CYAN: Color = Color::Rgb(104, 216, 224);
const TEXT: Color = Color::Rgb(203, 218, 222);
const DIM: Color = Color::Rgb(111, 139, 150);
const AMBER: Color = Color::Rgb(233, 183, 107);
const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Clone, Copy)]
struct Hit {
    area: Rect,
    control: Control,
}

#[derive(Default)]
pub(super) struct HitMap(Vec<Hit>);

impl HitMap {
    pub(super) fn at(&self, x: u16, y: u16) -> Option<Control> {
        self.0
            .iter()
            .find(|hit| hit.area.contains((x, y).into()))
            .map(|hit| hit.control)
    }

    pub(super) fn region(&self, control: Control) -> Option<Rect> {
        self.0
            .iter()
            .find(|hit| hit.control == control)
            .map(|hit| hit.area)
    }

    fn add(&mut self, area: Rect, control: Control) {
        if !area.is_empty() {
            self.0.push(Hit { area, control });
        }
    }
}

pub(super) fn draw(frame: &mut Frame<'_>, app: &App) -> HitMap {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::default().bg(BACKGROUND).fg(TEXT)),
        area,
    );
    let mut hits = HitMap::default();
    if area.width < 80 || area.height < 24 {
        small_terminal(frame, area, app, &mut hits);
    } else {
        header(
            frame,
            Rect::new(area.x, area.y, area.width, 3),
            app,
            &mut hits,
        );
        tabs(
            frame,
            Rect::new(area.x, area.y + 3, area.width, 1),
            app,
            &mut hits,
        );
        if app.tab == Tab::Sfx {
            effects(
                frame,
                Rect::new(area.x, area.y + 4, area.width, area.height - 7),
                app,
                &mut hits,
            );
        } else if app.tab == Tab::Monitor {
            monitor(
                frame,
                Rect::new(area.x, area.y + 4, area.width, area.height - 11),
                app,
            );
            telemetry(
                frame,
                Rect::new(area.x + 1, area.bottom() - 7, area.width - 2, 4),
                app,
            );
        } else if area.width >= 140 {
            let controls_width = (area.width * 11 / 20).max(80).min(area.width - 59);
            let controls = Rect::new(area.x, area.y + 4, controls_width, area.height - 11);
            controls_panel(frame, controls, app, &mut hits);
            telemetry(
                frame,
                Rect::new(area.x + 1, area.bottom() - 7, controls_width - 2, 4),
                app,
            );
            monitor_column(
                frame,
                Rect::new(
                    area.x + controls_width + 1,
                    area.y + 4,
                    area.width - controls_width - 1,
                    area.height - 7,
                ),
                app,
            );
        } else {
            controls_panel(
                frame,
                Rect::new(area.x, area.y + 4, area.width, area.height - 13),
                app,
                &mut hits,
            );
            monitor_strip(
                frame,
                Rect::new(area.x + 1, area.bottom() - 9, area.width - 2, 6),
                app,
            );
        }
        frame.render_widget(
            Paragraph::new(app.notice.as_str()).style(Style::default().fg(AMBER)),
            Rect::new(area.x + 1, area.bottom() - 3, area.width - 2, 1),
        );
        footer(
            frame,
            Rect::new(area.x, area.bottom() - 2, area.width, 2),
            app,
            &mut hits,
        );
    }
    match app.overlay {
        Overlay::None => {}
        Overlay::Help => {
            help(frame, area);
            return HitMap::default();
        }
        Overlay::SymbolEntry => return symbol_entry(frame, area, app),
        Overlay::SfxEditor(page) => return sfx_editor(frame, area, app, page),
    }
    hits
}

fn controls_panel(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    match app.tab {
        Tab::Scene => scene(frame, area, app, hits),
        Tab::Market => market(frame, area, app, hits),
        Tab::Mixer => mixer(frame, area, app, hits),
        Tab::Sfx => effects(frame, area, app, hits),
        Tab::Inspect => inspector(frame, area, app, hits),
        Tab::Reverb => reverb(frame, area, app, hits),
        Tab::Monitor => unreachable!("the full monitor has a dedicated layout"),
    }
}

fn panel(frame: &mut Frame<'_>, area: Rect, title: impl Into<Line<'static>>) -> Rect {
    let block = Block::new()
        .title(title)
        .borders(Borders::ALL)
        .border_set(ASCII_BORDER)
        .border_style(Style::default().fg(DIM))
        .style(Style::default().bg(PANEL));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn header(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let status = match app.status {
        AudioStatus::Running => "RUNNING",
        AudioStatus::Stopped => "STOPPED",
        AudioStatus::Failed(_) => "FAILED",
    };
    let mute = if app.mix.master.get() == 0.0 {
        " / MASTER MUTED"
    } else {
        ""
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " SHZX / SOUND LAB ",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" {status}{mute}"), Style::default().fg(AMBER)),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let symbol_label = format!("[ Symbol {} ]", app.symbol);
    let symbol_width = symbol_label.len() as u16;
    let symbol_rect = Rect::new(area.right() - symbol_width, area.y, symbol_width, 1);
    frame.render_widget(
        Paragraph::new(symbol_label).style(Style::default().fg(CYAN).bg(
            if app.focus == Control::Symbol {
                FOCUSED
            } else {
                BACKGROUND
            },
        )),
        symbol_rect,
    );
    hits.add(symbol_rect, Control::Symbol);
    let requested = match app.output.buffer_size {
        BufferSize::SystemDefault => "system".into(),
        BufferSize::Fixed(frames) => frames.to_string(),
    };
    let observed = if app.callback_stats.callbacks == 0 {
        "waiting".into()
    } else {
        format!(
            "{} [{}..{}]",
            app.callback_stats.last_frames,
            app.callback_stats.min_frames,
            app.callback_stats.max_frames
        )
    };
    frame.render_widget(
        Paragraph::new(format!(
            " {} | {} Hz / {}ch {} | buffer {requested} / actual {observed}",
            app.output.device,
            app.output.sample_rate,
            app.output.channels,
            app.output.sample_format
        ))
        .style(Style::default().fg(DIM)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
    if let Some(snapshot) = app.snapshot {
        frame.render_widget(
            Paragraph::new(format!(
                " {} | {} | gain {:+.0}dB | v{}/{} | P{} R{} dBFS",
                snapshot.background.harmony.mode.label(),
                root_text(&snapshot),
                snapshot.output_gain_db,
                snapshot.active_voices,
                app.max_voices,
                db(snapshot.peak),
                db(snapshot.rms)
            ))
            .style(Style::default().fg(DIM)),
            Rect::new(area.x, area.y + 2, area.width, 1),
        );
    }
}

fn root_text(snapshot: &Snapshot) -> String {
    match snapshot.background.harmony.root_transition {
        RootTransition::Steady { root } => format!("root {}", root.note_name()),
        RootTransition::Crossfading {
            from, to, progress, ..
        } => format!(
            "root {}>{} {:.0}%",
            from.note_name(),
            to.note_name(),
            progress.get() * 100.0
        ),
    }
}

fn tabs(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let width = (area.width / Tab::ALL.len() as u16).min(14);
    for (index, tab) in Tab::ALL.into_iter().enumerate() {
        let rectangle = Rect::new(area.x + index as u16 * width, area.y, width, 1);
        let style = if app.tab == tab {
            Style::default()
                .fg(BACKGROUND)
                .bg(CYAN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        frame.render_widget(
            Paragraph::new(format!(" {} {} ", index + 1, tab.label()))
                .alignment(Alignment::Center)
                .style(style),
            rectangle,
        );
        hits.add(rectangle, Control::Tab(tab));
    }
}

fn button(
    frame: &mut Frame<'_>,
    rectangle: Rect,
    label: &str,
    control: Control,
    selected: bool,
    app: &App,
    hits: &mut HitMap,
) {
    let focus = app.focus == control;
    let border_style = Style::default().fg(if focus {
        CYAN
    } else if selected {
        AMBER
    } else {
        DIM
    });
    let style = Style::default()
        .fg(if selected { AMBER } else { TEXT })
        .bg(if focus { FOCUSED } else { PANEL });
    let paragraph = Paragraph::new(label.to_owned())
        .alignment(Alignment::Center)
        .style(style);
    let paragraph = if rectangle.height >= 3 {
        paragraph.block(
            Block::new()
                .borders(Borders::ALL)
                .border_set(ASCII_BORDER)
                .border_style(border_style),
        )
    } else {
        paragraph
    };
    frame.render_widget(paragraph, rectangle);
    hits.add(rectangle, control);
}

fn buttons<const N: usize>(area: Rect) -> [Rect; N] {
    std::array::from_fn(|index| {
        let left = area.width as usize * index / N;
        let right = area.width as usize * (index + 1) / N;
        Rect::new(
            area.x + left as u16,
            area.y,
            (right - left).saturating_sub(1) as u16,
            area.height,
        )
    })
}

fn scene(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(frame, area, " SCENE / Modes, market, harmony, and ticker ");
    for (mode, rectangle) in [Mode::Browse, Mode::Dialog, Mode::Focus]
        .into_iter()
        .zip(buttons::<3>(Rect::new(inner.x, inner.y, inner.width, 1)))
    {
        button(
            frame,
            rectangle,
            match mode {
                Mode::Browse => "Browse / bunker",
                Mode::Dialog => "Dialog / distant",
                Mode::Focus => "Focus / thinned",
            },
            Control::SetMode(mode),
            app.mode == mode,
            app,
            hits,
        );
    }
    for (preset, rectangle) in Preset::ALL.into_iter().zip(buttons::<5>(Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        1,
    ))) {
        button(
            frame,
            rectangle,
            preset.label(),
            Control::Preset(preset),
            false,
            app,
            hits,
        );
    }
    let controls = buttons::<5>(Rect::new(inner.x, inner.y + 2, inner.width, 1));
    for ((spatial, label), rectangle) in [
        (SpatialMode::Pan, "Pan only"),
        (SpatialMode::Haas, "Haas spatial"),
    ]
    .into_iter()
    .zip(controls)
    {
        button(
            frame,
            rectangle,
            label,
            Control::SetSpatial(spatial),
            app.spatial == spatial,
            app,
            hits,
        );
    }
    for (rectangle, (kind, label)) in controls.into_iter().skip(2).zip([
        (SfxKind::OrderCreate, "Order create"),
        (SfxKind::OrderFill, "Order fill"),
        (SfxKind::Error, "Error"),
    ]) {
        button(
            frame,
            rectangle,
            label,
            Control::Fire(kind),
            false,
            app,
            hits,
        );
    }
    for ((mode, label), rectangle) in [
        (HarmonyMode::Diatonic, "Diatonic / A minor"),
        (HarmonyMode::Dissonant, "Dissonant / ticker"),
    ]
    .into_iter()
    .zip(buttons::<2>(Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        1,
    ))) {
        button(
            frame,
            rectangle,
            label,
            Control::SetHarmony(mode),
            app.harmony_mode == mode,
            app,
            hits,
        );
    }
    let navigation = Rect::new(inner.right() - 24, inner.y + 4, 24, 1);
    frame.render_widget(
        Paragraph::new("Ticker grid / click, Enter, or T").style(Style::default().fg(DIM)),
        Rect::new(inner.x, navigation.y, inner.width - navigation.width - 1, 1),
    );
    let remaining = app.ticker_navigation.remaining();
    let label = match remaining {
        Some(remaining) => format!("Auto ON / {}s", remaining.as_secs_f64().ceil() as u64),
        None => "Auto OFF".into(),
    };
    button(
        frame,
        navigation,
        &label,
        Control::ToggleTickerNavigation,
        remaining.is_some(),
        app,
        hits,
    );
    let columns = buttons::<6>(inner);
    for (index, symbol) in ticker_choices().enumerate() {
        let column = columns[index % columns.len()];
        button(
            frame,
            Rect::new(
                column.x,
                inner.y + 5 + (index / columns.len()) as u16,
                column.width,
                1,
            ),
            symbol.as_str(),
            Control::PickSymbol(symbol),
            app.symbol == symbol,
            app,
            hits,
        );
    }
    let log_top = inner.y + 5 + ticker_choices().count().div_ceil(columns.len()) as u16;
    let log_height = inner.bottom().saturating_sub(log_top);
    if log_height >= 2 {
        user_events(
            frame,
            Rect::new(inner.x, log_top, inner.width, log_height),
            app,
        );
    }
}

fn user_events(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(
        Paragraph::new("USER EVENTS / latest")
            .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let entries = app.event_log.entries();
    if entries.len() == 0 {
        frame.render_widget(
            Paragraph::new("No user events yet.").style(Style::default().fg(DIM)),
            Rect::new(area.x, area.y + 1, area.width, 1),
        );
        return;
    }
    let visible = entries.len().min(usize::from(area.height - 1));
    let skipped = entries.len() - visible;
    let first_row = area.bottom() - visible as u16;
    for (index, entry) in entries.skip(skipped).enumerate() {
        frame.render_widget(
            Paragraph::new(format!(
                "{:7.2}s {:4} {}",
                entry.at.as_secs_f64(),
                entry.source.label(),
                command_text(entry.command),
            ))
            .style(Style::default().fg(TEXT)),
            Rect::new(area.x, first_row + index as u16, area.width, 1),
        );
    }
}

fn command_text(command: Command) -> String {
    match command {
        Command::Trigger(event) => format!(
            "{:<18} pan {:+.2} / {:3.0}%",
            event.kind,
            event.pan.get(),
            event.intensity.get() * 100.0,
        ),
        Command::Mode(mode) => format!(
            "scene {}",
            match mode {
                Mode::Browse => "browse / bunker",
                Mode::Dialog => "dialog / distant",
                Mode::Focus => "focus / thinned",
            }
        ),
        Command::Market(market) => format!(
            "market {} | zoom {:3.0}% | volume {:3.0}% | volatility {:3.0}%",
            market.trend,
            market.zoom.get() * 100.0,
            market.volume.get() * 100.0,
            market.volatility.get() * 100.0,
        ),
        Command::Symbol(symbol) => format!("symbol {symbol}"),
        Command::Harmony(mode) => format!("harmony {mode}"),
        Command::Mix(mix) => format!(
            "mix master {:.0}% | UI {:.0}% | output {:+.1}dB",
            mix.master.get() * 100.0,
            mix.ui.get() * 100.0,
            mix.output_gain.db(),
        ),
        Command::Reverb(_) => "reverb settings updated".into(),
        Command::Spatial(mode) => format!("spatial {mode}"),
        Command::DataTransmission(transmission) => match transmission {
            DataTransmissionCommand::Start(settings) => {
                format!("transmission start | pan {:+.2}", settings.pan.get())
            }
            DataTransmissionCommand::Pan(pan) => format!("transmission pan {:+.2}", pan.get()),
            DataTransmissionCommand::End => "transmission end".into(),
        },
    }
}

struct SliderValue {
    position: f32,
    display: String,
}

impl SliderValue {
    fn percent(value: Unit) -> Self {
        Self {
            position: value.get(),
            display: format!("{:3.0}%", value.get() * 100.0),
        }
    }
    fn pan(value: Pan) -> Self {
        Self {
            position: (value.get() + 1.0) * 0.5,
            display: format!("{:+.2}", value.get()),
        }
    }
}

fn slider(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: SliderValue,
    control: Control,
    app: &App,
    hits: &mut HitMap,
) {
    let focus = app.focus == control;
    let value_width = if label.is_empty() {
        value.display.len() as u16 + 2
    } else if area.width < 32 {
        value.display.len() as u16 + 3
    } else {
        (value.display.len() as u16 + 4).max(10)
    };
    let bar_padding = if area.width < 32 { 6 } else { 7 };
    let label_chars = if label.is_empty() {
        0
    } else {
        16.min(area.width.saturating_sub(value_width + bar_padding)) as usize
    };
    let label_width = if label.is_empty() {
        1
    } else {
        label_chars as u16 + 3
    };
    let bar_width = area.width.saturating_sub(label_width + value_width);
    let filled = (value.position.clamp(0.0, 1.0) * bar_width as f32).round() as usize;
    let bar = format!(
        "{}{}",
        "#".repeat(filled),
        "-".repeat(bar_width as usize - filled)
    );
    let style = Style::default()
        .fg(if focus { CYAN } else { TEXT })
        .bg(if focus { FOCUSED } else { PANEL });
    let marker = if focus { ">" } else { " " };
    let text = if label.is_empty() {
        format!("{marker}[{bar}]{}", value.display)
    } else {
        format!(
            "{} {label:<label_chars$.label_chars$} [{bar}] {}",
            marker, value.display
        )
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
    hits.add(
        Rect::new(area.x + label_width + 1, area.y, bar_width, 1),
        control,
    );
}

fn market(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(frame, area, " MARKET / Current visible graph ");
    for (trend, rectangle) in [Trend::Bearish, Trend::Sideways, Trend::Bullish]
        .into_iter()
        .zip(buttons::<3>(Rect::new(inner.x, inner.y, inner.width, 3)))
    {
        button(
            frame,
            rectangle,
            match trend {
                Trend::Bearish => "Bearish",
                Trend::Sideways => "Sideways",
                Trend::Bullish => "Bullish",
            },
            Control::SetTrend(trend),
            app.market.trend == trend,
            app,
            hits,
        );
    }
    for (index, field) in MarketField::ALL.into_iter().enumerate() {
        let value = SliderValue::percent(field.get(app.market));
        slider(
            frame,
            Rect::new(inner.x, inner.y + 4 + index as u16, inner.width, 1),
            field.label(),
            value,
            Control::Market(field),
            app,
            hits,
        );
    }
    if inner.height >= 11 {
        frame.render_widget(
            Paragraph::new("Zoom adds distance; volume adds density; volatility adds tension.")
                .style(Style::default().fg(DIM)),
            Rect::new(inner.x + 1, inner.y + 10, inner.width - 2, 1),
        );
    }
}

fn mixer(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(frame, area, " MIXER / Independent layer levels ");
    let columns = buttons::<2>(inner);
    let rows = MixLayer::ALL.len().div_ceil(columns.len());
    for (index, layer) in MixLayer::ALL.into_iter().enumerate() {
        let column = columns[index / rows];
        let control = Control::Mix(layer);
        let gain = layer.get(app.mix).get();
        let value = SliderValue {
            position: app.numeric(control).expect("mixer faders are numeric").0,
            display: if gain == 0.0 {
                "mute".into()
            } else {
                format!("{:+.1}dB", 20.0 * gain.log10())
            },
        };
        let label = match layer {
            MixLayer::Master => "Master",
            MixLayer::Drone => "Drone",
            MixLayer::Sub => "Sub rumble",
            MixLayer::Texture => "Noise / glitches",
            MixLayer::Rain => "Rain",
            MixLayer::Harmony => "Harmony",
            MixLayer::Melody => "Distant melody",
            MixLayer::DemonicVoices => "Demonic voices",
            MixLayer::DemonicStutters => "Demonic stutters",
            MixLayer::FlyingCoins => "Flying coins",
            MixLayer::LaserBeams => "Laser beams",
            MixLayer::Ui => "UI / events",
            MixLayer::Transmission => "Transmission",
            MixLayer::Reverb => "Reverb return",
        };
        slider(
            frame,
            Rect::new(column.x, column.y + (index % rows) as u16, column.width, 1),
            label,
            value,
            control,
            app,
            hits,
        );
    }
    let (gain, low, high) = app
        .numeric(Control::OutputGain)
        .expect("output gain is numeric");
    slider(
        frame,
        Rect::new(inner.x, inner.y + rows as u16 + 1, inner.width, 1),
        "Output gain",
        SliderValue {
            position: (gain - low) / (high - low),
            display: format!("{gain:+.1} dB"),
        },
        Control::OutputGain,
        app,
        hits,
    );
    if inner.height >= rows as u16 + 5 {
        frame.render_widget(
            Paragraph::new("Perceptual faders; values show gain in dB. Zero mutes; full is unity.")
                .style(Style::default().fg(DIM)),
            Rect::new(inner.x + 1, inner.y + rows as u16 + 4, inner.width - 2, 1),
        );
    }
}

fn reverb(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(frame, area, " REVERB / Independent FX spaces ");
    let label_width = if inner.width >= 140 { 14 } else { 6 };
    let columns = buttons::<6>(Rect::new(
        inner.x + label_width,
        inner.y,
        inner.width - label_width,
        inner.height,
    ));
    for (index, field) in ReverbField::ALL.into_iter().enumerate() {
        frame.render_widget(
            Paragraph::new(field.label(label_width)).style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y + 2 + index as u16, label_width, 1),
        );
    }
    for (bus, column) in ReverbBus::ALL.into_iter().zip(columns) {
        frame.render_widget(
            Paragraph::new(bus.label(column.width).to_ascii_uppercase())
                .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Rect::new(column.x, column.y, column.width, 1),
        );
        let parameters = bus.get(app.reverb);
        for (index, field) in ReverbField::ALL.into_iter().enumerate() {
            slider(
                frame,
                Rect::new(column.x, column.y + 2 + index as u16, column.width, 1),
                "",
                SliderValue::percent(field.get(parameters)),
                Control::Reverb(bus, field),
                app,
                hits,
            );
        }
    }
    if inner.height >= 8 {
        frame.render_widget(
            Paragraph::new("Send feeds the space; wet sets its return. Room changes the tail.")
                .style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y + 7, inner.width, 1),
        );
    }
}

fn effects(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let palette_width = (area.width / 3).clamp(28, 48);
    effect_browser(
        frame,
        Rect::new(area.x, area.y, palette_width, area.height),
        app,
        hits,
    );
    inspector(
        frame,
        Rect::new(
            area.x + palette_width + 1,
            area.y,
            area.width - palette_width - 1,
            area.height,
        ),
        app,
        hits,
    );
}

fn effect_browser(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let selected = SfxKind::iter()
        .position(|kind| kind == app.effect)
        .expect("effect belongs to the palette");
    let total = SfxKind::iter().count();
    let inner = panel(
        frame,
        area,
        format!(" PALETTE / {} of {total} ", selected + 1),
    );
    let visible = inner.height.saturating_sub(3) as usize;
    let start = selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible));
    for (index, kind) in SfxKind::iter().skip(start).take(visible).enumerate() {
        let rectangle = Rect::new(inner.x, inner.y + index as u16, inner.width, 1);
        let style = Style::default()
            .fg(if kind == app.effect { CYAN } else { TEXT })
            .bg(if app.focus == Control::SelectEffect(kind) {
                FOCUSED
            } else {
                PANEL
            });
        frame.render_widget(
            Paragraph::new(format!(
                "{} {kind}",
                if kind == app.effect { ">" } else { " " }
            ))
            .style(style),
            rectangle,
        );
        hits.add(rectangle, Control::SelectEffect(kind));
    }
    compact_slider(
        frame,
        Rect::new(inner.x, inner.bottom() - 3, inner.width, 1),
        "Pan",
        SliderValue::pan(app.event_pan),
        Control::EventPan,
        app,
        hits,
    );
    compact_slider(
        frame,
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
        "Level",
        SliderValue::percent(app.event_intensity),
        Control::EventIntensity,
        app,
        hits,
    );
    let actions = buttons::<2>(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1));
    for (rectangle, label, control) in [
        (actions[0], "[ Play ]", Control::Trigger),
        (actions[1], "[ Expand i ]", Control::Inspect),
    ] {
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(CYAN).bg(if app.focus == control {
                FOCUSED
            } else {
                PANEL
            })),
            rectangle,
        );
        hits.add(rectangle, control);
    }
}

fn compact_slider(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    value: SliderValue,
    control: Control,
    app: &App,
    hits: &mut HitMap,
) {
    let bar_width = area.width.saturating_sub(14);
    let filled = (value.position.clamp(0.0, 1.0) * bar_width as f32).round() as usize;
    let bar = format!(
        "{}{}",
        "#".repeat(filled),
        "-".repeat(bar_width as usize - filled)
    );
    let style = Style::default()
        .fg(if app.focus == control { CYAN } else { TEXT })
        .bg(if app.focus == control { FOCUSED } else { PANEL });
    frame.render_widget(
        Paragraph::new(format!("{label:<5} [{bar}] {:>5}", value.display)).style(style),
        area,
    );
    hits.add(Rect::new(area.x + 7, area.y, bar_width, 1), control);
}

fn inspector(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let recipe = app.edited_recipe();
    let edits = app.current_edits();
    let recipe_label = if edits == shzx_sfx::SfxEdits::default() {
        "Nominal"
    } else {
        "Edited"
    };
    let compact = area.width < 72;
    let inner = panel(
        frame,
        area,
        if compact {
            format!(" BUILD / {} / {recipe_label} ", app.effect)
        } else {
            format!(
                " BUILD / {} / {recipe_label} / EQ{} {:.0}dB Q{:.2} ",
                app.effect,
                compact_hz(recipe.eq.center_hz),
                recipe.eq.gain_db,
                recipe.eq.q
            )
        },
    );
    let play_all = Rect::new(inner.right() - 13, inner.y, 13, 1);
    let recent = app
        .snapshot
        .and_then(|snapshot| snapshot.last_event)
        .filter(|event| event.kind == app.effect)
        .map(|event| match event.layers {
            SfxLayerSelection::All => "all",
            SfxLayerSelection::Only(layer) => layer.label(),
        });
    let mut summary = if compact {
        format!(
            "EQ{} {:.0}dB Q{:.2} | dB HP/LP",
            compact_hz(recipe.eq.center_hz),
            recipe.eq.gain_db,
            recipe.eq.q
        )
    } else {
        format!(
            "{} | {:.0}Hz/{:.0}ms",
            recipe.patch,
            recipe.pitch_hz,
            recipe.duration_seconds * 1000.0
        )
    };
    if let Some(layer) = recent {
        let last = format!(" | last {layer}");
        if summary.len() + last.len() <= usize::from(inner.width.saturating_sub(14)) {
            summary.push_str(&last);
        }
    }
    frame.render_widget(
        Paragraph::new(summary).style(Style::default().fg(DIM)),
        Rect::new(inner.x, inner.y, inner.width.saturating_sub(14), 1),
    );
    frame.render_widget(
        Paragraph::new("[ Play mix ]").style(Style::default().fg(CYAN).bg(
            if app.focus == Control::AuditionAll {
                FOCUSED
            } else {
                PANEL
            },
        )),
        play_all,
    );
    hits.add(play_all, Control::AuditionAll);

    for (index, layer) in recipe.layers.iter().enumerate() {
        let control = Control::AuditionLayer(layer.layer);
        let selected = app.inspector_layer == layer.layer;
        let muted = edits.layer(layer.layer).state == SfxLayerState::Muted;
        let available = app.output.sample_rate >= layer.minimum_sample_rate;
        let timing = if available {
            format!("{:.0}+{:.0}ms", layer.start_ms, layer.duration_ms)
        } else {
            format!("needs {}kHz", layer.minimum_sample_rate / 1000)
        };
        let row = Rect::new(inner.x, inner.y + index as u16 + 1, inner.width - 15, 1);
        let text = if compact || !available {
            let detail = if available {
                format!(
                    "{}/{}",
                    compact_hz(layer.highpass_hz),
                    compact_hz(layer.lowpass_hz)
                )
            } else {
                timing.clone()
            };
            format!(
                "{} {:<7} {:>5}dB {detail}",
                if selected { ">" } else { " " },
                layer.layer.label(),
                db(layer.level),
            )
        } else {
            format!(
                "{} {:<7} {:>6}dB {:<15} HP {:>5} LP {:>5} {timing}",
                if selected { ">" } else { " " },
                layer.layer.label(),
                db(layer.level),
                layer.generator,
                compact_hz(layer.highpass_hz),
                compact_hz(layer.lowpass_hz)
            )
        };
        frame.render_widget(
            Paragraph::new(text).style(
                Style::default()
                    .fg(if muted {
                        DIM
                    } else if selected {
                        CYAN
                    } else if layer.layer == SfxLayer::Texture {
                        AMBER
                    } else {
                        TEXT
                    })
                    .bg(if app.focus == control { FOCUSED } else { PANEL }),
            ),
            row,
        );
        hits.add(row, control);
        inline_button(
            frame,
            Rect::new(row.right() + 1, row.y, 8, 1),
            if muted { "[Unmute]" } else { "[Mute]" },
            Control::MuteLayer(layer.layer),
            app,
            hits,
        );
        inline_button(
            frame,
            Rect::new(row.right() + 9, row.y, 6, 1),
            "[Edit]",
            Control::EditLayer(layer.layer),
            app,
            hits,
        );
    }

    let selected = recipe
        .layers
        .iter()
        .find(|layer| layer.layer == app.inspector_layer)
        .expect("every selectable layer has a recipe");
    let source = selected.modulation.map_or_else(
        || {
            format!(
                "{}: {} | pitch {}",
                selected.layer.label(),
                selected.generator,
                recipe.pitch
            )
        },
        |modulation| format!("{}: {modulation}", selected.layer.label()),
    );
    let summary = selected.modulation.map_or_else(
        || source.clone(),
        |modulation| {
            format!(
                "{}: {}",
                selected.layer.label(),
                modulation_summary(modulation)
            )
        },
    );
    let compact_source = if summary.len() > usize::from(inner.width) {
        format!(
            "{}... [inspect CLI]",
            summary
                .chars()
                .take(usize::from(inner.width).saturating_sub(17))
                .collect::<String>()
        )
    } else {
        summary
    };
    let carrier = selected
        .carrier_hz
        .map_or_else(String::new, |hz| format!(" @{}Hz", compact_hz(hz)));
    let scope = match selected.layer {
        SfxLayer::Body | SfxLayer::Texture => "event",
        SfxLayer::Tonal => "one slot",
        SfxLayer::Flecks | SfxLayer::Shimmer | SfxLayer::Crisp => "per-part",
    };
    let window = format!(
        "{}{} | {scope} {:.1}+{:.1}ms | A{:.2}/R{:.2}ms | {}",
        selected.layer.label(),
        carrier,
        selected.start_ms,
        selected.duration_ms,
        selected.attack_ms,
        selected.release_ms,
        recipe.filter_gesture
    );
    let stereo = stereo_text(selected.stereo);
    let detail_offset = 1 + recipe.layers.len() as u16;
    let room = inner.height.saturating_sub(detail_offset);
    let detail_y = inner.y + detail_offset;
    if room == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(compact_source).style(Style::default().fg(AMBER)),
        Rect::new(inner.x, detail_y, inner.width, 1),
    );
    if room == 2 {
        let sequence = match selected.layer {
            SfxLayer::Texture => format!(" | Micro {}", recipe.micro_pattern.len()),
            SfxLayer::Tonal => format!(" | {}", tonal_rate_text(&recipe)),
            _ => String::new(),
        };
        let brief = format!(
            "{}{} {scope} | {stereo}{sequence}",
            selected.layer.label(),
            carrier,
        );
        frame.render_widget(
            Paragraph::new(brief).style(Style::default().fg(DIM)),
            Rect::new(inner.x, detail_y + 1, inner.width, 1),
        );
        return;
    }
    if room > 1 {
        frame.render_widget(
            Paragraph::new(stereo).style(Style::default().fg(CYAN)),
            Rect::new(inner.x, detail_y + 1, inner.width, 1),
        );
    }
    if room > 2 {
        frame.render_widget(
            Paragraph::new(window).style(Style::default().fg(DIM)),
            Rect::new(inner.x, detail_y + 2, inner.width, 1),
        );
    }
    if room > 3 {
        frame.render_widget(
            Paragraph::new(sequence_text(&recipe, selected, inner.width)),
            Rect::new(inner.x, detail_y + 3, inner.width, 1),
        );
    }
    if room > 4 {
        let env = recipe.envelope;
        frame.render_widget(
            Paragraph::new(format!(
                "Voice ADSR {:.2}/{:.1}/{:.0}%/{:.1} ms | gain {}dB",
                env.attack_ms,
                env.decay_ms,
                env.sustain * 100.0,
                env.release_ms,
                db(recipe.gain)
            ))
            .style(Style::default().fg(DIM)),
            Rect::new(inner.x, detail_y + 4, inner.width, 1),
        );
    }
    if room > 5 {
        frame.render_widget(
            Paragraph::new(macro_text(&recipe)).style(Style::default().fg(DIM)),
            Rect::new(inner.x, detail_y + 5, inner.width, 1),
        );
    }
    if room > 6 {
        let source_rows = (source.len() as u16).div_ceil(inner.width).min(room - 6);
        frame.render_widget(
            Paragraph::new(source)
                .style(Style::default().fg(AMBER))
                .wrap(Wrap { trim: true }),
            Rect::new(inner.x, detail_y + 6, inner.width, source_rows),
        );
        let note_row = detail_offset + 6 + source_rows;
        frame.render_widget(Paragraph::new("Click/Enter solos a layer. Play mix keeps layer mutes and edits.\nEdit adjusts layer level/filters and event pitch/duration. Each play varies the seed.")
            .style(Style::default().fg(DIM)).wrap(Wrap { trim: true }),
            Rect::new(inner.x, inner.y + note_row, inner.width, inner.height.saturating_sub(note_row)));
    }
}

fn inline_button(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    control: Control,
    app: &App,
    hits: &mut HitMap,
) {
    frame.render_widget(
        Paragraph::new(label.to_owned()).style(
            Style::default()
                .fg(CYAN)
                .bg(if app.focus == control { FOCUSED } else { PANEL }),
        ),
        area,
    );
    hits.add(area, control);
}

fn compact_hz(value: f32) -> String {
    if value >= 1000.0 {
        format!("{:.1}k", value / 1000.0)
    } else {
        format!("{value:.0}")
    }
}

fn stereo_text(stereo: SfxLayerStereo) -> String {
    format!("Stereo {stereo}")
}

fn tonal_rate_text(recipe: &SfxRecipe) -> String {
    let start = recipe.tonal_rate.start().get();
    let end = recipe.tonal_rate.end().get();
    if start == end {
        format!("{start:.0}Hz fixed")
    } else {
        format!("{start:.0}>{end:.0}Hz")
    }
}

fn modulation_summary(modulation: SfxModulation) -> String {
    match modulation {
        SfxModulation::Intermodulation {
            fm_deviation,
            pm_cycles,
            quantizer_step,
            ring_ratios,
            ..
        } => format!(
            "FM dev {fm_deviation:.2}x | PM {pm_cycles:.2}cy | crush {quantizer_step:.3} | ring {:.2}/{:.2}x",
            ring_ratios[0], ring_ratios[1]
        ),
        other => other.to_string(),
    }
}

fn macro_text(recipe: &SfxRecipe) -> String {
    let steps = recipe
        .phrase
        .iter()
        .map(|step| {
            format!(
                "{:.0}ms {:.2}x/{:.0}ms",
                step.start_fraction * recipe.duration_seconds * 1000.0,
                step.pitch_ratio,
                step.length_fraction * recipe.duration_seconds * 1000.0
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!("Macro {steps}")
}

fn sequence_text(recipe: &SfxRecipe, layer: &SfxLayerRecipe, width: u16) -> String {
    let sequence = match layer.layer {
        SfxLayer::Tonal => {
            let pitches = recipe
                .tonal_pitch_ratios
                .iter()
                .map(|ratio| format!("{ratio:.2}x"))
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "Tones {}Hz: {pitches} | {}",
                compact_hz(recipe.tonal_base_hz),
                tonal_rate_text(recipe)
            )
        }
        SfxLayer::Texture => {
            let pattern = recipe
                .micro_pattern
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "Micro {pattern} {:.0}-{:.0}Hz",
                recipe.detail_rate_hz[0], recipe.detail_rate_hz[1]
            )
        }
        SfxLayer::Body => format!("Body pitch {}", recipe.pitch),
        SfxLayer::Flecks => format!("{} timed fleck bursts per part", recipe.fleck_bursts.len()),
        SfxLayer::Shimmer => {
            let Some(SfxModulation::Phase { rate_hz, .. }) = layer.modulation else {
                unreachable!("shimmer uses phase modulation")
            };
            format!("Phase tone {rate_hz:.2}Hz modulation")
        }
        SfxLayer::Crisp => {
            let Some(SfxModulation::WideIntermodulation { ratchet_hz, .. }) = layer.modulation
            else {
                unreachable!("crisp uses a wide intermodulation gesture")
            };
            format!("Spectral envelope | accents {ratchet_hz:.0}Hz")
        }
    };
    let macro_steps = recipe
        .phrase
        .iter()
        .map(|step| {
            format!(
                "{:.0}:{}x",
                step.start_fraction * recipe.duration_seconds * 1000.0,
                compact_ratio(step.pitch_ratio)
            )
        })
        .collect::<Vec<_>>()
        .join("/");
    let full = format!("{sequence} | M {macro_steps}");
    if full.len() <= usize::from(width) {
        return full;
    }
    let short = match layer.layer {
        SfxLayer::Texture => format!(
            "Micro {} slots {:.0}-{:.0}Hz | M{}",
            recipe.micro_pattern.len(),
            recipe.detail_rate_hz[0],
            recipe.detail_rate_hz[1],
            recipe.phrase.len()
        ),
        SfxLayer::Tonal => format!(
            "Tones {}Hz, {} steps {} | M{}",
            compact_hz(recipe.tonal_base_hz),
            recipe.tonal_pitch_ratios.len(),
            tonal_rate_text(recipe),
            recipe.phrase.len()
        ),
        _ => format!("{sequence} | M{}", recipe.phrase.len()),
    };
    short.chars().take(width as usize).collect()
}

fn compact_ratio(value: f32) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn monitor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let inner = panel(frame, area, " MONITOR / Background NOW > TARGET ");
    let Some(snapshot) = app.snapshot else {
        frame.render_widget(
            Paragraph::new(
                "Waiting for the first audio callback. Current and target values appear here.",
            )
            .style(Style::default().fg(DIM))
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    };
    let background = snapshot.background;
    let left_width = inner.width / 2;
    let left = Rect::new(inner.x, inner.y, left_width, 3);
    let right = Rect::new(
        inner.x + left_width + 1,
        inner.y,
        inner.width - left_width - 1,
        3,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(format!(
                "Trend {}>{} | strength {}",
                trend_short(background.market.trend),
                trend_short(snapshot.market.trend),
                unit_pair(
                    background.market.trend_strength,
                    snapshot.market.trend_strength
                )
            )),
            Line::raw(format!(
                "Volume {} | volatility {}",
                unit_pair(background.market.volume, snapshot.market.volume),
                unit_pair(background.market.volatility, snapshot.market.volatility)
            )),
            Line::raw(format!(
                "Zoom depth {} | activity {}",
                unit_pair(background.market.zoom, snapshot.market.zoom),
                unit_pair(background.market.activity, snapshot.market.activity)
            )),
        ]),
        left,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw(focus_room_text(background.focus, background.reverb_room)),
            Line::raw(format!(
                "LP {:.0} Hz>{:.0} Hz | HP {:.0}>{:.0}",
                background.lowpass_hz.current,
                background.lowpass_hz.target,
                background.highpass_hz.current,
                background.highpass_hz.target
            )),
            Line::raw(format!(
                "Width {:.0}>{:.0}% | swell {:.0}% | {}",
                background.reverb_width.current * 100.0,
                background.reverb_width.target * 100.0,
                background.harmonic_swell.get() * 100.0,
                snapshot.mode
            )),
        ]),
        right,
    );
    frame.render_widget(
        Paragraph::new(arrangement_lines(&snapshot, inner.width).to_vec()),
        Rect::new(inner.x, inner.y + 3, inner.width, 4),
    );
    frame.render_widget(
        Paragraph::new(delay_lines(&snapshot).to_vec()),
        Rect::new(inner.x, inner.y + 7, inner.width, 2),
    );
    frame.render_widget(
        Paragraph::new(ambient_lines(&snapshot).to_vec()),
        Rect::new(inner.x, inner.y + 9, inner.width, 2),
    );
    if inner.height > 11 {
        frame.render_widget(
            Paragraph::new(rain_line(&snapshot)),
            Rect::new(inner.x, inner.y + 11, inner.width, 1),
        );
    }

    if inner.height > 12 {
        let harmony_help = match background.harmony.mode {
            HarmonyMode::Diatonic => {
                "The next chord is a preview: new market inputs can change it before the clock is due.\nDialog and Focus hold harmony selection and melodic entrances while their clocks keep running."
            }
            HarmonyMode::Dissonant => {
                "Dissonant harmony is fixed by the ticker. Market, Dialog, and Focus reshape the texture without changing its notes.\nFocus can still hold melodic entrances."
            }
        };
        frame.render_widget(
            Paragraph::new(format!(
                "NOW is the smoothed state used by the audio callback. TARGET is its latest accepted input.\n{harmony_help}"
            ))
            .style(Style::default().fg(DIM))
            .wrap(Wrap { trim: true }),
            Rect::new(inner.x, inner.y + 12, inner.width, inner.height - 12),
        );
    }
}

fn harmony_text(transition: ChordTransition, width: u16) -> String {
    match transition {
        ChordTransition::Steady { chord } => format!("HARMONY {chord} | steady"),
        ChordTransition::Crossfading {
            from,
            to,
            progress,
            remaining_seconds,
        } => {
            let filled = (progress.get() * 10.0).round() as usize;
            let full = format!(
                "HARMONY {from} > {to} [{}{}] {:3.0}% | {remaining_seconds:.2}s left",
                "#".repeat(filled),
                "-".repeat(10 - filled),
                progress.get() * 100.0
            );
            if full.len() <= width as usize {
                full
            } else {
                format!(
                    "HARMONY {from} > {to} {:.0}% | {remaining_seconds:.2}s left",
                    progress.get() * 100.0
                )
            }
        }
    }
}

fn arrangement_lines(snapshot: &Snapshot, width: u16) -> [Line<'static>; 4] {
    let background = snapshot.background;
    [
        Line::styled(
            harmony_text(background.harmony.transition, width),
            Style::default().fg(CYAN),
        ),
        Line::styled(
            format!(
                "NEXT (preview) {} | {}",
                background.harmony.next,
                clock(background.harmony.clock)
            ),
            Style::default().fg(AMBER),
        ),
        Line::raw(if width >= 76 {
            format!(
                "DETAIL {} | next {:.2}s | generated {}",
                voice(background.details.voice),
                background.details.next_in_seconds,
                background.details.generated
            )
        } else {
            format!(
                "DETAIL {} | next {:.2}s | #{}",
                voice_short(background.details.voice),
                background.details.next_in_seconds,
                background.details.generated
            )
        }),
        Line::raw(if width >= 76 {
            format!(
                "MELODY {} | {} | generated {}",
                voice(background.melody.voice),
                clock(background.melody.clock),
                background.melody.generated
            )
        } else {
            format!(
                "MELODY {} | {} | #{}",
                voice_short(background.melody.voice),
                clock_short(background.melody.clock),
                background.melody.generated
            )
        }),
    ]
}

fn ambient_text(label: &str, source: AmbientSnapshot) -> String {
    format!("{label} {} / {}", source.active, clock_short(source.clock))
}

fn ambient_lines(snapshot: &Snapshot) -> [Line<'static>; 2] {
    let state = snapshot.background;
    [
        Line::raw(format!(
            "{} | {}",
            ambient_text("Demonic voices", state.demonic_voices),
            ambient_text("Stutters", state.demonic_stutters),
        )),
        Line::raw(format!(
            "{} | {}",
            ambient_text("Flying coins", state.flying_coins),
            ambient_text("Lasers", state.laser_beams),
        )),
    ]
}

fn rain_line(snapshot: &Snapshot) -> Line<'static> {
    let rain = snapshot.background.rain;
    Line::raw(format!(
        "RAIN LP{:.0}>{:.0}Hz dry{:.0}>{:.0}% wet{:.0}>{:.0}% car{:.0}>{:.0}%",
        rain.exterior_lowpass_hz.current,
        rain.exterior_lowpass_hz.target,
        rain.dry_gain.current * 100.0,
        rain.dry_gain.target * 100.0,
        rain.reverb_gain.current * 100.0,
        rain.reverb_gain.target * 100.0,
        rain.cabin_mix.current * 100.0,
        rain.cabin_mix.target * 100.0,
    ))
}

fn monitor_column(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let inner = panel(frame, area, " BACKGROUND MONITOR / NOW > TARGET ");
    let Some(snapshot) = app.snapshot else {
        frame.render_widget(
            Paragraph::new("Waiting for the first audio callback...")
                .style(Style::default().fg(DIM))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    };
    let background = snapshot.background;
    let mut lines = vec![
        Line::styled(
            format!(
                "{} | {}",
                snapshot.background.harmony.mode.label(),
                root_text(&snapshot)
            ),
            Style::default().fg(DIM),
        ),
        Line::raw(format!(
            "Trend {}>{} | str {} | swell {:.0}%",
            trend_short(background.market.trend),
            trend_short(snapshot.market.trend),
            unit_pair(
                background.market.trend_strength,
                snapshot.market.trend_strength
            ),
            background.harmonic_swell.get() * 100.0
        )),
    ];
    for [first, second] in [
        [MarketField::Volume, MarketField::Volatility],
        [MarketField::Zoom, MarketField::Activity],
    ] {
        lines.push(Line::raw(format!(
            "{} {} | {} {}",
            first.label(),
            unit_pair(first.get(background.market), first.get(snapshot.market)),
            second.label(),
            unit_pair(second.get(background.market), second.get(snapshot.market)),
        )));
    }
    lines.extend([
        Line::raw(format!(
            "{} | width {:.0}>{:.0}% | {}",
            focus_room_text(background.focus, background.reverb_room),
            background.reverb_width.current * 100.0,
            background.reverb_width.target * 100.0,
            snapshot.mode
        )),
        parameter("Low-pass", background.lowpass_hz, hz),
        parameter("High-pass", background.highpass_hz, hz),
    ]);
    lines.extend(arrangement_lines(&snapshot, inner.width));
    lines.extend(delay_lines(&snapshot));
    lines.extend(ambient_lines(&snapshot));
    lines.push(rain_line(&snapshot));
    if inner.height > 17 {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "New view inputs may change the next chord preview.",
            Style::default().fg(DIM),
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn monitor_strip(frame: &mut Frame<'_>, area: Rect, app: &App) {
    frame.render_widget(Block::new().style(Style::default().bg(PANEL)), area);
    let Some(snapshot) = app.snapshot else {
        frame.render_widget(
            Paragraph::new("BACKGROUND MONITOR / Waiting for the first audio callback...")
                .style(Style::default().fg(CYAN)),
            area,
        );
        return;
    };
    let state = snapshot.background;
    let lines = vec![
        Line::styled(
            format!(
                "MONITOR NOW>TARGET | zoom {} | vol {} | activity {}",
                unit_pair(state.market.zoom, snapshot.market.zoom),
                unit_pair(state.market.volume, snapshot.market.volume),
                unit_pair(state.market.activity, snapshot.market.activity)
            ),
            Style::default().fg(CYAN),
        ),
        Line::raw(format!(
            "Voices{} {} | Stutters{} {} | Coins{} {} | Lasers{} {}",
            state.demonic_voices.active,
            clock_short(state.demonic_voices.clock),
            state.demonic_stutters.active,
            clock_short(state.demonic_stutters.clock),
            state.flying_coins.active,
            clock_short(state.flying_coins.clock),
            state.laser_beams.active,
            clock_short(state.laser_beams.clock),
        )),
        Line::raw(format!(
            "LP {:.0}>{:.0}Hz | HP {:.0}>{:.0}Hz | focus {:.0}>{:.0}% | room {:.2}>{:.2} | W {:.0}>{:.0}%",
            state.lowpass_hz.current,
            state.lowpass_hz.target,
            state.highpass_hz.current,
            state.highpass_hz.target,
            state.focus.current * 100.0,
            state.focus.target * 100.0,
            state.reverb_room.current,
            state.reverb_room.target,
            state.reverb_width.current * 100.0,
            state.reverb_width.target * 100.0
        )),
        Line::styled(harmony_clock_text(&snapshot), Style::default().fg(CYAN)),
        Line::raw(format!(
            "DETAIL {} #{} due {:.1}s | {}",
            voice_short(state.details.voice),
            state.details.generated,
            state.details.next_in_seconds,
            delay_text("DLY", state.detail_delay)
        )),
        Line::raw(format!(
            "MELODY {} #{} {} | {}",
            voice_short(state.melody.voice),
            state.melody.generated,
            clock_short(state.melody.clock),
            delay_text("DLY", state.melody_delay)
        )),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn unit_pair(current: Unit, target: Unit) -> String {
    format!("{:.0}>{:.0}%", current.get() * 100.0, target.get() * 100.0)
}

fn trend_short(trend: Trend) -> &'static str {
    match trend {
        Trend::Bearish => "bear",
        Trend::Sideways => "flat",
        Trend::Bullish => "bull",
    }
}

fn focus_room_text(focus: ParameterSnapshot, room: ParameterSnapshot) -> String {
    format!(
        "Focus {:.0}>{:.0}% | room {:.2}>{:.2}",
        focus.current * 100.0,
        focus.target * 100.0,
        room.current,
        room.target
    )
}

fn delay_text(label: &str, delay: DelaySnapshot) -> String {
    format!(
        "{label} {:.0}>{:.0}ms FB{:.0}>{:.0}% SND{:.0}>{:.0}%",
        delay.time_ms.current,
        delay.time_ms.target,
        delay.feedback.current * 100.0,
        delay.feedback.target * 100.0,
        delay.send.current * 100.0,
        delay.send.target * 100.0
    )
}

fn delay_lines(snapshot: &Snapshot) -> [Line<'static>; 2] {
    [
        Line::styled(
            delay_text("DETAIL DELAY", snapshot.background.detail_delay),
            Style::default().fg(DIM),
        ),
        Line::styled(
            delay_text("MELODY DELAY", snapshot.background.melody_delay),
            Style::default().fg(DIM),
        ),
    ]
}

fn harmony_clock_text(snapshot: &Snapshot) -> String {
    let harmony = snapshot.background.harmony;
    let current = match harmony.transition {
        ChordTransition::Steady { chord } => format!("HARMONY {chord} steady"),
        ChordTransition::Crossfading {
            from,
            to,
            progress,
            remaining_seconds,
        } => format!(
            "HARMONY {from} > {to} {:.0}% {remaining_seconds:.2}s left",
            progress.get() * 100.0
        ),
    };
    format!(
        "{current} | NEXT {} {}",
        harmony.next,
        clock_short(harmony.clock)
    )
}

fn voice_short(value: BackgroundVoice) -> String {
    match value {
        BackgroundVoice::Silent => "idle".into(),
        BackgroundVoice::Playing { frequency_hz, .. } => format!("{frequency_hz:.0}Hz"),
    }
}

fn clock_short(value: ClockState) -> String {
    match value {
        ClockState::Fixed => "fixed".into(),
        ClockState::Scheduled { in_seconds } => format!("due {in_seconds:.1}s"),
        ClockState::Held { recheck_in_seconds } => format!("HELD {recheck_in_seconds:.1}s"),
    }
}

fn comparison(label: &str, current: String, target: String) -> Line<'static> {
    Line::from(vec![
        Span::raw(format!("{label:<14}")),
        Span::styled(format!("{current:>9}"), Style::default().fg(CYAN)),
        Span::raw(" "),
        Span::styled(format!("{target:>9}"), Style::default().fg(AMBER)),
    ])
}

fn parameter(
    label: &str,
    value: ParameterSnapshot,
    format: impl Fn(f32) -> String,
) -> Line<'static> {
    comparison(label, format(value.current), format(value.target))
}

fn hz(value: f32) -> String {
    format!("{value:.0} Hz")
}

fn clock(value: ClockState) -> String {
    match value {
        ClockState::Fixed => "fixed".into(),
        ClockState::Scheduled { in_seconds } => format!("due {in_seconds:.2}s"),
        ClockState::Held { recheck_in_seconds } => {
            format!("HELD / recheck {recheck_in_seconds:.2}s")
        }
    }
}

fn voice(value: BackgroundVoice) -> String {
    match value {
        BackgroundVoice::Silent => "idle".into(),
        BackgroundVoice::Playing {
            frequency_hz,
            remaining_seconds,
        } => format!("{frequency_hz:.0} Hz / {remaining_seconds:.2}s left"),
    }
}

fn telemetry(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let lines = match app.snapshot {
        Some(snapshot) => {
            let last = snapshot.last_event.map_or_else(
                || "LAST  No UI sound generated yet.".into(),
                |event| {
                    format!(
                        "LAST  {} | {:.1} Hz / {:.0} ms | LP {:.0} Hz / gain {:.3}",
                        event.kind,
                        event.pitch_hz,
                        event.duration_seconds * 1000.0,
                        event.cutoff_hz,
                        event.gain
                    )
                },
            );
            vec![
                Line::styled(
                    format!(
                        "LIVE  {:8.2}s | {} | LP {:.0} Hz | focus {:3.0}% / zoom {:3.0}%",
                        snapshot.time_seconds,
                        snapshot.chord,
                        snapshot.cutoff_hz,
                        snapshot.focus_amount * 100.0,
                        snapshot.zoom_amount * 100.0
                    ),
                    Style::default().fg(CYAN),
                ),
                Line::raw(format!(
                    "POOL  {} / {} voices | generated {} / stolen {} / skipped {}",
                    snapshot.active_voices,
                    app.max_voices,
                    snapshot.generated_events,
                    snapshot.stolen_voices,
                    snapshot.coalesced_events
                )),
                Line::styled(
                    format!(
                        "LEVEL Peak {} dBFS / RMS {} dBFS (since start) | {} callbacks",
                        db(snapshot.peak),
                        db(snapshot.rms),
                        app.callback_stats.callbacks
                    ),
                    Style::default().fg(DIM),
                ),
                Line::raw(last),
            ]
        }
        None => vec![Line::styled(
            "LIVE  Waiting for the first audio callback...",
            Style::default().fg(CYAN),
        )],
    };
    frame.render_widget(Paragraph::new(lines), area);
}

fn db(value: f32) -> String {
    if value > 0.0 {
        format!("{:.1}", value.log10() * 20.0)
    } else {
        "-inf".into()
    }
}

fn footer(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    frame.render_widget(
        Paragraph::new(
            " Tab/S-Tab focus | Arrows adjust | Enter act | Space play | 1-7 tabs | ? help",
        )
        .style(Style::default().fg(DIM)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let mut x = area.x;
    for (control, label) in [
        (
            Control::ToggleMute,
            if app.mix.master.get() == 0.0 {
                "[ Unmute (m) ]"
            } else {
                "[ Mute (m) ]"
            },
        ),
        (Control::Reset, "[ Reset (r) ]"),
        (
            Control::ToggleUserActivity,
            if app.activity.enabled() {
                "[ User sim ON (u) ]"
            } else {
                "[ User sim OFF (u) ]"
            },
        ),
        (Control::Quit, "[ Quit (q) ]"),
    ] {
        let rectangle = Rect::new(x, area.y + 1, label.len() as u16, 1);
        frame.render_widget(
            Paragraph::new(label).style(
                Style::default()
                    .fg(
                        if control == Control::ToggleUserActivity && app.activity.enabled() {
                            AMBER
                        } else {
                            CYAN
                        },
                    )
                    .bg(if app.focus == control {
                        FOCUSED
                    } else {
                        BACKGROUND
                    }),
            ),
            rectangle,
        );
        hits.add(rectangle, control);
        x += rectangle.width + 1;
    }
    let hint = if app.activity.enabled() {
        app.activity.description()
    } else {
        "Mouse: click / drag"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(DIM)),
        Rect::new(x + 1, area.y + 1, area.right().saturating_sub(x + 1), 1),
    );
}

fn small_terminal(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(frame, area, " SHZX / SOUND LAB ");
    frame.render_widget(Paragraph::new(format!("Resize the terminal to at least 80 x 24.\nCurrent size: {} x {}\nAudio continues playing. Press q or Esc to quit.", area.width, area.height)).style(Style::default().fg(AMBER)).wrap(Wrap { trim: true }), inner);
    if inner.height >= 5 {
        let rectangle = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(13), 1);
        frame.render_widget(
            Paragraph::new("[ Quit (q) ]").style(Style::default().fg(
                if app.focus == Control::Quit {
                    AMBER
                } else {
                    CYAN
                },
            )),
            rectangle,
        );
        hits.add(rectangle, Control::Quit);
    }
}

fn help(frame: &mut Frame<'_>, area: Rect) {
    let width = area.width.min(76);
    let height = area.height.min(24);
    let rectangle = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rectangle);
    let inner = panel(frame, rectangle, " SOUND LAB / Controls ");
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                "Navigate",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Line::raw("1-7  Scene / Market / Mixer / SFX / Monitor / Reverb / Inspect"),
            Line::raw("Scene: auto tickers default ON; random 25..180s; toggle by grid."),
            Line::raw("Tab / Shift-Tab Move focus between controls"),
            Line::raw("Up / Down       Move focus; browse sounds in the SFX list"),
            Line::raw("Left / Right    Adjust a focused value"),
            Line::raw("Mixer faders are perceptual; readouts show dB. Shift gives finer steps."),
            Line::raw("Enter           Activate the focused button"),
            Line::raw("Space           Audition the selected effect"),
            Line::raw("I / Inspect     Show layers; PgUp/Down browses inspected sounds"),
            Line::raw("Inspector Mute/Edit buttons change layers for the next play."),
            Line::styled(
                "Mouse",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Line::raw("Click a sound to play; click/drag value bars to adjust."),
            Line::raw(""),
            Line::styled(
                "Global",
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            ),
            Line::raw("M mute/unmute master   R reset scene   Q quit"),
            Line::raw("U toggle user simulator (starts ON): an abstract trading terminal."),
            Line::raw("Spatial SFX and scene/zoom/market changes; orders are rare."),
            Line::raw("? help   Esc or ? closes this help"),
            Line::raw(""),
            Line::styled(
                "Controls send live commands; harmony changes on its musical clock.",
                Style::default().fg(DIM),
            ),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}

fn symbol_entry(frame: &mut Frame<'_>, area: Rect, app: &App) -> HitMap {
    let width = area.width.min(72);
    let height = area.height.min(14);
    let rectangle = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rectangle);
    let inner = panel(frame, rectangle, " SYMBOL / Ticker harmony ");
    let mut hits = HitMap::default();
    if inner.is_empty() {
        return hits;
    }
    let button_height = if inner.height >= 9 { 3 } else { 1 };
    let actions = Rect::new(
        inner.x,
        inner.bottom() - button_height,
        inner.width,
        button_height,
    );
    if inner.height >= 9 {
        frame.render_widget(
            Paragraph::new(format!(
                "Current: {} | {}",
                app.symbol,
                app.harmony_mode.label()
            ))
            .style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(format!("{}_", app.symbol_input))
                .style(Style::default().fg(CYAN))
                .block(
                    Block::new()
                        .title(" New ticker ")
                        .borders(Borders::ALL)
                        .border_set(ASCII_BORDER)
                        .border_style(Style::default().fg(CYAN)),
                ),
            Rect::new(inner.x, inner.y + 1, inner.width, 3),
        );
        frame.render_widget(
            Paragraph::new("Enter applies; Esc cancels. Ctrl-U clears.")
                .style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
        );
        frame.render_widget(
            Paragraph::new(app.notice.as_str())
                .style(Style::default().fg(AMBER))
                .wrap(Wrap { trim: true }),
            Rect::new(
                inner.x,
                inner.y + 5,
                inner.width,
                actions.y.saturating_sub(inner.y + 5),
            ),
        );
    } else {
        frame.render_widget(
            Paragraph::new(format!("{}_: Enter applies, Esc cancels", app.symbol_input))
                .style(Style::default().fg(CYAN))
                .wrap(Wrap { trim: true }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                actions.y.saturating_sub(inner.y),
            ),
        );
    }
    for ((control, label), button_area) in [
        (Control::CommitSymbol, "Apply / Enter"),
        (Control::CancelSymbol, "Cancel / Esc"),
    ]
    .into_iter()
    .zip(buttons::<2>(actions))
    {
        if button_height == 3 {
            button(
                frame,
                button_area,
                label,
                control,
                control == Control::CommitSymbol,
                app,
                &mut hits,
            );
        } else {
            frame.render_widget(
                Paragraph::new(label).style(Style::default().fg(CYAN)),
                button_area,
            );
            hits.add(button_area, control);
        }
    }
    hits
}

fn sfx_editor(frame: &mut Frame<'_>, area: Rect, app: &App, page: EditorPage) -> HitMap {
    let width = area.width.min(72);
    let height = area.height.min(18);
    let rectangle = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rectangle);
    let inner = panel(frame, rectangle, format!(" EDIT / {} ", app.effect));
    let mut hits = HitMap::default();
    if inner.height < 16 || inner.width < 50 {
        frame.render_widget(
            Paragraph::new("Enlarge the terminal to edit. Esc closes.")
                .style(Style::default().fg(AMBER))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return hits;
    }
    let muted = app.current_edits().layer(app.inspector_layer).state == SfxLayerState::Muted;
    let tonal = app.inspector_layer == SfxLayer::Tonal;
    let recipe = app.edited_recipe();
    let selected = recipe
        .layers
        .iter()
        .find(|layer| layer.layer == app.inspector_layer)
        .expect("every selectable layer has a recipe");
    frame.render_widget(
        Paragraph::new(format!(
            "{} layer{} | {}",
            app.inspector_layer.label(),
            if muted { " / MUTED" } else { "" },
            stereo_text(selected.stereo),
        ))
        .style(Style::default().fg(if muted { AMBER } else { CYAN })),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    if tonal {
        for (page_control, rectangle) in EditorPage::ALL.into_iter().zip(buttons::<4>(Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            1,
        ))) {
            frame.render_widget(
                Paragraph::new(format!("[ {} ]", page_control.label()))
                    .alignment(Alignment::Center)
                    .style(
                        Style::default()
                            .fg(if page == page_control { AMBER } else { CYAN })
                            .bg(if app.focus == Control::SfxEditorPage(page_control) {
                                FOCUSED
                            } else {
                                PANEL
                            }),
                    ),
                rectangle,
            );
            hits.add(rectangle, Control::SfxEditorPage(page_control));
        }
    }
    for (index, parameter) in app.editor_parameters().enumerate() {
        let control = Control::SfxParameter(parameter);
        let (current, low, high) = app.numeric(control).expect("SFX parameters are numeric");
        let display = match parameter {
            SfxParameter::Level => format!("{:.1}%", current * 100.0),
            SfxParameter::Highpass | SfxParameter::Lowpass => format!("{current:.0} Hz"),
            SfxParameter::Pitch | SfxParameter::Duration => format!("{current:.2}x"),
            SfxParameter::Tonal(parameter) => match parameter {
                TonalParameter::Carrier => {
                    format!("{current:.2}x / {:.0}Hz", recipe.tonal_base_hz)
                }
                TonalParameter::FmIndex => format!("{current:.3}"),
                TonalParameter::FmRatio => format!("{current:.2}x"),
                TonalParameter::RateStart | TonalParameter::RateEnd => format!("{current:.0} Hz"),
                TonalParameter::Attack | TonalParameter::Release => format!("{current:.2} ms"),
                TonalParameter::StepPitch(_) => {
                    format!("{current:.2}x / {:.0}Hz", recipe.tonal_base_hz * current)
                }
            },
        };
        slider(
            frame,
            Rect::new(inner.x, inner.y + index as u16 + 2, inner.width, 1),
            parameter.label(),
            SliderValue {
                position: if low == high {
                    0.0
                } else {
                    (current - low) / (high - low)
                },
                display,
            },
            control,
            app,
            &mut hits,
        );
    }
    if page == EditorPage::Rate {
        frame.render_widget(
            Paragraph::new("Clock rate ramps linearly from start to end within each phrase.\nEqual endpoints keep it fixed. Rates count pitch steps per second.")
                .style(Style::default().fg(CYAN)),
            Rect::new(inner.x, inner.y + 5, inner.width, 2),
        );
    }
    let explanation = if tonal {
        "Sine + sine FM -> six pitch steps -> slot attack/release\n-> event envelope -> HP/LP. Mix pitch/duration affect the whole event."
    } else {
        "Level / HP / LP apply to this layer.\nPitch / duration affect the whole event."
    };
    frame.render_widget(
        Paragraph::new(explanation).style(Style::default().fg(CYAN)),
        Rect::new(inner.x, inner.y + 8, inner.width, 2),
    );
    frame.render_widget(
        Paragraph::new(if tonal {
            "Arrows adjust; Shift fine; Space plays Tonal; M mute; Esc closes.\nPgUp/Down pages. Edits apply next play and stay for this session."
        } else {
            "Arrows adjust; Shift fine; Space plays mix; M mute; Esc closes.\nEdits apply on the next play and stay for this session."
        })
        .style(Style::default().fg(DIM)),
        Rect::new(inner.x, inner.y + 10, inner.width, 2),
    );
    frame.render_widget(
        Paragraph::new(app.notice.as_str())
            .style(Style::default().fg(AMBER))
            .wrap(Wrap { trim: true }),
        Rect::new(inner.x, inner.y + 12, inner.width, 1),
    );
    let actions = Rect::new(inner.x, inner.bottom() - 3, inner.width, 3);
    let action_count = app.editor_actions().count();
    for (index, (control, label)) in app.editor_actions().enumerate() {
        let left = actions.width as usize * index / action_count;
        let right = actions.width as usize * (index + 1) / action_count;
        let rectangle = Rect::new(
            actions.x + left as u16,
            actions.y,
            (right - left - 1) as u16,
            actions.height,
        );
        button(frame, rectangle, label, control, false, app, &mut hits);
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use shzx_sfx::{
        Chord, ChordShape, MarketContext, OutputInfo, RenderConfig, Renderer, ScaleDegree,
    };

    fn app() -> App {
        App::new(
            RenderConfig::default(),
            OutputInfo {
                device: "Test output".into(),
                sample_rate: 48_000,
                channels: 2,
                sample_format: "f32".into(),
                buffer_size: BufferSize::SystemDefault,
            },
        )
    }

    fn effects_fixture(snapshot: &mut Snapshot) {
        snapshot.background.reverb_width = ParameterSnapshot {
            current: 0.9,
            target: 0.2,
        };
        snapshot.background.detail_delay = DelaySnapshot {
            time_ms: ParameterSnapshot {
                current: 120.0,
                target: 240.0,
            },
            feedback: ParameterSnapshot {
                current: 0.2,
                target: 0.4,
            },
            send: ParameterSnapshot {
                current: 0.15,
                target: 0.3,
            },
        };
        snapshot.background.melody_delay = DelaySnapshot {
            time_ms: ParameterSnapshot {
                current: 430.0,
                target: 900.0,
            },
            feedback: ParameterSnapshot {
                current: 0.45,
                target: 0.6,
            },
            send: ParameterSnapshot {
                current: 0.3,
                target: 0.5,
            },
        };
    }

    #[test]
    fn mixer_mouse_positions_match_the_perceptual_bar_and_db_readout() {
        for (width, height) in [(80, 24), (140, 24), (280, 36)] {
            let mut app = app();
            app.select_tab(Tab::Mixer);
            app.mix.master = Unit::new(0.125).unwrap();
            app.mix.demonic_stutters = Unit::ZERO;
            let control = Control::Mix(MixLayer::Master);
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            let region = hits.region(control).unwrap();
            let buffer = terminal.backend().buffer();
            let bar = (region.x..region.right())
                .map(|x| buffer[(x, region.y)].symbol())
                .collect::<String>();
            assert_eq!(
                bar.chars().filter(|&ch| ch == '#').count(),
                (region.width as f32 * 0.5).round() as usize
            );
            let row = (0..width)
                .map(|x| buffer[(x, region.y)].symbol())
                .collect::<String>();
            assert!(row.contains("-18.1dB"), "{width}x{height}: {row}");
            let muted = hits
                .region(Control::Mix(MixLayer::DemonicStutters))
                .unwrap();
            let row = (0..width)
                .map(|x| buffer[(x, muted.y)].symbol())
                .collect::<String>();
            assert!(row.contains("mute"), "{width}x{height}: {row}");
            let mut send = |_| Ok(());
            super::super::slide(&mut app, &hits, control, region.x, &mut send);
            assert_eq!(app.mix.master, Unit::ZERO);
            super::super::slide(&mut app, &hits, control, region.right() - 1, &mut send);
            assert_eq!(app.mix.master, Unit::ONE);
            // A drag's middle cell can straddle the exact half-position.
            let middle = region.x + (region.width - 1) / 2;
            super::super::slide(&mut app, &hits, control, middle, &mut send);
            assert!(app.mix.master.get() > 0.06 && app.mix.master.get() <= 0.125);
            let position = app.numeric(control).unwrap().0;
            let step = App::numeric_step(control, false);
            app.adjust(control, step, &mut send);
            assert!((app.numeric(control).unwrap().0 - position - step).abs() < 1e-6);
        }
    }

    #[test]
    fn every_tab_keeps_all_focusable_controls_accessible_in_supported_sizes() {
        let mut app = app();
        for (width, height) in [
            (80, 24),
            (100, 32),
            (120, 40),
            (140, 24),
            (150, 24),
            (160, 36),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for tab in Tab::ALL {
                app.select_tab(tab);
                let mut hits = HitMap::default();
                terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
                for control in app
                    .focus_order()
                    .into_iter()
                    .chain(Tab::ALL.map(Control::Tab))
                {
                    let rectangle = hits.region(control).expect("focusable control is visible");
                    assert_eq!(hits.at(rectangle.x, rectangle.y), Some(control));
                }
                for (index, hit) in hits.0.iter().enumerate() {
                    assert!(hit.area.right() <= width && hit.area.bottom() <= height);
                    assert!(!hit.area.is_empty());
                    for other in &hits.0[index + 1..] {
                        assert!(!hit.area.intersects(other.area), "mouse targets overlap");
                    }
                }
            }
        }
    }

    #[test]
    fn background_monitor_stays_visible_beside_environment_control_tabs() {
        let mut app = app();
        let mut snapshot = Renderer::new(RenderConfig::default()).unwrap().snapshot();
        snapshot.market.zoom = Unit::ONE;
        snapshot.background.market.zoom = Unit::new(0.25).unwrap();
        snapshot.background.lowpass_hz = ParameterSnapshot {
            current: 900.0,
            target: 650.0,
        };
        snapshot.background.harmony.transition = ChordTransition::Crossfading {
            from: Chord::new(ScaleDegree::I, ChordShape::Subdominant),
            to: Chord::new(ScaleDegree::I, ChordShape::Tension),
            progress: Unit::new(0.25).unwrap(),
            remaining_seconds: 2.25,
        };
        snapshot.chord = Chord::new(ScaleDegree::I, ChordShape::Tension);
        snapshot.cutoff_hz = 900.0;
        snapshot.zoom_amount = 0.25;
        effects_fixture(&mut snapshot);
        app.market = snapshot.market;
        app.snapshot = Some(snapshot);
        for (width, height) in [
            (80, 24),
            (100, 32),
            (140, 24),
            (150, 24),
            (160, 36),
            (280, 36),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for (tab, title) in [
                (Tab::Scene, "SCENE /"),
                (Tab::Market, "MARKET /"),
                (Tab::Mixer, "MIXER /"),
                (Tab::Reverb, "REVERB /"),
                (Tab::Inspect, "BUILD /"),
            ] {
                app.select_tab(tab);
                let mut hits = HitMap::default();
                terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
                let buffer = terminal.backend().buffer();
                let text = (0..height)
                    .map(|y| {
                        (0..width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                for expected in [
                    title,
                    "MONITOR",
                    "TARGET",
                    "HARMONY Dm(add9)/A > A(color)",
                    "25%",
                    "2.25s left",
                    "NEXT",
                    "DETAIL",
                    "MELODY",
                    "120>240ms FB20>40% SND15>30%",
                    "430>900ms FB45>60% SND30>50%",
                    "90>20%",
                ] {
                    assert!(
                        text.contains(expected),
                        "missing {expected:?} for {tab:?} at {width}x{height}:\n{text}"
                    );
                }
                for expected in if width >= 140 {
                    [
                        "Demonic voices 0",
                        "Stutters 0",
                        "Flying coins 0",
                        "Lasers 0",
                    ]
                } else {
                    ["Voices0", "Stutters0", "Coins0", "Lasers0"]
                } {
                    assert!(
                        text.contains(expected),
                        "missing ambient activity {expected:?}"
                    );
                }
                if tab == Tab::Mixer {
                    assert!(
                        text.contains("Output gain")
                            && text.contains(&format!("{:+.1} dB", app.mix.output_gain.db()))
                    );
                    assert!(hits.region(Control::OutputGain).is_some());
                }
                if tab == Tab::Reverb {
                    for expected in [
                        "ENVIRONMENT",
                        "UI / EVENTS",
                        "MELODY",
                        "ACCENTS",
                        "RAIN",
                        "Room",
                        "Damp",
                        "Width",
                        "Send",
                        "Wet",
                    ] {
                        assert!(
                            text.contains(expected),
                            "missing reverb control {expected:?}"
                        );
                    }
                    assert!(text.contains("TRANSMISSION") || text.contains("TRANSMIT"));
                    for full_label in ["Room / tail", "Damping", "Wet level"] {
                        assert_eq!(
                            text.contains(full_label),
                            width == 280,
                            "incorrect reverb label {full_label:?} at {width}x{height}:\n{text}"
                        );
                    }
                    for bus in ReverbBus::ALL {
                        for field in ReverbField::ALL {
                            let hit = hits
                                .region(Control::Reverb(bus, field))
                                .expect("every reverb slider stays clickable");
                            assert!(hit.width >= 3, "{bus:?} {field:?} at {width}x{height}");
                        }
                    }
                }
                let monitor = if width >= 140 {
                    let controls = (width * 11 / 20).max(80).min(width - 59);
                    assert!(text.contains("900 Hz") && text.contains("650 Hz"));
                    Rect::new(controls + 1, 4, width - controls - 1, height - 7)
                } else {
                    assert!(text.contains("900>650Hz") && text.contains("zoom 25>100%"));
                    Rect::new(1, height - 9, width - 2, 6)
                };
                assert!(
                    hits.0.iter().all(|hit| !hit.area.intersects(monitor)),
                    "control hit regions overlap the persistent monitor"
                );
            }
        }
    }

    #[test]
    fn palette_uses_a_right_hand_inspector_with_every_component_solo_at_all_sizes() {
        let mut app = app();
        app.effect = SfxKind::Error;
        app.select_tab(Tab::Sfx);
        app.inspector_layer = SfxLayer::Crisp;
        app.focus = Control::AuditionLayer(SfxLayer::Crisp);
        app.snapshot = Some(Renderer::new(RenderConfig::default()).unwrap().snapshot());
        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "PALETTE /",
                "BUILD / error",
                "Nominal",
                "Play mix",
                "EQ3.0k -9dB",
                "Crisp",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
            let left_width = (width / 3).clamp(28, 48);
            let left = Rect::new(0, 4, left_width, height - 7);
            let right = Rect::new(left_width + 1, 4, width - left_width - 1, height - 7);
            for control in [
                Control::SelectEffect(app.effect),
                Control::EventPan,
                Control::EventIntensity,
                Control::Trigger,
                Control::Inspect,
            ] {
                let region = hits
                    .region(control)
                    .expect("palette controls remain accessible");
                assert_eq!(
                    region.intersection(left),
                    region,
                    "{control:?} escaped the left pane"
                );
            }
            assert!(hits.region(Control::EventPan).unwrap().width >= 10);
            for control in
                [Control::AuditionAll]
                    .into_iter()
                    .chain(SfxLayer::ALL.into_iter().flat_map(|layer| {
                        [
                            Control::AuditionLayer(layer),
                            Control::MuteLayer(layer),
                            Control::EditLayer(layer),
                        ]
                    }))
            {
                let region = hits
                    .region(control)
                    .expect("every source component can be auditioned");
                assert_eq!(
                    region.intersection(right),
                    region,
                    "{control:?} escaped the inspector pane"
                );
                assert_eq!(hits.at(region.x, region.y), Some(control));
            }
            for (index, hit) in hits.0.iter().enumerate() {
                assert_eq!(
                    hit.area.intersection(Rect::new(0, 0, width, height)),
                    hit.area
                );
                assert!(
                    hits.0[index + 1..]
                        .iter()
                        .all(|other| !hit.area.intersects(other.area)),
                    "inspector and palette mouse targets overlap at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn inspector_shows_layer_mutes_and_editor_captures_mouse_targets() {
        use shzx_sfx::{SfxFilterBand, SfxLayerEdits, SfxScale};

        let mut app = app();
        app.effect = SfxKind::Confirm;
        app.inspector_layer = SfxLayer::Tonal;
        let mut edits = app.current_edits();
        edits.pitch = SfxScale::new(1.25).unwrap();
        edits.duration = SfxScale::new(2.0).unwrap();
        edits.set_layer(
            SfxLayer::Tonal,
            SfxLayerEdits {
                state: SfxLayerState::Muted,
                level: Some(Unit::new(0.2).unwrap()),
                filter: Some(SfxFilterBand::new(1_200.0, 4_000.0).unwrap()),
            },
        );
        app.effect_edits.insert(app.effect, edits);
        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            for tab in [Tab::Sfx, Tab::Inspect] {
                app.select_tab(tab);
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| {
                        draw(frame, &app);
                    })
                    .unwrap();
                let text: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect();
                assert!(text.contains("Edited") && text.contains("[Unmute]"));
                let recipe = app.edited_recipe();
                let tonal = recipe
                    .layers
                    .iter()
                    .find(|layer| layer.layer == SfxLayer::Tonal)
                    .unwrap();
                assert!(text.contains(&stereo_text(tonal.stereo)));
                assert!(text.contains(&tonal_rate_text(&recipe)));

                for page in EditorPage::ALL {
                    app.overlay = Overlay::SfxEditor(page);
                    let parameter = app.editor_parameters().next().unwrap();
                    app.focus = Control::SfxParameter(parameter);
                    let mut hits = HitMap::default();
                    terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
                    let text: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    for expected in [
                        "Tonal layer / MUTED",
                        "this session",
                        "Sine + sine FM",
                        "Play tone",
                        "Play mix",
                        "Stereo L/R +/-4ct; rate +/-0.15%",
                    ] {
                        assert!(
                            text.contains(expected),
                            "missing {expected:?} at {width}x{height}"
                        );
                    }
                    for parameter in app.editor_parameters() {
                        assert!(
                            text.contains(parameter.label()),
                            "missing label for {parameter:?}"
                        );
                    }
                    match page {
                        EditorPage::Mix => {
                            for expected in [
                                "20.0%",
                                "1200 Hz",
                                "4000 Hz",
                                "1.25x",
                                "2.00x",
                                "Reset event",
                            ] {
                                assert!(text.contains(expected), "missing {expected:?}");
                            }
                        }
                        EditorPage::Tone => {
                            for expected in ["1.600", "4.70x", "0.12 ms", "0.35 ms", "Reset tone"] {
                                assert!(text.contains(expected), "missing {expected:?}");
                            }
                        }
                        EditorPage::Rate => {
                            let recipe = app.edited_recipe();
                            for expected in [
                                format!("{:.0} Hz", recipe.tonal_rate.start().get()),
                                format!("{:.0} Hz", recipe.tonal_rate.end().get()),
                                "ramps linearly".into(),
                                "Equal endpoints".into(),
                            ] {
                                assert!(text.contains(&expected), "missing {expected:?}");
                            }
                        }
                        EditorPage::Sequence => {
                            let recipe = app.edited_recipe();
                            for step in shzx_sfx::SfxTonalStep::ALL {
                                let ratio = recipe.tonal_pitch_ratio(step);
                                let expected =
                                    format!("{ratio:.2}x / {:.0}Hz", recipe.tonal_base_hz * ratio);
                                assert!(text.contains(&expected), "missing {expected:?}");
                            }
                        }
                    }
                    for control in app.focus_order() {
                        let hit = hits
                            .region(control)
                            .expect("editor focus has a visible mouse target");
                        assert_eq!(hits.at(hit.x, hit.y), Some(control));
                    }
                    assert!(hits.region(Control::MuteLayer(SfxLayer::Tonal)).is_none());
                    assert!(hits.region(Control::SelectEffect(app.effect)).is_none());
                    for (index, hit) in hits.0.iter().enumerate() {
                        assert_eq!(
                            hit.area.intersection(Rect::new(0, 0, width, height)),
                            hit.area
                        );
                        assert!(
                            hits.0[index + 1..]
                                .iter()
                                .all(|other| !hit.area.intersects(other.area))
                        );
                    }
                    app.overlay = Overlay::None;
                }
            }
        }
    }

    #[test]
    fn palette_scrolls_to_the_selected_effect_and_help_blocks_background_hits() {
        let mut app = app();
        app.tab = Tab::Sfx;
        app.effect = SfxKind::Disconnect;
        app.focus = Control::SelectEffect(app.effect);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut hits = HitMap::default();
        terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
        assert!(
            hits.region(Control::SelectEffect(SfxKind::Disconnect))
                .is_some()
        );
        let pan = hits.region(Control::EventPan).unwrap();
        assert!(pan.width >= 10);
        app.overlay = Overlay::Help;
        terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
        assert!(hits.0.is_empty());
    }

    #[test]
    fn inspector_uses_render_recipe_values_and_keeps_solo_controls_visible() {
        let mut app = app();
        app.effect = SfxKind::Error;
        app.select_tab(Tab::Inspect);
        app.inspector_layer = SfxLayer::Texture;
        app.focus = Control::AuditionLayer(SfxLayer::Texture);
        let recipe = app.effect.recipe();
        for (width, height) in [(80, 24), (100, 32), (160, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "BUILD / error",
                "Nominal",
                "Play mix",
                "Texture",
                "intermodulation",
                "FM",
                "PM",
                "crush",
                "event",
                "Micro",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
            assert!(hits.region(Control::AuditionAll).is_some());
            assert!(
                text.contains(&recipe.patch.to_string()),
                "actual patch parameters must be visible"
            );
            let texture = recipe
                .layers
                .iter()
                .find(|layer| layer.layer == SfxLayer::Texture)
                .unwrap();
            assert!(text.contains(&stereo_text(texture.stereo)));
            assert!(text.contains(&format!("@{}Hz", compact_hz(texture.carrier_hz.unwrap()))));
            for layer in recipe.layers {
                assert!(text.contains(layer.layer.label()));
                assert!(text.contains(&format!("{}dB", db(layer.level))));
                let region = hits.region(Control::AuditionLayer(layer.layer)).unwrap();
                assert_eq!(
                    hits.at(region.x, region.y),
                    Some(Control::AuditionLayer(layer.layer))
                );
            }
            if height > 24 {
                for field in ["sync", "pulse", "fold", "Voice ADSR", "Macro"] {
                    assert!(
                        text.contains(field),
                        "expanded inspector lost {field:?}:\n{text}"
                    );
                }
            }
        }
        app.output.sample_rate = 8_000;
        app.inspector_layer = SfxLayer::Shimmer;
        app.focus = Control::AuditionLayer(SfxLayer::Shimmer);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        let shimmer = recipe
            .layers
            .iter()
            .find(|layer| layer.layer == SfxLayer::Shimmer)
            .unwrap();
        assert!(text.contains(&format!("needs {}kHz", shimmer.minimum_sample_rate / 1000)));
        assert!(text.contains(&format!("@{}Hz", compact_hz(shimmer.carrier_hz.unwrap()))));
        assert!(text.contains(&stereo_text(shimmer.stereo)));
    }

    #[test]
    fn monitor_distinguishes_current_targets_crossfade_and_held_schedulers() {
        let mut app = app();
        app.harmony_mode = HarmonyMode::Diatonic;
        app.select_tab(Tab::Monitor);
        let mut snapshot = Renderer::new(RenderConfig {
            harmony_mode: HarmonyMode::Diatonic,
            ..RenderConfig::default()
        })
        .unwrap()
        .snapshot();
        snapshot.market = MarketContext {
            zoom: Unit::ONE,
            trend: Trend::Bullish,
            ..MarketContext::default()
        };
        snapshot.background.market.zoom = Unit::new(0.25).unwrap();
        snapshot.background.lowpass_hz = ParameterSnapshot {
            current: 900.0,
            target: 650.0,
        };
        snapshot.background.harmony.transition = ChordTransition::Crossfading {
            from: Chord::new(ScaleDegree::I, ChordShape::Sus2),
            to: Chord::new(ScaleDegree::I, ChordShape::Add9),
            progress: Unit::new(0.25).unwrap(),
            remaining_seconds: 2.25,
        };
        snapshot.background.harmony.next = Chord::new(ScaleDegree::I, ChordShape::Submediant);
        snapshot.background.harmony.clock = ClockState::Held {
            recheck_in_seconds: 8.0,
        };
        snapshot.background.details.generated = 7;
        snapshot.background.details.voice = BackgroundVoice::Playing {
            frequency_hz: 440.0,
            remaining_seconds: 0.04,
        };
        snapshot.background.melody.clock = ClockState::Held {
            recheck_in_seconds: 8.0,
        };
        effects_fixture(&mut snapshot);
        app.snapshot = Some(snapshot);
        for (width, height) in [(80, 24), (120, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            let lines: Vec<_> = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            let text = lines.join("\n");
            for expected in [
                "MONITOR /",
                "NOW",
                "TARGET",
                "900 Hz",
                "650 Hz",
                "HARMONY Asus2 > Am(add9)",
                "25%",
                "2.25s left",
                "NEXT (preview) Fmaj7/A",
                "HELD / recheck 8.00s",
                "DETAIL 440 Hz / 0.04s left",
                "generated 7",
                "MELODY idle",
                "DETAIL DELAY 120>240ms FB20>40% SND15>30%",
                "MELODY DELAY 430>900ms FB45>60% SND30>50%",
                "Width 90>20%",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
            let zoom_row = lines
                .iter()
                .find(|line| line.contains("Zoom depth"))
                .unwrap();
            assert!(zoom_row.contains("25>100%"), "{zoom_row}");
        }
    }

    #[test]
    fn small_terminal_still_offers_a_quit_button() {
        let app = app();
        let mut terminal = Terminal::new(TestBackend::new(50, 12)).unwrap();
        let mut hits = HitMap::default();
        terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
        assert!(hits.region(Control::Quit).is_some());
    }

    #[test]
    fn symbol_editor_shows_the_draft_and_blocks_background_mouse_targets() {
        let mut app = app();
        app.focus = Control::Symbol;
        app.activate(Control::Symbol, &mut |_| Ok(()));
        app.symbol_input = "NVDA".into();
        app.notice = "Invalid symbols remain editable.".into();
        for (width, height) in [(80, 24), (140, 24), (160, 36), (50, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            assert_eq!(hits.0.len(), 2);
            assert!(hits.region(Control::CommitSymbol).is_some());
            assert!(hits.region(Control::CancelSymbol).is_some());
            assert!(hits.region(Control::Symbol).is_none());
            assert!(
                hits.0
                    .iter()
                    .all(|hit| hit.area.right() <= width && hit.area.bottom() <= height)
            );
            let buffer = terminal.backend().buffer();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "NVDA_",
                "Apply / Enter",
                "Cancel / Esc",
                HarmonyMode::Dissonant.label(),
                "Invalid symbols",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
        }
    }
    #[test]
    fn scene_ticker_grid_shows_all_choices_and_routes_each_mouse_target() {
        let mut app = app();
        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(ticker_choices().count(), 24);
            for symbol in ticker_choices() {
                let control = Control::PickSymbol(symbol);
                let target = hits.region(control).expect("ticker has a mouse target");
                assert!(target.right() <= width && target.bottom() <= height);
                assert_eq!(hits.at(target.x, target.y), Some(control));
                assert!(
                    text.contains(symbol.as_str()),
                    "missing {symbol} at {width}x{height}"
                );
                let mut sent = Vec::new();
                app.activate(control, &mut |command| {
                    sent.push(command);
                    Ok(())
                });
                assert_eq!(sent, [Command::Symbol(symbol)]);
                assert_eq!(app.symbol, symbol);
            }
        }
    }

    #[test]
    fn ticker_navigation_toggle_and_countdown_share_the_grid_heading_at_all_sizes() {
        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            let mut app = app();
            if app.activity.enabled() {
                app.activity.toggle();
            }
            let initial = app.ticker_navigation.remaining().unwrap();
            app.advance(std::time::Duration::from_secs(1), &mut |_| {
                panic!("the first ticker interval is at least 25 seconds")
            });
            assert_eq!(
                app.ticker_navigation.remaining(),
                Some(initial - std::time::Duration::from_secs(1))
            );
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for enabled in [true, false, true] {
                let remaining = app.ticker_navigation.remaining();
                assert_eq!(remaining.is_some(), enabled);
                let expected = match remaining {
                    Some(remaining) => {
                        format!("Auto ON / {}s", remaining.as_secs_f64().ceil() as u64)
                    }
                    None => "Auto OFF".into(),
                };
                let mut hits = HitMap::default();
                terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
                let control = Control::ToggleTickerNavigation;
                let target = hits.region(control).expect("auto ticker toggle is visible");
                assert_eq!(target.width, 24);
                assert!(target.right() <= width && target.bottom() <= height);
                assert_eq!(hits.at(target.x, target.y), Some(control));
                assert!(app.focus_order().contains(&control));
                let first_ticker = hits
                    .region(Control::PickSymbol(ticker_choices().next().unwrap()))
                    .unwrap();
                assert_eq!(target.y + 1, first_ticker.y);
                let row = (0..width)
                    .map(|x| terminal.backend().buffer()[(x, target.y)].symbol())
                    .collect::<String>();
                assert!(row.contains("Ticker grid / click, Enter, or T"));
                assert!(row.contains(&expected), "{width}x{height}: {row}");
                app.activate(control, &mut |_| panic!("toggling navigation is local"));
            }
            app.overlay = Overlay::Help;
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap();
            let text = (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|position| terminal.backend().buffer()[position].symbol())
                .collect::<String>();
            assert!(text.contains("auto tickers default ON; random 25..180s"));
        }
    }

    #[test]
    fn scene_event_log_uses_spare_rows_below_tickers_and_follows_the_latest_events() {
        use shzx_sfx::SfxEvent;
        use std::time::Duration;

        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            let mut app = app();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            let mut hits = HitMap::default();
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            let title_row = hits
                .region(Control::PickSymbol(ticker_choices().last().unwrap()))
                .unwrap()
                .y
                + 1;
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            if (width, height) == (80, 24) {
                assert!(!text.contains("USER EVENTS"));
            } else {
                assert!(text.contains("USER EVENTS / latest"));
                assert!(text.contains("No user events yet."));
            }
            for index in 0..40 {
                app.event_log.advance(Duration::from_secs(index));
                app.submit(
                    Command::Trigger(SfxEvent {
                        kind: if index == 39 {
                            SfxKind::Confirm
                        } else {
                            SfxKind::Click
                        },
                        pan: Pan::new(-0.65).unwrap(),
                        intensity: Unit::new(0.42).unwrap(),
                        ..SfxEvent::new(SfxKind::Click)
                    }),
                    &mut |_| Ok(()),
                );
            }
            terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
            if (width, height) == (80, 24) {
                continue;
            }
            let bottom_row = height - if width >= 140 { 9 } else { 11 };
            let log = Rect::new(
                1,
                title_row,
                if width >= 140 {
                    (width * 11 / 20).max(80).min(width - 59) - 2
                } else {
                    width - 2
                },
                bottom_row - title_row + 1,
            );
            assert!(hits.0.iter().all(|hit| !hit.area.intersects(log)));
            let row = |y| {
                (log.x..log.right())
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            };
            assert!(row(title_row).contains("USER EVENTS / latest"));
            let latest = row(bottom_row);
            for expected in ["39.00s", "USER", "confirm", "pan -0.65", "42%"] {
                assert!(
                    latest.contains(expected),
                    "missing {expected:?} at {width}x{height}: {latest}"
                );
            }
            let visible = bottom_row - title_row;
            let first = 40 - u64::from(visible);
            assert!(row(title_row + 1).contains(&format!("{first:.2}s", first = first as f64)));
            let log_text = (title_row + 1..=bottom_row)
                .map(row)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(!log_text.contains("  0.00s"));
            let monitor_row = if width >= 140 { 4 } else { height - 9 };
            let monitor = (0..width)
                .map(|x| terminal.backend().buffer()[(x, monitor_row)].symbol())
                .collect::<String>();
            assert!(
                monitor.contains("BACKGROUND MONITOR"),
                "{width}x{height}: {monitor}"
            );
        }
    }

    #[test]
    fn scene_event_log_shows_simulated_scene_market_and_deep_zoom_commands() {
        use super::super::events::EventSource;
        use std::time::Duration;

        for (width, height) in [(80, 32), (160, 36)] {
            let mut app = app();
            for (index, command) in [
                Command::Mode(Mode::Browse),
                Command::Mode(Mode::Dialog),
                Command::Mode(Mode::Focus),
                Command::Market(MarketContext {
                    trend: Trend::Bearish,
                    zoom: Unit::new(0.95).unwrap(),
                    volume: Unit::new(0.7).unwrap(),
                    volatility: Unit::new(0.6).unwrap(),
                    ..MarketContext::default()
                }),
            ]
            .into_iter()
            .enumerate()
            {
                app.event_log.advance(Duration::from_secs(index as u64));
                app.event_log.record(EventSource::Simulator, command);
            }
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            for expected in [
                "scene browse / bunker",
                "scene dialog / distant",
                "scene focus / thinned",
                "market bearish",
                "zoom  95%",
                "volume  70%",
                "volatility  60%",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
            assert_eq!(text.matches("SIM").count(), 4);
        }
    }

    #[test]
    fn user_activity_toggle_stays_visible_and_accessible_at_all_supported_sizes() {
        for (width, height) in [(80, 24), (100, 32), (140, 24), (160, 36)] {
            let mut app = app();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for enabled in [true, false, true] {
                assert_eq!(app.activity.enabled(), enabled);
                for tab in Tab::ALL {
                    app.select_tab(tab);
                    let mut hits = HitMap::default();
                    terminal.draw(|frame| hits = draw(frame, &app)).unwrap();
                    let control = Control::ToggleUserActivity;
                    let target = hits
                        .region(control)
                        .expect("user simulator toggle is visible");
                    assert_eq!(target.y, height - 1);
                    assert!(target.right() <= width);
                    assert_eq!(hits.at(target.x, target.y), Some(control));
                    assert!(app.focus_order().contains(&control));
                    assert_eq!(
                        terminal.backend().buffer()[(target.x, target.y)].fg,
                        if enabled { AMBER } else { CYAN }
                    );
                    let row = (0..width)
                        .map(|x| terminal.backend().buffer()[(x, target.y)].symbol())
                        .collect::<String>();
                    let expected = if enabled {
                        "User sim ON"
                    } else {
                        "User sim OFF"
                    };
                    assert!(row.contains(expected), "{width}x{height}: {row}");
                    assert!(row.contains("Quit (q)"), "{width}x{height}: {row}");
                }
                app.activate(Control::ToggleUserActivity, &mut |_| {
                    panic!("toggling the user simulator is local")
                });
            }
            app.overlay = Overlay::Help;
            terminal
                .draw(|frame| {
                    draw(frame, &app);
                })
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            for expected in [
                "U toggle user simulator (starts ON)",
                "abstract trading terminal",
                "orders are rare",
                "Esc or ? closes this help",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn dissonant_monitor_and_symbol_editor_show_the_mode_and_fixed_clock() {
        let mut app = app();
        app.harmony_mode = HarmonyMode::Dissonant;
        app.snapshot = Some(
            Renderer::new(RenderConfig {
                harmony_mode: HarmonyMode::Dissonant,
                ..RenderConfig::default()
            })
            .unwrap()
            .snapshot(),
        );
        app.select_tab(Tab::Monitor);
        let mut terminal = Terminal::new(TestBackend::new(80, 36)).unwrap();
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains(HarmonyMode::Dissonant.label()));
        assert!(text.contains("fixed"));
        assert!(text.contains("fixed by the ticker"));
        assert!(!text.contains("The next chord is a preview"));
        assert!(!text.contains(HarmonyMode::Diatonic.label()));
        app.activate(Control::Symbol, &mut |_| Ok(()));
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains(HarmonyMode::Dissonant.label()));
    }
}
