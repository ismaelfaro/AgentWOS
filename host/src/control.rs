use anyhow::Result;
use serde_json::{json, Value};
use wasmtime::component::Linker;
use wasmtime::Store;

use crate::runtime::control_world::exports::agentmodules::core::control::Verdict;
use crate::runtime::control_world::ControlModule;
use crate::runtime::{Components, WasiState};

struct ControlInstance {
    name: String,
    store: Store<WasiState>,
    bindings: ControlModule,
}

/// Chain of OS control modules. Runs in order: `modify` changes the text the
/// following modules see; `block` stops the chain and the message is not
/// delivered (the colonist's action is not executed).
pub struct ControlChain {
    controls: Vec<ControlInstance>,
}

pub struct Outcome {
    pub delivered: Option<String>,
    pub records: Vec<Value>,
}

impl ControlChain {
    pub fn spawn(c: &Components) -> Result<Self> {
        let controls = c
            .controls
            .iter()
            .map(|(name, comp)| {
                let mut store = Store::new(&c.engine, WasiState::new(&[]));
                let mut linker = Linker::<WasiState>::new(&c.engine);
                wasmtime_wasi::add_to_linker_sync(&mut linker)?;
                let bindings = ControlModule::instantiate(&mut store, comp, &linker)?;
                Ok(ControlInstance { name: name.clone(), store, bindings })
            })
            .collect::<Result<_>>()?;
        Ok(Self { controls })
    }

    pub fn names(&self) -> Vec<String> {
        self.controls.iter().map(|c| c.name.clone()).collect()
    }

    pub fn apply(&mut self, author: &str, text: String, context: &str) -> Outcome {
        let mut current = text;
        let mut records = Vec::new();
        for ctrl in &mut self.controls {
            let result = ctrl
                .bindings
                .agentmodules_core_control()
                .call_inspect(&mut ctrl.store, author, &current, context);
            let (kind, detail) = match result {
                Ok(Verdict::Allow) => ("allow", String::new()),
                Ok(Verdict::Modify(t)) => {
                    current = t;
                    ("modify", "text modified".to_string())
                }
                Ok(Verdict::Block(reason)) => ("block", reason),
                Ok(Verdict::Flag(reason)) => ("flag", reason),
                // A failing control must not take the colony down: log it and carry on.
                Err(e) => ("error", e.to_string()),
            };
            records.push(json!({ "module": ctrl.name, "verdict": kind, "detail": detail }));
            if kind == "block" {
                return Outcome { delivered: None, records };
            }
        }
        Outcome { delivered: Some(current), records }
    }
}
