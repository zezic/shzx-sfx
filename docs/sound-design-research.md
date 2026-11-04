# Sound design research

Research checked 2026-11-04. This document records the evidence behind the
palette and proposed tuning directions. Numeric settings below are design
starting points for listening tests, not experimentally established thresholds
or a promise that a particular chord causes a particular emotion.

## Harmony and movement

Open Music Theory identifies Aeolian with natural minor and Phrygian as a
minor-tonic mode distinguished by its lowered second. It also describes a tonic
drone and repeating or emphasizing the tonic as ways to establish a modal
center. This supports retaining a low reference pitch while changing the upper
color tones. [Open Music Theory: Diatonic Modes](https://viva.pressbooks.pub/openmusictheory/chapter/diatonic-modes/)

Suspended chord symbols replace the third with another interval, commonly the
fourth. Removing the third leaves the chord without the major/minor distinction
of a conventional triad. Common tones and small voice movements give smooth
connections between voicings. [Open Music Theory: Chord Symbols](https://viva.pressbooks.pub/openmusictheory/chapter/chord-symbols/),
[Open Music Theory: Jazz Voicings](https://viva.pressbooks.pub/openmusictheory/chapter/jazz-voicings/)

Musical mood is contextual. A cross-cultural listening experiment found the
major/minor happiness association strongly related to exposure to Western
musical culture; the result does not establish a universal emotional meaning
for either mode. Treat labels such as "cold" and "dramatic" as artistic
intentions to evaluate by listening. [Smit et al.: Emotional responses in Papua New Guinea](https://pmc.ncbi.nlm.nih.gov/articles/PMC9242494/)

Proposed palette:

| Situation | Harmonic direction | Intended character |
| --- | --- | --- |
| Quiet or sideways view | sus2, sus4, minor add9, long pedal | Unresolved, restrained |
| Rising trend | Minor add9 into wider minor7 or relative-major colors | More space and movement without a victory fanfare |
| Falling trend | Minor add9, minor6 colors, diatonic tritone tension | Greater tension without an alarm |
| High volatility | Retain harmonic center, introduce a quiet dissonant upper tone | Unsettled texture |
| Dialog focus | Hold or thin the current voicing | Less competing musical information |

A useful A-centered audition sequence is Am(add9), Fmaj7/A, Dm(add9)/A,
Esus4/A. Compare that with a less functional sequence of Asus2, Am(add9),
Asus4, Am7. The implemented shared A-natural-minor collection uses F/B tension rather than a borrowed flat-second note, so symbol-root transitions keep every voiced note in the same scale. Keep close clusters out of the lowest register. These are compositional
proposals, not claims about the cited textbook's recommended progressions.

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
Voicings transpose diatonically, including their actual major, minor, or
half-diminished qualities; scale membership also keeps old delay/reverb tails
compatible while the next ticker's pedal and pad crossfade. The base changes
only on a symbol command. Continuous view updates shape the arrangement and
the next chord choice within that base.

Routine event durations are reduced by approximately 35%. Orders, dialogs, alerts, and connection
signatures retain their longer structures. Short clean oscillator flecks
alternate with FM, PM, sync, pulse/folded, ring, crushed, and noise components.
Articulated LP/HP profiles shape the main body; a separate quiet high-pass
path supplies brief 1.6-3.3 kHz HP / 4.8-7.8 kHz LP flecks. Bright components
end within milliseconds. Main pitch is fixed or settles at onset; no medium
pitch sweep is mixed into a full-band event.

Details and melody have independent damped ping-pong sends whose times,
feedback, and sends react to zoom, volatility, activity, volume, and focus.
Changing times crossfades between fixed taps, retaining old tails without
turning the transition into a pitch slide.

The environment and UI each use a diffuse, slowly modulated 32-channel hybrid
FDN with live loop filtering. This uses FunDSP's native Rust processor,
preallocated at startup. Independent sends and returns preserve a close dry
UI image while allowing longer, quieter tails. Focus modes narrow the wet
image without altering dry panning. [FunDSP reverb2_stereo](https://docs.rs/fundsp/0.20.0/fundsp/hacker32/fn.reverb2_stereo.html)

Source balance makes the noise, pad, and melody audible relative to the pedal;
an independent -60..+24 dB output trim defaults to +12 dB. The monitor exposes
current and target filter, width, delay, and harmonic state while those
transitions happen.

## Auditable event layers

The event voice has five explicit paths: a clean muted Body; attenuated
nonlinear Texture; high-passed low-index FM Tonal micro-sequences; short
filtered-noise Flecks; and quiet high-HP phase-modulated Shimmer. Texture
contains the folded PM, FM, sync, pulse, quantized/ring, and held-noise
material, making its contribution directly soloable instead of buried inside
other named components. Its relative amplitude is 0.25.

Tonal slots use a 1.2 kHz HP and 8 kHz LP, FM index 0.08, and a 1.5x
modulator ratio, with short ratio sequences selected by interaction family.
Shimmer uses a 7.2 kHz HP and 16 kHz LP around a nominal 8.4-12.5 kHz carrier,
PM depth 0.035 cycles at 0.7 Hz, and a short bounded window. It is disabled
below 20 kHz sample rate. Filter bands are clamped for the actual sample rate.

The Tonal editor exposes the sine carrier's tuning, FM index and modulator
ratio, six pitch ratios, a separate step rate, and slot attack/release. Its
signal passes through the slot gate, the event envelope, and the Tonal HP/LP
filters. The default clock follows the seeded 230-670 Hz detail rate and
starting position; an edited rate changes only Tonal. Carrier and sequence
ratios remain relative to the event's pitch, preserving whole-event
transposition while allowing Tonal to be tuned independently. Resetting its
synthesis retains its level, mute, and filter settings.

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
