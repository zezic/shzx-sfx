use std::{collections::VecDeque, time::Duration};

use shzx_sfx::Command;

use super::activity::thoughts::{Thought, UiEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventSource {
    Simulator,
    User,
}

impl EventSource {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Simulator => "SIM",
            Self::User => "USER",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "The bounded history keeps commands inline without allocating for each entry"
)]
pub(super) enum LoggedEventKind {
    Command(Command),
    Activity(UiEvent),
    Thought(&'static Thought),
}

pub(super) struct LoggedEvent {
    pub(super) at: Duration,
    pub(super) source: EventSource,
    pub(super) kind: LoggedEventKind,
}

#[derive(Default)]
pub(super) struct EventLog {
    elapsed: Duration,
    entries: VecDeque<LoggedEvent>,
}

impl EventLog {
    const CAPACITY: usize = 128;

    pub(super) fn advance(&mut self, elapsed: Duration) {
        self.elapsed = elapsed;
    }

    pub(super) fn record(&mut self, source: EventSource, command: Command) {
        self.push(source, LoggedEventKind::Command(command));
    }

    pub(super) fn record_activity(&mut self, event: UiEvent) {
        self.push(EventSource::Simulator, LoggedEventKind::Activity(event));
    }

    pub(super) fn record_thought(&mut self, thought: &'static Thought) {
        self.push(EventSource::Simulator, LoggedEventKind::Thought(thought));
    }

    fn push(&mut self, source: EventSource, kind: LoggedEventKind) {
        if self.entries.len() == Self::CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(LoggedEvent {
            at: self.elapsed,
            source,
            kind,
        });
    }

    pub(super) fn entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = &LoggedEvent> + ExactSizeIterator {
        self.entries.iter()
    }
}
