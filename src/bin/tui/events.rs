use std::{collections::VecDeque, time::Duration};

use shzx_sfx::Command;

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

pub(super) struct LoggedEvent {
    pub(super) at: Duration,
    pub(super) source: EventSource,
    pub(super) command: Command,
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
        if self.entries.len() == Self::CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(LoggedEvent {
            at: self.elapsed,
            source,
            command,
        });
    }

    pub(super) fn entries(
        &self,
    ) -> impl DoubleEndedIterator<Item = &LoggedEvent> + ExactSizeIterator {
        self.entries.iter()
    }
}
