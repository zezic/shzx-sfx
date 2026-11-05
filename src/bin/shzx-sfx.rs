use std::{
    error::Error,
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use shzx_sfx::{
    Command, DataTransmission, DataTransmissionCommand, MarketContext, Mix, Mode, OutputGain, Pan,
    RenderConfig, Renderer, ReverbSettings, SfxEvent, SfxKind, SfxLayerSelection, Snapshot,
    SpatialMode, Symbol, Trend, Unit,
};
use strum::IntoEnumIterator;

#[cfg(feature = "playback")]
use shzx_sfx::ReverbParameters;

#[cfg(feature = "tui")]
mod tui;

type CliResult<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(
    version,
    about = "Procedural sci-fi audio laboratory",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List every UI and trading effect name.
    List,
    /// Explain an effect's synthesis layers and timing without opening audio.
    Inspect(InspectArgs),
    /// List native output devices.
    #[cfg(feature = "playback")]
    Devices,
    /// Listen interactively, or play a timed demo/script.
    #[cfg(feature = "playback")]
    Play(PlayArgs),
    /// Open the keyboard and mouse terminal control panel.
    #[cfg(feature = "tui")]
    Tui(TuiArgs),
    /// Export a deterministic soundscape and its parameter trace.
    Render(RenderArgs),
    /// Export every UI effect, with gaps and no background layers.
    Palette(PaletteArgs),
    /// Measure renderer wall time without an audio device or file I/O.
    Bench(BenchArgs),
}

#[derive(Args)]
struct InspectArgs {
    kind: SfxKind,
    /// Emit the complete machine-readable recipe.
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Args)]
struct EngineArgs {
    /// Ticker used to select a deterministic harmonic base in the shared scale.
    #[arg(long, default_value = "AAPL")]
    symbol: String,
    #[arg(long, default_value_t = 48_000)]
    sample_rate: u32,
    #[arg(long, default_value_t = 0x5348_495a_4558)]
    seed: u64,
    #[arg(long, default_value_t = 32)]
    max_voices: usize,
    #[arg(long, default_value_t = 0.55)]
    master: f32,
    /// Output trim in decibels; positive values boost the whole mix.
    #[arg(long, default_value_t = OutputGain::default().db(), allow_hyphen_values = true)]
    output_gain_db: f32,
    /// Disable Haas delay while keeping ordinary gain panning.
    #[arg(long)]
    pan_only: bool,
    /// Set an initial layer to zero; repeat for multiple layers.
    #[arg(long, value_enum)]
    mute_layer: Vec<MixLayer>,
}

impl EngineArgs {
    fn config(&self) -> CliResult<RenderConfig> {
        let mut mix = Mix {
            master: Unit::new(self.master)?,
            output_gain: OutputGain::new(self.output_gain_db)?,
            ..Mix::default()
        };
        for layer in &self.mute_layer {
            layer.set(&mut mix, Unit::ZERO);
        }
        Ok(RenderConfig {
            symbol: Symbol::new(&self.symbol)?,
            sample_rate: self.sample_rate,
            seed: self.seed,
            max_voices: self.max_voices,
            mix,
            reverb: ReverbSettings::default(),
            spatial: if self.pan_only {
                SpatialMode::Pan
            } else {
                SpatialMode::Haas
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum MixLayer {
    Master,
    Drone,
    Sub,
    Texture,
    Harmony,
    Melody,
    DemonicVoices,
    DemonicStutters,
    FlyingCoins,
    LaserBeams,
    Ui,
    Transmission,
    Reverb,
}

#[cfg(feature = "playback")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReverbBus {
    Environment,
    Events,
    Transmission,
    Melody,
    Accents,
}

#[cfg(feature = "playback")]
impl ReverbBus {
    #[cfg(feature = "tui")]
    const ALL: [Self; 5] = [
        Self::Environment,
        Self::Events,
        Self::Transmission,
        Self::Melody,
        Self::Accents,
    ];
    #[cfg(feature = "tui")]
    fn label(self) -> &'static str {
        match self {
            Self::Environment => "Environment",
            Self::Events => "UI / events",
            Self::Transmission => "Transmission",
            Self::Melody => "Melody",
            Self::Accents => "Accents",
        }
    }
    fn get(self, settings: ReverbSettings) -> ReverbParameters {
        match self {
            Self::Environment => settings.environment,
            Self::Events => settings.events,
            Self::Transmission => settings.transmission,
            Self::Melody => settings.melody,
            Self::Accents => settings.accents,
        }
    }
    fn set(self, settings: &mut ReverbSettings, parameters: ReverbParameters) {
        *match self {
            Self::Environment => &mut settings.environment,
            Self::Events => &mut settings.events,
            Self::Transmission => &mut settings.transmission,
            Self::Melody => &mut settings.melody,
            Self::Accents => &mut settings.accents,
        } = parameters;
    }
}

#[cfg(feature = "playback")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ReverbField {
    Send,
    Room,
    Damping,
    Width,
    Wet,
}

#[cfg(feature = "playback")]
impl ReverbField {
    #[cfg(feature = "tui")]
    const ALL: [Self; 5] = [
        Self::Send,
        Self::Room,
        Self::Damping,
        Self::Width,
        Self::Wet,
    ];
    #[cfg(feature = "tui")]
    fn label(self, width: u16) -> &'static str {
        match self {
            Self::Send => "Send",
            Self::Room if width < 14 => "Room",
            Self::Room => "Room / tail",
            Self::Damping if width < 14 => "Damp",
            Self::Damping => "Damping",
            Self::Width => "Width",
            Self::Wet if width < 14 => "Wet",
            Self::Wet => "Wet level",
        }
    }
    #[cfg(feature = "tui")]
    fn get(self, parameters: ReverbParameters) -> Unit {
        match self {
            Self::Send => parameters.send,
            Self::Room => parameters.room_size,
            Self::Damping => parameters.damping,
            Self::Width => parameters.width,
            Self::Wet => parameters.wet,
        }
    }
    fn set(self, parameters: &mut ReverbParameters, value: Unit) {
        *match self {
            Self::Send => &mut parameters.send,
            Self::Room => &mut parameters.room_size,
            Self::Damping => &mut parameters.damping,
            Self::Width => &mut parameters.width,
            Self::Wet => &mut parameters.wet,
        } = value;
    }
}

impl MixLayer {
    #[cfg(feature = "tui")]
    const ALL: [Self; 13] = [
        Self::Master,
        Self::Drone,
        Self::Sub,
        Self::Texture,
        Self::Harmony,
        Self::Melody,
        Self::DemonicVoices,
        Self::DemonicStutters,
        Self::FlyingCoins,
        Self::LaserBeams,
        Self::Ui,
        Self::Transmission,
        Self::Reverb,
    ];

    #[cfg(feature = "tui")]
    fn get(self, mix: Mix) -> Unit {
        match self {
            Self::Master => mix.master,
            Self::Drone => mix.drone,
            Self::Sub => mix.sub,
            Self::Texture => mix.texture,
            Self::Harmony => mix.harmony,
            Self::Melody => mix.melody,
            Self::DemonicVoices => mix.demonic_voices,
            Self::DemonicStutters => mix.demonic_stutters,
            Self::FlyingCoins => mix.flying_coins,
            Self::LaserBeams => mix.laser_beams,
            Self::Ui => mix.ui,
            Self::Transmission => mix.transmission,
            Self::Reverb => mix.reverb,
        }
    }

    fn set(self, mix: &mut Mix, level: Unit) {
        *match self {
            Self::Master => &mut mix.master,
            Self::Drone => &mut mix.drone,
            Self::Sub => &mut mix.sub,
            Self::Texture => &mut mix.texture,
            Self::Harmony => &mut mix.harmony,
            Self::Melody => &mut mix.melody,
            Self::DemonicVoices => &mut mix.demonic_voices,
            Self::DemonicStutters => &mut mix.demonic_stutters,
            Self::FlyingCoins => &mut mix.flying_coins,
            Self::LaserBeams => &mut mix.laser_beams,
            Self::Ui => &mut mix.ui,
            Self::Transmission => &mut mix.transmission,
            Self::Reverb => &mut mix.reverb,
        } = level;
    }
}

#[derive(Args)]
struct TransmissionDemoArgs {
    /// Include sporadic transmissions in the TUI or built-in demo.
    #[arg(long = "data-transmission-demo", default_value_t = true, action = clap::ArgAction::Set)]
    enabled: bool,
}

#[derive(Args)]
struct RenderArgs {
    #[command(flatten)]
    engine: EngineArgs,
    #[command(flatten)]
    transmission_demo: TransmissionDemoArgs,
    #[arg(short, long)]
    output: PathBuf,
    /// JSONL trace destination; defaults to OUTPUT with .jsonl extension.
    #[arg(long)]
    trace: Option<PathBuf>,
    #[arg(long, default_value_t = 30.0)]
    seconds: f64,
    /// JSON array of {at_seconds, command}; otherwise use the built-in demo.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Render one effect without the background instead of a scripted scene.
    #[arg(long, conflicts_with = "script")]
    effect: Option<SfxKind>,
    /// Component to render: all, body, texture, tonal, flecks, shimmer, or crisp.
    #[arg(long, requires = "effect")]
    layer: Option<String>,
    #[arg(long, default_value_t = 256)]
    block_frames: usize,
}

#[derive(Args)]
struct PaletteArgs {
    #[command(flatten)]
    engine: EngineArgs,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(long)]
    trace: Option<PathBuf>,
    /// Seconds per effect, including its decay and the gap before the next.
    #[arg(long, default_value_t = 3.0)]
    spacing: f64,
    /// Variations of each effect, independently generated from the same stream.
    #[arg(long, default_value_t = 2)]
    variations: usize,
}

#[derive(Args)]
struct BenchArgs {
    #[command(flatten)]
    engine: EngineArgs,
    #[arg(long, default_value_t = 30.0)]
    seconds: f64,
    #[arg(long, default_value_t = 256)]
    block_frames: usize,
    /// Idle ambience, realistic interactions, or sustained voice-pool overload.
    #[arg(long, value_enum, default_value = "interactive")]
    workload: BenchWorkload,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BenchWorkload {
    Idle,
    Interactive,
    Stress,
}

#[cfg(feature = "playback")]
#[derive(Args)]
struct PlayArgs {
    #[command(flatten)]
    engine: EngineArgs,
    #[command(flatten)]
    transmission_demo: TransmissionDemoArgs,
    #[command(flatten)]
    output: OutputArgs,
    /// Use the line-oriented prompt instead of the terminal control panel.
    #[arg(long, conflicts_with_all = ["demo", "script"])]
    repl: bool,
    /// Play the built-in scenario instead of opening interactive controls.
    #[arg(long, conflicts_with = "script")]
    demo: bool,
    #[arg(long)]
    script: Option<PathBuf>,
    /// Duration of timed demo/script playback.
    #[arg(long, default_value_t = 30.0)]
    seconds: f64,
}

#[cfg(feature = "playback")]
#[derive(Args)]
struct OutputArgs {
    #[arg(long)]
    device: Option<String>,
    /// Requested native buffer frames; omit to retain the system default.
    #[arg(long)]
    buffer_frames: Option<std::num::NonZeroU32>,
}

#[cfg(feature = "playback")]
impl OutputArgs {
    fn start(self, renderer: RenderConfig) -> CliResult<shzx_sfx::AudioEngine> {
        use shzx_sfx::{AudioConfig, AudioEngine, BufferSize};
        Ok(AudioEngine::start(AudioConfig {
            device: self.device,
            renderer,
            buffer_size: self
                .buffer_frames
                .map_or(BufferSize::SystemDefault, BufferSize::Fixed),
            ..AudioConfig::default()
        })?)
    }
}

#[cfg(feature = "tui")]
#[derive(Args)]
struct TuiArgs {
    #[command(flatten)]
    engine: EngineArgs,
    #[command(flatten)]
    transmission_demo: TransmissionDemoArgs,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TimedCommand {
    at_seconds: f64,
    command: Command,
}

struct ScheduledCommand {
    frame: u64,
    entry: TimedCommand,
}

#[derive(Serialize)]
#[serde(tag = "record", rename_all = "kebab-case")]
enum TraceRecord<'a> {
    Config {
        symbol: Symbol,
        seed: u64,
        sample_rate: u32,
        max_voices: usize,
        mix: Mix,
        reverb: ReverbSettings,
        spatial: SpatialMode,
        duration_seconds: f64,
    },
    Command {
        requested_at_seconds: f64,
        applied_at_seconds: f64,
        command: &'a Command,
        snapshot: Snapshot,
    },
    Snapshot {
        snapshot: Snapshot,
    },
    Summary {
        frames: u64,
        peak: f32,
        rms: f64,
        peak_dbfs: f64,
        rms_dbfs: f64,
    },
}

#[derive(Default)]
struct Meter {
    frames: u64,
    peak: f32,
    square_sum: f64,
}

impl Meter {
    fn observe(&mut self, frame: [f32; 2]) -> CliResult<()> {
        if !frame.into_iter().all(f32::is_finite) {
            return Err(invalid("renderer produced a non-finite sample").into());
        }
        self.frames += 1;
        self.peak = self.peak.max(frame[0].abs()).max(frame[1].abs());
        self.square_sum += f64::from(frame[0]).powi(2) + f64::from(frame[1]).powi(2);
        Ok(())
    }

    fn rms(&self) -> f64 {
        (self.square_sum / (self.frames as f64 * 2.0)).sqrt()
    }

    fn summary(&self) -> TraceRecord<'_> {
        TraceRecord::Summary {
            frames: self.frames,
            peak: self.peak,
            rms: self.rms(),
            peak_dbfs: dbfs(f64::from(self.peak)),
            rms_dbfs: dbfs(self.rms()),
        }
    }

    fn print(&self, sample_rate: u32) {
        println!(
            "{:.3} s / {} stereo frames at {} Hz; peak {:.6} ({:.2} dBFS), RMS {:.6} ({:.2} dBFS)",
            self.frames as f64 / f64::from(sample_rate),
            self.frames,
            sample_rate,
            self.peak,
            dbfs(f64::from(self.peak)),
            self.rms(),
            dbfs(self.rms())
        );
    }
}

// A finite floor keeps silence metrics valid in JSON.
fn dbfs(value: f64) -> f64 {
    20.0 * value.max(1e-12).log10()
}
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn total_frames(seconds: f64, sample_rate: u32) -> CliResult<u64> {
    if !seconds.is_finite() || !(0.01..=3600.0).contains(&seconds) {
        return Err(invalid("duration must be finite and between 0.01 and 3600 seconds").into());
    }
    if !(8_000..=192_000).contains(&sample_rate) {
        return Err(invalid("sample rate must be between 8000 and 192000 Hz").into());
    }
    let frames = (seconds * f64::from(sample_rate)).round() as u64;
    if frames * 8 + 128 > u64::from(u32::MAX) {
        return Err(invalid("duration and sample rate exceed the WAV RIFF size limit").into());
    }
    Ok(frames)
}

fn validate_block_frames(frames: usize) -> CliResult<()> {
    if !(1..=65_536).contains(&frames) {
        return Err(invalid("block frames must be between 1 and 65536").into());
    }
    Ok(())
}

fn schedule(
    mut entries: Vec<TimedCommand>,
    frames: u64,
    sample_rate: u32,
) -> CliResult<Vec<ScheduledCommand>> {
    for entry in &entries {
        if !entry.at_seconds.is_finite() || entry.at_seconds < 0.0 {
            return Err(invalid("timeline times must be finite and nonnegative").into());
        }
        if (entry.at_seconds * f64::from(sample_rate)).round() >= frames as f64 {
            return Err(invalid(format!(
                "timeline event at {} s lies outside the requested duration",
                entry.at_seconds
            ))
            .into());
        }
    }
    // Stable sorting preserves the declared order of simultaneous commands.
    entries.sort_by(|left, right| left.at_seconds.total_cmp(&right.at_seconds));
    Ok(entries
        .into_iter()
        .map(|entry| ScheduledCommand {
            frame: (entry.at_seconds * f64::from(sample_rate)).round() as u64,
            entry,
        })
        .collect())
}

fn read_script(path: &Path) -> CliResult<Vec<TimedCommand>> {
    Ok(serde_json::from_reader(io::BufReader::new(File::open(
        path,
    )?))?)
}

fn unit(value: f32) -> Unit {
    Unit::new(value).expect("built-in normalized parameters are valid")
}

fn demo(seconds: f64) -> Vec<TimedCommand> {
    let at = |fraction: f64, command| TimedCommand {
        at_seconds: seconds * fraction,
        command,
    };
    let market = |trend, strength, volume, volatility, zoom, activity| {
        Command::Market(MarketContext {
            trend,
            trend_strength: unit(strength),
            volume: unit(volume),
            volatility: unit(volatility),
            zoom: unit(zoom),
            activity: unit(activity),
        })
    };
    let trigger = |kind, pan| {
        Command::Trigger(SfxEvent {
            layers: shzx_sfx::SfxLayerSelection::All,
            edits: shzx_sfx::SfxEdits::DEFAULT,
            kind,
            pan: Pan::new(pan).expect("valid demo pan"),
            intensity: unit(0.65),
        })
    };
    vec![
        at(0.0, market(Trend::Sideways, 0.15, 0.3, 0.15, 0.0, 0.15)),
        at(0.035, trigger(SfxKind::HoverEnter, -0.6)),
        at(0.065, trigger(SfxKind::Click, -0.6)),
        at(0.10, trigger(SfxKind::Scroll, 0.4)),
        at(0.14, market(Trend::Bearish, 0.8, 0.65, 0.7, 0.2, 0.5)),
        at(0.19, trigger(SfxKind::ZoomIn, 0.2)),
        at(0.20, market(Trend::Bearish, 0.8, 0.8, 0.6, 0.9, 0.7)),
        at(0.25, trigger(SfxKind::DragStart, -0.3)),
        at(0.28, trigger(SfxKind::Drop, 0.5)),
        at(0.33, Command::Mode(Mode::Dialog)),
        at(0.33, trigger(SfxKind::DialogOpen, 0.0)),
        at(0.41, trigger(SfxKind::Confirm, 0.2)),
        at(0.44, trigger(SfxKind::OrderCreate, 0.2)),
        at(0.48, trigger(SfxKind::DialogClose, 0.0)),
        at(0.48, Command::Mode(Mode::Browse)),
        at(0.51, market(Trend::Bullish, 0.7, 0.8, 0.35, 0.5, 0.45)),
        at(0.58, trigger(SfxKind::OrderFill, 0.65)),
        at(0.66, Command::Mode(Mode::Focus)),
        at(0.70, trigger(SfxKind::Notification, -0.55)),
        at(0.74, trigger(SfxKind::Warning, 0.55)),
        at(0.78, Command::Mode(Mode::Browse)),
        at(0.80, market(Trend::Sideways, 0.1, 0.2, 0.1, 0.1, 0.1)),
        at(0.86, trigger(SfxKind::ZoomOut, 0.0)),
    ]
}

#[derive(Clone, Copy)]
struct TransmissionDemoBurst {
    start_seconds: f64,
    end_seconds: f64,
    settings: DataTransmission,
}

impl TransmissionDemoBurst {
    fn commands(self) -> [TimedCommand; 2] {
        [
            TimedCommand {
                at_seconds: self.start_seconds,
                command: Command::DataTransmission(DataTransmissionCommand::Start(self.settings)),
            },
            TimedCommand {
                at_seconds: self.end_seconds,
                command: Command::DataTransmission(DataTransmissionCommand::End),
            },
        ]
    }
}

fn transmission_demo(
    seed: u64,
    max_duration: Duration,
) -> impl Iterator<Item = TransmissionDemoBurst> {
    let mut rng = fastrand::Rng::with_seed(seed ^ 0x5458_4445_4d4f);
    let mut start = 1.5 + rng.f64() * 2.0;
    let max_duration_ms =
        u32::try_from(max_duration.as_millis()).expect("demo durations fit in u32 milliseconds");
    let settings = DataTransmission {
        max_triggers: shzx_sfx::TransmissionCount::new(shzx_sfx::TransmissionCount::MAX)
            .expect("the maximum pulse budget is valid"),
        ..DataTransmission::default()
    };
    std::iter::from_fn(move || {
        let duration_ms = rng.u32(300..=max_duration_ms);
        let end = start + f64::from(duration_ms) / 1_000.0;
        let burst = TransmissionDemoBurst {
            start_seconds: start,
            end_seconds: end,
            settings: DataTransmission {
                pan: Pan::new(rng.f32() * 1.6 - 0.8).expect("demo pan is in range"),
                ..settings
            },
        };
        start = end + 3.0 + rng.f64() * 5.0;
        Some(burst)
    })
}

fn demo_scene(seconds: f64, seed: u64, data_transmission: bool) -> Vec<TimedCommand> {
    let mut entries = demo(seconds);
    if data_transmission {
        entries.extend(
            transmission_demo(seed, Duration::from_secs(2))
                // Keep both commands inside the finite export or playback.
                .take_while(|burst| burst.end_seconds < seconds - 0.25)
                .flat_map(TransmissionDemoBurst::commands),
        );
    }
    entries
}

struct Session {
    renderer: Renderer,
    schedule: Vec<ScheduledCommand>,
    next_command: usize,
    frame: u64,
    next_snapshot: u64,
    sample_rate: u32,
}

impl Session {
    fn new(config: RenderConfig, schedule: Vec<ScheduledCommand>) -> CliResult<Self> {
        Ok(Self {
            renderer: Renderer::new(config)?,
            schedule,
            next_command: 0,
            frame: 0,
            next_snapshot: u64::from(config.sample_rate) / 2,
            sample_rate: config.sample_rate,
        })
    }

    fn fill(
        &mut self,
        output: &mut [[f32; 2]],
        observer: &mut impl FnMut(TraceRecord<'_>) -> CliResult<()>,
    ) -> CliResult<()> {
        self.fill_inner::<true>(output, observer)
    }

    fn fill_unobserved(&mut self, output: &mut [[f32; 2]]) -> CliResult<()> {
        self.fill_inner::<false>(output, &mut |_| Ok(()))
    }

    fn fill_inner<const TRACE: bool>(
        &mut self,
        mut output: &mut [[f32; 2]],
        observer: &mut impl FnMut(TraceRecord<'_>) -> CliResult<()>,
    ) -> CliResult<()> {
        while !output.is_empty() {
            while let Some(scheduled) = self
                .schedule
                .get(self.next_command)
                .filter(|command| command.frame <= self.frame)
            {
                self.renderer.apply(scheduled.entry.command);
                if TRACE {
                    observer(TraceRecord::Command {
                        requested_at_seconds: scheduled.entry.at_seconds,
                        applied_at_seconds: self.frame as f64 / f64::from(self.sample_rate),
                        command: &scheduled.entry.command,
                        snapshot: self.renderer.snapshot(),
                    })?;
                }
                self.next_command += 1;
            }
            if TRACE && self.frame >= self.next_snapshot {
                observer(TraceRecord::Snapshot {
                    snapshot: self.renderer.snapshot(),
                })?;
                self.next_snapshot += u64::from(self.sample_rate) / 2;
            }
            let next_event = self
                .schedule
                .get(self.next_command)
                .map_or(u64::MAX, |command| command.frame);
            let next_snapshot = if TRACE { self.next_snapshot } else { u64::MAX };
            let frames = output
                .len()
                .min((next_event.min(next_snapshot) - self.frame) as usize);
            let (current, rest) = output.split_at_mut(frames);
            self.renderer.render(current);
            self.frame += frames as u64;
            output = rest;
        }
        Ok(())
    }
}

fn trace_line(writer: &mut impl Write, record: TraceRecord<'_>) -> CliResult<()> {
    serde_json::to_writer(&mut *writer, &record)?;
    writer.write_all(b"\n")?;
    Ok(())
}

fn resolved_path(path: &Path) -> CliResult<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file = path
        .file_name()
        .ok_or_else(|| invalid("an output path must name a file"))?;
    Ok(parent.canonicalize()?.join(file))
}

fn export(
    config: RenderConfig,
    output: &Path,
    trace: Option<&Path>,
    seconds: f64,
    block_frames: usize,
    entries: Vec<TimedCommand>,
    source: Option<&Path>,
) -> CliResult<()> {
    let frames = total_frames(seconds, config.sample_rate)?;
    validate_block_frames(block_frames)?;
    let schedule = schedule(entries, frames, config.sample_rate)?;
    let mut session = Session::new(config, schedule)?;
    let trace = trace
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output.with_extension("jsonl"));
    let output_resolved = resolved_path(output)?;
    let trace_resolved = resolved_path(&trace)?;
    if output_resolved == trace_resolved {
        return Err(invalid("WAV and trace paths must differ").into());
    }
    if let Some(source) = source {
        let source = source.canonicalize()?;
        if source == output_resolved || source == trace_resolved {
            return Err(invalid("output paths must not overwrite the input script").into());
        }
    }
    let mut trace_writer = BufWriter::new(File::create(&trace)?);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: config.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(output, spec)?;
    trace_line(
        &mut trace_writer,
        TraceRecord::Config {
            symbol: config.symbol,
            seed: config.seed,
            sample_rate: config.sample_rate,
            max_voices: config.max_voices,
            mix: config.mix,
            reverb: config.reverb,
            spatial: config.spatial,
            duration_seconds: seconds,
        },
    )?;
    let mut buffer = vec![[0.0; 2]; block_frames];
    let mut meter = Meter::default();
    let fade_frames = (f64::from(config.sample_rate) * 0.05).round() as u64;
    while session.frame < frames {
        let count = buffer.len().min((frames - session.frame) as usize);
        session.fill(&mut buffer[..count], &mut |record| {
            trace_line(&mut trace_writer, record)
        })?;
        for sample in &buffer[..count] {
            // Exports are finite excerpts from a continuous environment.
            let fade = ((frames - meter.frames) as f32 / fade_frames as f32).min(1.0);
            let frame = [sample[0] * fade, sample[1] * fade];
            meter.observe(frame)?;
            writer.write_sample(frame[0])?;
            writer.write_sample(frame[1])?;
        }
    }
    trace_line(
        &mut trace_writer,
        TraceRecord::Snapshot {
            snapshot: session.renderer.snapshot(),
        },
    )?;
    trace_line(&mut trace_writer, meter.summary())?;
    writer.finalize()?;
    trace_writer.flush()?;
    println!(
        "WAV: {}\nTrace: {}\nSeed: {}",
        output.display(),
        trace.display(),
        config.seed
    );
    meter.print(config.sample_rate);
    Ok(())
}

fn palette(args: PaletteArgs) -> CliResult<()> {
    if !args.spacing.is_finite() || !(2.0..=20.0).contains(&args.spacing) {
        return Err(invalid("palette spacing must be finite and between 2 and 20 seconds").into());
    }
    if !(1..=8).contains(&args.variations) {
        return Err(invalid("palette variations must be between 1 and 8").into());
    }
    let mut config = args.engine.config()?;
    config.mix.drone = Unit::ZERO;
    config.mix.sub = Unit::ZERO;
    config.mix.texture = Unit::ZERO;
    config.mix.harmony = Unit::ZERO;
    config.mix.melody = Unit::ZERO;
    config.mix.demonic_voices = Unit::ZERO;
    config.mix.demonic_stutters = Unit::ZERO;
    config.mix.flying_coins = Unit::ZERO;
    config.mix.laser_beams = Unit::ZERO;
    let mut entries = Vec::new();
    for kind in SfxKind::iter() {
        for variation in 0..args.variations {
            let at_seconds = 0.25 + entries.len() as f64 * args.spacing;
            let pan = if args.variations == 1 {
                0.0
            } else {
                -0.7 + 1.4 * variation as f32 / (args.variations - 1) as f32
            };
            entries.push(TimedCommand {
                at_seconds,
                command: Command::Trigger(SfxEvent {
                    layers: shzx_sfx::SfxLayerSelection::All,
                    edits: shzx_sfx::SfxEdits::DEFAULT,
                    kind,
                    pan: Pan::new(pan)?,
                    intensity: unit(0.65),
                }),
            });
            println!(
                "{:7.2}s  {kind}  variation {}  pan {pan:+.2}",
                at_seconds,
                variation + 1
            );
        }
    }
    let seconds = 0.25 + entries.len() as f64 * args.spacing;
    export(
        config,
        &args.output,
        args.trace.as_deref(),
        seconds,
        256,
        entries,
        None,
    )
}

fn benchmark(args: BenchArgs) -> CliResult<()> {
    let config = args.engine.config()?;
    let frames = total_frames(args.seconds, config.sample_rate)?;
    validate_block_frames(args.block_frames)?;
    let entries = benchmark_timeline(args.workload, args.seconds, config.sample_rate, frames)?;
    let entries = schedule(entries, frames, config.sample_rate)?;
    // Warm code/data caches without counting setup or warmup in the result.
    let mut warmup = Renderer::new(config)?;
    let mut buffer = vec![[0.0; 2]; args.block_frames];
    for _ in 0..(config.sample_rate as usize).div_ceil(buffer.len()) {
        warmup.render(&mut buffer);
    }
    drop(warmup);
    let mut session = Session::new(config, entries)?;
    let start = Instant::now();
    let mut checksum = 0.0f64;
    let mut peak_active_voices = 0;
    while session.frame < frames {
        let count = buffer.len().min((frames - session.frame) as usize);
        session.fill_unobserved(&mut buffer[..count])?;
        peak_active_voices = peak_active_voices.max(session.renderer.snapshot().active_voices);
        checksum += f64::from(std::hint::black_box(buffer[count - 1][0]));
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "Rendered {:.3}s in {:.6}s: {:.3}% of realtime wall time ({:.1}x realtime)",
        args.seconds,
        elapsed,
        elapsed / args.seconds * 100.0,
        args.seconds / elapsed
    );
    println!(
        "{} Hz, {}-frame renderer blocks, {} voices, seed {}; checksum {:.6}",
        config.sample_rate, args.block_frames, config.max_voices, config.seed, checksum
    );
    let snapshot = session.renderer.snapshot();
    println!(
        "Workload {:?}; active voices {}/{}, peak observed at block ends {}; generated {}, stolen {}, coalesced {}",
        args.workload,
        snapshot.active_voices,
        config.max_voices,
        peak_active_voices,
        snapshot.generated_events,
        snapshot.stolen_voices,
        snapshot.coalesced_events
    );
    println!(
        "Offline throughput only; native callback scheduling, core choice, and device latency are excluded."
    );
    println!(
        "For macOS, compare live callback measurements for requested buffers <=256 and >256 separately."
    );
    if cfg!(debug_assertions) {
        println!("Debug build: use cargo run --release for representative DSP throughput.");
    }
    Ok(())
}

fn benchmark_timeline(
    workload: BenchWorkload,
    seconds: f64,
    sample_rate: u32,
    frames: u64,
) -> CliResult<Vec<TimedCommand>> {
    let mut entries = demo(seconds);
    if !matches!(workload, BenchWorkload::Interactive) {
        entries.retain(|entry| !matches!(entry.command, Command::Trigger(_)));
    }
    let (rate, kinds): (f64, Vec<SfxKind>) = match workload {
        BenchWorkload::Idle => return Ok(entries),
        BenchWorkload::Interactive => (6.0, SfxKind::iter().collect()),
        BenchWorkload::Stress => (
            200.0,
            vec![
                SfxKind::OrderFill,
                SfxKind::OrderCreate,
                SfxKind::OrderFill,
                SfxKind::Error,
                SfxKind::OrderCancel,
                SfxKind::Disconnect,
            ],
        ),
    };
    for index in 0..(seconds * rate).ceil() as usize {
        let at_seconds = 0.02 + index as f64 / rate;
        if (at_seconds * f64::from(sample_rate)).round() >= frames as f64 {
            break;
        }
        entries.push(TimedCommand {
            at_seconds,
            command: Command::Trigger(SfxEvent {
                layers: shzx_sfx::SfxLayerSelection::All,
                edits: shzx_sfx::SfxEdits::DEFAULT,
                kind: kinds[index % kinds.len()],
                pan: Pan::new((index as f32 * 0.37).sin() * 0.8)?,
                intensity: unit(0.6),
            }),
        });
    }
    Ok(entries)
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> CliResult<()> {
    match cli.command {
        Action::List => {
            for kind in SfxKind::iter() {
                println!("{kind}");
            }
            Ok(())
        }
        Action::Inspect(args) => print_recipe(args.kind, args.json),
        #[cfg(feature = "playback")]
        Action::Devices => {
            for device in shzx_sfx::output_devices()? {
                println!(
                    "{}{}",
                    device.name,
                    if device.is_default { " (default)" } else { "" }
                );
            }
            Ok(())
        }
        #[cfg(feature = "playback")]
        Action::Play(args) => play(args),
        #[cfg(feature = "tui")]
        Action::Tui(args) => {
            tui::require_terminal()?;
            let config = args.engine.config()?;
            tui::run(
                args.output.start(config)?,
                config,
                args.transmission_demo
                    .enabled
                    .then(|| transmission_demo(config.seed, Duration::from_secs(5))),
            )
        }
        Action::Render(args) => {
            let mut config = args.engine.config()?;
            let entries = if let Some(kind) = args.effect {
                config.mix.drone = Unit::ZERO;
                config.mix.sub = Unit::ZERO;
                config.mix.texture = Unit::ZERO;
                config.mix.harmony = Unit::ZERO;
                config.mix.melody = Unit::ZERO;
                config.mix.demonic_voices = Unit::ZERO;
                config.mix.demonic_stutters = Unit::ZERO;
                config.mix.flying_coins = Unit::ZERO;
                config.mix.laser_beams = Unit::ZERO;
                vec![TimedCommand {
                    at_seconds: (args.seconds * 0.1).min(0.25),
                    command: Command::Trigger(SfxEvent {
                        layers: parse_layers(args.layer.as_deref().unwrap_or("all"))?,
                        ..SfxEvent::new(kind)
                    }),
                }]
            } else {
                match &args.script {
                    Some(path) => read_script(path)?,
                    None => demo_scene(args.seconds, config.seed, args.transmission_demo.enabled),
                }
            };
            export(
                config,
                &args.output,
                args.trace.as_deref(),
                args.seconds,
                args.block_frames,
                entries,
                args.script.as_deref(),
            )
        }
        Action::Palette(args) => palette(args),
        Action::Bench(args) => benchmark(args),
    }
}

fn parse_layers(value: &str) -> CliResult<SfxLayerSelection> {
    if value.eq_ignore_ascii_case("all") {
        Ok(SfxLayerSelection::All)
    } else {
        Ok(SfxLayerSelection::Only(value.parse()?))
    }
}

fn print_recipe(kind: SfxKind, json: bool) -> CliResult<()> {
    let recipe = kind.recipe();
    if json {
        println!("{}", serde_json::to_string_pretty(&recipe)?);
        return Ok(());
    }
    println!("{kind}: nominal procedural recipe");
    println!(
        "Shared SFX EQ: {:.0} Hz, {:+.1} dB, Q {:.2} (dry and reverb send)",
        recipe.eq.center_hz, recipe.eq.gain_db, recipe.eq.q
    );
    println!(
        "Signal path: phrase/envelopes -> layer generators and HP/LP filters -> layer levels/stereo spread -> stereo sum -> pan/Haas -> SFX EQ -> UI dry and reverb send -> output mixer."
    );
    println!("Patch: {}", recipe.patch);
    println!(
        "Pitch {:.1} Hz ({}) | source {:.1} ms | voice gain {:.3}",
        recipe.pitch_hz,
        recipe.pitch,
        recipe.duration_seconds * 1000.0,
        recipe.gain
    );
    let envelope = recipe.envelope;
    println!(
        "Body envelope: attack {:.2} ms, decay {:.2} ms, sustain {:.0}%, release {:.2} ms",
        envelope.attack_ms,
        envelope.decay_ms,
        envelope.sustain * 100.0,
        envelope.release_ms
    );
    println!("Body filter articulation: {}", recipe.filter_gesture);
    println!(
        "Body/Texture cover the event; their envelope repeats on each phrase part. Tonal windows are one slot; Flecks/Shimmer restart per part."
    );
    for layer in recipe.layers {
        println!(
            "\n{}: {} | level {:.3} ({:+.1} dB relative)",
            layer.layer.label(),
            layer.generator,
            layer.level,
            20.0 * layer.level.log10()
        );
        println!(
            "  HP {:.0} Hz / LP {:.0} Hz | start {:.2} ms, window {:.2} ms | attack {:.2} / release {:.2} ms",
            layer.highpass_hz,
            layer.lowpass_hz,
            layer.start_ms,
            layer.duration_ms,
            layer.attack_ms,
            layer.release_ms
        );
        if let Some(modulation) = layer.modulation {
            println!("  {modulation}");
        }
        println!("  Stereo: {}", layer.stereo);
        if let Some(carrier) = layer.carrier_hz {
            println!(
                "  Nominal carrier {:.0} Hz before note ratios and seeded pitch variation.",
                carrier
            );
        }
        if layer.minimum_sample_rate > 0 {
            println!(
                "  Available from {} Hz sample rate; bands clamp below Nyquist.",
                layer.minimum_sample_rate
            );
        }
    }
    println!(
        "\nTexture micro-pattern: {}",
        recipe
            .micro_pattern
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" > ")
    );
    println!(
        "Detail clock {:.0}..{:.0} Hz; held-noise clock {:.0}..{:.0} Hz",
        recipe.detail_rate_hz[0],
        recipe.detail_rate_hz[1],
        recipe.hold_rate_hz[0],
        recipe.hold_rate_hz[1]
    );
    println!(
        "The {}-part pattern repeats at {:.1}..{:.1} Hz.",
        recipe.micro_pattern.len(),
        recipe.detail_rate_hz[0] / recipe.micro_pattern.len() as f32,
        recipe.detail_rate_hz[1] / recipe.micro_pattern.len() as f32
    );
    println!("Bright bursts:");
    for burst in recipe.fleck_bursts {
        println!(
            "  start {:.2} ms, length {:.2} ms, level {:.2}",
            burst.start_ms, burst.duration_ms, burst.level
        );
    }
    println!(
        "Tonal notes: {:.0} Hz x {:?}",
        recipe.tonal_base_hz, recipe.tonal_pitch_ratios
    );
    println!("Tonal step clock: {} per phrase part", recipe.tonal_rate);
    println!("Macro phrase:");
    for step in recipe.phrase {
        println!(
            "  {:.1} ms + {:.1} ms, pitch {:.2}x, level {:.2}",
            step.start_fraction * recipe.duration_seconds * 1000.0,
            step.length_fraction * recipe.duration_seconds * 1000.0,
            step.pitch_ratio,
            step.level
        );
    }
    println!(
        "\nParameters are nominal; seeded variations are recorded per trigger. Solo playback retains the component's original gain."
    );
    Ok(())
}

#[cfg(feature = "playback")]
fn play(args: PlayArgs) -> CliResult<()> {
    let config = args.engine.config()?;
    let timed = args.demo || args.script.is_some();
    let entries = if timed {
        let frames = total_frames(args.seconds, config.sample_rate)?;
        let entries = match &args.script {
            Some(path) => read_script(path)?,
            None => demo_scene(args.seconds, config.seed, args.transmission_demo.enabled),
        };
        Some(schedule(entries, frames, config.sample_rate)?)
    } else {
        None
    };
    #[cfg(feature = "tui")]
    if !timed
        && !args.repl
        && std::io::IsTerminal::is_terminal(&io::stdin())
        && std::io::IsTerminal::is_terminal(&io::stdout())
    {
        return tui::run(
            args.output.start(config)?,
            config,
            args.transmission_demo
                .enabled
                .then(|| transmission_demo(config.seed, Duration::from_secs(5))),
        );
    }
    let mut engine = args.output.start(config)?;
    println!("Output: {:?}", engine.output_info());
    println!(
        "Device rate overrides --sample-rate during native playback. Seed: {}",
        config.seed
    );
    if let Some(entries) = entries {
        play_timeline(&mut engine, &entries, args.seconds)?;
    } else {
        repl(&mut engine, config.mix)?;
    }
    engine.shutdown()?;
    Ok(())
}

#[cfg(feature = "playback")]
fn print_status(engine: &mut shzx_sfx::AudioEngine) -> CliResult<()> {
    println!("Status: {:?}", engine.status());
    println!("Observed native callbacks: {:?}", engine.callback_stats());
    if let Some(snapshot) = engine.latest_snapshot() {
        println!("{}", serde_json::to_string(&snapshot)?);
    }
    while let Some(notice) = engine.take_notice() {
        eprintln!("Audio notice: {notice:?}");
    }
    Ok(())
}

#[cfg(feature = "playback")]
fn play_timeline(
    engine: &mut shzx_sfx::AudioEngine,
    entries: &[ScheduledCommand],
    seconds: f64,
) -> CliResult<()> {
    use std::{thread, time::Duration};
    let handle = engine.handle();
    let start = Instant::now();
    let mut next = 0;
    let mut status_at = 0.0;
    while start.elapsed().as_secs_f64() < seconds {
        let now = start.elapsed().as_secs_f64();
        while let Some(entry) = entries
            .get(next)
            .filter(|entry| entry.entry.at_seconds <= now)
        {
            handle.send(entry.entry.command)?;
            println!(
                "{:7.3}s  {}",
                now,
                serde_json::to_string(&entry.entry.command)?
            );
            next += 1;
        }
        if now >= status_at {
            print_status(engine)?;
            status_at += 1.0;
        }
        if !matches!(engine.status(), shzx_sfx::AudioStatus::Running) {
            return Err(invalid(format!("audio output stopped: {:?}", engine.status())).into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    print_status(engine)
}

#[cfg(feature = "playback")]
enum ReplAction {
    Send(Box<Command>),
    Status,
    Wait(std::time::Duration),
    Help,
    Inspect(SfxKind),
    Quit,
}

#[cfg(feature = "playback")]
impl ReplAction {
    fn send(command: Command) -> Self {
        Self::Send(Box::new(command))
    }
}

#[cfg(feature = "playback")]
fn repl_command(line: &str, mix: Mix, reverb: ReverbSettings) -> CliResult<ReplAction> {
    use ReplAction::*;
    let words: Vec<_> = line.split_whitespace().collect();
    let Some(verb) = words.first().copied() else {
        return Ok(Help);
    };
    match (verb, words.len()) {
        ("trigger", 2..=5) => Ok(ReplAction::send(Command::Trigger(SfxEvent {
            layers: parse_layers(words.get(4).copied().unwrap_or("all"))?,
            edits: shzx_sfx::SfxEdits::DEFAULT,
            kind: words[1].parse()?,
            pan: Pan::new(words.get(2).map_or(Ok(0.0), |value| value.parse::<f32>())?)?,
            intensity: Unit::new(words.get(3).map_or(Ok(0.6), |value| value.parse::<f32>())?)?,
        }))),
        ("market", 7) => Ok(ReplAction::send(Command::Market(MarketContext {
            trend: words[1].parse()?,
            trend_strength: Unit::new(words[2].parse()?)?,
            volume: Unit::new(words[3].parse()?)?,
            volatility: Unit::new(words[4].parse()?)?,
            zoom: Unit::new(words[5].parse()?)?,
            activity: Unit::new(words[6].parse()?)?,
        }))),
        ("mode", 2) => Ok(ReplAction::send(Command::Mode(words[1].parse()?))),
        ("spatial", 2) => Ok(ReplAction::send(Command::Spatial(words[1].parse()?))),
        ("symbol", 2) => Ok(ReplAction::send(Command::Symbol(Symbol::new(words[1])?))),
        ("transmission", 2..=3) => {
            let command = match (words[1], words.len()) {
                ("start", _) => DataTransmissionCommand::Start(DataTransmission {
                    pan: Pan::new(words.get(2).map_or(Ok(0.0), |value| value.parse::<f32>())?)?,
                    ..DataTransmission::default()
                }),
                ("pan", 3) => DataTransmissionCommand::Pan(Pan::new(words[2].parse()?)?),
                ("end", 2) => DataTransmissionCommand::End,
                _ => return Err(invalid("use transmission start [PAN], pan PAN, or end").into()),
            };
            Ok(ReplAction::send(Command::DataTransmission(command)))
        }
        ("mix", 3) => {
            let layer = MixLayer::from_str(words[1], true).map_err(invalid)?;
            let mut next = mix;
            layer.set(&mut next, Unit::new(words[2].parse()?)?);
            Ok(ReplAction::send(Command::Mix(next)))
        }
        ("gain", 2) => {
            let mut next = mix;
            next.output_gain = OutputGain::new(words[1].parse()?)?;
            Ok(ReplAction::send(Command::Mix(next)))
        }
        ("reverb", 4) => {
            let bus = ReverbBus::from_str(words[1], true).map_err(invalid)?;
            let field = ReverbField::from_str(words[2], true).map_err(invalid)?;
            let mut settings = reverb;
            let mut parameters = bus.get(settings);
            field.set(&mut parameters, Unit::new(words[3].parse()?)?);
            bus.set(&mut settings, parameters);
            Ok(ReplAction::send(Command::Reverb(settings)))
        }
        ("status", 1) => Ok(Status),
        ("inspect", 2) => Ok(Inspect(words[1].parse()?)),
        ("wait", 2) => {
            let seconds: f64 = words[1].parse()?;
            if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                return Err(
                    invalid("wait duration must be finite and between 0 and 60 seconds").into(),
                );
            }
            Ok(Wait(std::time::Duration::from_secs_f64(seconds)))
        }
        ("help", 1) => Ok(Help),
        ("quit" | "exit", 1) => Ok(Quit),
        _ => Err(invalid("unknown command or wrong argument count; enter help").into()),
    }
}

#[cfg(feature = "playback")]
fn repl(engine: &mut shzx_sfx::AudioEngine, mut mix: Mix) -> CliResult<()> {
    use std::io::BufRead;
    let handle = engine.handle();
    repl_help();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();
    let mut reverb = ReverbSettings::default();
    loop {
        print!("sfx> ");
        io::stdout().flush()?;
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        match repl_command(&line, mix, reverb) {
            Ok(ReplAction::Send(command)) => match handle.send(*command) {
                Ok(()) => {
                    if let Command::Mix(next) = *command {
                        mix = next;
                    }
                    if let Command::Reverb(next) = *command {
                        reverb = next;
                    }
                    println!("Queued {}", serde_json::to_string(&command)?);
                }
                Err(error) => eprintln!("error: {error}"),
            },
            Ok(ReplAction::Status) => print_status(engine)?,
            Ok(ReplAction::Wait(duration)) => std::thread::sleep(duration),
            Ok(ReplAction::Help) => repl_help(),
            Ok(ReplAction::Inspect(kind)) => print_recipe(kind, false)?,
            Ok(ReplAction::Quit) => break,
            Err(error) => eprintln!("error: {error}"),
        }
        if !matches!(engine.status(), shzx_sfx::AudioStatus::Running) {
            return Err(invalid(format!("audio output stopped: {:?}", engine.status())).into());
        }
    }
    Ok(())
}

#[cfg(feature = "playback")]
fn repl_help() {
    println!(
        "Commands:\n  trigger KIND [PAN INTENSITY LAYER]\n                                 layer all/body/texture/tonal/flecks/shimmer/crisp\n  market TREND STRENGTH VOLUME VOLATILITY ZOOM ACTIVITY\n                                 trend bearish/sideways/bullish, values 0..1\n  mode browse|dialog|focus\n  symbol TICKER                  choose a deterministic root in the shared scale\n  mix master|drone|sub|texture|harmony|melody|ui|transmission|reverb LEVEL\n  mix demonic-voices|demonic-stutters|flying-coins|laser-beams LEVEL\n  gain DB                        output trim -60..+24 dB\n  reverb environment|events|transmission|melody|accents send|room|damping|width|wet LEVEL\n                                 independent sends and wet returns, 0..1\n  transmission start [PAN] | pan PAN | end\n  spatial pan|haas\n  inspect KIND                    show the nominal synthesis recipe\n  status\n  wait SECONDS\n  help\n  quit\nUse `shzx-sfx list` for all effect names."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "tui")]
    #[test]
    fn both_tui_entrypoints_enable_transmission_demos_and_accept_opt_out() {
        for entrypoint in ["tui", "play"] {
            for (flags, expected) in [
                (vec![], true),
                (vec!["--data-transmission-demo", "false"], false),
            ] {
                let cli =
                    Cli::try_parse_from(["shzx-sfx", entrypoint].into_iter().chain(flags)).unwrap();
                let enabled = match cli.command {
                    Action::Tui(args) => args.transmission_demo.enabled,
                    Action::Play(args) => args.transmission_demo.enabled,
                    _ => unreachable!(),
                };
                assert_eq!(enabled, expected, "{entrypoint}");
            }
        }
    }

    #[test]
    fn transmission_demo_is_enabled_by_default_and_schedules_seeded_bounded_pairs() {
        let cli = Cli::try_parse_from(["shzx-sfx", "render", "--output", "/tmp/demo.wav"]).unwrap();
        let Action::Render(args) = cli.command else {
            unreachable!()
        };
        assert!(args.transmission_demo.enabled);
        let cli = Cli::try_parse_from([
            "shzx-sfx",
            "render",
            "--output",
            "/tmp/demo.wav",
            "--data-transmission-demo",
            "false",
        ])
        .unwrap();
        let Action::Render(args) = cli.command else {
            unreachable!()
        };
        assert!(!args.transmission_demo.enabled);
        let enabled = demo_scene(60.0, 123, true);
        let repeated = demo_scene(60.0, 123, true);
        assert_eq!(
            serde_json::to_string(&enabled).unwrap(),
            serde_json::to_string(&repeated).unwrap()
        );
        let pairs: Vec<_> = enabled
            .iter()
            .filter(|entry| matches!(entry.command, Command::DataTransmission(_)))
            .collect();
        assert!(pairs.len() >= 4);
        let (pairs, remainder) = pairs.as_chunks::<2>();
        assert!(remainder.is_empty());
        for pair in pairs {
            assert!(matches!(
                pair[0].command,
                Command::DataTransmission(DataTransmissionCommand::Start(_))
            ));
            assert_eq!(
                pair[1].command,
                Command::DataTransmission(DataTransmissionCommand::End)
            );
            let duration_ms = (pair[1].at_seconds - pair[0].at_seconds) * 1000.0;
            assert!((299.999..=2000.001).contains(&duration_ms));
            assert!(pair[1].at_seconds < 60.0);
        }
        assert!(
            demo_scene(60.0, 123, false)
                .iter()
                .all(|entry| !matches!(entry.command, Command::DataTransmission(_)))
        );
    }

    #[cfg(feature = "tui")]
    #[test]
    fn tui_demo_includes_long_bursts_with_ticks_until_the_end_event() {
        let bursts: Vec<_> = transmission_demo(123, Duration::from_secs(5))
            .take(64)
            .collect();
        assert!(bursts.iter().all(|burst| {
            (0.299999..=5.000001).contains(&(burst.end_seconds - burst.start_seconds))
        }));
        let longest = bursts
            .iter()
            .max_by(|a, b| {
                (a.end_seconds - a.start_seconds).total_cmp(&(b.end_seconds - b.start_seconds))
            })
            .unwrap();
        let duration = longest.end_seconds - longest.start_seconds;
        assert!(duration > 4.5);
        let mut renderer = Renderer::new(RenderConfig {
            sample_rate: 8_000,
            mix: Mix {
                master: Unit::ZERO,
                ..Mix::default()
            },
            ..RenderConfig::default()
        })
        .unwrap();
        renderer.apply(Command::DataTransmission(DataTransmissionCommand::Start(
            longest.settings,
        )));
        for _ in 0..(duration * 8_000.0) as usize {
            renderer.next_frame();
        }
        let before_end = renderer.snapshot().transmission;
        assert_eq!(before_end.phase, shzx_sfx::TransmissionPhase::Repeating);
        assert!(before_end.repeated > 24);
        renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
        for _ in 0..8_000 {
            renderer.next_frame();
        }
        assert_eq!(
            renderer.snapshot().transmission.phase,
            shzx_sfx::TransmissionPhase::Idle
        );
    }

    #[test]
    fn timeline_validation_is_strict_and_preserves_simultaneous_command_order() {
        let command = Command::Mode(Mode::Browse);
        for time in [f64::NAN, f64::INFINITY, -0.1, 1.0] {
            assert!(
                schedule(
                    vec![TimedCommand {
                        at_seconds: time,
                        command
                    }],
                    48_000,
                    48_000
                )
                .is_err()
            );
        }
        let result = schedule(
            vec![
                TimedCommand {
                    at_seconds: 0.5,
                    command: Command::Mode(Mode::Dialog),
                },
                TimedCommand {
                    at_seconds: 0.5,
                    command: Command::Mode(Mode::Focus),
                },
                TimedCommand {
                    at_seconds: 0.1,
                    command,
                },
            ],
            48_000,
            48_000,
        )
        .unwrap();
        assert_eq!(
            result.iter().map(|entry| entry.frame).collect::<Vec<_>>(),
            [4_800, 24_000, 24_000]
        );
        assert_eq!(result[1].entry.command, Command::Mode(Mode::Dialog));
        assert_eq!(result[2].entry.command, Command::Mode(Mode::Focus));
    }

    #[test]
    fn script_rejects_unbounded_parameters_before_rendering() {
        let invalid = r#"[{"at_seconds":0,"command":{"type":"trigger","value":{"kind":"click","pan":0,"intensity":1.1}}}]"#;
        assert!(serde_json::from_str::<Vec<TimedCommand>>(invalid).is_err());
        assert!(total_frames(f64::INFINITY, 48_000).is_err());
        assert!(total_frames(3600.0, 192_000).is_err());
    }

    #[test]
    fn command_timing_is_sample_accurate_across_renderer_block_sizes() {
        let config = RenderConfig::default();
        let entries = vec![
            TimedCommand {
                at_seconds: 0.00531,
                command: Command::Trigger(SfxEvent::new(SfxKind::Click)),
            },
            TimedCommand {
                at_seconds: 0.01213,
                command: Command::Mode(Mode::Dialog),
            },
        ];
        let render = |block_frames: usize| {
            let mut session = Session::new(
                config,
                schedule(entries.clone(), 1024, config.sample_rate).unwrap(),
            )
            .unwrap();
            let mut samples = vec![[0.0; 2]; 1024];
            let mut times = Vec::new();
            for block in samples.chunks_mut(block_frames) {
                session
                    .fill(block, &mut |record| {
                        if let TraceRecord::Command {
                            applied_at_seconds, ..
                        } = record
                        {
                            times.push(applied_at_seconds);
                        }
                        Ok(())
                    })
                    .unwrap();
            }
            (samples, times)
        };
        let small = render(17);
        let large = render(512);
        assert_eq!(small, large);
        assert_eq!(small.1, vec![255.0 / 48_000.0, 582.0 / 48_000.0]);
    }
}
