use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::OnceLock,
    time::Duration,
};

use serde::Deserialize;
use strum::IntoEnumIterator;

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Deserialize,
    strum::EnumIter,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[serde(try_from = "String")]
pub(in super::super) enum UiEvent {
    #[strum(serialize = "market.open")]
    MarketOpen,
    #[strum(serialize = "market.switch")]
    MarketSwitch,
    #[strum(serialize = "instrument.select")]
    InstrumentSelect,
    #[strum(serialize = "instrument.search")]
    InstrumentSearch,
    #[strum(serialize = "search.clear")]
    SearchClear,
    #[strum(serialize = "chart.zoom_in")]
    ChartZoomIn,
    #[strum(serialize = "chart.zoom_out")]
    ChartZoomOut,
    #[strum(serialize = "chart.pan")]
    ChartPan,
    #[strum(serialize = "chart.timeframe_change")]
    ChartTimeframeChange,
    #[strum(serialize = "book.open")]
    BookOpen,
    #[strum(serialize = "book.scroll")]
    BookScroll,
    #[strum(serialize = "order.ticket_open")]
    OrderTicketOpen,
    #[strum(serialize = "order.side_select")]
    OrderSideSelect,
    #[strum(serialize = "order.quantity_edit")]
    OrderQuantityEdit,
    #[strum(serialize = "order.price_edit")]
    OrderPriceEdit,
    #[strum(serialize = "order.submit")]
    OrderSubmit,
    #[strum(serialize = "order.accepted")]
    OrderAccepted,
    #[strum(serialize = "order.filled")]
    OrderFilled,
    #[strum(serialize = "order.partial_fill")]
    OrderPartialFill,
    #[strum(serialize = "order.cancel_request")]
    OrderCancelRequest,
    #[strum(serialize = "order.canceled")]
    OrderCanceled,
    #[strum(serialize = "order.rejected")]
    OrderRejected,
    #[strum(serialize = "position.open")]
    PositionOpen,
    #[strum(serialize = "position.close")]
    PositionClose,
    #[strum(serialize = "popup.open")]
    PopupOpen,
    #[strum(serialize = "popup.read")]
    PopupRead,
    #[strum(serialize = "popup.dismiss")]
    PopupDismiss,
    #[strum(serialize = "warning.open")]
    WarningOpen,
    #[strum(serialize = "balance.view")]
    BalanceView,
    #[strum(serialize = "portfolio.view")]
    PortfolioView,
    #[strum(serialize = "history.open")]
    HistoryOpen,
    #[strum(serialize = "watchlist.edit")]
    WatchlistEdit,
    #[strum(serialize = "ui.scroll")]
    UiScroll,
    #[strum(serialize = "ui.idle")]
    UiIdle,
    #[strum(serialize = "ui.hover")]
    UiHover,
    #[strum(serialize = "connection.lag")]
    ConnectionLag,
    #[strum(serialize = "session.end")]
    SessionEnd,
}

impl TryFrom<String> for UiEvent {
    type Error = strum::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl UiEvent {
    pub(in super::super) fn label(self) -> &'static str {
        self.into()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "u8")]
enum Profanity {
    Clean,
    Mild,
    Strong,
}

impl TryFrom<u8> for Profanity {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Clean),
            1 => Ok(Self::Mild),
            2 => Ok(Self::Strong),
            _ => Err("profanity must be 0, 1, or 2"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
struct Gap {
    min: u64,
    max: Option<u64>,
}

impl Gap {
    fn contains(self, duration: Duration) -> bool {
        duration >= Duration::from_millis(self.min)
            && self
                .max
                .is_none_or(|max| duration <= Duration::from_millis(max))
    }

    fn validate(self) {
        assert!(self.min > 0, "thought gaps must have a positive minimum");
        assert!(
            self.max.is_none_or(|max| max >= self.min),
            "thought gap bounds must be ordered"
        );
    }
}

#[derive(Debug, PartialEq, Deserialize)]
struct Defaults {
    gap_ms: Gap,
    delay_ms: u64,
    weight: u32,
    cooldown_events: u64,
    dedupe_window: usize,
    profanity_ceiling: Profanity,
}

#[derive(Debug, PartialEq, Deserialize)]
pub(in super::super) struct Thought {
    pub(in super::super) id: String,
    pub(in super::super) text: String,
    after_any: Vec<UiEvent>,
    before_any: Vec<UiEvent>,
    profanity: Profanity,
    gap_ms: Option<Gap>,
    delay_ms: Option<u64>,
    weight: Option<u32>,
    cooldown_events: Option<u64>,
    context: Option<serde_json::Value>,
    #[serde(default)]
    fallback: bool,
}

impl Thought {
    pub(in super::super) fn cooldown_events(&self) -> u64 {
        self.cooldown_events
            .unwrap_or(library().defaults.cooldown_events)
    }

    pub(in super::super) fn fits(&self, after: UiEvent, before: UiEvent, gap: Duration) -> bool {
        let defaults = &library().defaults;
        self.context.is_none()
            && self.profanity <= defaults.profanity_ceiling
            && (self.after_any.is_empty() || self.after_any.contains(&after))
            && (self.before_any.is_empty() || self.before_any.contains(&before))
            && self.gap_ms.unwrap_or(defaults.gap_ms).contains(gap)
    }
}

#[derive(Debug, Deserialize)]
pub(in super::super) struct Library {
    event_vocabulary: Vec<UiEvent>,
    defaults: Defaults,
    pub(in super::super) thoughts: Vec<Thought>,
}

impl Library {
    pub(in super::super) fn dedupe_window(&self) -> usize {
        self.defaults.dedupe_window
    }
}

pub(in super::super) fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let library: Library =
            serde_json::from_str(include_str!("../../../../assets/trader_thoughts.json"))
                .expect("embedded trader thoughts must match the catalog schema");
        let vocabulary: BTreeSet<_> = library.event_vocabulary.iter().copied().collect();
        assert_eq!(vocabulary.len(), library.event_vocabulary.len());
        assert_eq!(vocabulary, UiEvent::iter().collect());
        library.defaults.gap_ms.validate();
        assert!(
            library.defaults.weight > 0,
            "thought weights must be positive"
        );
        let mut ids = BTreeSet::new();
        assert!(
            !library.thoughts.is_empty(),
            "thought catalog must not be empty"
        );
        for thought in &library.thoughts {
            assert!(!thought.id.is_empty() && ids.insert(&thought.id));
            assert!(!thought.text.trim().is_empty());
            assert!(!thought.text.contains(['\n', '\r', '*']));
            thought.gap_ms.unwrap_or(library.defaults.gap_ms).validate();
            assert!(thought.weight.unwrap_or(library.defaults.weight) > 0);
        }
        library
    })
}

#[derive(Default)]
pub(in super::super) struct ThoughtDeck {
    events: u64,
    last_used: BTreeMap<&'static str, u64>,
    recent: VecDeque<&'static str>,
}

impl ThoughtDeck {
    pub(in super::super) fn event(&mut self) {
        self.events += 1;
    }

    pub(in super::super) fn select(
        &mut self,
        after: UiEvent,
        before: UiEvent,
        gap: Duration,
        rng: &mut fastrand::Rng,
    ) -> Vec<(&'static Thought, Duration)> {
        const SPACING: Duration = Duration::from_millis(100);
        let Some(latest) = gap.checked_sub(SPACING).filter(|&latest| latest >= SPACING) else {
            return Vec::new();
        };
        let library = library();
        let mut candidates: Vec<_> = library
            .thoughts
            .iter()
            .filter(|thought| {
                thought.fits(after, before, gap)
                    && !self.recent.contains(&thought.id.as_str())
                    && self
                        .last_used
                        .get(thought.id.as_str())
                        .is_none_or(|&last| self.events - last >= thought.cooldown_events())
            })
            .collect();
        if candidates.iter().any(|thought| !thought.fallback) {
            candidates.retain(|thought| !thought.fallback);
        }
        // The simulator allows occasional short runs of thoughts within one pause.
        let count = if rng.usize(..4) == 0 {
            rng.usize(2..=3)
        } else {
            1
        };
        let mut selected = Vec::new();
        let mut earliest = SPACING;
        while selected.len() < count && !candidates.is_empty() && earliest <= latest {
            let unseen = candidates
                .iter()
                .any(|thought| !self.last_used.contains_key(thought.id.as_str()));
            let total: u64 = candidates
                .iter()
                .filter(|thought| !unseen || !self.last_used.contains_key(thought.id.as_str()))
                .map(|thought| u64::from(thought.weight.unwrap_or(library.defaults.weight)))
                .sum();
            let mut draw = rng.u64(..total);
            let index = candidates
                .iter()
                .position(|thought| {
                    if unseen && self.last_used.contains_key(thought.id.as_str()) {
                        return false;
                    }
                    let weight = u64::from(thought.weight.unwrap_or(library.defaults.weight));
                    if draw < weight {
                        true
                    } else {
                        draw -= weight;
                        false
                    }
                })
                .expect("positive candidate weights contain the draw");
            let thought = candidates.remove(index);
            let delay =
                Duration::from_millis(thought.delay_ms.unwrap_or(library.defaults.delay_ms))
                    .clamp(earliest, latest);
            selected.push((thought, delay));
            earliest = delay + SPACING;
            self.last_used.insert(&thought.id, self.events);
            self.recent.push_back(&thought.id);
            if self.recent.len() > library.dedupe_window() {
                self.recent.pop_front();
            }
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_retains_every_thought_and_semantic_event() {
        let library = library();
        assert_eq!(library.thoughts.len(), 64);
        for event in UiEvent::iter() {
            let encoded = format!("\"{}\"", event.label());
            assert_eq!(serde_json::from_str::<UiEvent>(&encoded).unwrap(), event);
        }
        for thought in &library.thoughts {
            let after = thought
                .after_any
                .first()
                .copied()
                .unwrap_or(UiEvent::UiIdle);
            let before = thought
                .before_any
                .first()
                .copied()
                .unwrap_or(UiEvent::UiIdle);
            assert!(
                thought.fits(after, before, Duration::from_secs(6)),
                "{}",
                thought.id
            );
        }
    }

    #[test]
    fn selection_uses_related_gaps_and_occasional_consecutive_thoughts() {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut consecutive = false;
        for _ in 0..100 {
            let mut deck = ThoughtDeck::default();
            deck.event();
            let gap = Duration::from_secs(6);
            let selected = deck.select(
                UiEvent::InstrumentSelect,
                UiEvent::ChartZoomIn,
                gap,
                &mut rng,
            );
            consecutive |= selected.len() > 1;
            for (thought, delay) in &selected {
                assert!(thought.fits(UiEvent::InstrumentSelect, UiEvent::ChartZoomIn, gap));
                assert!(!thought.fallback);
                assert!(*delay > Duration::ZERO && *delay < gap);
            }
            assert!(selected.windows(2).all(|pair| pair[0].1 < pair[1].1));
        }
        assert!(consecutive);
    }

    #[test]
    fn cooldown_dedupe_and_fallbacks_apply_to_real_events() {
        let mut deck = ThoughtDeck::default();
        let mut rng = fastrand::Rng::with_seed(42);
        let gap = Duration::from_secs(6);
        let mut seen = BTreeSet::new();
        let mut had_fallback = false;
        for _ in 0..10 {
            let selected = deck.select(UiEvent::OrderRejected, UiEvent::PopupOpen, gap, &mut rng);
            for (thought, _) in selected {
                assert!(
                    seen.insert(thought.id.as_str()),
                    "thoughts cannot repeat without real events"
                );
                had_fallback |= thought.fallback;
            }
        }
        assert!(seen.contains("TT041") && seen.contains("TT042"));
        assert!(had_fallback);
        assert!(
            deck.select(
                UiEvent::OrderRejected,
                UiEvent::PopupOpen,
                Duration::from_millis(100),
                &mut rng
            )
            .is_empty()
        );
    }

    #[test]
    fn cooldown_expires_at_the_real_event_boundary() {
        let mut deck = ThoughtDeck::default();
        deck.last_used.insert("TT041", 0);
        let mut rng = fastrand::Rng::with_seed(9);
        let gap = Duration::from_secs(6);
        for _ in 0..19 {
            deck.event();
        }
        let selected = deck.select(UiEvent::OrderRejected, UiEvent::PopupOpen, gap, &mut rng);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0.id, "TT042");
        deck.event();
        let selected = deck.select(UiEvent::OrderRejected, UiEvent::PopupOpen, gap, &mut rng);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0.id, "TT041");
    }
}
