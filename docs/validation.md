# Engine validation

Checked on 2026-11-05 on an Apple M4 MacBook Pro with Rust 1.98.1.
These are host observations, not guarantees for other machines or devices.

## Checks

- All-target release suite: 170 passed, with five preexisting tuning/expectation failures.
- Headless compilation (`--no-default-features`) passed.
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


## Autonomous voices and high-register formations

The background has four separately mixed sources: Demonic voices, Demonic
stutters, Flying coins, and Laser beams. Voices share a two-slot pool across
both delivery styles and take opposing Haas positions during overlap. English
letter-name gestures advance through the live ticker, skipping punctuation;
pink/sine excitation shapes letter articulations. Each utterance lasts
0.4..7 seconds. Male/female formant centers use measured English-whisper
profiles from the research notes. Their bandwidths are design values of
200/150/175 Hz for F1/F2/F3, with nominal noise-envelope peak targets of
12/-4/-12 dB. The F1 emphasis compensates background masking. These are
engineering settings, not universal whisper norms. Branch gains compensate
pink-noise tilt and the 6.2 kHz low-pass response. Frequencies, bandwidths, and peak targets morph
together; intentional +/-33% frequency excursions scale bandwidths with
frequency to preserve Q. Morphs occupy the final 30 ms of each phone,
capped at 55% of very short phones; branch gains smooth over 5 ms. Both
delivery styles use the library pinking filter, soft-clip its output,
add a seeded 30..110 Hz sine at 0.1 amplitude, apply +6 dB, and soft-clip
again before the formant and frication filters. Peak-normalized resonators
keep this distorted excitation controlled at 192 kHz. Actual-signal tests
verify the periodic source contributes nonlinear harmonics without exceeding
the excitation ceiling.

Stutter rate and amplitude targets change over 0.3..1.5 seconds, staying within
3..20 Hz with rounded pulse edges. Coins contain 2..10 independent sine partials
at 1..10 kHz, amplitude LFOs at 0.3..4 Hz, and separate
3..7 second attack/release times. One or two partials oppose the main stereo
group. Their stereo sum receives two Q 0.8 bell cuts: -12 dB at 3.3 kHz and
-9 dB at 9 kHz, before the dry/reverb split. Coin and laser mix gains are
independent controls. Lasers use 10..14 kHz carriers, weak PM, clustered
positions,
40..200 Hz onset shaking that decays in 100 ms, small level motion, and
a shared 50% duty amplitude pulse at a seeded 3..10 Hz rate. All partials
begin in the same on half-cycle and switch together throughout the formation,
with 2 ms gain smoothing at each switch. Their source duration is 6..14 seconds.
Unsupported high registers stay silent rather than moving into a lower band.

The first normal voice, stutter, coin formation, and laser formation are
scheduled at 3, 6, 4, and 12 seconds. Subsequent voice opportunities are
2..11 seconds apart; formation opportunities are 16..34 seconds apart.
Dialog focus defers entrances. Arrangement and modulation follow the sample
clock and reproduce independently of render block sizes or source gains.

All four sources feed the separately controlled Accents reverb. Its defaults
are 85% send, 88% room, 8% damping, 92% width, and 32% wet. Both dry accents and
their return bypass the scene's low-pass; source filters retain band limits.
The existing low-passed bed and its darker space keep their settings. Actual
waveform tests compare send-zero and wet-zero renders against an audible
tail, alongside isolated gain, ticker articulation, stereo, onset/dropout,
frequency-band, EQ-response, transition-timing, and deterministic rendering
checks cover the source contracts. The allocator audit includes active new sources,
mutations, formation starts/retirement, and reverb at 8, 48, and 192 kHz,
with zero allocations or deallocations after construction.

The broad release suite reports 176 passes and five failures:
two sub tuning assertions expect less than 0.8% detuning despite the existing
+/-0.24-semitone spread, and two harmony assertions expect a three-second
crossfade despite the existing half-second increment. The sub failures were
also reproduced from an untouched HEAD archive; the unchanged timing code and
assertions establish the other two timing mismatches. The default environment's
RMS floor test measures 0.04731 against its 0.05 floor with the current 0.35
drone gain. Explicit Diatonic mode also falls below that floor at 0.04765;
restoring the 0.65 drone reference measures 0.06623. These measurements use
the test's seconds 1..3 window, before coins and lasers begin. Formatting,
all-target Clippy with warnings denied and the headless build pass. CLI/TUI test
coverage includes the four new gains, activity monitoring, and five reverb
buses at 80x24 and larger sizes.

The F1-compensation reference export runs 40 seconds at 48 kHz with seed 123,
changing AAPL to QWIX at 14 seconds and
TSLA at 27 seconds. UI and transmissions are muted. Isolated exports retain
default layer gains, master/output trim, and reverb:

| Signal | Peak dBFS | RMS dBFS including gaps | RMS dBFS in active trace bins |
| --- | ---: | ---: | ---: |
| Environment | -9.85 | -23.37 | -23.37 |
| Demonic voices | -30.05 | -49.23 | -47.07 |
| Demonic stutters | -44.05 | -70.07 | -65.29 |
| Flying coins | -43.75 | -66.11 | -63.31 |
| Laser beams | -43.68 | -63.10 | -59.24 |

Active windows use half-second source-activity traces, so their boundaries are
approximate. Coins retain 99.882% of measured spectral energy at 1..10 kHz;
lasers retain 99.939% at 10..14 kHz. Voice mono fold-down retains about 52%
of mean channel energy, coins 91%, and lasers 92%. The largest sample step in
the environment is 0.01425. High-band reverb tails decay quickly: the laser's
19.1..19.6 second window measures -103.37 dBFS; the voice's 19.1..19.6 second
window measures -68.10 dBFS. These are signal measurements, not a listening
verdict. WAVs, JSONL traces, measurements, and repeatable export/analysis
scripts are in `/private/tmp/shzx-sfx-voice-f1-boost/`.

Actual impulse-response checks at 8, 48, and 192 kHz measure peak error below
0.003 dB and half-power bandwidth error below 0.41%. A separate actual-wave
and coefficient regression checks the late, fast vowel morph at 400 ms and
seven-second utterance lengths across these rates, including its midpoint
and endpoint. The F1-compensation reference voice export places 7.07% of
energy below 300 Hz and 58.31% at 300..1000 Hz; stutters place 8.29% and
60.20% in those bands. These describe signal balance, not transcription
accuracy.

Laser amplitudes share one seeded fixed 3..10 Hz rate per formation, 50% duty,
and 2 ms edge smoothing. The pulse belongs to the beam envelope and gates the
summed partials before filtering. Endpoint tests at 8, 48, and 192 kHz measure
maximum period error of 0.025% and duty error of 0.0187%. Rendered formations
at 44.1, 48, and 192 kHz verify that all partials fade out together: across
four seeds and three cycles at each rate endpoint, energy in the settled off
windows stays below one ten-millionth of the on-window energy. Rates describe
full on/off cycles. Random choices occur at source construction, and the
pulse phase clock uses f64 to retain sample-level timing at high rates.
All seven tone tests, four ambient integration tests, and the allocation audit
pass. Formatting and all-target, all-feature Clippy with warnings denied pass.

Fresh 30-second isolated formation exports at 48 kHz, seed 123, current default
gains and reverb measure -55.02 dBFS peak and -70.55 dBFS full-file RMS for
coins, and -50.43/-70.08 dBFS for lasers. All samples are finite. Coins retain
99.99999% of spectral energy at 1..10 kHz; lasers retain 99.9857% at 10..14 kHz.
Their maximum sample steps are 0.00137 and 0.00433. The coin reverb tail at
17.1..17.6 seconds measures -113.48 dBFS, while the dry comparison is silent;
the laser tail at 22.1..22.6 seconds measures -118.73 dBFS. WAV/JSONL exports,
coin dry comparison, previews, and measurements are in
`/private/tmp/shzx-sfx-dissonant-coins/`. The repeatable export and analysis
script is `/private/tmp/shzx-sfx-audio-update.py`.

The 30-second interactive benchmark at 48 kHz, 256-frame blocks, seed 123,
and 32 available UI voices renders in 2.211 seconds (7.369% of realtime wall
time), with six UI voices observed at block boundaries, no stealing, and no
suppressed requests. This measures offline throughput; native device playback
and callback scheduling were not measured for this revision.


## Ticker chromatic harmony and audition grid

Dissonant harmony is the library and CLI default. `--harmony diatonic`,
`Command::Harmony`, and Scene's Diatonic/Dissonant selectors retain explicit
palette selection. Its four distinct chromatic pitch classes and
pedal derive from every normalized ticker ASCII byte using the documented
fixed integer hash/shuffle. All twelve pitch classes are reachable, with no
scale quantization. The chord stays fixed across render seeds, market data,
focus modes, and elapsed time. Chord changes continue through the existing
pad/pedal crossfade mechanism, including rapid ticker and mode commands.

Harmony is represented by typed diatonic/chromatic variants with validated
chromatic pitch sets; the clock is explicitly fixed in Dissonant mode.
Tests reject malformed/duplicate pitch classes, preserve diatonic chord JSON,
and verify canonical identity for equivalent sets with the same pedal.
Integration tests verify full-ticker changes even when the old diatonic root
and new chromatic root collide, exact irregular-block rendering, bounded
levels at 8/48/192 kHz, and allocation-free mode/symbol commands. The four new
public dissonance tests and six private harmony tests pass.

The Scene page displays a six-column, four-row grid of 24 tickers. Mouse and
keyboard selections remain silent UI actions and use the same validated
symbol command as manual entry. Rejected queue submissions leave selections
unchanged; reset restores the startup ticker and harmony mode. All 47 CLI/TUI
tests pass, including visibility/hit targets at 80x24, 100x32, 140x24, and
160x36. Headless CLI checks also pass. Actual Scene buffers at 80x24 and
160x36 were captured and inspected for readable ticker names and mode
controls. Text captures are in `/private/tmp/shzx-sfx-dissonance-audit/`.

A 32-second, 48 kHz, seed-123 default environment export switches AAPL to IBM
at 8 seconds and TSLA at 20 seconds, with UI and transmission muted. All
snapshots report Dissonant harmony. Peak/RMS are -11.44/-25.95 dBFS, with
92.98% of measured energy below 420 Hz and a maximum sample step of 0.01274.
Around the two ticker changes, maximum steps are 0.00751 and 0.00760. All
samples are finite. The WAV and mode-bearing JSONL trace are in
`/private/tmp/shzx-sfx-dissonant-coins/`.

All 13 TUI mixer faders share a cubic response: normalized position cubed is
the linear amplitude gain sent to the renderer. A halfway position applies
0.125 gain and displays -18.1 dB; zero displays `mute`, and full travel gives
unity. Output trim retains its independent dB scale. Tests exercise keyboard
and fine steps, endpoint clicks, midpoint dragging, reset, command values,
bar positions, and gain readouts at 80x24, 140x24, and 280x36. All use the
existing numeric-control path without separate stored fader positions.

Automatic TUI ticker navigation starts enabled. Its seeded host-clock schedule
chooses a different grid ticker and a fresh 25..180 second interval after each
accepted selection. Manual selections restart the wait, queue rejection retains
the pending target, and a late host update submits at most one change. Symbol
entry defers automatic changes. The Scene toggle pauses or resumes navigation,
and successful reset restores the default enabled state. Deterministic tests
cover 128 deadlines, grid membership, seed reproducibility, queue retries,
manual selection, editor deferral, toggle/reset, and host suspension. The toggle
and countdown fit the existing grid heading at 80x24 through 160x36. All 47
CLI/TUI tests pass; the renderer and audio callback receive the existing typed
symbol command without a separate navigation channel.


## Reference-calibrated demonic voices

Checked 2026-11-05. A/O/U now use vowel-specific narrow principal resonances
and weak supporting bands fitted against `../demonic-voices-reference.wav`.
Two cascaded peak-normalized sections per branch suppress unwanted skirts;
a quiet broad upper branch preserves the rounded vowel's raised region
between its upper details. The six branches and their states are bounded
and preallocated. Excitation, utterance scheduling, and public controls retain
their existing contracts. See [the analysis](demonic-voices-research.md) for
methods, plots, source citations, and local listening artifacts.

At 44.1 kHz, seed 123, isolated dry voice gain 1, the representative O
measurement changes from a 576 Hz dominant peak and 231 Hz apparent width
to 382/108 Hz; the reference is 371/108 Hz. O power at 250-600 Hz changes
from 52.17% to 97.05%, against 96.62% in the reference. Its 2-4 kHz share
changes from 1.390% to 0.190%, against 0.184%. A changes from 969/199 Hz
to 845/102 Hz, against 829/81 Hz, while its 2-4 kHz share changes from
2.29% to 9.29%, against 9.82%. Widths describe smoothed output spectra,
not configured per-section bandwidths.

The eight voice unit tests pass, including output-spectrum checks across
8/48/192 kHz, both profiles, three seeds, and 30/70/110 Hz excitation cases.
They check vowel-dependent peaks and power, supporting detail, upper bridge
contrast, A-to-I articulation, bounded excitation, filter response, and
extreme-rate retirement. In the sustained-vowel regression, O upper-band
share spans 0.182-0.197%, A spans 9.35-10.09%, and the rounded upper
bridge-to-mid-valley mean power ratio spans 1.82-2.41.

`cargo test --release --no-default-features --no-fail-fast` yields 128 passed
and four failures. A separately compiled archive of HEAD yields 127 passed
and five failures. Every current failure is also present in that archive:

- `background::tests::sub_is_a_quiet_stereo_rumble_an_octave_below_each_root`
- `background::tests::sub_bell_attenuates_the_original_root_and_tracks_retuning`
- `pending_harmony_uses_live_context_and_crossfade_progress_tracks_rendering`
- `rapid_symbol_changes_finish_the_current_fade_before_the_latest_pending_root`

The archive additionally fails the universal whisper-amplitude test replaced
by the reference-envelope regression. Current ambient gain/determinism,
reverb routing/tails, and allocation-free rendering/command tests pass.
All 41 CLI/TUI tests pass with all features, and
`cargo clippy --all-targets --all-features -- -D warnings` passes. Full logs
and baseline comparisons are in ignored `target/voice-analysis/`.

Paired release benchmarks used frozen original and tuned executables,
48 kHz, 256-frame blocks, seed/default settings, and two alternating
before/after repetitions per workload. Interactive 30-second means were
2.059634 seconds before and 2.158429 after: 6.865% to 7.195% of realtime
wall time, or 4.80% more wall time. Idle means were 1.869128 and 1.903818
seconds: 6.230% to 6.346%, or 1.86% more wall time, with greater variation
between idle runs. These measure offline throughput, not native callback
CPU usage or scheduling. Raw runs and hashes are in
`target/voice-analysis/paired-benchmarks.json`.


## Restrained demonic register and micro resonances

Checked 2026-11-05. Murmurs and stutters share the approved vowel shapes
with occasional whole-tract motion limited to -6%..+2%. The higher UH and
schwa principal resonances stay below the A anchor. Two small seeded bell
boosts on opposite flanks of each principal branch add local contour, using
the existing flutter phase for gentle movement. A seventh branch slot adds
a faint rounded-vowel detail near 3.7 kHz. Unsupported branches fade out
rather than being moved into another audible band.

At 44.1 kHz, seed 123, the representative O's principal peak stays 382 Hz.
Its 3.5-3.9 kHz power share changes from 0.01499% to 0.02273%, compared with
0.02454% in the reference. Its new upper peak is 3741 Hz at -38.70 dB relative
to the principal peak, versus 3658 Hz/-39.34 dB in the reference. Mean
250-600 Hz power remains 97.11%. These figures use 16384-point Welch spectra
with mean channel power and 25 Hz Gaussian smoothing. A separate 32768-point
analysis with 3 Hz smoothing resolves small flanking ripples while the broad
envelope stays anchored. Plot, methods, and local audio are linked from
[the voice analysis](demonic-voices-research.md).

All 11 voice unit tests pass. New checks cover live constructor-selected
up/down excursions in both delivery styles, deterministic ripple impulse
responses with limited peak/energy changes, and the 3.7 kHz branch at 44.1/48 kHz
with exact silence at 8 kHz. Existing output-spectrum, articulation,
extreme-rate retirement, stage-response, and excitation checks also pass. The full
release headless suite reports 131 passed and the same four known unrelated
sub/harmony failures recorded above. All 41 CLI/TUI tests and strict Clippy
checks pass. Allocation-free rendering/commands, ambient determinism/gain,
and reverb-tail integration tests pass.

Two alternating paired runs per workload compare the frozen approved and
detailed executables. For 30 seconds of audio, interactive mean wall time is
2.187803 seconds before and 2.088201 after; idle is 1.890573 before and 1.908660
after. These runs span roughly 6-7.3% of realtime wall time. Variation prevents
attributing the apparent interactive improvement to the implementation. They
measure offline throughput, not native callback CPU or scheduling. Hashes,
raw runs, and means are in `target/voice-analysis/detail-paired-benchmarks.json`;
test logs use the `detail-` prefix in the same directory.
