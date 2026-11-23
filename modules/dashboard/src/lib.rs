//! Dashboard agent. It watches the other agents through the `observe` import,
//! which is read-only: it can see what the kernel mediated, and nothing else.
//! It decides what counts as trouble (stuck, failing) and draws the page; the
//! kernel only keeps the events and serves what this component returns.

wit_bindgen::generate!({
    world: "dashboard-module",
    path: "../../wit/world.wit",
});

use agentwos::core::observe::{self, AgentStat, Event};
use exports::agentwos::core::dashboard::Guest;
use serde_json::{json, Value};

struct Component;

/// A call that has been running this long is probably stuck, not slow.
const STUCK_MS: u64 = 60_000;
/// Events sent to a page that has seen none.
const FIRST_EVENTS: usize = 300;
/// Most events one snapshot carries.
const MAX_EVENTS: u32 = 500;
/// A model call this slow is worth a line.
const SLOW_THINK_MS: u64 = 30_000;

struct Alert {
    level: &'static str,
    text: String,
}

fn alerts_of(a: &AgentStat, now: u64) -> Vec<Alert> {
    let mut out = Vec::new();
    if a.busy > 0 && now.saturating_sub(a.busy_since_ms) > STUCK_MS {
        let secs = now.saturating_sub(a.busy_since_ms) / 1000;
        out.push(Alert { level: "bad", text: format!("busy for {secs}s without answering") });
    }
    if a.calls >= 2 && a.errors * 2 >= a.calls {
        out.push(Alert { level: "bad", text: format!("{} failures in {} calls", a.errors, a.calls) });
    } else if a.errors > 0 {
        out.push(Alert { level: "warn", text: format!("{} failures in {} calls", a.errors, a.calls) });
    }
    out
}

fn state_of(a: &AgentStat, alerts: &[Alert]) -> &'static str {
    if alerts.iter().any(|x| x.level == "bad") {
        "alert"
    } else if a.busy > 0 {
        "busy"
    } else {
        "idle"
    }
}

fn event_json(e: &Event) -> Value {
    json!({
        "seq": e.seq, "at_ms": e.at_ms, "agent": e.agent, "caller": e.caller,
        "kind": e.kind, "detail": e.detail, "ms": e.ms, "ok": e.ok,
    })
}

fn agent_json(a: &AgentStat, now: u64) -> (Value, Vec<Alert>) {
    let alerts = alerts_of(a, now);
    let finished = a.calls.saturating_sub(a.busy);
    let avg = if finished > 0 { a.total_ms / finished as u64 } else { 0 };
    let value = json!({
        "name": a.name, "state": state_of(a, &alerts), "busy": a.busy, "calls": a.calls,
        "errors": a.errors, "thoughts": a.thoughts, "files": a.files, "programs": a.programs,
        "avg_ms": avg, "busy_since_ms": a.busy_since_ms, "last_ms": a.last_ms, "last": a.last,
        "alerts": alerts.iter().map(|x| json!({ "level": x.level, "text": x.text })).collect::<Vec<_>>(),
    });
    (value, alerts)
}

fn secs(ms: u64) -> String {
    format!("{:.1}s", ms as f64 / 1000.0)
}

impl Guest for Component {
    fn snapshot(after: u64) -> String {
        let now = observe::now();
        let stats = observe::agents();
        let mut agents = Vec::new();
        let mut alerts = Vec::new();
        let mut totals = [0u64; 6];
        for a in &stats {
            let (value, found) = agent_json(a, now);
            for x in found {
                alerts.push(json!({ "agent": a.name, "level": x.level, "text": x.text }));
            }
            agents.push(value);
            totals[0] += a.busy as u64;
            totals[1] += a.calls as u64;
            totals[2] += a.errors as u64;
            totals[3] += a.thoughts as u64;
            totals[4] += a.files as u64;
            totals[5] += a.programs as u64;
        }
        // Slow model calls are only seen in the events, and only worth a line while fresh.
        let mut events = observe::events(after, if after == 0 { 4096 } else { MAX_EVENTS });
        if after == 0 && events.len() > FIRST_EVENTS {
            events.drain(..events.len() - FIRST_EVENTS);
        }
        for e in events.iter().filter(|e| e.kind == "think" && e.ms > SLOW_THINK_MS && now.saturating_sub(e.at_ms) < 60_000) {
            alerts.push(json!({ "agent": e.agent, "level": "warn", "text": format!("model call took {}", secs(e.ms)) }));
        }
        let next = events.last().map_or(after, |e| e.seq);
        json!({
            "now_ms": now,
            "totals": { "agents": stats.len(), "busy": totals[0], "calls": totals[1], "errors": totals[2],
                        "thoughts": totals[3], "files": totals[4], "programs": totals[5] },
            "agents": agents,
            "edges": observe::edges().iter().map(|e| json!({ "from": e.caller, "to": e.agent, "calls": e.calls, "busy": e.busy })).collect::<Vec<_>>(),
            "alerts": alerts,
            "events": events.iter().map(event_json).collect::<Vec<_>>(),
            "next": next,
        })
        .to_string()
    }

    fn page() -> String {
        include_str!("../page.html").to_string()
    }

    fn report(agent: Option<String>) -> String {
        let now = observe::now();
        let stats = observe::agents();
        let Some(name) = agent else {
            if stats.is_empty() {
                return "no agent has run yet".into();
            }
            let mut lines = vec![format!("{} agents, kernel up {}", stats.len(), secs(now))];
            for a in &stats {
                let (v, _) = agent_json(a, now);
                let alerts: Vec<String> = v["alerts"].as_array().unwrap().iter().filter_map(|x| x["text"].as_str().map(str::to_string)).collect();
                let alerts = if alerts.is_empty() { String::new() } else { format!("  !! {}", alerts.join("; ")) };
                lines.push(format!(
                    "{:<12} {:<5} {} calls, {} failed, {} model, {} files, {} programs, avg {}{}",
                    a.name, v["state"].as_str().unwrap_or(""), a.calls, a.errors, a.thoughts, a.files, a.programs,
                    secs(v["avg_ms"].as_u64().unwrap_or(0)), alerts
                ));
            }
            return lines.join("\n");
        };
        let Some(a) = stats.iter().find(|a| a.name == name) else {
            let names: Vec<&str> = stats.iter().map(|a| a.name.as_str()).collect();
            return format!("no agent '{name}' has run (seen: {})", names.join(", "));
        };
        let (v, _) = agent_json(a, now);
        let mut lines = vec![format!(
            "{name}: {}, {} calls, {} failed, {} running now, avg {}",
            v["state"].as_str().unwrap_or(""), a.calls, a.errors, a.busy, secs(v["avg_ms"].as_u64().unwrap_or(0))
        )];
        for x in v["alerts"].as_array().unwrap() {
            lines.push(format!("  !! {}", x["text"].as_str().unwrap_or("")));
        }
        let mine: Vec<Event> = observe::events(0, 4096).into_iter().filter(|e| e.agent == name).collect();
        for e in mine.iter().rev().take(10).rev() {
            let mark = if e.ok { " " } else { "x" };
            lines.push(format!("  {mark} {:>8} {:<7} {}", secs(e.at_ms), e.kind, e.detail));
        }
        lines.join("\n")
    }
}

export!(Component);
