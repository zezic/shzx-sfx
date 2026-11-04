# shzx-sfx

A native Rust procedural sound engine for Shizex: cold bunker ambience, sparse minor and suspended harmony, distant melodic fragments, and a varied mechanical/digital UI palette. The engine synthesizes everything in memory. The audition CLI can explicitly export WAV files for listening and analysis; the runtime never loads audio assets.

`Renderer` runs independently of an audio device and reproduces a fixed seed, sample rate, and command timeline. `AudioEngine` owns native CPAL playback and its lifecycle thread. Cloneable `AudioHandle`s submit typed commands through a bounded nonblocking queue. Dropping the owner stops playback and joins the thread, including when handles remain alive. DSP memory is allocated before playback.

Start the listening laboratory:

```sh
cargo run --release -- list
cargo run --release -- devices
cargo run --release -- play
cargo run --release -- tui --buffer-frames 256
cargo run --release -- play --demo --seconds 30 --buffer-frames 256
cargo run --release -- play --script examples/demo.json --seconds 30
```

Running without arguments shows help. In an interactive terminal, `play` opens a Ratatui/Crossterm control panel with calibrated default levels. The explicit `tui` command opens the same panel and requires a terminal on both stdin and stdout. Both accept `--device NAME` and `--buffer-frames N`; omit them to use the default device and its system buffer.

The panel has seven tabs: Scene selects environment modes and market presets; Market changes trend, strength, volume, volatility, zoom, and interaction activity; Mixer adjusts each layer and output gain; SFX browses all 37 effects beside the recipe inspector and sets audition pan and intensity; Monitor expands the background state; Reverb controls the environment and UI effect processors separately; Inspect explains the selected event and auditions its components. The SFX page keeps the palette and audition controls on the left and its selected recipe inspector on the right, including at 80x24. Other control pages keep the background monitor: side by side at 140 columns or wider, stacked below on smaller terminals.

Monitor shows market input smoothing, focus, low/high-pass cutoffs, reverb room size, harmonic crossfade progress, the next chord preview and clock, harmonic swell, and detail/melody activity and schedules. Separate detail and melody ping-pong sends respond to view conditions; their current and target delay times, feedback, and sends are visible too. The chord preview can change with new market inputs. Held clocks identify entrances or harmonic changes deferred by a focused scene.

The mixer includes an output gain in decibels, defaulting to +12 dB with a -60 to +24 dB range. `--output-gain-db` sets its startup value. Master remains a separate 0..1 attenuation. The background balances noise, harmony, and melody against the sub-bass instead of relying on the drone for its measured level. The output remains linear below its stereo-linked overload knee.

Routine SFX are short and fast; longer order, dialog, and alert signatures keep their structure. Articulated LP/HP filters, short high-band flecks, and alternating clean/noisy/FM/PM/sync/pulse/folded micro-parts add diversity. Stable pitches and 2-3 ms onset settling keep the main bodies precise.

Every SFX passes through a broad bell cut centered at 3 kHz, -9 dB, Q 0.65, before the dry/reverb split. Its settings are shown in the recipe inspector. The new Crisp component combines low oscillators around 74-235 Hz, high oscillators around 7.6-12.6 kHz, an auxiliary oscillator, and LP-colored noise. Frequency, PM depth, noise blend, and ratcheted amplitude evolve during the gesture; a 24 dB/oct high-pass at 6 kHz leaves a quiet high-frequency texture. The existing lower-band Texture stays attenuated.

Sharp routine sounds use fast attacks with lower sustain and slightly longer decay/release, rather than a strict flat held body. Click uses roughly 0.25 ms attack, 8 ms decay, 5.5% sustain, and 4 ms release; SecondaryClick uses 0.3/10 ms, 4.5%, and 4.5 ms. Longer action signatures retain their envelopes.

Reverb tab `6` exposes independent Environment and UI/event Send, Room/tail, Damping, Width, and Wet controls. UI send defaults to 12%, versus 85% for the environment, keeping events close with quieter tails. Send controls new input to the processor; turning it down preserves the tail already in flight. Wet controls the returning tail. Dialog and Focus smoothly narrow the reverberant stereo field. Room controls tail character rather than a guaranteed number of decay seconds.

Use `1` through `7` to switch tabs, `Tab`/`Shift-Tab` to focus controls, and arrows to navigate or change values. Hold `Shift` with an arrow for finer adjustment; `Home`/`End` set slider bounds. `Enter` activates a button. In SFX, Up/Down browse sounds, Page Up/Down move eight sounds, and `Space` auditions the selection. You can also click buttons, click or drag sliders, and use the wheel to browse effects. Clicking a sound name immediately auditions it; repeated clicks create fresh variations. Keyboard and wheel browsing remain silent.

Press `i` from the SFX page, or `7`, to inspect the selected sound. The inspector reads the same recipe used by synthesis: Body, Texture, Tonal, Flecks, Shimmer, and Crisp levels; generators and carriers; HP/LP filters; envelopes; FM/PM, sync, pulse, and quantization parameters; and the micro/macro sequence. Click or press Enter on a layer to solo it at its current mix level. Each row has separate Mute and Edit buttons; Play mix auditions the event with its current mutes and edits. Arrow/Tab focus and Page Up/Down sound browsing remain silent. Shimmer and Crisp are labeled unavailable when the device sample rate cannot support their high bands.

Edit opens controls for the selected layer. Mix controls set its level and HP/LP cutoffs, plus pitch and duration multipliers for the whole event. Use arrows, Shift-arrows for finer steps, Home/End, or click and drag the sliders. Filter cutoffs stay ordered; Body and Texture retain their filter gestures around the edited onset values. `m` toggles the selected layer's mute, and Esc closes the editor. Mutes preserve stored levels and apply to solos too. Edits take effect on the next audition and stay with each effect while browsing for the current session.

Tonal opens a Synthesis page for carrier tuning, FM index (modulation depth), FM modulator/carrier ratio, step rate, and each slot's attack/release. Its Sequence page edits all six pitch ratios independently; Mix retains the level, filters, and whole-event controls. Click the page tabs or use Page Up/Down. The source is a sine carrier with sine-wave FM, stepped through six pitches and gated by the slot envelope, then shaped by the event envelope and HP/LP filters. Carrier tuning affects only Tonal; event pitch still transposes the whole sound. Frequency readouts are nominal before seeded variation and device limits. The clock starts at a seeded position; its rate varies from 230 to 670 steps/second until edited to a fixed 20..2000 Hz. Slot envelopes accept 0.01..20 ms.

Play tone or Space in the Tonal editor auditions that layer; Play mix includes the other layers. Space in other layer editors auditions the mix. `r` on Synthesis or Sequence resets only Tonal's construction, retaining its level, mute, and filters; `r` on Mix resets the complete event. Other recipe details remain read-only.

Texture contains the dense nonlinear grain path, including folded PM, FM, sync, pulse, quantization, ring products, and held noise. Its level is reduced by about 12 dB; short high-passed low-index tonal sequences now carry more of the event's presence. Quiet high-HP phase-modulated shimmer adds longer fine detail. The inspector labels original recipes Nominal and customized recipes Edited; values precede the seeded variation chosen for each live click.

Read the complete recipe without an audio device, or export matched solo components:

```sh
cargo run --release -- inspect confirm
cargo run --release -- inspect confirm --json
cargo run --release -- render --effect confirm --layer texture --seed 123 --mute-layer reverb --seconds 1.5 --output /tmp/texture.wav
cargo run --release -- render --effect confirm --layer tonal --seed 123 --mute-layer reverb --seconds 1.5 --output /tmp/tonal.wav
```

`render --effect` removes the background; `--layer` accepts all, body, texture, tonal, flecks, shimmer, or crisp. Equal seeds give matched offline component comparisons. Solo components retain their original gain and are not normalized. Recipe windows distinguish whole-event Body/Texture support, single Tonal slots, and Flecks/Shimmer/Crisp repeated on each phrase part. Bands clamp below Nyquist.

Press `t` or click the symbol button to switch tickers. The ticker deterministically chooses a harmonic root in the shared A-natural-minor scale; symbol changes crossfade the chord base while chart updates leave it fixed. The editor accepts Enter to apply and Esc to cancel. `--symbol` chooses the startup ticker.

Outside the sound editor, press `m` to mute or restore the master level, `r` to reset controls and all sound edits to startup values, and `?` for help. `Esc` closes an editor or help, or quits the panel; `q` and `Ctrl-C` quit directly. The panel restores terminal input, cursor, and mouse handling when it exits.

For the line-oriented prompt, run `cargo run --release -- play --repl`. Piped `play` input also keeps the prompt; timed `--demo` and `--script` playback retain their text output. Use `status` in the prompt to inspect synthesis parameters and event counters. Example REPL commands:

```text
inspect click
trigger click -0.7 0.5
trigger click 0 0.6 tonal
trigger secondary-click 0.4 0.4
trigger order-create 0.2 0.7
trigger order-fill 0.7 0.7
market bearish 0.8 0.7 0.6 0.9 0.5
mode dialog
mode browse
mix harmony 0.4
mix texture 0.3
gain 12
reverb events send 0.12
reverb events room 0.9
symbol MSFT
mix master 0
spatial pan
status
quit
```

`market` takes trend, trend strength, relative volume, volatility, zoom depth, and recent interaction activity. Numeric market fields and layer gains are 0..1; pan is -1..1. `mode focus` provides an intermediate distant environment. `spatial pan` removes the Haas component while retaining gain panning. `wait SECONDS` is useful when piping a sequence of commands into the prompt. Invalid commands are reported and leave the prompt running.

Export and inspect deterministic output:

```sh
cargo run --release -- render --output /tmp/shzx-demo.wav --seconds 30 --seed 123
cargo run --release -- render --output /tmp/shzx-script.wav --script examples/demo.json --seconds 30
cargo run --release -- palette --output /tmp/shzx-palette.wav --variations 2
cargo run --release -- render --output /tmp/shzx-drone.wav --mute-layer ui --mute-layer melody
```

Each export writes stereo float32 WAV and a sibling JSONL trace. `--trace PATH` chooses another trace destination. The trace records configuration, sample-aligned commands with actual synthesis snapshots, half-second snapshots, and peak/RMS metrics. The final 50 ms of a finite export fade to silence. `palette` removes all background layers and prints each effect's timestamp; use `--spacing` to leave longer gaps for decay. `--pan-only` provides a useful comparison without Haas delay.

Timeline scripts are JSON arrays of `{ "at_seconds": 1.0, "command": ... }`; [examples/demo.json](examples/demo.json) demonstrates every command shape used in a listening scenario. Simultaneous commands retain their order. Times must fall within the requested duration. Numeric bounds and enum names are validated before output files are opened. The built-in demo scales with `--seconds`; a script uses absolute times. Live script playback schedules controls from the host clock, while exported commands are aligned to the nearest audio sample.

Measure renderer throughput with release builds:

```sh
cargo run --release -- bench --seconds 60 --block-frames 256
cargo run --release -- bench --seconds 60 --block-frames 1024
cargo run --release -- bench --seconds 60 --workload idle
cargo run --release -- bench --seconds 60 --workload stress --max-voices 128
```

The benchmark warms caches and excludes initialization, device work, tracing, and file I/O. The default `interactive` workload renders changing market scenes and roughly six UI events per second. `idle` changes views without UI effects; `stress` submits 200 long event requests per second to exercise admission and voice stealing; these event families bypass the hover/scroll cooldowns, while priority admission can still reject requests when all voices are occupied. It reports final active voices, the highest count observed at block ends, and generated/stolen/coalesced event counters. Its percentage is elapsed wall time divided by audio duration, a throughput measure rather than native callback CPU utilization. macOS can schedule native audio work on different core classes for different buffer requests; compare actual live measurements for requested sizes <=256 and >256 separately. An offline renderer block size does not select a native buffer, thread QoS, or CPU core. Native playback retains the system's buffer choice unless `--buffer-frames` requests a fixed size. The output device chooses the native sample rate; `--sample-rate` controls offline rendering.

Native playback uses CPAL across macOS, Windows, and Linux. Linux builds need the ALSA development package (`libasound2-dev` on Debian/Ubuntu or `alsa-lib-devel` on Fedora). Headless generation and tests can exclude the playback backend:

```sh
cargo test --no-default-features
cargo run --release --no-default-features -- render --output /tmp/shzx-headless.wav
```

Native playback without the terminal UI dependencies is available with `--no-default-features --features playback`; for example, `cargo run --release --no-default-features --features playback -- play --repl`.

Embed the library by retaining the owner and forwarding UI commands:

```rust,no_run
use shzx_sfx::{AudioConfig, AudioEngine, Command, Mode, Pan, SfxEvent, SfxKind};

fn main() -> shzx_sfx::Result<()> {
    let engine = AudioEngine::start(AudioConfig::default())?;
    let audio = engine.handle();
    let mut event = SfxEvent::new(SfxKind::Click);
    event.pan = Pan::new(-0.6)?;
    audio.send(Command::Trigger(event))?;
    audio.send(Command::Mode(Mode::Dialog))?;
    // Retain `engine` for the application's lifetime.
    engine.shutdown()
}
```

`MarketContext::from_candles` derives a view summary from validated OHLCV candles and a host-provided reference volume. Hosts can also construct a typed `MarketContext` directly. Send updates when the visible view changes, and decay interaction activity in the host. Queue saturation is reported as `Error::QueueFull`, allowing the host to coalesce market and motion updates. Keep the owner on an application lifecycle path where joining the thread during shutdown is appropriate.

Harmony and DSP rationale, primary sources, and suggested tuning are in [docs/sound-design-research.md](docs/sound-design-research.md). Market mappings describe the visible chart and serve the atmosphere; they are configurable presentation choices rather than trading signals. Initial automated checks, throughput benchmarks, native macOS buffer measurements, and their limits are recorded in [docs/validation.md](docs/validation.md).
