# Engine validation

Checked on 2026-11-04 on an Apple M4 MacBook Pro with Rust 1.98.1.
These are host observations, not guarantees for other machines or devices.

## Checks

- `cargo test --all-targets --release --locked --offline`: 82 tests passed.
- `cargo test --no-default-features --release --locked --offline`: 56 tests passed.
- Strict all-target/all-feature Clippy, formatting, and Rustdoc passed.
- Playback-only builds without terminal UI dependencies passed.
- The scoped allocator audit includes first rendering, all effect families,
  gain/reverb commands, symbol changes, pending root fades, voice stealing,
  and retirement. It observed zero allocations and zero deallocations after
  renderer construction.

Release tests exercise the production DSP configuration. They cover numeric
and serialization boundaries; sample rates from 8 to 192 kHz; deterministic
samples across irregular blocks; signal levels and gain scaling; short event
bodies, releases, pitch settling, and bright detail lifetimes; bounded voices;
stereo positioning; transitions; delays; reverb routing/tails; and thread
shutdown with a full queue.

## Loudness and source balance

The quietness audit found that almost all background energy was in the low
pedal: full-background RMS was -31.67 dBFS, but only 1.24% survived a 200 Hz
high-pass. Texture and harmonic layers were around -55 to -57 dBFS. A global
peak/RMS check alone did not describe their audible presence.

The source balance now lowers the pedal by 6 dB and raises texture, harmony,
and melody by 12 dB. A separate typed output trim defaults to +12 dB and
accepts -60..+24 dB; master remains an independent 0..1 attenuation. Tests pin
active-window RMS and each background layer, rather than only transient peaks.

Current defaults, seed 123, ticker AAPL, 48 kHz, master 0.55, UI 0.7, event
intensity 0.6:

| Signal | Peak | RMS in the measured window |
| --- | ---: | ---: |
| Click, isolated UI | -4.65 dBFS | -23.50 dBFS |
| Confirm, isolated UI | -6.75 dBFS | -22.72 dBFS |
| Order fill, isolated UI | -3.02 dBFS | -21.07 dBFS |
| Full environment, 1-3 seconds | -13.51 dBFS | -24.09 dBFS |
| Texture alone, 1-3 seconds | -18.37 dBFS | -31.68 dBFS |
| Harmony alone, intro window | -26.57 dBFS | -37.23 dBFS |
| Melody alone, active source | -20.40 dBFS | -27.09 dBFS |

The loudest default effect in the current 37-effect sweep peaked at -2.23 dBFS.
The output is linear below its stereo-linked soft knee and approaches a 0.95
sample ceiling under overload. This is a sample ceiling, not a true-peak meter.
A 0-to-+6 dB boost comparison below the knee had 0.0401% relative waveform RMS
error from f32 recursive filtering, within the 0.1% regression bound.

## Harmony, delays, and space

Symbols are normalized uppercase and stored inline. Stable FNV-1a hashing
selects one of seven roots in A natural minor. All 49 root/voicing combinations
were checked against that pitch collection. AAPL selects C, TSLA selects D,
and IBM selects A. Market updates never change the selected base. Pads and
pedal share a three-second crossfade; rapid symbol requests preserve the active
fade and retain the latest pending destination.

Separate detail and melody sends feed preallocated, damped ping-pong delays.
Tests verify alternating channels, independent state, bounded condition-driven
time/feedback/send changes, and fixed-tap crossfades without a delay-time pitch
sweep. Continuous air noise is excluded from the detail send.

The reverb uses one modulated 32-line FunDSP hybrid FDN per bus. Live loop
loss/damping and mid/side width edits preserve the existing tail. Default UI
send is 12%, compared with 85% for the environment. Send zero stops new input;
wet zero mutes the return. Tests verify these operations independently and
show that restoring wet recovers the existing tail without a processor reset.
Dialog width settles to 20% of its saved base, Focus to about 48%, then Browse
restores the base; dry UI panning is unchanged.

The impulse-tail test measured over 95% sample occupancy in its late window.
A separate comparison with the prior comb-based processor measured normalized
local spectral peak/median crest falling from 7.14 to 5.06 and the 99th
percentile from 3.66 to 2.89. The comparison removed the local damping envelope
before measuring narrow peaks; the new tail is also darker overall. These are
structural signal checks, not a substitute for listening judgments.

## Terminal and native playback

View tests cover 80x24, 100x32, 140x24, and wider layouts. The monitor is always
visible with controls, stacked at narrow sizes and beside them at 140+ columns.
All focusable controls and mouse targets stay within their panes. Symbol entry
captures its mouse targets and accepts validated drafts. Clicking an effect
sends one audition per click; keyboard and wheel browsing remain silent.

A live native pseudo-terminal test verified output gain editing, separate
reverb sends, two mouse-triggered clicks, symbol entry from AAPL to TSLA,
actual C-to-D root crossfading, focus width changes, resizing into horizontal
split view, and REPL gain/reverb/symbol commands. It exited normally and
restored terminal attributes, cursor, mouse capture, and alternate screen.
Earlier startup checks also verified clean non-TTY and missing-device errors.

Physical MacBook speaker reference checks before the layer-inspector update used master zero while all DSP remained
active. Both eight-second demos exited successfully with no reported notices.
Negotiated audio was 44.1 kHz stereo f32:

| Requested and observed frames | Whole-process CPU / wall time |
| --- | ---: |
| 256 | 6.70% of one core |
| 1024 | 10.41% of one core |

The 1024-frame result is near the approximate 10% target, with startup and CLI
status printing included. Core assignment was not observed or inferred.
These native measurements are separate from offline renderer throughput.
Apple describes the device-workgroup relationship and deadline-dependent
power/performance scheduling. [Audio Workgroups](https://developer.apple.com/documentation/audiotoolbox/understanding-audio-workgroups),
[WWDC Audio Workgroups](https://developer.apple.com/videos/play/wwdc2020/10224/)

## Offline throughput

Release binary, 48 kHz, 256-frame renderer blocks, 32 voices, default seed and
ticker. Setup, warmup, tracing, and file I/O are outside the measured loop.
Runs were sequential without simultaneous native playback.

| Workload | Audio duration | Elapsed | Elapsed/audio duration |
| --- | ---: | ---: | ---: |
| Idle environment and scene changes (dense-reverb baseline) | 60 s | 1.598 s | 2.66% |
| Interactive, current layers, about six UI requests/s | 30 s | 0.886 s | 2.95% |
| Stress, current layers, 200 long requests/s | 15 s | 1.418 s | 9.45% |

Current stress filled 32 voices, generated 1,092, stole 989, and suppressed 1,904
requests through priority/cooldown admission. Interactive playback needed at
most six voices and did not steal or suppress events. Larger optional voice
caps increase worst-case cost. Offline percentages measure throughput, not
native callback utilization or a selected CPU core class.

Windows and Linux runtime playback still require checks on those hosts. CPAL
provides the native backend; Linux playback builds need ALSA development
headers. Headless generation excludes CPAL.

## Audition artifact

The generated 22-second audition uses seed 123 with representative effects,
zoom/focus changes, independent delay activity, and symbol changes AAPL to TSLA
to IBM. Its peak is -0.91 dBFS and RMS -23.40 dBFS, including the finite export
fade. The sibling JSONL records symbol, mix/output trim, reverb settings,
commands, generated event parameters, and complete current/target background
state. Generated WAV files are outside the repository; runtime audio remains
procedural and does not load assets from disk.

## Layer inspection and tonal balance

A causal ablation of Focus, Confirm, Click, and OrderFill found that removing
the dense nonlinear micro-grain path lowered RMS by 6.9-12.7 dB. Removing only
stochastic noise changed RMS by 0.05-0.76 dB. The six-part grain loops at
roughly 38-112 Hz, despite individual slots running at 230-670 Hz. FM/PM, sync,
pulse/folded shapes, quantization, and held noise made that path a strong
midrange contributor. The audit used dry output below the limiter.

The Texture component now applies a 0.25 (-12.04 dB) gain to that path.
Across eight seeds, the new Texture solo was 11.1-12.1 dB below the old full
source. New full Click changed by +0.31 dB RMS and OrderFill by -0.86 dB;
Focus and Confirm decreased about 3.6 dB. The master and output bus gains
were unchanged. High-passed low-index tonal sequences carry more presence;
quiet 7.2 kHz HP phase-modulated shimmer provides longer fine detail.

Four public composition tests prove that five same-seed solos reconstruct
the full dry voice below the output knee, selection metadata is truthful,
legacy event JSON defaults to all layers, all 37 recipes serialize valid
components, tonal/shimmer bands are high-passed, and Shimmer is silent at
8 kHz. The allocation audit now includes tonal and shimmer solo triggers.

The live native inspector test passed at 80x24 and 160x36: I/7 navigation,
mouse Texture/Tonal/Shimmer solos, Play All, silent PageDown palette browsing,
per-kind modulation and carrier readouts, background split monitoring, and
clean terminal restoration. The REPL recipe inspector and two solo commands
also passed. A source-backed JSON recipe export and dry component WAV exports
were checked. Layer levels remain unnormalized so comparisons preserve their
contribution to the mix.

An updated 12-second SFX-only audition is exported outside the repository,
with a sibling JSONL trace. It peaks at -2.64 dBFS and measures -27.70 dBFS RMS
including gaps. Runtime synthesis still uses no recorded audio assets.
