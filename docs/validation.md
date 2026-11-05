# Engine validation

Checked on 2026-11-05 on an Apple M4 MacBook Pro with Rust 1.98.1.
These are host observations, not guarantees for other machines or devices.

## Checks

- All-target release tests: 141 passed, with two known sub-tuning tests excluded.
- Headless release tests: 103 passed, with the same two sub-tuning tests excluded.
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

## Chord register, positional width, and melody space

All 70 root/voicing combinations retain their chord-tone pitch classes while
using octave inversions below 420 Hz. Oscillator checks at 8, 48, and 192 kHz
cover construction, retuning, startup, maximum positive note drift, and both
banks during queued root crossfades. G4 plus detuning and drift reaches about
404.8 Hz. The melody uses its independent open register.

Actual SFX waveform checks compare Haas gain panning against Pan mode at 33%
position, isolate the full-position 8 ms delayed crossfeed, preserve centered
levels, and exercise smooth edits on playing sources. Transmission checks
also cover live Pan/mode edits. Its four independently chosen rates stay
constant within each group, change only after ticks 4/8/12, and retain the
last group rate through the rest of a burst.

Melody waveform and telemetry checks measure +/-2.5% vibrato at 4.3 Hz and
reproducible split environment/melody outputs. Its fourth reverb bus is
independent of the other spaces, with a 2.5% dry path. At 8 kHz, an isolated
active-window test measures wet/dry energy ratio 73.6; muting and restoring
wet recovers the same continuing room waveform. All 20 reverb controls stay
clickable and legible in the tested terminal layouts, including 80x24.

Latest 48 kHz, seed 123 exports:

| Signal | Duration | Peak | RMS including gaps |
| --- | ---: | ---: | ---: |
| Full soundscape | 30 s | -6.44 dBFS | -25.75 dBFS |
| Vibrato melody and its reverb | 22 s | -44.32 dBFS | -64.95 dBFS |

In the melody's 10-14 second window, 94.0% of spectral energy lies outside
+/-2 Hz around its nominal 1318.5 Hz carrier. The unmodulated reference was
below 0.001%. These normalized spectral measurements describe pitch spread,
not a listening-comfort guarantee. Files and traces are
`/private/tmp/shzx-sfx-audition/latest-soundscape.*` and `vibrato-melody.*`.

The interactive 30-second workload renders in 1.812 seconds (6.04% of
realtime wall time) with all four reverb processors, six active voices at
peak, and no event stealing or suppression. This is offline throughput;
native callback utilization and device playback were not measured.

## Sub, boot pitch, darker harmony, and transmissions

Transmission rhythm and palette checks cover four fixed-rate sections with
at most three changes, a held final rate, and independent per-burst opening,
tick, and closing choices. Long-burst tests confirm that the TUI's 300..5000 ms
events retain pulse activity until End. Finite scene demos retain their
300..2000 ms range. Transmission wet defaults to 25%, with 67% send and
83% damping.

The current scoped run passes 141 tests, excluding the two known sub-tuning
tests whose frequency expectations conflict with local tuning edits. This
includes all 35 CLI/TUI tests, seven public and four private transmission
tests, the harmony and melody signal checks, and the allocation audit. Strict Clippy, formatting, headless CLI tests, and
playback-only compilation pass.

The distant melody uses a smooth three-second attack and eight-second decay,
with its mixer at 12% and UI/events at 40%. Actual waveform checks at 8, 48,
and 192 kHz verify the slow onset, full nominal source gain, long decay, and
retirement after eleven seconds. Telemetry follows the same lifetime, and the
allocation audit now renders through a complete melody note.

An isolated 22-second melody export at 48 kHz, seed 123, peaks at -34.31 dBFS
and measures -46.50 dBFS RMS including silence. RMS is -67.89 dBFS at 7-7.5
seconds, -39.69 dBFS at 10-12 seconds, and -59.46 dBFS at 16-17 seconds;
the source retires at 18 seconds and its 18-20 second tail is -110.15 dBFS.
The WAV and JSONL trace are in `/private/tmp/shzx-sfx-audition/slow-melody.*`.

A 10-second isolated transmission export at 48 kHz, seed 123, includes a
five-second burst, a pan update, and a 300 ms burst. It measures peak
-19.34 dBFS and RMS -53.13 dBFS including gaps, with 80.6% of measured spectral
energy above 8 kHz. The long burst reaches 45 ticks and holds 9 Hz before
End. The WAV and JSONL trace are in
`/private/tmp/shzx-sfx-audition/transmission-v2.*`.

The sub uses 16 stereo saw layers an octave below the pedal. Tests measure
frequency spread, stereo/mono energy, finite levels, and low-pass rejection at
8, 48, and 192 kHz. A separate sine measurement checks the -15 dB bell at
all seven original pedal frequencies. Chord-note drifts have independent
directions, bounded half-semitone excursions, smooth control-rate movement,
and long rests. Startup tests check actual oscillator phase integration and
a monotonic frequency multiplier that reaches its target at exactly one second.

Harmony tests inspect the actual notes and slash symbols across all ticker
roots and market extremes. Generated minor ninth/eleventh and half-diminished
voicings retain minor thirds in the shared pitch collection and avoid a major
triad on the pedal. Primary mood research and its limits are recorded in
[sound-design-research.md](sound-design-research.md#harmony-and-movement).

Transmission tests cover early End, pulse-budget expiry, one closing signature,
rapid Start replacement, seeded rate changes, irregular block sizes, pan,
the independent third reverb tail, default source level, and silence below
the required Nyquist rate. The actual cascaded HP stage measures -6.02 dB at
8 kHz and more than 65 dB rejection at 1 kHz. The allocator audit now includes
start, pan, early end, internal repetition, budget expiry, and closing.

At 48 kHz, seed 123, default ticker and levels, finite exports measured:

| Signal | Duration | Peak | RMS including gaps |
| --- | ---: | ---: | ---: |
| Full demo with transmissions | 45 s | -1.43 dBFS | -25.45 dBFS |
| Sub alone, dry | 8 s | -31.41 dBFS | -38.83 dBFS |
| Two transmissions alone | 8 s | -22.03 dBFS | -50.15 dBFS |

The sub's main spectral peak was 32.5 Hz, close to the C1 unison center;
0.058% of its measured spectral energy was above 200 Hz, and mono retained
63.3% of mean channel energy. The transmission export retained 67.9% of its
measured spectral energy above 8 kHz; a high-pass has a finite roll-off.
Its reverb tail still measured -85.43 dBFS RMS at 6-7 seconds, after the final
End at 4.7 seconds. Across seven additional seeds, transmission peaks ranged
from -23.76 to -18.81 dBFS and RMS from -50.37 to -46.25 dBFS without
saturation. These are signal measurements, not a listening verdict.

The 30-second interactive benchmark rendered in 1.395 seconds (4.65% of
realtime wall time), with six active voices at peak, no stealing, and no
suppressed events. It includes the additional idle transmission reverb but
uses the existing UI-event benchmark timeline. Native device playback was
not measured for this revision. WAVs and JSONL traces were exported outside
the repository to `/private/tmp/shzx-sfx-audition/`.

## Loudness and source balance

The quietness audit found that almost all background energy was in the low
pedal: full-background RMS was -31.67 dBFS, but only 1.24% survived a 200 Hz
high-pass. Texture and harmonic layers were around -55 to -57 dBFS. A global
peak/RMS check alone did not describe their audible presence.

The source balance now lowers the pedal by 6 dB and raises texture, harmony,
and melody by 12 dB. A separate typed output trim defaults to +12 dB and
accepts -60..+24 dB; master remains an independent 0..1 attenuation. Tests pin
active-window RMS and each background layer, rather than only transient peaks.

Source-balance reference, seed 123, ticker AAPL, 48 kHz, master 0.55, UI 0.7, event
intensity 0.6:

| Signal | Peak | RMS in the measured window |
| --- | ---: | ---: |
| Click, isolated UI after comfort EQ | -5.89 dBFS | -27.62 dBFS |
| Confirm, isolated UI after comfort EQ | -9.60 dBFS | -26.74 dBFS |
| Order fill, isolated UI after comfort EQ | -5.65 dBFS | -26.47 dBFS |
| Full environment, 1-3 seconds | -13.51 dBFS | -24.09 dBFS |
| Texture alone, 1-3 seconds | -18.37 dBFS | -31.68 dBFS |
| Harmony alone, intro window | -26.57 dBFS | -37.23 dBFS |
| Melody alone, active source | -20.40 dBFS | -27.09 dBFS |

The loudest default effect in the current 37-effect sweep peaked at -2.39 dBFS.
The output is linear below its stereo-linked soft knee and approaches a 0.95
sample ceiling under overload. This is a sample ceiling, not a true-peak meter.
A 0-to-+6 dB boost comparison below the knee had 0.0401% relative waveform RMS
error from f32 recursive filtering, within the 0.1% regression bound.

## Harmony, delays, and space

Symbols are normalized uppercase and stored inline. Stable FNV-1a hashing
selects one of seven roots in A natural minor. All 70 explicit root/voicing combinations
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
| Interactive, six layers, about six UI requests/s | 30 s | 1.170 s | 3.90% |
| Stress, six layers, 200 long requests/s | 15 s | 1.585 s | 10.57% |

Current stress filled 32 voices, generated 1,092, stole 989, and suppressed 1,904
requests through priority/cooldown admission. Interactive playback needed at
most six voices and did not steal or suppress events. The saturated 32-voice stress case is near the approximate 10% throughput
target. Larger optional voice caps increase worst-case cost. Offline percentages measure throughput, not
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

Four public composition tests prove that six same-seed solos reconstruct
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

## Comfort EQ, Crisp layer, and palette-side inspection

Three actual-filter tests verify a -9 dB bell at 3 kHz across 8, 44.1, 48, 96,
and 192 kHz, with Q 0.65. At 48 kHz the 1.5 and 6 kHz attenuation is about
4.5 dB; 300 Hz and upper-air bands remain largely unchanged. The two channel
states are independent and finite. All 37 public recipes report the same EQ
constant used to construct the renderer filters. The stage follows the SFX
voice sum, before both dry playback and its reverb send; BG is unaffected.

A matched OnlyTonal OrderFill comparison isolates the EQ from the new layer:
RMS fell 7.19 dB and 2-4 kHz power fell 8.51 dB. The rendered result matches a
reference bell applied to the pinned pre-EQ waveform within 0.0645% relative
RMS error. The loudness regression floors were revised to include this
requested attenuation, rather than adding global gain that would undo it.

Crisp adds independently seeded low/high oscillators, an auxiliary oscillator,
LP-colored noise, frequency/PM/noise parameter contours, and ratcheted gain.
Two HP stages leave only the high band starting near 6 kHz, with an upper LP
at 16 kHz. Actual-wave tests verify more than 92% energy above 6 kHz, a quiet
level relative to the tonal path, absence at 8 kHz, and a spectral-centroid
change exceeding 1 kHz during the contour. Existing five-layer RNG choices
are preserved. The full/solo reconstruction test now covers six components.

Click/SecondaryClick and other sharp routines now have lower held levels and
slightly longer decay/release. A waveform regression verifies late-body
energy below 12% of front energy. Long order/dialog/alert/connectivity
signatures retain their timing. The scoped allocator audit now runs at both
8 and 48 kHz, including an active high-band Crisp solo, with zero allocations
and deallocations.

A new native muted TUI test at 80x24 and 160x36 verified palette left and
inspector right, visible EQ and all six components, mouse all/Crisp/secondary
auditions, and BG monitor preservation on other pages. It exited normally
and restored terminal state. A 12-second SFX audition peaks at -5.66 dBFS,
RMS -32.92 dBFS including gaps; a separate Crisp-only export is available for
checking the high-frequency texture without the other components.

## Event layer muting and parameter editing

Each inspector row has independent mute and edit controls. The editor changes
the selected layer's level and ordered HP/LP band, and the event's pitch and
duration multipliers. Session edits follow the effect while browsing; reset
can restore one event or all controls. Edits apply to subsequent triggers,
including solos, without changing voices or tails already playing.

Deterministic render tests verify that muting removes only the chosen path,
unmuting restores the same samples, halving a layer's level quarters its
energy, and edited filters reach every layer. Spectral comparisons verify
that pitch edits shift Tonal, Shimmer, and Crisp beyond their nominal carrier
registers. Numeric and serialization tests reject invalid scales and filter
bands. The allocation audit includes edited events at 8 and 48 kHz and still
observes zero allocations and deallocations during command application and
rendering.

A default Confirm export was bitwise identical to a separate build of the
preceding code. Its 0.6-second file measured peak 0.258062 and RMS 0.022594.
Existing release checks for transitions, spectral balance, and effect tails
also pass. TestBackend checks cover focus, silent editing, effect retention,
reset scope, and mouse targets; rendered 80x24 palette and editor snapshots
were visually inspected. Layout checks also cover 100x32, 140x24, and 160x36.
The native muted smoke test could not open CoreAudio on this host and exited
with OSStatus 560947818 before entering the panel.

The Tonal editor exposes carrier tuning, FM index and ratio, a separate step
clock, slot attack/release, and all six pitch ratios. Its Mix, Synthesis, and
Sequence pages fit at 80x24. Focus, keyboard page switching, preserved sequence
values, tone-only auditions, and synthesis-only reset scope are covered by
terminal tests; all three compact pages were also visually inspected.

Waveform tests vary each synthesis control and pitch slot independently,
checking that Tonal changes reproducibly while the other five solos remain
bitwise identical. Extreme settings remain finite at 8, 48, and 192 kHz;
edited triggers remain allocation-free. A 74.25-second palette export of all
37 default events, seed 123 at 48 kHz with two-second spacing and reverb tails,
was bitwise identical to a separate build of the preceding code. It measured
peak 0.792593 and RMS 0.013188.

## Richer palette and intrinsic stereo

The palette uses 0.52x Tonal carrier tuning, FM index 1.6, and modulator ratio
4.7x. Family-specific six-step patterns span 0.3-3.8x. Each phrase part has a
linear clock ramp: contact 760->190 Hz, digital 140->980 Hz, servo 540->85 Hz,
and air 90->420 Hz. Rate edits expose both endpoints; equal endpoints hold a
constant rate, and numeric fixed-rate values in existing JSON still parse.
Flecks use 12-17 kHz low-pass cutoffs and 7-15% nominal layer levels.

Tonal uses independent L/R oscillator and filter histories with +/-4-cent
tuning and +/-0.15% clock-rate offsets. Flecks, Shimmer, and Crisp blend
submillisecond delayed copies with their direct signal. Tests reconstruct
the stereo event from all six solos through pan and Haas, check center width
and mono fold-down at 8/48/192 kHz, and verify that unsupported high bands
stay silent. Analytic clock tests check ramp endpoints, integrated cycle
count, derivative, and strictly forward sample progression during long ramps.
Delay tests cover exact retirement and cleared taps after voice stealing.
The allocator audit exercises maximum FM settings and both ramp directions
at 8/48/192 kHz with zero allocations and deallocations after construction.

A matched centered palette comparison uses seed 123, 48 kHz, one variation
per effect, and two-second spacing, including the default reverb and gaps.
The 74.25-second exports measured:

| Measurement | Before tuning/spread | Richer stereo palette |
| --- | ---: | ---: |
| Peak | 0.792593 (-2.02 dBFS) | 0.741616 (-2.60 dBFS) |
| RMS including gaps | 0.013188 (-37.60 dBFS) | 0.012606 (-37.99 dBFS) |
| L/R correlation | 0.99987 | 0.66187 |
| Mono energy / mean channel energy | 99.99% | 83.09% |
| Spectral centroid | 1867 Hz | 2654 Hz |

The stereo palette gains upper-band detail and width while its full-file RMS
changes by less than 0.4 dB. A dry Confirm Tonal comparison measured RMS
0.012501 before and 0.012266 after, with L/R correlation changing from 1.0
to 0.3848. No gain normalization was applied to these exports.

All four Tonal editor pages and the stereo recipe readouts were visually
checked at 80x24. The 12.5-second SFX-only audition uses seven representative
events with centered and panned positions, peaking at -4.07 dBFS and measuring
-34.77 dBFS RMS including gaps.

The 30-second interactive benchmark rendered in 0.958497 seconds (3.195% of
realtime wall time) at 48 kHz, 256-frame blocks, and 32 available voices. A
separate preceding-build run measured 0.949178 seconds (3.164%). These offline
measurements exclude device scheduling and native callback utilization.
