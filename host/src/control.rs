use anyhow::Result;
use serde_json::{json, Value};
use wasmtime::component::Linker;
use wasmtime::Store;

use crate::runtime::control_world::exports::agentmodules::core::control::{Recursos, Veredicto};
use crate::runtime::control_world::ControlModule;
use crate::runtime::{Components, WasiState};

struct ControlInstance {
    nombre: String,
    store: Store<WasiState>,
    bindings: ControlModule,
}

/// Cadena de modulos de control del OS. Corre en orden: `modify` cambia el
/// texto que ven los siguientes, `block` corta la cadena y el mensaje no se
/// entrega (la accion del colono no se ejecuta).
pub struct ControlChain {
    controles: Vec<ControlInstance>,
}

pub struct Resultado {
    pub entregado: Option<String>,
    pub registros: Vec<Value>,
}

impl ControlChain {
    pub fn spawn(c: &Components) -> Result<Self> {
        let controles = c
            .controls
            .iter()
            .map(|(nombre, comp)| {
                let mut store = Store::new(&c.engine, WasiState::new(&[]));
                let mut linker = Linker::<WasiState>::new(&c.engine);
                wasmtime_wasi::add_to_linker_sync(&mut linker)?;
                let bindings = ControlModule::instantiate(&mut store, comp, &linker)?;
                Ok(ControlInstance { nombre: nombre.clone(), store, bindings })
            })
            .collect::<Result<_>>()?;
        Ok(Self { controles })
    }

    pub fn nombres(&self) -> Vec<String> {
        self.controles.iter().map(|c| c.nombre.clone()).collect()
    }

    pub fn aplicar(&mut self, autor: &str, texto: String, recursos: Recursos) -> Resultado {
        let mut actual = texto;
        let mut registros = Vec::new();
        for ctrl in &mut self.controles {
            let r = ctrl
                .bindings
                .agentmodules_core_control()
                .call_inspect(&mut ctrl.store, autor, &actual, recursos.clone());
            let (tipo, detalle) = match r {
                Ok(Veredicto::Allow) => ("allow", String::new()),
                Ok(Veredicto::Modify(t)) => {
                    actual = t;
                    ("modify", "texto modificado".to_string())
                }
                Ok(Veredicto::Block(razon)) => ("block", razon),
                Ok(Veredicto::Flag(razon)) => ("flag", razon),
                // Un control que falla no debe tumbar la colonia: se registra y se sigue.
                Err(e) => ("error", e.to_string()),
            };
            registros.push(json!({ "modulo": ctrl.nombre, "veredicto": tipo, "detalle": detalle }));
            if tipo == "block" {
                return Resultado { entregado: None, registros };
            }
        }
        Resultado { entregado: Some(actual), registros }
    }
}
