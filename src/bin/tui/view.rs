use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use shzx_sfx::{
    AudioStatus, BackgroundVoice, BufferSize, ChordTransition, ClockState, DelaySnapshot,
    GLOBAL_SCALE, Mode, Pan, ParameterSnapshot, RootTransition, SfxKind, SfxLayer, SfxLayerRecipe,
    SfxLayerSelection, SfxModulation, SfxRecipe, Snapshot, SpatialMode, Trend, Unit,
};
use strum::IntoEnumIterator;

use super::{App, Control, MarketField, Overlay, Preset, ReverbBus, ReverbField, Tab};
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
        if app.tab == Tab::Monitor {
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
                " {GLOBAL_SCALE} | {} | gain {:+.0}dB | v{}/{} | P{} R{} dBFS",
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
        RootTransition::Steady { root } => format!("root {root}"),
        RootTransition::Crossfading {
            from, to, progress, ..
        } => format!("root {from}>{to} {:.0}%", progress.get() * 100.0),
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
    frame.render_widget(
        Paragraph::new(label.to_owned())
            .alignment(Alignment::Center)
            .block(
                Block::new()
                    .borders(Borders::ALL)
                    .border_set(ASCII_BORDER)
                    .border_style(border_style),
            )
            .style(style),
        rectangle,
    );
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
    let inner = panel(frame, area, " SCENE / Mode, market presets, spatial sound ");
    let row_step = if inner.height < 11 { 3 } else { 4 };
    for (mode, rectangle) in [Mode::Browse, Mode::Dialog, Mode::Focus]
        .into_iter()
        .zip(buttons::<3>(Rect::new(inner.x, inner.y, inner.width, 3)))
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
        inner.y + row_step,
        inner.width,
        3,
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
    let bottom = buttons::<5>(Rect::new(inner.x, inner.y + row_step * 2, inner.width, 3));
    for (index, (spatial, label)) in [
        (SpatialMode::Pan, "Pan only"),
        (SpatialMode::Haas, "Haas spatial"),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            frame,
            bottom[index],
            label,
            Control::SetSpatial(spatial),
            app.spatial == spatial,
            app,
            hits,
        );
    }
    for (rectangle, (kind, label)) in bottom.into_iter().skip(2).zip([
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
    if inner.height > 12 {
        frame.render_widget(Paragraph::new("Presets shape the visible market; mode changes only the background character.\nHarmony evolves slowly. UI effects stay direct when a dialog is open.").style(Style::default().fg(DIM)), Rect::new(inner.x + 1, inner.y + 12, inner.width - 2, inner.height - 12));
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
    let label_width = 19_u16;
    let value_width = (value.display.len() as u16 + 4).max(10);
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
    frame.render_widget(
        Paragraph::new(format!(
            "{} {label:<16} [{bar}] {}",
            if focus { ">" } else { " " },
            value.display
        ))
        .style(style),
        area,
    );
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
    for (index, layer) in MixLayer::ALL.into_iter().enumerate() {
        let value = SliderValue::percent(layer.get(app.mix));
        let label = match layer {
            MixLayer::Master => "Master",
            MixLayer::Drone => "Drone",
            MixLayer::Texture => "Noise / glitches",
            MixLayer::Harmony => "Harmony",
            MixLayer::Melody => "Distant melody",
            MixLayer::Ui => "UI / events",
            MixLayer::Reverb => "Reverb return",
        };
        slider(
            frame,
            Rect::new(inner.x, inner.y + 1 + index as u16, inner.width, 1),
            label,
            value,
            Control::Mix(layer),
            app,
            hits,
        );
    }
    let (gain, low, high) = app
        .numeric(Control::OutputGain)
        .expect("output gain is numeric");
    slider(
        frame,
        Rect::new(inner.x, inner.y + 8, inner.width, 1),
        "Output gain",
        SliderValue {
            position: (gain - low) / (high - low),
            display: format!("{gain:+.1} dB"),
        },
        Control::OutputGain,
        app,
        hits,
    );
    if inner.height >= 11 {
        frame.render_widget(
            Paragraph::new("Gains are linear; zero silences a layer. Changes are smoothed.")
                .style(Style::default().fg(DIM)),
            Rect::new(inner.x + 1, inner.y + 10, inner.width - 2, 1),
        );
    }
}

fn reverb(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let inner = panel(
        frame,
        area,
        " REVERB / Independent ambience and event spaces ",
    );
    let columns = buttons::<2>(Rect::new(inner.x, inner.y, inner.width, inner.height));
    for (bus, column) in ReverbBus::ALL.into_iter().zip(columns) {
        frame.render_widget(
            Paragraph::new(bus.label().to_ascii_uppercase())
                .style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Rect::new(column.x, column.y, column.width, 1),
        );
        let parameters = bus.get(app.reverb);
        for (index, field) in ReverbField::ALL.into_iter().enumerate() {
            slider(
                frame,
                Rect::new(column.x, column.y + 2 + index as u16, column.width, 1),
                field.label(),
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
    let list_width = (area.width * 2 / 5).clamp(27, 36);
    let selected = SfxKind::iter()
        .position(|kind| kind == app.effect)
        .expect("effect belongs to the palette");
    let total = SfxKind::iter().count();
    let list = panel(
        frame,
        Rect::new(area.x, area.y, list_width, area.height),
        format!(" PALETTE / {} of {total} ", selected + 1),
    );
    let visible = list.height as usize;
    let start = selected
        .saturating_sub(visible / 2)
        .min(total.saturating_sub(visible));
    for (index, kind) in SfxKind::iter().skip(start).take(visible).enumerate() {
        let rectangle = Rect::new(list.x, list.y + index as u16, list.width, 1);
        let focused = app.focus == Control::SelectEffect(kind);
        let current = kind == app.effect;
        let style = Style::default()
            .fg(if current { CYAN } else { TEXT })
            .bg(if focused { FOCUSED } else { PANEL });
        frame.render_widget(
            Paragraph::new(format!("{} {kind}", if current { ">" } else { " " })).style(style),
            rectangle,
        );
        hits.add(rectangle, Control::SelectEffect(kind));
    }
    let inner = panel(
        frame,
        Rect::new(
            area.x + list_width + 1,
            area.y,
            area.width - list_width - 1,
            area.height,
        ),
        " AUDITION / Event controls ",
    );
    frame.render_widget(
        Paragraph::new(format!("{}", app.effect))
            .style(Style::default().fg(AMBER).add_modifier(Modifier::BOLD)),
        Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(17), 1),
    );
    let inspect = Rect::new(inner.right() - 14, inner.y, 14, 1);
    frame.render_widget(
        Paragraph::new("[ Inspect (i) ]").style(Style::default().fg(CYAN).bg(
            if app.focus == Control::Inspect {
                FOCUSED
            } else {
                PANEL
            },
        )),
        inspect,
    );
    hits.add(inspect, Control::Inspect);
    let recipe = app.effect.recipe();
    let layer_level = |kind| {
        recipe
            .layers
            .iter()
            .find(|layer| layer.layer == kind)
            .expect("recipe contains every source layer")
            .level
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Texture {}dB | tonal {}dB",
            db(layer_level(SfxLayer::Texture)),
            db(layer_level(SfxLayer::Tonal))
        ))
        .style(Style::default().fg(DIM)),
        Rect::new(inner.x + 1, inner.y + 1, inner.width - 2, 1),
    );
    slider(
        frame,
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
        "Screen pan",
        SliderValue::pan(app.event_pan),
        Control::EventPan,
        app,
        hits,
    );
    slider(
        frame,
        Rect::new(inner.x, inner.y + 4, inner.width, 1),
        "Intensity",
        SliderValue::percent(app.event_intensity),
        Control::EventIntensity,
        app,
        hits,
    );
    button(
        frame,
        Rect::new(inner.x + 1, inner.y + 6, inner.width - 2, 3),
        "PLAY / Space",
        Control::Trigger,
        false,
        app,
        hits,
    );
    if inner.height >= 11 {
        frame.render_widget(Paragraph::new("Click a sound to play; I inspects its layers.\nSpace / PLAY replays a fresh variation.").style(Style::default().fg(DIM)).wrap(Wrap { trim: true }), Rect::new(inner.x + 1, inner.y + 9, inner.width - 2, 2));
    }
}

fn inspector(frame: &mut Frame<'_>, area: Rect, app: &App, hits: &mut HitMap) {
    let recipe = app.effect.recipe();
    let inner = panel(
        frame,
        area,
        format!(" BUILD / {} / Nominal recipe ", app.effect),
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
    let mut summary = format!(
        "{} | {:.0}Hz/{:.0}ms",
        recipe.patch,
        recipe.pitch_hz,
        recipe.duration_seconds * 1000.0
    );
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
        Paragraph::new("[ Play all ]").style(Style::default().fg(CYAN).bg(
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
        let focused = app.focus == control;
        let color = if selected {
            CYAN
        } else if layer.layer == SfxLayer::Texture {
            AMBER
        } else {
            TEXT
        };
        let available = app.output.sample_rate >= layer.minimum_sample_rate;
        let timing = if available {
            format!("{:.0}+{:.0}ms", layer.start_ms, layer.duration_ms)
        } else {
            format!("needs {}kHz", layer.minimum_sample_rate / 1000)
        };
        let row = Rect::new(inner.x, inner.y + index as u16 + 1, inner.width, 1);
        frame.render_widget(
            Paragraph::new(format!(
                "{} {:<7} {:>6}dB {:<15} HP {:>5} LP {:>5} {timing}",
                if selected { ">" } else { " " },
                layer.layer.label(),
                db(layer.level),
                layer.generator,
                compact_hz(layer.highpass_hz),
                compact_hz(layer.lowpass_hz)
            ))
            .style(Style::default().fg(color).bg(if focused {
                FOCUSED
            } else {
                PANEL
            })),
            row,
        );
        hits.add(row, control);
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
    let carrier = selected
        .carrier_hz
        .map_or_else(String::new, |hz| format!(" @{}Hz", compact_hz(hz)));
    let window = format!(
        "{}{} | {} {:.1}+{:.1}ms | A{:.2}/R{:.2}ms | {}",
        selected.layer.label(),
        carrier,
        match selected.layer {
            SfxLayer::Body | SfxLayer::Texture => "event",
            SfxLayer::Tonal => "one slot",
            SfxLayer::Flecks | SfxLayer::Shimmer => "per-part",
        },
        selected.start_ms,
        selected.duration_ms,
        selected.attack_ms,
        selected.release_ms,
        recipe.filter_gesture
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
    frame.render_widget(
        Paragraph::new(compact_source).style(Style::default().fg(AMBER)),
        Rect::new(inner.x, inner.y + 6, inner.width, 1),
    );
    frame.render_widget(
        Paragraph::new(window).style(Style::default().fg(DIM)),
        Rect::new(inner.x, inner.y + 7, inner.width, 1),
    );
    frame.render_widget(
        Paragraph::new(sequence_text(&recipe, selected, inner.width)),
        Rect::new(inner.x, inner.y + 8, inner.width, 1),
    );

    if inner.height > 9 {
        let env = recipe.envelope;
        frame.render_widget(
            Paragraph::new(format!(
                "Voice ADSR {:.2}/{:.1}/{:.0}%/{:.1} ms | gain {}dB | pitch {}",
                env.attack_ms,
                env.decay_ms,
                env.sustain * 100.0,
                env.release_ms,
                db(recipe.gain),
                recipe.pitch
            ))
            .style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y + 9, inner.width, 1),
        );
    }
    if inner.height > 10 {
        frame.render_widget(
            Paragraph::new(macro_text(&recipe)).style(Style::default().fg(DIM)),
            Rect::new(inner.x, inner.y + 10, inner.width, 1),
        );
    }
    if inner.height > 11 {
        let source_rows = (source.len() as u16)
            .div_ceil(inner.width)
            .min(inner.height - 11);
        frame.render_widget(
            Paragraph::new(source)
                .style(Style::default().fg(AMBER))
                .wrap(Wrap { trim: true }),
            Rect::new(inner.x, inner.y + 11, inner.width, source_rows),
        );
        let note_row = 11 + source_rows;
        frame.render_widget(Paragraph::new("Click/Enter a layer to hear its real mix level; Play all restores the whole sound.\nTexture contains the sustained nonlinear/noise path. Compare it with the filtered tonal layers.\nLayer dB is before event intensity and mixer. Pitch, duration and detail rate vary on each seeded playback.")
            .style(Style::default().fg(DIM)).wrap(Wrap { trim: true }),
            Rect::new(inner.x, inner.y + note_row, inner.width, inner.height.saturating_sub(note_row)));
    }
}

fn compact_hz(value: f32) -> String {
    if value >= 1000.0 {
        format!("{:.1}k", value / 1000.0)
    } else {
        format!("{value:.0}")
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
    let sequence = if layer.layer == SfxLayer::Tonal {
        let pitches = recipe
            .tonal_pitch_ratios
            .iter()
            .map(|ratio| format!("{ratio:.2}x"))
            .collect::<Vec<_>>()
            .join("/");
        format!("Tones {}Hz: {pitches}", compact_hz(recipe.tonal_base_hz))
    } else {
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
    let combined = format!("{sequence} | M {macro_steps}");
    if combined.len() <= usize::from(width) {
        combined
    } else {
        format!("{sequence} | {} macro steps", recipe.phrase.len())
    }
}

fn compact_ratio(value: f32) -> String {
    format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn monitor(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let inner = panel(frame, area, " MONITOR / Background response ");
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
    let left = Rect::new(inner.x, inner.y, left_width, 5);
    let right = Rect::new(
        inner.x + left_width + 1,
        inner.y,
        inner.width - left_width - 1,
        5,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("GRAPH NOW > TARGET", Style::default().fg(DIM)),
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
            Line::styled(
                format!("{GLOBAL_SCALE} | {}", root_text(&snapshot)),
                Style::default().fg(DIM),
            ),
        ]),
        left,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled("ENVIRONMENT NOW > TARGET", Style::default().fg(DIM)),
            Line::raw(focus_room_text(background.focus, background.reverb_room)),
            parameter("Low-pass", background.lowpass_hz, hz),
            parameter("High-pass", background.highpass_hz, hz),
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
        Rect::new(inner.x, inner.y + 5, inner.width, 4),
    );
    frame.render_widget(
        Paragraph::new(delay_lines(&snapshot).to_vec()),
        Rect::new(inner.x, inner.y + 9, inner.width, 2),
    );

    if inner.height > 12 {
        frame.render_widget(Paragraph::new("NOW is the smoothed state used by the audio callback. TARGET is its latest accepted input.\nThe next chord is a preview: new market inputs can change it before the clock is due.\nDialog and Focus hold harmony selection and melodic entrances while their clocks keep running.").style(Style::default().fg(DIM)).wrap(Wrap { trim: true }), Rect::new(inner.x, inner.y + 12, inner.width, inner.height - 12));
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
            format!("{GLOBAL_SCALE} | {}", root_text(&snapshot)),
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
    for field in [
        MarketField::Volume,
        MarketField::Volatility,
        MarketField::Zoom,
        MarketField::Activity,
    ] {
        lines.push(comparison(
            field.label(),
            percent(field.get(background.market).get()),
            percent(field.get(snapshot.market).get()),
        ));
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
                "MONITOR NOW>TARGET | trend {}>{} | str {} | vol {}",
                state.market.trend,
                snapshot.market.trend,
                unit_pair(state.market.trend_strength, snapshot.market.trend_strength),
                unit_pair(state.market.volume, snapshot.market.volume)
            ),
            Style::default().fg(CYAN),
        ),
        Line::raw(format!(
            "Volatility {} | zoom {} | activity {} | swell {:.0}%",
            unit_pair(state.market.volatility, snapshot.market.volatility),
            unit_pair(state.market.zoom, snapshot.market.zoom),
            unit_pair(state.market.activity, snapshot.market.activity),
            state.harmonic_swell.get() * 100.0
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

fn percent(value: f32) -> String {
    format!("{:3.0}%", value * 100.0)
}

fn hz(value: f32) -> String {
    format!("{value:.0} Hz")
}

fn clock(value: ClockState) -> String {
    match value {
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
        (Control::Quit, "[ Quit (q) ]"),
    ] {
        let rectangle = Rect::new(x, area.y + 1, label.len() as u16, 1);
        frame.render_widget(
            Paragraph::new(label).style(Style::default().fg(CYAN).bg(if app.focus == control {
                FOCUSED
            } else {
                BACKGROUND
            })),
            rectangle,
        );
        hits.add(rectangle, control);
        x += rectangle.width + 1;
    }
    frame.render_widget(
        Paragraph::new("Mouse: click controls / drag bars").style(Style::default().fg(DIM)),
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
    let height = area.height.min(20);
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
            Line::raw("T or Symbol button  Select a stock within the shared scale"),
            Line::raw("Tab / Shift-Tab Move focus between controls"),
            Line::raw("Up / Down       Move focus; browse sounds in the SFX list"),
            Line::raw("Left / Right    Adjust a focused value"),
            Line::raw("Mixer output gain uses dB; Shift adjusts it in smaller steps."),
            Line::raw("Enter           Activate the focused button"),
            Line::raw("Space           Audition the selected effect"),
            Line::raw("I / Inspect     Show layers; PgUp/Down browses inspected sounds"),
            Line::raw(""),
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
    let inner = panel(frame, rectangle, " SYMBOL / Shared harmonic palette ");
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
            Paragraph::new(format!("Current: {} | {GLOBAL_SCALE}", app.symbol))
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
    fn background_monitor_stays_visible_beside_every_control_tab() {
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
        for (width, height) in [(80, 24), (100, 32), (140, 24), (150, 24), (160, 36)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for (tab, title) in [
                (Tab::Scene, "SCENE /"),
                (Tab::Market, "MARKET /"),
                (Tab::Mixer, "MIXER /"),
                (Tab::Sfx, "PALETTE /"),
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
                        "Room / tail",
                        "Damping",
                        "Width",
                        "Send",
                        "Wet level",
                    ] {
                        assert!(
                            text.contains(expected),
                            "missing reverb control {expected:?}"
                        );
                    }
                    for bus in ReverbBus::ALL {
                        for field in ReverbField::ALL {
                            assert!(hits.region(Control::Reverb(bus, field)).is_some());
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
                "Play all",
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
    }

    #[test]
    fn monitor_distinguishes_current_targets_crossfade_and_held_schedulers() {
        let mut app = app();
        app.select_tab(Tab::Monitor);
        let mut snapshot = Renderer::new(RenderConfig::default()).unwrap().snapshot();
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
                GLOBAL_SCALE,
                "Invalid symbols",
            ] {
                assert!(
                    text.contains(expected),
                    "missing {expected:?} at {width}x{height}:\n{text}"
                );
            }
        }
    }
}
