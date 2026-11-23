//! The kernel's record of what it mediated. Every call between agents, every
//! model call, file and program becomes one event here, whether or not anyone
//! is watching, so a dashboard that connects late can still see the recent past.
//! The record is bounded: it keeps the latest events and drops the oldest.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How many events are kept.
const KEPT: usize = 4000;
/// Longest `detail` stored, in characters.
const DETAIL_CHARS: usize = 160;

#[derive(Clone, Debug)]
pub struct Event {
    pub seq: u64,
    pub at_ms: u64,
    pub agent: String,
    pub caller: String,
    pub kind: &'static str,
    pub detail: String,
    pub ms: u64,
    pub ok: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AgentStat {
    pub name: String,
    pub busy: u32,
    pub calls: u32,
    pub errors: u32,
    pub thoughts: u32,
    pub files: u32,
    pub programs: u32,
    pub total_ms: u64,
    pub busy_since_ms: u64,
    pub last_ms: u64,
    pub last: String,
}

/// Who asked whom: how many times, and how many of those are running now.
#[derive(Clone, Debug)]
pub struct Edge {
    pub caller: String,
    pub agent: String,
    pub calls: u32,
    pub busy: u32,
}

struct Inner {
    events: VecDeque<Event>,
    next: u64,
    agents: BTreeMap<String, AgentStat>,
    edges: BTreeMap<(String, String), (u32, u32)>,
}

/// Cheap to clone; every clone writes to and reads from the same record.
#[derive(Clone)]
pub struct Ledger {
    started: Instant,
    inner: Arc<Mutex<Inner>>,
}

fn short(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= DETAIL_CHARS {
        return line;
    }
    let cut: String = line.chars().take(DETAIL_CHARS - 1).collect();
    format!("{cut}…")
}

impl Ledger {
    pub fn new() -> Self {
        Self { started: Instant::now(), inner: Arc::new(Mutex::new(Inner { events: VecDeque::new(), next: 1, agents: BTreeMap::new(), edges: BTreeMap::new() })) }
    }

    pub fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Writes one event and updates the agent's counters. `kind` is `start`
    /// and `end` for a call to the agent, `think`, `file` or `program` for
    /// what the agent does while it works.
    pub fn record(&self, agent: &str, caller: &str, kind: &'static str, detail: &str, took: Duration, ok: bool) {
        let at_ms = self.now_ms();
        let ms = took.as_millis() as u64;
        let detail = short(detail);
        let mut inner = self.inner.lock().unwrap();
        let seq = inner.next;
        inner.next += 1;
        let stat = inner.agents.entry(agent.to_string()).or_insert_with(|| AgentStat { name: agent.to_string(), ..AgentStat::default() });
        stat.last_ms = at_ms;
        stat.last = detail.clone();
        match kind {
            "start" => {
                if stat.busy == 0 {
                    stat.busy_since_ms = at_ms;
                }
                stat.busy += 1;
                stat.calls += 1;
            }
            "end" => {
                stat.busy = stat.busy.saturating_sub(1);
                if stat.busy == 0 {
                    stat.busy_since_ms = 0;
                }
                stat.total_ms += ms;
                if !ok {
                    stat.errors += 1;
                }
            }
            "think" => stat.thoughts += 1,
            "file" => stat.files += 1,
            "program" => stat.programs += 1,
            _ => {}
        }
        // What an agent does can fail without its call failing: a program that
        // exits with an error still lets the agent answer.
        if !ok && matches!(kind, "think" | "file" | "program") {
            stat.errors += 1;
        }
        // A call is an edge from whoever asked to the agent, kept for good, so
        // the shape of who works for whom survives the events being trimmed.
        if matches!(kind, "start" | "end") {
            let edge = inner.edges.entry((caller.to_string(), agent.to_string())).or_insert((0, 0));
            if kind == "start" {
                edge.0 += 1;
                edge.1 += 1;
            } else {
                edge.1 = edge.1.saturating_sub(1);
            }
        }
        inner.events.push_back(Event { seq, at_ms, agent: agent.to_string(), caller: caller.to_string(), kind, detail, ms, ok });
        if inner.events.len() > KEPT {
            inner.events.pop_front();
        }
    }

    pub fn edges(&self) -> Vec<Edge> {
        let inner = self.inner.lock().unwrap();
        inner.edges.iter().map(|((caller, agent), (calls, busy))| Edge { caller: caller.clone(), agent: agent.clone(), calls: *calls, busy: *busy }).collect()
    }

    pub fn agents(&self) -> Vec<AgentStat> {
        self.inner.lock().unwrap().agents.values().cloned().collect()
    }

    /// Events with `seq` greater than `after`, oldest first.
    pub fn events_after(&self, after: u64, limit: usize) -> Vec<Event> {
        self.inner.lock().unwrap().events.iter().filter(|e| e.seq > after).take(limit).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_TIME: Duration = Duration::ZERO;

    #[test]
    fn a_call_is_busy_until_it_ends() {
        let ledger = Ledger::new();
        ledger.record("coder", "router", "start", "fix it", NO_TIME, true);
        let now = ledger.agents();
        assert_eq!((now[0].busy, now[0].calls), (1, 1));
        ledger.record("coder", "router", "end", "done", Duration::from_millis(40), false);
        let after = ledger.agents();
        assert_eq!((after[0].busy, after[0].errors, after[0].total_ms), (0, 1, 40));
        assert_eq!(after[0].busy_since_ms, 0);
    }

    #[test]
    fn a_failed_action_counts_even_when_the_call_succeeds() {
        let ledger = Ledger::new();
        ledger.record("python", "router", "start", "run x", NO_TIME, true);
        ledger.record("python", "python", "program", "python3 x", NO_TIME, false);
        ledger.record("python", "router", "end", "answered", NO_TIME, true);
        assert_eq!(ledger.agents()[0].errors, 1);
    }

    #[test]
    fn calls_make_edges_between_who_asked_and_who_answered() {
        let ledger = Ledger::new();
        ledger.record("coder", "router", "start", "x", NO_TIME, true);
        ledger.record("git", "coder", "start", "status", NO_TIME, true);
        ledger.record("git", "coder", "end", "answered", NO_TIME, true);
        let edges = ledger.edges();
        let find = |from: &str, to: &str| edges.iter().find(|e| e.caller == from && e.agent == to).map(|e| (e.calls, e.busy));
        assert_eq!(find("router", "coder"), Some((1, 1)));
        assert_eq!(find("coder", "git"), Some((1, 0)));
    }

    #[test]
    fn readers_ask_for_events_after_a_number() {
        let ledger = Ledger::new();
        for _ in 0..5 {
            ledger.record("a", "b", "file", "x", NO_TIME, true);
        }
        assert_eq!(ledger.events_after(0, 100).len(), 5);
        assert_eq!(ledger.events_after(3, 100).iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(ledger.events_after(0, 2).len(), 2);
    }

    #[test]
    fn the_record_is_bounded() {
        let ledger = Ledger::new();
        for _ in 0..KEPT + 10 {
            ledger.record("a", "a", "file", "x", NO_TIME, true);
        }
        let all = ledger.events_after(0, usize::MAX);
        assert_eq!(all.len(), KEPT);
        assert_eq!(all[0].seq, 11);
        assert_eq!(ledger.agents()[0].files as usize, KEPT + 10);
    }

    #[test]
    fn details_are_one_short_line() {
        let ledger = Ledger::new();
        ledger.record("a", "a", "file", &format!("line one\nline two {}", "x".repeat(500)), NO_TIME, true);
        let detail = &ledger.events_after(0, 1)[0].detail;
        assert!(!detail.contains('\n'));
        assert!(detail.chars().count() <= DETAIL_CHARS);
    }
}
