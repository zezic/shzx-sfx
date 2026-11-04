use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

use shzx_sfx::{
    Command, MarketContext, Mix, Mode, OutputGain, Pan, RenderConfig, Renderer, ReverbSettings,
    SfxEvent, SfxKind, SpatialMode, Symbol, Trend, Unit,
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
    for sample_rate in [8_000, 48_000] {
        let mut renderer = Renderer::new(RenderConfig {
            sample_rate,
            max_voices: 4,
            ..RenderConfig::default()
        })
        .unwrap();
        let mut commands: Vec<_> = SfxKind::iter()
            .map(|kind| {
                Command::Trigger(SfxEvent {
                    layers: shzx_sfx::SfxLayerSelection::All,
                    kind,
                    pan: Pan::new(-0.8).unwrap(),
                    intensity: Unit::ONE,
                })
            })
            .collect();
        commands.extend([
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
                ..Mix::default()
            }),
            Command::Reverb(ReverbSettings::default()),
            Command::Symbol(Symbol::new("IBM").unwrap()),
            Command::Symbol(Symbol::new("MSFT").unwrap()),
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
        ]);
        let mut block = [[0.0_f32; 2]; 64];
        let scope = AllocationScope::begin();
        // Include the very first rendered frame: initialization must not leak into
        // the native callback. A small pool exercises voice replacement repeatedly.
        for _ in 0..3 {
            for &command in &commands {
                renderer.apply(command);
                renderer.render(&mut block);
                black_box(renderer.snapshot());
            }
        }
        // Let active voices retire, covering destruction as well as admission.
        for _ in 0..(sample_rate as usize * 2).div_ceil(block.len()) {
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
        let snapshot = renderer.snapshot();
        assert!(snapshot.stolen_voices > 0);
        assert!(snapshot.coalesced_events > 0);
        assert_eq!(snapshot.active_voices, 0);
    }
}
