# Procedural rain synthesis

Research checked 2026-11-05. The sources below inform a compact, entirely
procedural background effect. This implementation is a perceptual model, not
a fluid simulation or a calibrated model of a particular vehicle. Numeric
filter, event-rate, resonance, decay, and mix settings are listening-test
choices unless explicitly identified as measurements.

## Rain microstructure and diffuse bed

Verron and Drettakis distinguish three rain-impact sounds: liquid bubbles
represented by chirped, exponentially decaying sinusoids; resonant solids such
as plates and windows represented by sums of fixed-frequency decaying modes;
and short noisy impacts for surfaces such as leaves or stone. They add an
equalized noise background to represent many simultaneous distant drops at low
cost. Their controls independently set impact probabilities and gains, with
randomized amplitudes. Impulsive sounds are localized, whereas continuous
noise represents a diffuse field.
[Verron and Drettakis, 2012, sections 2.1 and 3: author-hosted paper](https://www-sop.inria.fr/reves/Basilic/2012/VD12/soundParticles_aes.pdf)

Miklavcic, Zita, and Arvidsson synthesize rain by superposing individual drop
sounds with randomized sizes and positions. They distinguish the initial
impact from a later liquid-surface contribution and caution that not every
drop creates an entrained bubble. Their real-time scheduling randomizes sound
arrival times to avoid an artificial buildup caused by scheduling distant
impacts without accounting for travel time.
[Miklavcic et al., DAFx 2004, sections 1-3: official proceedings](https://www.dafx.de/paper-archive/2004/P_169.PDF)

Liu, Cheng, and Tong likewise separate initial impacts and entrained-bubble
oscillation. Their method distinguishes solid materials using sound textures
and uses statistical clustering to reduce the cost of large rain scenes.
Their recorded material textures and precomputed sound bank are separate
techniques from the purely generated sources proposed here.
[Liu et al., SIGGRAPH 2019: author-hosted paper](https://haonancheng.cn/attaches/2019%20SIGGRAPH.pdf)

Implementation choices for this engine:

- Combine a quiet, band-limited stereo noise bed with irregular brief noisy
  drops. Keep the individual events below the level where rain becomes a
  sequence of conspicuous clicks or pitched notes.
- Use decorrelated left/right excitation and randomized impact positions.
  Modulate the diffuse level slowly and shallowly so it remains rain rather
  than an obvious repeating swell.
- Use a fixed preallocated bank of material resonators and seeded random state
  owned by rain. Overlapping drops excite the existing material responses.
  Schedule at sample time so callback block size cannot change the sequence.
  Avoid consuming another background layer's random stream.
- Use a small modal contribution for the roof/cabin surface. A pitched liquid
  bubble is not the right model for every roof strike.

The current texture keeps the wash below the combined drop detail, with
independent patch excitation and short smoothed splash envelopes of varied
duration. Roof strikes combine a softened contact with a noise-dominant body,
using a 2 ms attack and 30-61 ms exponential decay. Larger drops contribute
more of that body; retriggering bounds the level within each material patch.
The existing low modes and cascaded low-pass filters supply the enclosure's
color without relying on a high-Q ringing tail.
These are tuning choices based on listening feedback, rather than parameters
prescribed by the papers.

These choices simplify the papers' models to suit the renderer's bounded
real-time work and existing DSP. The diffuse bed does not reproduce the
papers' measured multiband noise envelopes or a statistical rainfall-rate
calibration.

## Dialog and focus distance

Kolarik, Cirstea, and Pardhan tested level and direct-to-reverberant-ratio cues
using speech at virtual distances. Both supplied distance information; their
relative usefulness depended on the simulated room. This supports changing
level and dry/wet balance together, without claiming that one wet ratio
specifies a physical distance for a diffuse rain field.
[Kolarik et al., JASA 2013: original study abstract](https://pubmed.ncbi.nlm.nih.gov/24180748/)

Implementation choice: dialog/distant and focus/thinned modes reduce rain's
direct gain and increase the relative reverb return. Feed the reverb before
the direct-gain reduction so ducking does not erase the diffuse tail. Smooth
mode changes and continue generating rain through them; do not reset its
random generator, event envelopes, or reverb history. Preserve the existing
independent layer gain and spatial controls.

## Deep zoom: inside a parked car

Veen, Mealman, and Eppard directly excite complete vehicle roofs with
repeatable artificial rain and measure resulting interior sound pressure.
Their published abstract establishes roof excitation as part of a vehicle's
interior acoustic signature and compares several vehicles/roof systems; it
does not provide a universal cabin transfer function.
[Veen et al., SAE 2001-01-1490: publisher abstract](https://saemobilus.sae.org/papers/a-method-apparatus-generating-full-vehicle-roof-system-rain-noise-purpose-development-benchmarking-interior-noise-performance-measurement-2001-01-1490)

Steam Audio treats occlusion, frequency-dependent transmission, and reflected
sound as distinct controls. Its transmission EQ separates low, middle, and
high bands. This is a useful architecture reference for keeping transmitted
exterior sound separate from locally excited surfaces; it is not a measured
car-rain response.
[Valve: Steam Audio source propagation controls](https://valvesoftware.github.io/steam-audio/doc/unreal/source.html)

Implementation choice: as zoom deepens, crossfade toward two components: a
stronger low-pass-filtered exterior wash and nearby, dull, short roof impacts.
Keep the roof impacts on a separate spectral path so filtering the outside
wash does not remove the cue that drops are striking the enclosure. Use
several weak, damped modes and a brief noisy onset rather than a ringing bell.
Limit the roof's long reverb contribution: a small enclosed cabin should not
turn into a large resonant hall. The low-pass cutoff and roof modes are sound
design approximations, not dimensions inferred from the cited vehicle study.

## Validation

Render identical command timelines with fixed seeds and varied block sizes.
Inspect isolated rain at supported sample rates, mode transitions, and deep
zoom. Compare peak/RMS and high-band power, verify bounded nonzero stereo
output, and check that deep zoom reduces exterior brightness while retaining
roof-impact transients. Verify that lowering direct rain gain preserves its
reverb tail. Listen to these isolated paths and the complete mix before
treating numerical checks as evidence of convincing rain.
