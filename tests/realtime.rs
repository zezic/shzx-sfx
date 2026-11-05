use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

use shzx_sfx::{
    Command, DataTransmission, DataTransmissionCommand, HarmonyMode, MarketContext, Mix, Mode,
    OutputGain, Pan, RenderConfig, Renderer, ReverbSettings, SfxEdits, SfxEvent, SfxFilterBand,
    SfxFmIndex, SfxFmRatio, SfxKind, SfxLayer, SfxLayerEdits, SfxLayerState, SfxMilliseconds,
    SfxScale, SfxStepRate, SfxTonalEdits, SfxTonalRate, SfxTonalSequence, SpatialMode, Symbol,
    Trend, Unit,
};
use strum::IntoEnumIterator;

#[derive(Clone, Copy, Debug, Default)]
struct Calls {
    allocations: usize,
    deallocations: usize,
}

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<Calls> = const { Cell::new(Calls { allocations: 0, deallocations: 0 }) };
}

struct CountingAllocator;

fn record(allocation: bool, deallocation: bool) {
    if TRACKING.try_with(Cell::get) == Ok(true) {
        let _ = CALLS.try_with(|calls| {
            let current = calls.get();
            calls.set(Calls {
                allocations: current.allocations + usize::from(allocation),
                deallocations: current.deallocations + usize::from(deallocation),
            });
        });
    }
}

// The wrapped system allocator retains ownership and layout responsibilities;
// the extra thread-local counters observe calls without allocating themselves.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(true, false);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(true, false);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(false, true);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(true, true);
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct AllocationScope;

impl AllocationScope {
    fn begin() -> Self {
        CALLS.with(|calls| calls.set(Calls::default()));
        TRACKING.with(|tracking| tracking.set(true));
        Self
    }

    fn finish(self) -> Calls {
        TRACKING.with(|tracking| tracking.set(false));
        CALLS.with(Cell::get)
    }
}

impl Drop for AllocationScope {
    fn drop(&mut self) {
        TRACKING.with(|tracking| tracking.set(false));
    }
}

#[test]
fn rendering_and_every_command_remain_allocation_free_after_construction() {
    for sample_rate in [8_000, 48_000, 192_000] {
        let mut renderer = Renderer::new(RenderConfig {
            sample_rate,
            max_voices: 4,
            ..RenderConfig::default()
        })
        .unwrap();
        let mut edits = SfxEdits::default();
        edits.pitch = SfxScale::new(4.0).unwrap();
        edits.duration = SfxScale::new(4.0).unwrap();
        edits.tonal = SfxTonalEdits {
            carrier: SfxScale::new(4.0).unwrap(),
            fm_index: SfxFmIndex::new(SfxFmIndex::MAX).unwrap(),
            fm_ratio: SfxFmRatio::new(SfxFmRatio::MAX).unwrap(),
            step_rate: Some(SfxTonalRate::Fixed(
                SfxStepRate::new(SfxStepRate::MAX_HZ).unwrap(),
            )),
            attack: SfxMilliseconds::new(SfxMilliseconds::MIN).unwrap(),
            release: SfxMilliseconds::new(SfxMilliseconds::MIN).unwrap(),
            pitch_ratios: Some(SfxTonalSequence::new([SfxScale::new(4.0).unwrap(); 6])),
        };
        for layer in SfxLayer::ALL {
            edits.set_layer(
                layer,
                SfxLayerEdits {
                    state: if layer == SfxLayer::Texture {
                        SfxLayerState::Muted
                    } else {
                        SfxLayerState::Audible
                    },
                    level: Some(Unit::ONE),
                    filter: Some(SfxFilterBand::new(1_500.0, 24_000.0).unwrap()),
                },
            );
        }
        let mut accelerating = edits;
        accelerating.tonal.step_rate = Some(SfxTonalRate::Sweep {
            start: SfxStepRate::new(SfxStepRate::MIN_HZ).unwrap(),
            end: SfxStepRate::new(SfxStepRate::MAX_HZ).unwrap(),
        });
        let mut decelerating = edits;
        decelerating.tonal.step_rate = Some(SfxTonalRate::Sweep {
            start: SfxStepRate::new(SfxStepRate::MAX_HZ).unwrap(),
            end: SfxStepRate::new(SfxStepRate::MIN_HZ).unwrap(),
        });
        let mut commands: Vec<_> = SfxKind::iter()
            .map(|kind| {
                Command::Trigger(SfxEvent {
                    layers: shzx_sfx::SfxLayerSelection::All,
                    edits: shzx_sfx::SfxEdits::DEFAULT,
                    kind,
                    pan: Pan::new(-0.8).unwrap(),
                    intensity: Unit::ONE,
                })
            })
            .collect();
        commands.extend([
            Command::Trigger(SfxEvent {
                edits: accelerating,
                ..SfxEvent::new(SfxKind::Confirm)
            }),
            Command::Trigger(SfxEvent {
                edits: decelerating,
                ..SfxEvent::new(SfxKind::Cancel)
            }),
            Command::Trigger(SfxEvent {
                edits,
                ..SfxEvent::new(SfxKind::OrderFill)
            }),
            Command::Market(MarketContext {
                trend: Trend::Bearish,
                trend_strength: Unit::ONE,
                volume: Unit::ONE,
                volatility: Unit::ONE,
                zoom: Unit::ONE,
                activity: Unit::ONE,
            }),
            Command::Mode(Mode::Dialog),
            Command::Mode(Mode::Focus),
            Command::Mode(Mode::Browse),
            Command::Mix(Mix {
                master: Unit::ONE,
                output_gain: OutputGain::new(24.0).unwrap(),
                demonic_voices: Unit::ONE,
                demonic_stutters: Unit::ONE,
                flying_coins: Unit::ONE,
                laser_beams: Unit::ONE,
                rain: Unit::ONE,
                ..Mix::default()
            }),
            Command::Reverb(ReverbSettings::default()),
            Command::Symbol(Symbol::new("IBM").unwrap()),
            Command::Harmony(HarmonyMode::Dissonant),
            Command::Symbol(Symbol::new("AAPL").unwrap()),
            Command::Symbol(Symbol::new("MSFT").unwrap()),
            Command::Harmony(HarmonyMode::Diatonic),
            Command::Trigger(SfxEvent {
                layers: shzx_sfx::SfxLayerSelection::Only(shzx_sfx::SfxLayer::Tonal),
                ..SfxEvent::new(SfxKind::Confirm)
            }),
            Command::Trigger(SfxEvent {
                layers: shzx_sfx::SfxLayerSelection::Only(shzx_sfx::SfxLayer::Shimmer),
                ..SfxEvent::new(SfxKind::OrderFill)
            }),
            Command::Trigger(SfxEvent {
                layers: shzx_sfx::SfxLayerSelection::Only(shzx_sfx::SfxLayer::Crisp),
                ..SfxEvent::new(SfxKind::Confirm)
            }),
            Command::Spatial(SpatialMode::Pan),
            Command::Spatial(SpatialMode::Haas),
            Command::DataTransmission(DataTransmissionCommand::Start(DataTransmission::default())),
            Command::DataTransmission(DataTransmissionCommand::Pan(Pan::new(-0.8).unwrap())),
            Command::DataTransmission(DataTransmissionCommand::End),
        ]);
        let mut block = [[0.0_f32; 2]; 64];
        let mut dissonant_seen = false;
        let scope = AllocationScope::begin();
        // Include the very first rendered frame: initialization must not leak into
        // the native callback. A small pool exercises voice replacement repeatedly.
        for _ in 0..3 {
            for &command in &commands {
                renderer.apply(command);
                renderer.render(&mut block);
                let snapshot = black_box(renderer.snapshot());
                dissonant_seen |= snapshot.background.harmony.mode == HarmonyMode::Dissonant;
            }
        }
        // Exercise the sequencer's internal trigger path and pulse-budget expiry.
        let transmission = DataTransmission {
            max_triggers: shzx_sfx::TransmissionCount::new(2).unwrap(),
            rate: shzx_sfx::TransmissionRate::new(80.0).unwrap(),
            ..DataTransmission::default()
        };
        renderer.apply(Command::DataTransmission(DataTransmissionCommand::Start(
            transmission,
        )));
        for _ in 0..(sample_rate as usize).div_ceil(block.len()) {
            renderer.render(&mut block);
        }
        renderer.apply(Command::DataTransmission(DataTransmissionCommand::End));
        // Include a full distant melody note and retirement of event voices.
        for _ in 0..(sample_rate as usize * 20).div_ceil(block.len()) {
            renderer.render(&mut block);
        }
        black_box(&block);
        let calls = scope.finish();
        assert_eq!(
            calls.allocations, 0,
            "allocator calls during rendering: {calls:?}"
        );
        assert_eq!(
            calls.deallocations, 0,
            "deallocator calls during rendering: {calls:?}"
        );
        assert!(dissonant_seen);
        let snapshot = renderer.snapshot();
        assert!(snapshot.stolen_voices > 0);
        assert!(snapshot.coalesced_events > 0);
        assert_eq!(snapshot.active_voices, 0);
        assert!(snapshot.background.demonic_voices.generated > 0);
        assert!(snapshot.background.demonic_stutters.generated > 0);
        if sample_rate >= 48_000 {
            assert!(snapshot.background.flying_coins.generated > 0);
            assert!(snapshot.background.laser_beams.generated > 0);
        }
    }
}
