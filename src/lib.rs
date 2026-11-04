//! Procedural, market-responsive sound for the Shizex trading terminal.
//!
//! [`Renderer`] is deterministic and independent of an audio device. With the
//! default `playback` feature, `AudioEngine` owns native playback and accepts
//! nonblocking commands through a cloneable `AudioHandle`.

mod background;
mod dsp;
mod error;
mod harmony;
mod model;
#[cfg(feature = "playback")]
mod playback;
mod renderer;
mod reverb;
mod sfx;
mod sfx_eq;

pub use error::{Error, Result};
pub use harmony::{Chord, ChordShape, GLOBAL_SCALE, ScaleDegree};
pub use model::*;
#[cfg(feature = "playback")]
pub use playback::*;
pub use renderer::{Renderer, Snapshot};
pub use sfx::{
    SfxBurst, SfxDetail, SfxEnvelope, SfxFilterGesture, SfxGenerator, SfxLayer, SfxLayerRecipe,
    SfxLayerStereo, SfxModulation, SfxPatch, SfxPitch, SfxRecipe, SfxStep,
};
pub use sfx_eq::{SFX_EQ, SfxEq};
