//! A fast, rule-based brain for the evolution runs.
//!
//! The language model that drives the ducks in the turn-based scenario answers in
//! seconds; an evolution needs millions of decisions. This brain picks each duck's
//! next state from its personality and its energy, as the stage's urges do.
//! The bodies and the talk are the same code the scenario runs.

use crate::body::{Ball, Decision, Duck, Policy, Rng, Room};

pub struct Profile {
    pub name: &'static str,
    /// walk, zoomies, look around, quack, nap, ball
    pub weights: [f64; 6],
    pub start: (f64, f64, f64),
}

pub const DUCKS: [Profile; 4] = [
    Profile { name: "Pip", weights: [5.0, 1.0, 2.0, 1.0, 0.5, 1.0], start: (1.0, 1.4, 0.0) },
    Profile { name: "Momo", weights: [1.5, 3.0, 1.0, 1.0, 0.5, 4.0], start: (2.7, 0.6, 1.57) },
    Profile { name: "Nilo", weights: [1.5, 0.3, 2.0, 0.7, 4.0, 0.5], start: (3.2, 2.2, 3.14) },
    Profile { name: "Lila", weights: [2.0, 1.0, 1.5, 4.0, 0.5, 1.0], start: (1.8, 2.1, -1.57) },
];

const PLACES: [(f64, f64); 6] = [(3.4, 2.6), (0.8, 2.0), (2.6, 1.5), (1.0, 0.85), (3.25, 0.6), (1.9, 1.3)];

/// Chooses the state each duck runs for the coming round.
pub fn decide(ducks: &mut [Duck], ball: &Ball, room: &Room, rng: &mut Rng) {
    let n = ducks.len();
    for i in 0..n {
        if matches!(ducks[i].policy, Policy::Fallen | Policy::Limp) {
            continue;
        }
        let energy = ducks[i].energy;
        // A duck that is napping keeps at it until it is rested.
        if ducks[i].policy == Policy::Sit && energy < 0.95 {
            continue;
        }
        let w = DUCKS[i].weights;
        let tired = energy < 0.3;
        // walk to a place, walk to another duck, zoomies, look, quack, nap, ball
        let weights = [
            w[0] * 0.6,
            w[0] * 0.5,
            if energy < 0.6 { 0.0 } else { w[1] },
            w[2],
            w[3],
            if energy >= 0.6 { 0.0 } else { w[4] },
            w[5],
        ];
        let choice = if tired {
            5
        } else {
            let total: f64 = weights.iter().sum();
            let mut roll = rng.unit() * total;
            let mut pick = 0;
            for (k, x) in weights.iter().enumerate() {
                if roll < *x {
                    pick = k;
                    break;
                }
                roll -= x;
            }
            pick
        };
        let (state, goal) = match choice {
            0 => ("wander", Some(PLACES[(rng.unit() * PLACES.len() as f64) as usize % PLACES.len()])),
            1 => {
                let mut j = (rng.unit() * n as f64) as usize % n;
                if j == i {
                    j = (j + 1) % n;
                }
                ("wander", Some((ducks[j].x, ducks[j].y)))
            }
            2 => ("zoomies", None),
            3 => ("look_around", None),
            4 => ("quack", None),
            5 => ("nap", None),
            _ => ("ball", None),
        };
        let _ = (ball, room);
        ducks[i].decision = Decision { state: state.into(), goal, thought: String::new() };
    }
}
