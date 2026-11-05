# Demonic voices: reference analysis and tuning

Analysis and tuning checked 2026-11-05. Reference:
`../demonic-voices-reference.wav`, 5.6039 seconds, stereo 44.1 kHz,
signed 16-bit PCM. The implementation fits its broad spectral characteristics
without sampling the recording or reconstructing its production chain.

## Result

The rounded vowel now has a low, narrow main resonance, a much quieter
secondary peak, two faint upper details, and a gently raised region between
those upper details. The open vowel has a narrow main resonance, a low
supporting component, a broad shoulder, and substantially stronger upper
resonances. These use the existing clipped-pink/sine excitation and voice
scheduler.

![Reference, original, and tuned spectral envelopes](assets/demonic-voices-tuning.png)

Representative dry renders use seed 123 at 44.1 kHz. All three curves are
normalized to their own principal peak for shape comparison. Reference
windows are 0.4-2.6 seconds for oouu and 3.4-5.5 seconds for aaahh. Original
and tuned render windows are 3.4-5.2 seconds of the first utterance, using
symbol `O` for O-to-U and `I` for A-to-I; this window is within the first
vowel. These are representative trajectories, not population statistics.

| Vowel | Version | Main peak, Hz | Apparent width, Hz | Power at 250-600 Hz | Power at 2-4 kHz |
| --- | --- | ---: | ---: | ---: | ---: |
| oouu | Reference | 371 | 108 | 96.62% | 0.184% |
| O | Original | 576 | 231 | 52.17% | 1.390% |
| O | Tuned | 382 | 108 | 97.05% | 0.190% |
| aaahh | Reference | 829 | 81 | 1.94% | 9.82% |
| A | Original | 969 | 199 | 3.98% | 2.29% |
| A | Tuned | 845 | 102 | 0.94% | 9.29% |

Widths are measured half-power widths in the smoothed output spectrum,
including noise statistics, motion, averaging, and channel differences.
They are not exact vocal-tract bandwidth measurements. Upper-band power
comes from the unsmoothed spectrum. The open vowel's below-250 Hz share is
5.61%, compared with 6.29% in the reference.

The faint upper details matter even though their total energy is small.
For the reference oouu, the envelope near 797 Hz is about 19 dB below the
main peak; the upper region near 2740 Hz is about 34 dB down. The intervening
1.4-1.8 kHz region is much darker. Raising a global noise floor would erase
that separation. Instead, a weak broad upper branch lifts only the relevant
region while narrower branches retain local peaks.

The aaahh reference has secondary structure near 1222 Hz (-11 dB), 2622 Hz
(-16 dB), and 3122 Hz (-17 dB), relative to the principal peak. Its 2-4 kHz
power fraction is about 55 times the rounded reference's. Different widths
and levels for each vowel are therefore central to this match.

## Filter design

Each [voice](../src/background/voices.rs) owns six preallocated resonance
branches. Every branch groups two cascaded peak-normalized bandpasses with
its own smoothed gain. Three principal branches are accompanied by optional
low, body, and upper bands, represented by `Option<Formant>`. Consonants and
other vowels use the same mechanism without fabricated silent detail bands.
The extra resonances do not create another audio layer or command path.

Two sections give steeper skirts than one section and reduce the main
resonance's spill into spectral valleys. Their transfer functions sum
coherently because they receive the same excitation. Consequently, gains
must be fitted against the combined response, including interference, rather
than treated as independent acoustic peak levels. The broad upper shoulder
and narrow details are fitted together for the same reason.

`Formant.bandwidth_hz` is the nominal bandwidth parameter of each section,
not a promised final branch width. Two identical narrow sections produce a
composite half-power width approximately 0.644 times that of one section.
Broad sections depart from that approximation and can shift their peak;
the actual digital response and rendered spectrum determine calibration.
Source-tilt and output-low-pass compensation apply once to the complete
branch, not once per section.

Peak normalization preserves a filter's peak response rather than its
integrated noise energy. Narrowing therefore also changes overall level.
[Csound: reson](https://csound.com/docs/manual/reson.html).
The distinction between coherent parallel formant branches and a cascade
tract model is also described by Klatt.
[Klatt, 1980](https://www.source-code.biz/klattSyn/Klatt-1980.pdf).

A compact numerical fit of the reference used the actual resonator equations,
a measured recreation of the existing clipped-pink/sine source, and the
combined complex transfer function. Two-section models reduced the rounded
vowel's smoothed log-envelope error substantially relative to single-section
models. Actual Rust renders then determined the final A width and low-band
level. The objective is broad envelope agreement; individual random FFT
spikes are not targets.

The A, O, and U presets follow these reference-derived shapes. Other vowel
centers retain anchors from the published English-whisper study, with their
own design widths and levels. Low and High identify synthesis variants,
not demographic speaker measurements. The published fronted OO centers
are not substituted for the rounded reference target.
[Sharifzadeh et al.: original whisper study](https://lintech.org/webpapers/JVoice3Revised1.pdf).
Csound's vowel presets independently illustrate vowel-specific resonance and
amplitude choices, but they are not measurements of this recording.
[Csound: Formant Values](https://csound.com/docs/manual/MiscFormants.html).

## Source, transitions, and space

Excitation retains soft-clipped pink noise, a seeded 30-110 Hz sine at
amplitude 0.1, +6 dB drive, and another soft clip. The reference's dense,
irregular spectral detail did not establish a shared stable vocal-fold
harmonic ladder. Raw autocorrelation peaks can arise from resonator ringing.
After envelope flattening, mid-channel periodicity was weak and inconsistent;
a repeatable feature in the left channel alone could reflect either delay
coloration or periodic content. This does not prove an entirely unvoiced
source, and does not justify adding a conventional pulse source or
subharmonics as part of this tuning.

Phone timing remains tied to English ticker-letter names. Frequencies,
section bandwidths, and dB targets interpolate during the final 30 ms of a
phone, capped at 55% for short phones. When an optional band appears or
disappears, its present endpoint retains its frequency and width while its
linear amplitude fades. All gains smooth over 5 ms. The existing seeded
formant excursion, utterance envelope, and stutter clocks remain in use.

Two slots retain their Haas positions and the Accents reverb routing.
The reference also has channel-dependent spectral differences: its first
window's left/right dominant peaks are about 409/361 Hz, versus 840/824 Hz
in the second. This tuning does not reconstruct those independent channel
trajectories or the recording's amplitude phrasing. Dry spectral agreement
is the current implementation target. Default voice/stutter mixer gains
remain zero.

## Validation and reproducibility

The voice regression renders multiple seeds and excitation frequencies over
8/48/192 kHz, checking main peak placement and width, vowel-specific band
energy, the weak rounded secondary peak, upper supporting details, and the
raised upper region. It also exercises A-to-I articulation, where the A's
supporting bands retire. The existing stage-response, excitation, extreme-rate,
block determinism, gain, allocation, and reverb-tail contracts remain relevant.
Exact command results are recorded in [validation.md](validation.md).

Welch analysis uses 8192-sample Hann windows with 75% overlap and arithmetic
mean channel power. Gaussian smoothing uses sigma 25 Hz. Band percentages
integrate unsmoothed power over the specified range and divide by full-band
power. Channel power averaging avoids Haas comb filtering from a mono
fold-down. An independent analysis using longer FFTs and 20 Hz smoothing
confirmed the principal reference features.

For the representative renders, a time-zero mixer command targets voice gain
1, master 0.55, and all other source gains and the reverb return at zero.
The 40 ms gain ramps settle before the first utterance starts at 3 seconds.
Output trim remains +12 dB. Source filtering, Haas delay, and final output
processing remain active. Trace snapshots confirm one murmur throughout the
measurement window. Symbol `I` starts with A; symbol `A` starts with E.

Generated local artifacts are in ignored `target/voice-analysis/`:

- `voice-only.json`: exact mixer timeline.
- `current-ou.wav` / `current-ai.wav`: original dry renders and JSONL traces.
- `tuned-ou.wav` / `tuned-ai.wav`: tuned dry renders and JSONL traces.
- `tuning-analysis.py` and `tuning-metrics.json`: analysis, plotting, and data.
- `reference-before-after.wav`: reference, original, and tuned rounded vowel,
  followed by reference, original, and tuned open vowel, with 0.5-second gaps.
  Each region is scaled to -20 dBFS RMS before 15 ms edge fades. Equal RMS is
  not equal perceived loudness.
- `audition-timeline.json`: exact comparison timestamps.

Example reproduction, with the local mixer script present:

```sh
cargo build --release --no-default-features
target/release/shzx-sfx render --symbol O --seed 123 --sample-rate 44100 --script target/voice-analysis/voice-only.json --seconds 24 --output target/voice-analysis/tuned-ou.wav
target/release/shzx-sfx render --symbol I --seed 123 --sample-rate 44100 --script target/voice-analysis/voice-only.json --seconds 24 --output target/voice-analysis/tuned-ai.wav
python3 target/voice-analysis/tuning-analysis.py
```

The Python analysis uses NumPy, SciPy, SoundFile, and Matplotlib. The input is
a short processed recording: source and tract cannot be uniquely separated,
and matching its spectrum does not itself establish perceptual realism.
