use std::{collections::VecDeque, time::Duration};

use shzx_sfx::{Pan, SfxEvent, SfxKind, Unit};

/// A virtual trading terminal, independent of the sound lab's controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Watchlist,
    Chart,
    ChartTools,
    OrderTicket,
    Positions,
    Tabs,
    Dialog,
    SearchField,
    Status,
    Alerts,
}

impl Region {
    const BROWSE: [Self; 5] = [
        Self::Watchlist,
        Self::ChartTools,
        Self::Chart,
        Self::Positions,
        Self::OrderTicket,
    ];

    fn position(self, rng: &mut fastrand::Rng) -> Position {
        let (x, y) = match self {
            Self::Watchlist => (0.13, 0.43),
            Self::Chart => (0.50, 0.43),
            Self::ChartTools => (0.50, 0.20),
            Self::OrderTicket => (0.87, 0.43),
            Self::Positions => (0.50, 0.82),
            Self::Tabs => (0.42, 0.08),
            Self::Dialog => (0.50, 0.50),
            Self::SearchField => (0.13, 0.09),
            Self::Status => (0.10, 0.95),
            Self::Alerts => (0.88, 0.09),
        };
        Position {
            x: Unit::new(x + (rng.f32() - 0.5) * 0.10)
                .expect("virtual targets stay inside the screen"),
            y: Unit::new(y + (rng.f32() - 0.5) * 0.06)
                .expect("virtual targets stay inside the screen"),
        }
    }
}

#[derive(Clone, Copy)]
struct Position {
    x: Unit,
    y: Unit,
}

impl Position {
    fn pan(self) -> Pan {
        Pan::new(self.x.get() * 2.0 - 1.0).expect("screen coordinates give bounded pan")
    }

    fn distance(self, other: Self) -> f32 {
        (self.x.get() - other.x.get()).hypot(self.y.get() - other.y.get())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Routine {
    Browse,
    Scroll,
    Scrollbar,
    Rearrange,
    RejectedDrop,
    Chart,
    Tabs,
    Dialog,
    Search,
    Settings,
    Refresh,
    Connection,
}

impl Routine {
    const ALL: [Self; 12] = [
        Self::Browse,
        Self::Scroll,
        Self::Scrollbar,
        Self::Rearrange,
        Self::RejectedDrop,
        Self::Chart,
        Self::Tabs,
        Self::Dialog,
        Self::Search,
        Self::Settings,
        Self::Refresh,
        Self::Connection,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrderOutcome {
    Fill,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Workflow {
    Routine(Routine),
    Order(OrderOutcome),
}

impl Workflow {
    fn description(self) -> &'static str {
        match self {
            Self::Routine(routine) => match routine {
                Routine::Browse => "Browsing watchlist",
                Routine::Scroll => "Scrolling positions",
                Routine::Scrollbar => "Dragging scrollbar",
                Routine::Rearrange => "Rearranging panels",
                Routine::RejectedDrop => "Rejected panel drop",
                Routine::Chart => "Zooming and resizing chart",
                Routine::Tabs => "Opening chart tabs",
                Routine::Dialog => "Editing dialog",
                Routine::Search => "Searching symbols",
                Routine::Settings => "Changing view settings",
                Routine::Refresh => "Refreshing market data",
                Routine::Connection => "Reconnecting terminal",
            },
            Self::Order(OrderOutcome::Fill) => "Order placed / awaiting fill",
            Self::Order(OrderOutcome::Cancel) => "Placing and canceling order",
        }
    }

    fn interactions(self) -> impl Iterator<Item = (SfxKind, Region)> {
        use Region::*;
        use Routine as R;
        use SfxKind::*;
        let interactions: &[(SfxKind, Region)] = match self {
            Self::Routine(R::Browse) => {
                &[(Click, Watchlist), (SecondaryClick, Chart), (Cancel, Chart)]
            }
            Self::Routine(R::Scroll) => &[(Scroll, Positions), (ScrollBoundary, Positions)],
            Self::Routine(R::Scrollbar) => &[
                (ScrollbarGrab, Watchlist),
                (ScrollbarMove, Watchlist),
                (ScrollbarMove, Watchlist),
                (ScrollbarRelease, Watchlist),
            ],
            Self::Routine(R::Rearrange) => &[
                (DragStart, Watchlist),
                (DragOver, Chart),
                (DragOver, OrderTicket),
                (Drop, Positions),
            ],
            Self::Routine(R::RejectedDrop) => &[
                (DragStart, Positions),
                (DragOver, Chart),
                (DropRejected, OrderTicket),
            ],
            Self::Routine(R::Chart) => &[
                (ZoomIn, Chart),
                (ZoomIn, Chart),
                (ZoomOut, Chart),
                (Resize, ChartTools),
                (Resize, ChartTools),
            ],
            Self::Routine(R::Tabs) => &[
                (Click, Tabs),
                (TabOpen, Tabs),
                (Focus, Chart),
                (Blur, Chart),
                (TabClose, Tabs),
            ],
            Self::Routine(R::Dialog) => &[
                (Click, ChartTools),
                (DialogOpen, Dialog),
                (Focus, Dialog),
                (Confirm, Dialog),
                (Blur, Dialog),
                (DialogClose, Dialog),
            ],
            Self::Routine(R::Search) => &[
                (Focus, SearchField),
                (Search, SearchField),
                (Click, Watchlist),
                (Confirm, Watchlist),
                (Blur, SearchField),
            ],
            Self::Routine(R::Settings) => &[
                (ToggleOn, ChartTools),
                (ToggleOff, ChartTools),
                (DestructiveClick, Tabs),
                (Cancel, Tabs),
            ],
            Self::Routine(R::Refresh) => &[
                (Click, ChartTools),
                (DataRefresh, Chart),
                (Notification, Alerts),
            ],
            Self::Routine(R::Connection) => &[
                (Disconnect, Status),
                (Warning, Alerts),
                (Error, Alerts),
                (Connect, Status),
                (DataRefresh, Chart),
            ],
            Self::Order(OrderOutcome::Fill) => &[
                (Click, OrderTicket),
                (Focus, OrderTicket),
                (Confirm, OrderTicket),
                (OrderCreate, OrderTicket),
                (OrderFill, Positions),
            ],
            Self::Order(OrderOutcome::Cancel) => &[
                (Click, OrderTicket),
                (Confirm, OrderTicket),
                (OrderCreate, OrderTicket),
                (SecondaryClick, Positions),
                (DestructiveClick, Positions),
                (OrderCancel, Positions),
            ],
        };
        interactions.iter().copied()
    }
}

struct Gesture {
    event: SfxEvent,
    after: Duration,
}

struct Simulation {
    workflow: Workflow,
    events: VecDeque<Gesture>,
    next_at: Duration,
    next_order_at: Duration,
    order_outcome: OrderOutcome,
    routines: [Routine; Routine::ALL.len()],
    routine_index: usize,
    cursor: Position,
    hovered: Option<Region>,
}

impl Simulation {
    fn new(elapsed: Duration, rng: &mut fastrand::Rng) -> Self {
        let mut routines = Routine::ALL;
        rng.shuffle(&mut routines);
        Self {
            workflow: Workflow::Routine(routines[0]),
            events: VecDeque::new(),
            next_at: elapsed + Duration::from_millis(rng.u64(500..=1_500)),
            next_order_at: elapsed + Duration::from_secs(rng.u64(45..=75)),
            order_outcome: if rng.bool() {
                OrderOutcome::Fill
            } else {
                OrderOutcome::Cancel
            },
            routines,
            routine_index: 0,
            cursor: Region::Chart.position(rng),
            hovered: None,
        }
    }

    fn push(
        &mut self,
        kind: SfxKind,
        position: Position,
        after: Duration,
        rng: &mut fastrand::Rng,
    ) {
        let (low, high) = match kind {
            SfxKind::HoverEnter | SfxKind::HoverLeave => (0.20, 0.38),
            SfxKind::Scroll | SfxKind::ScrollbarMove | SfxKind::DragOver | SfxKind::Resize => {
                (0.30, 0.48)
            }
            _ => (0.42, 0.65),
        };
        self.events.push_back(Gesture {
            event: SfxEvent {
                pan: position.pan(),
                intensity: Unit::new(low + rng.f32() * (high - low))
                    .expect("humanized intensity is normalized"),
                ..SfxEvent::new(kind)
            },
            after,
        });
    }

    fn visit(&mut self, region: Region, rng: &mut fastrand::Rng) {
        let destination = region.position(rng);
        if self.hovered != Some(region) {
            if self.hovered.is_some() {
                let travel = Duration::from_millis(
                    (self.cursor.distance(destination) * rng.u64(400..=900) as f32) as u64
                        + rng.u64(100..=220),
                );
                self.push(SfxKind::HoverLeave, self.cursor, travel, rng);
            }
            self.cursor = destination;
            self.hovered = Some(region);
            self.push(
                SfxKind::HoverEnter,
                self.cursor,
                Duration::from_millis(rng.u64(100..=350)),
                rng,
            );
        } else {
            self.cursor = destination;
        }
    }

    fn scroll_burst(&mut self, ticks: usize, rng: &mut fastrand::Rng) {
        for _ in 0..ticks {
            self.push(
                SfxKind::Scroll,
                self.cursor,
                Duration::from_millis(rng.u64(100..=290)),
                rng,
            );
        }
        self.events
            .back_mut()
            .expect("scroll bursts contain wheel ticks")
            .after = Duration::from_millis(rng.u64(350..=950));
    }

    fn plan(&mut self, elapsed: Duration, rng: &mut fastrand::Rng) {
        assert!(self.events.is_empty());
        self.workflow = if elapsed >= self.next_order_at {
            Workflow::Order(self.order_outcome)
        } else {
            if self.routine_index == self.routines.len() {
                rng.shuffle(&mut self.routines);
                self.routine_index = 0;
            }
            let routine = self.routines[self.routine_index];
            self.routine_index += 1;
            Workflow::Routine(routine)
        };

        // Cross nearby controls before settling on an action, like a moving mouse.
        for _ in 0..rng.usize(4..=7) {
            let previous = self.hovered;
            let candidates = || {
                Region::BROWSE
                    .into_iter()
                    .filter(|&region| Some(region) != previous)
            };
            let region = candidates()
                .nth(rng.usize(..candidates().count()))
                .expect("other browsing targets are available");
            self.visit(region, rng);
            if matches!(
                region,
                Region::Watchlist | Region::Chart | Region::Positions
            ) && rng.bool()
            {
                self.scroll_burst(rng.usize(2..=4), rng);
            }
        }
        for (kind, region) in self.workflow.interactions() {
            let position = match kind {
                SfxKind::Notification
                | SfxKind::Warning
                | SfxKind::Error
                | SfxKind::DataRefresh
                | SfxKind::Connect
                | SfxKind::Disconnect
                | SfxKind::OrderFill => region.position(rng),
                _ => {
                    self.visit(region, rng);
                    self.cursor
                }
            };
            if kind == SfxKind::Scroll {
                self.scroll_burst(rng.usize(6..=10), rng);
                continue;
            }
            let after = Duration::from_millis(match kind {
                SfxKind::ScrollbarMove | SfxKind::DragOver | SfxKind::Resize => rng.u64(100..=290),
                SfxKind::Focus | SfxKind::Search => rng.u64(550..=1_500),
                SfxKind::OrderCreate => rng.u64(1_800..=4_000),
                SfxKind::Disconnect => rng.u64(1_200..=2_800),
                _ => rng.u64(220..=800),
            });
            self.push(kind, position, after, rng);
        }
        self.events
            .back_mut()
            .expect("workflows contain actions")
            .after = Duration::from_millis(rng.u64(1_200..=4_500));
    }
}

pub(super) struct UserActivity {
    rng: fastrand::Rng,
    elapsed: Duration,
    running: Option<Simulation>,
}

impl UserActivity {
    pub(super) fn new(seed: u64) -> Self {
        let mut rng = fastrand::Rng::with_seed(seed ^ 0x5553_4552_4445_4d4f);
        let running = Some(Simulation::new(Duration::ZERO, &mut rng));
        Self {
            rng,
            elapsed: Duration::ZERO,
            running,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn description(&self) -> &'static str {
        self.running
            .as_ref()
            .map_or("Paused", |running| running.workflow.description())
    }

    pub(super) fn toggle(&mut self) {
        if self.enabled() {
            self.running = None;
        } else {
            self.reset();
        }
    }

    pub(super) fn reset(&mut self) {
        self.running = Some(Simulation::new(self.elapsed, &mut self.rng));
    }

    pub(super) fn advance(
        &mut self,
        elapsed: Duration,
        send: &mut impl FnMut(SfxEvent) -> shzx_sfx::Result<()>,
    ) -> shzx_sfx::Result<()> {
        self.elapsed = elapsed;
        let Some(running) = &mut self.running else {
            return Ok(());
        };
        if elapsed < running.next_at {
            return Ok(());
        }
        if running.events.is_empty() {
            running.plan(elapsed, &mut self.rng);
        }
        let gesture = running.events.front().expect("planning queues a workflow");
        send(gesture.event)?;
        // Anchor to the host's current time: missed frames never become a burst.
        running.next_at = elapsed + gesture.after;
        running.events.pop_front();
        if running.events.is_empty() && matches!(running.workflow, Workflow::Order(_)) {
            running.next_order_at = elapsed + Duration::from_secs(self.rng.u64(45..=90));
            running.order_outcome = match running.order_outcome {
                OrderOutcome::Fill => OrderOutcome::Cancel,
                OrderOutcome::Cancel => OrderOutcome::Fill,
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
