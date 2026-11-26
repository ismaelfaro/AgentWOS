//! The duck soccer simulator for the browser lab. JSON in, JSON out, through
//! linear memory: the page writes a request with `alloc`, calls `run`, and reads
//! the answer at `out_ptr`/`out_len`. No bindings generator needed.

#[path = "../../src/sim.rs"]
#[allow(dead_code)]
mod sim;

use serde_json::{json, Value};
use sim::{play, Options, Rng, Team, G, N};

static mut OUT: Vec<u8> = Vec::new();

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    unsafe { (*std::ptr::addr_of!(OUT)).as_ptr() }
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    unsafe { (*std::ptr::addr_of!(OUT)).len() }
}

fn team(v: &Value, rng: &mut Rng) -> Team {
    let mut t = Team::random(rng);
    if let Some(g) = v["genome"].as_array() {
        for d in 0..N {
            for k in 0..G {
                t.genome[d][k] = g[d][k].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);
            }
        }
    }
    if let Some(u) = v["use_call"].as_array() {
        for m in 0..3 {
            t.use_call[m] = u[m].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
        }
    }
    t
}

/// Request: { teams: [team, team], talk: [bool, bool], seed, seconds, matches }.
/// One match returns its replay; more return the tally for team 0.
#[no_mangle]
pub extern "C" fn run(ptr: *mut u8, len: usize) {
    let req: Value = unsafe { serde_json::from_slice(&Vec::from_raw_parts(ptr, len, len)) }.unwrap_or(Value::Null);
    let seed = req["seed"].as_u64().unwrap_or(1);
    let mut rng = Rng::new(seed ^ 0x5eed);
    let a = team(&req["teams"][0], &mut rng);
    let b = team(&req["teams"][1], &mut rng);
    let comms = [req["talk"][0].as_bool().unwrap_or(true), req["talk"][1].as_bool().unwrap_or(false)];
    let seconds = req["seconds"].as_f64().unwrap_or(90.0);
    let matches = req["matches"].as_u64().unwrap_or(1).max(1);
    let out = if matches == 1 {
        let o = play([&a, &b], seed, &Options { comms, record: true, seconds, calls: [true; 3] });
        json!({ "score": o.score, "frames": o.frames, "events": o.events, "passes": o.passes,
                "talk": { "said": o.talk[0].said, "ok": o.talk[0].ok, "missed": o.talk[0].missed, "correct": o.talk[0].correct } })
    } else {
        let (mut w, mut d, mut l, mut gf, mut ga) = (0, 0, 0, 0u32, 0u32);
        for k in 0..matches {
            let o = play([&a, &b], seed.wrapping_add(k * 7919), &Options { comms, record: false, seconds, calls: [true; 3] });
            gf += o.score[0];
            ga += o.score[1];
            match o.score[0].cmp(&o.score[1]) {
                std::cmp::Ordering::Greater => w += 1,
                std::cmp::Ordering::Equal => d += 1,
                std::cmp::Ordering::Less => l += 1,
            }
        }
        json!({ "matches": matches, "w": w, "d": d, "l": l, "gf": gf, "ga": ga })
    };
    unsafe { *std::ptr::addr_of_mut!(OUT) = serde_json::to_vec(&out).unwrap() };
}
