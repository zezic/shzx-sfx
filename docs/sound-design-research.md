# Sound design research

Research checked 2026-11-05. This document records the evidence behind the
palette and proposed tuning directions. Numeric settings below are design
starting points for listening tests, not experimentally established thresholds
or a promise that a particular chord causes a particular emotion.

## Harmony and movement

Open Music Theory identifies Aeolian with natural minor and Phrygian as a
minor-tonic mode distinguished by its lowered second. It also describes a tonic
drone and repeating or emphasizing the tonic as ways to establish a modal
center. This supports retaining a low reference pitch while changing the upper
color tones. [Open Music Theory: Diatonic Modes](https://viva.pressbooks.pub/openmusictheory/chapter/diatonic-modes/)

Suspended chords omit the third, which removes the ordinary major/minor
identity. A minor atmosphere needs an audible minor third rather than relying
on a minor key signature alone. Berklee's voicing guidance treats the third
and seventh as the defining notes of a seventh chord and allows the perfect
fifth to be omitted. It also recommends placing tensions above the bass to
avoid muddy spacing. [Open Music Theory: Chord Symbols](https://viva.pressbooks.pub/openmusictheory/chapter/chord-symbols/),
[Berklee: Piano Voicing Techniques](https://online.berklee.edu/takenote/basic-piano-voicing-techniques/)

In Lahdelma and Eerola's single-chord experiment, minor triads received the
highest melancholy/sadness ratings, diminished and augmented triads were
especially tense, and timbre and inversion affected the responses. A separate
experiment found strong association between dissonance and perceived tension,
while mildly dissonant minor-ninth and minor-seventh chords were among the
most preferred. This supports quiet minor extensions as a melancholic base,
with stronger tension used selectively; it does not establish that adding
more dissonance always improves the result. [Lahdelma and Eerola: Single chords convey distinct emotional qualities](https://doi.org/10.1177/0305735614552006),
[Lahdelma and Eerola: Mild Dissonance Preferred Over Consonance](https://doi.org/10.1177/2041669516655812)

Temperley and Tan compared melodies in six diatonic modes with the same tonic.
Ionian was perceived as happiest; Aeolian and Phrygian provide useful darker
alternatives, but familiarity explained much of the ordering. These results
concern melodies, so applying them to this engine's sustained chords is an
artistic inference. [Temperley and Tan: Emotional Connotations of Diatonic Modes](https://davidtemperley.com/wp-content/uploads/2015/11/temperley-tan.pdf)

Musical mood is contextual. A cross-cultural experiment found the major/minor
happiness association strongly related to exposure to Western-influenced
music. These findings do not justify a universal emotion label for a chord or
mode. "Melancholic" and "anxious" describe this palette's intended character,
which still needs listening evaluation. [Smit et al.: Emotional responses in Papua New Guinea](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0269597)

The implemented upper palette always retains a minor third, using A, D, or E
minor over the ticker's unchanged diatonic pedal. A minor ninth retains the
root, minor third, seventh, and ninth while omitting the fifth. E uses the
shared collection's F natural as a flat ninth. A second
profile uses a complementary minor upper chord with an added eleventh,
choosing its anchor so the pedal does not complete a major triad. The anxious
profile is the diatonic B half-diminished seventh,
including its D minor third and F diminished fifth. Over a G pedal, use the
minor-eleventh profile instead: B half-diminished over G becomes G9.
Slash symbols report the actual upper chord and pedal separately. Keeping chord
and pedal notes in A natural minor also preserves compatibility with sounding
harmony reverb tails.
These are design decisions informed by the findings above, rather than
progressions prescribed by those studies.

| Situation | Upper-harmony direction | Intended character |
| --- | --- | --- |
| Quiet or sideways view | Minor ninth and minor seventh with added eleventh | Melancholic, unresolved |
| Rising trend | Minor extensions with occasional half-diminished color | Movement without a major-key celebration |
| Falling trend | More frequent half-diminished upper chords | Anxious, restrained |
| High volatility | Periodic half-diminished color over the same pedal | Increased tension |
| Very low volume | Prefer a minor eleventh to the half-diminished color | Less intrusive tension |
| Dialog focus | Hold the current voicing | Less competing musical information |

An A-pedal audition cycle is Am9(no5), Dm7(add11,no5)/A, Am9(no5),
Bm7b5/A. A C pedal begins with Dm9(no5)/C; an F pedal begins with
Em7(b9,no5)/F. In each case the upper chord retains its minor third rather
than becoming a major voicing at a different ticker root.

Choose chord changes on a slow musical clock, initially around 8-24 seconds.
Changing the visible chart should update the next choice and gradually reshape
the current texture. Avoid coupling every pointer move or price tick to a new
chord. Preserve shared upper notes where possible and crossfade between
voicings so a chart update does not become a pitch jump.

## A smooth, subdued environment

Chorus can simulate several sources in unison by independently modifying
copies using delay, frequency shift, or amplitude modulation. Unison detuning
and chorus are related techniques but operate at different stages of a patch.
[Julius O. Smith: Chorus Effect](https://ccrma.stanford.edu/~jos/pasp/Chorus_Effect.html)

Proposed drone architecture:

- A quiet filtered noise bed supplies air and ventilation texture. Slow,
  independent modulation avoids a static hiss.
- A low sine-rich pedal supplies the room's tonal center. Use a faint second
  harmonic for small-speaker translation instead of relying solely on sub-bass.
- A restrained upper pad uses two or three oscillators per note, initially
  detuned approximately 3-8 cents in each direction, with different phases.
  Feed a low-level modulated delay chorus; do not make stereo width depend
  entirely on cancellation-prone copies.
- Let harmonic swells emerge and recede over several seconds. A continuously
  audible chord is optional; the noise environment can carry quiet intervals.
- Distant melodic fragments can use a sine and a faint second harmonic, slow
  onset, low-pass filtering, feedback delay, and reverb. Leave long gaps.

Start the background low-pass around 0.6-1.5 kHz, with additional damping in
the reverberation path. Deeper chart zoom can move it toward 0.2-0.6 kHz and
reduce the dry signal. Small, quiet transients may remain higher in frequency
because they briefly reveal detail without making the whole mix bright.
Keep modulation of frequencies and gains continuous.

Reverb depth is a compositional control as well as a decay time. A reverb
designer's documentation exposes separate early/late balance, pre-delay,
low-pass high-cut, and decay controls, providing useful independent controls
for this environment. [ValhallaRoom: High Level Sliders](https://valhalladsp.com/2011/05/02/valhallaroom-the-high-level-sliders/)

For dialog focus, audition a broad band-pass on the background, approximately
250-900 Hz, less dry signal, and a stronger long diffuse tail. Apply the
transformation gradually. Keep the dialog's own UI confirmations comparatively
direct so their meaning remains clear. Keep reverb tails alive across mode
changes; switch the send and return balance rather than abruptly resetting the
reverberator.

## Mechanical and digital sound ingredients

The Sound Design Toolkit's own documentation describes a motor model combining
harmonic partials, modulation for uneven rotation, chassis resonances, and
band-pass-filtered noise for turbulence. Its impact models expose each mode's
frequency, decay, and gain. These are useful independent controls for small
mechanical impressions. [SDT: Electric Motors](https://skat-vg.github.io/SDT/group__dcmotor.html),
[SDT Handbook, impact model](https://soundobject.org/SDT/downloads/SDT_handbook_Max6.pdf)

Modal resonators are also used directly in research on rigid-body sound
synthesis. A small resonator bank is a reasonable procedural starting point;
it need not claim to reconstruct a physical device. [Diaz et al.: Rigid-Body Sound Synthesis with Differentiable Modal Resonators](https://arxiv.org/abs/2210.15306)

Proposed ingredients, synthesized without recorded assets:

- Contact: a sharp noise injection, inharmonic ring components, and quantized
  details. Keep the clean fundamental subordinate to the synthetic texture.
- Servo: segmented ring/FM and held-noise friction rather than a continuous
  musical bend. Stable pitches or tiny onset settling suit the short gesture.
- Digital glitch: alternating held noise, crushed FM, ring modulation,
  inharmonic components, and gaps. Filter the resulting discontinuities.
- Air release: filtered noise grains with abrupt internal detail and a short
  release rather than a long natural decay.
- Large action: combine these components into overlapping, distinguishable
  bursts and cascades. Add temporal structure within a short event instead of
  stretching a single tone.

## Event envelope and detail palette

The event palette uses an artificial amplitude contour: 0.35-0.9 ms attack,
4-9 ms decay to 15-22% sustain, a held body, and a 2-3 ms release. These are
artistic tuning values for this engine. Contact/modal noise injections can
decay inside that envelope, while the main body keeps its low sustain level.
The event's delay and reverb can continue after its source release.

Pitch stays fixed in digital and air components. Mechanical components settle
by 0.75-1.5 semitones within 2-3 ms at onset. Discrete note ratios distinguish
event signatures without a gesture-duration glide.

Four families alternate noise, ring modulation, crushed FM, inharmonic detail,
and gaps in six-step micro-patterns. Each detail lasts approximately 1.5-4.3 ms;
seeded rate, offset, phase, and sample-hold bandwidth vary each performance.
Longer event signatures layer several such bursts. Cascaded low-pass filtering
keeps the dense modulation and short noise injections subdued.

Source calibration gives background and UI buses separate reference levels
before the ordinary linear mixer controls. The output passes normal signals
linearly and shares a soft-knee gain across both stereo channels when events
overload the available headroom. This is a sample-level ceiling, not a
true-peak measurement.

## Interaction families

The Glasgow earcon researchers recommend designing families through timbre,
register, and rhythm, then differentiating subgroups with other parameters.
They caution that loudness variation can annoy listeners and should not be the
sole way to distinguish meanings. Spatial placement can help distinguish
overlapping sounds. [Brewster, Wright, and Edwards: Earcon Construction Guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)

The following palette is a design proposal; meaningful distinctions should
survive small random pitch, duration, and noise variations.

| Family | Candidate events | Proposed distinction |
| --- | --- | --- |
| Pointer | hover enter, hover leave | Quiet, abruptly bounded noise grains |
| Buttons | soft click, firm click, toggle on/off | Muted contact, firmer latch, opposed detail/rhythm signatures |
| Navigation | tab, panel, symbol change | Brief scanner/servo gesture |
| Scrolling | wheel, scrollbar grab/release, zoom in/out | Rate-limited ticks, friction slices, brief onset settling |
| Dragging | pick up, drag motion, valid drop, rejected drop | Contact, optional friction, latch, broken double gesture |
| Editing | focus, submit, validation failure | Sparse focus grain; short confirmation; irregular rejection |
| Dialogs | open, close | Airlock or servo contour, plus background focus transition |
| Trading | order created, filled, canceled, rejected | Distinct multi-part signatures with a shared mechanical timbre |
| Connectivity | connected, disconnected, recovered | Separate rhythms so status is recognizable without volume escalation |

Coalesce rapid hover and scroll events. Sustain behavior only where the caller
can supply its start, update, and end; otherwise use bounded, short impulses.
Reserve longer signatures for low-frequency, consequential events. Background
ducking can expose these events without making them loud.

## Position and depth

The precedence effect favors localization toward the first-arriving source
when closely spaced arrivals fuse. Its behavior depends on timing, level, and
signal; it is not a complete binaural positioning model. [2CAudio: Precedence Manual, Haas discussion](https://www.2caudio.com/sitecontent/products/precedence/2CAudio_Precedence_Manual.pdf)

Start with equal-power gain panning from the horizontal UI coordinate. Add a
small opposite-channel delay, initially 2-8 ms, for selected transients and
make this optional. Centered sources need no interchannel delay. Derive the
delay from the absolute pan position and put the earlier arrival on the source
side. Check mono fold-down: summing a signal with a delayed copy creates comb
filtering, so essential event identity must also exist in rhythm and timbre.

## Market and view mapping

These are interface design choices, not established perceptual encodings.

| Input from the visible region | Control to audition |
| --- | --- |
| Normalized price slope | Harmonic contour and voicing family |
| Relative traded volume | Impulse density, ventilation movement, slight pad width |
| Realized volatility | Noise roughness, modulation depth, glitch probability |
| Zoom depth | Lower background cutoff, more distance, faint detail grains |
| Interaction activity | Temporary texture movement, then gradual settling |
| Dialog/focus mode | Band limitation, lower dry level, longer reverberant impression |

Normalize volume against a reference window rather than using raw share count.
Choose slope from the visible series rather than the last tick. Smooth
continuous controls and introduce a neutral region for trend selection so
negligible changes do not oscillate the harmony. Use a deterministic seed to
compare mappings and parameter changes in CLI renders.

## Rust implementation reference

FunDSP supplies noise, oscillators, linear filters, chorus, and stereo reverb
primitives. Its examples connect DSP graphs to CPAL and set graph sample rate
from the output configuration. Use the actual selected device rate and allocate
long delay/reverb state before rendering. [FunDSP official documentation](https://docs.rs/crate/fundsp/latest),
[FunDSP native input/output example](https://github.com/SamiPerttu/fundsp/blob/master/examples/input/src/main.rs)

The references DEFCON and Elite: Dangerous define the requested aesthetic here;
none of these recipes claim to reproduce their original synthesis systems or
reuse their audio. Evaluate the engine's own voices and long-term comfort in
the CLI before connecting it to the terminal.

## Current implementation controls

Ticker names select an anchor degree in A natural minor through a stable hash.
Generated upper voicings retain minor or half-diminished quality over that
pedal; scale membership keeps sounding harmony reverb tails compatible while
the next ticker's pedal and pad crossfade. The base changes
only on a symbol command. Continuous view updates shape the arrangement and
the next chord choice within that base.

Pad chord tones use octave folding, inversions, and dropped octaves to stay
below 420 Hz while preserving pitch classes, minor qualities, and chord
symbols. The four voices remain distinct, sorted, and above the pedal;
semitone neighbors created by folding are spread across octaves where possible.
G4 is the highest nominal note in this range of A natural minor. With the
maximum +0.055-semitone unison offset and +0.5-semitone drift, its oscillator
frequency remains about 404.8 Hz.

A separate sub layer spreads sixteen saw oscillators across pitch and panorama,
one octave below the existing pedal. A resonant low-pass at 0.85 times the
pedal frequency and a -15 dB, Q 1.3 bell at that pedal leave a low rumble.
The sub shares the pad/pedal root crossfade and environment filtering. All
audible oscillators share a smooth frequency ramp from zero to target over
one second. Independent pad-note drifts wait 12-36 seconds initially, make
6-12 second excursions within +/-0.5 semitone, then rest 18-50 seconds.

Transmission start/end commands delimit a complex opening, up to a validated
pulse budget, and a complex closing. Each burst independently chooses its
opening, tick, and closing sounds from suitable effect pools. A separate
preallocated SFX voice pool feeds two cascaded 8 kHz high-pass stages and its
own reverb. At or below 16 kHz sample rate that high band stays silent.
Defaults use 8% dry gain, 67% send, 83% damping, 25% wet, and a 22% transmission
mixer level; the source bus has its own calibration after the strong high-pass
attenuation. The nominal rate defaults to 10 Hz. At burst start, four group
rates are chosen independently and uniformly from 0.75x to 1.25x the nominal
rate, clamped to 2..80 Hz. Each group holds a constant rate, with at most three
switches after ticks 4, 8, and 12; the final group holds its rate through the
rest of the burst. These choices reproduce for a fixed seed and command
timeline. UI mixer gain and reverb settings do not control this bus; the shared
master and reverb return remain available.

TUI transmission demos last a seeded random 300..5000 ms and allow up to 256
pulses per burst, leaving room for repetition through the five-second maximum.
Finite render and playback demos use 300..2000 ms bursts. Both leave 3..8 second
gaps. The public transmission pulse budget defaults to 24.

Routine event durations are reduced by approximately 35%. Orders, dialogs, alerts, and connection
signatures retain their longer structures. Short clean oscillator flecks
alternate with FM, PM, sync, pulse/folded, ring, crushed, and noise components.
Articulated LP/HP profiles shape the main body; a separate high-pass
path supplies brief 1.6-3.3 kHz HP / 12-17 kHz LP flecks at 7-15% level. Bright components
end within milliseconds. After the one-second startup glide, main pitch is
fixed or settles at onset.

Distant melody notes are chosen uniformly from C5..B5 (MIDI 72..83), covering
all twelve chromatic pitch classes independently of the chord and pedal's
A-natural-minor collection. They use a smooth three-second attack and
eight-second decay, for an eleven-second source envelope. Their oscillator
frequency has +/-2.5% vibrato at 4.3 Hz, with phase derived from note age for
reproducible rendering. The first entrance is scheduled seven seconds after
startup, with at least eleven seconds between later entrances. The melody
mixer defaults to 8%, and the UI/event mixer defaults to 40%.

Melody and its existing ping-pong delay feed a dedicated reverb, with defaults
of 95% send, 88% room, 68% damping, 92% width, and 85% wet. A 2.5% dry gain
leaves the direct melody barely audible beneath its wet return. The TUI's
Melody reverb selector, `ReverbSettings.melody`, and `reverb melody` commands
expose these controls. Melody feeds this processor separately from the
environment reverb, while mode and zoom HP/LP filters still shape the combined
background output.

Details and melody have independent damped ping-pong sends whose times,
feedback, and sends react to zoom, volatility, activity, volume, and focus.
Changing times crossfades between fixed taps, retaining old tails without
turning the transition into a pitch slide.

The environment, UI, transmission, and melody buses each use a diffuse, slowly
modulated 32-channel hybrid
FDN with live loop filtering. This uses FunDSP's native Rust processor,
preallocated at startup. Independent sends and returns preserve a close dry
UI image while allowing longer, quieter tails. Focus modes narrow the
environment, UI, and melody wet image without altering dry panning. [FunDSP reverb2_stereo](https://docs.rs/fundsp/0.20.0/fundsp/hacker32/fn.reverb2_stereo.html)

Global Haas mode maps the requested SFX and transmission position range
-1..1 to -0.33..0.33 for amplitude panning. Haas delay timing and crossfeed
use the full requested position. Pan mode uses the full amplitude-pan range.
Mode changes smoothly update gains for existing SFX voices and live
transmissions. Ambient stereo width retains its separate controls.

Source balance makes the noise, pad, and melody audible relative to the pedal;
an independent -60..+24 dB output trim defaults to +12 dB. The monitor exposes
current and target filter, width, delay, and harmonic state while those
transitions happen.

## Auditable event layers

The event voice has six explicit paths: a clean muted Body; attenuated
nonlinear Texture; high-passed FM Tonal micro-sequences; short
filtered-noise Flecks; quiet high-HP phase-modulated Shimmer; and high-band
intermodulated Crisp. Texture
contains the folded PM, FM, sync, pulse, quantized/ring, and held-noise
material, making its contribution directly soloable instead of buried inside
other named components. Its relative amplitude is 0.25.

Tonal slots use a 1.2 kHz HP and 8 kHz LP, carrier tuning 0.52x, FM index 1.6,
and a 4.7x modulator ratio. Six-step patterns span approximately 0.3-3.8x,
with different orders selected by interaction family.
Shimmer uses a 7.2 kHz HP and 16 kHz LP around a nominal 8.4-12.5 kHz carrier,
PM depth 0.035 cycles at 0.7 Hz, and a short bounded window. It is disabled
below 20 kHz sample rate. Filter bands are clamped for the actual sample rate.

The Tonal editor exposes the sine carrier's tuning, FM index and modulator
ratio, six pitch ratios, a separate rate envelope, and slot attack/release. Its
signal passes through the slot gate, the event envelope, and the Tonal HP/LP
filters. Default rate contours are 380->25 Hz for contact, 30->490 Hz for
digital, 270->42.5 Hz for servo, and 45->210 Hz for air. Fixed and sweep
step-rate overrides retain their supplied nominal rates. The linear envelope
restarts on each macro phrase part. Clock phase integrates that rate rather
than multiplying elapsed time by the instantaneous rate. The starting
position remains seeded; edits to this clock change only Tonal, while
Texture retains its own grain clock. Carrier and sequence
ratios remain relative to the event's pitch, preserving whole-event
transposition while allowing Tonal to be tuned independently. Resetting its
synthesis retains its level, mute, and filter settings.

Tonal has independent stereo oscillator and filter histories, with +/-4-cent
tuning and +/-0.15% sequence-rate offsets. Flecks, Shimmer, and Crisp use
0/0.24 ms, 0.32/0 ms, and 0/0.16 ms L/R delays, blended at 25%, 20%, and 22%
with the direct signal. Their delay state is allocated with the voice pool,
flushed when a slot starts a new event, and included in voice retirement.
Pan and optional positional Haas crossfeed follow the stereo layer sum.
Keeping a dominant direct component limits mono fold-down cancellation from
the delayed paths. [Smith: Feedforward Comb Filters](https://ccrma.stanford.edu/~jos/pasp/Feedforward_Comb_Filters.html)

Recipe metadata comes from the same profiles used by rendering: relative
gains, carriers, filters and gestures, envelope timing, per-kind patch
parameters, FM/PM and quantizer values, clock ranges, fleck bursts, and pitch
ratios. I/7 in the terminal panel exposes it with real-level layer solos;
`inspect KIND --json` exports it without an audio device. Body/Texture windows
cover the event with envelopes restarted on macro phrase parts; Tonal uses
single slots; Flecks and Shimmer repeat per part. Actual seed/pitch/duration
variation and component selection remain in generated-event traces.

## Broad comfort EQ and high-band intermodulation

A shared SFX bell EQ applies -9 dB at 3 kHz with Q 0.65 to reduce the prominent
mid/high-mid emphasis. It precedes the dry/reverb split and is exposed in each
recipe. No global makeup gain compensates for the intended ear-pressure cut.

Crisp is a separate sixth path, rather than a boost to the lower-band dirty
Texture. It combines widely separated low/high oscillators, an auxiliary
oscillator and LP-colored noise, with three-stage PM/noise contours, moving
frequencies, and ratcheted amplitude. Its final 24 dB/oct HP is 6 kHz and LP
16 kHz. The high-pass keeps complex modulation out of the lower body band.
Like Shimmer it is disabled below 20 kHz sample rate; bands clamp for the
actual device. All parameter contours, levels, and filters are source-backed
recipe metadata and can be soloed at their real gain.

Sharp routine sounds use lower sustain with slightly longer decay/release
while retaining an immediate attack. The envelope is chosen per interaction
family; it is not a requirement that every sound hold a rigid plateau.
The SFX page keeps the palette left and construction inspector right at all
supported terminal sizes. Other pages retain their background monitoring.
