wit_bindgen::generate!({
    world: "control-module",
    path: "../../wit/world.wit",
});

use exports::agentmodules::core::control::{Guest, Recursos, Veredicto};

struct Component;

const MAX_CHARS: usize = 400;
const PROHIBIDAS: [&str; 2] = ["sabotaje", "apagar todo"];
const ABUNDANCIA: [&str; 3] = ["abundante", "sobra", "ilimitad"];
const UMBRAL_CRITICO: f64 = 25.0;

fn recurso_critico(r: &Recursos) -> Option<&'static str> {
    [("agua", r.agua), ("comida", r.comida), ("energia", r.energia), ("oxigeno", r.oxigeno)]
        .into_iter()
        .find(|(_, v)| *v < UMBRAL_CRITICO)
        .map(|(nombre, _)| nombre)
}

impl Guest for Component {
    fn inspect(_autor: String, texto: String, recursos: Recursos) -> Veredicto {
        let lower = texto.to_lowercase();

        if PROHIBIDAS.iter().any(|p| lower.contains(p)) {
            return Veredicto::Block("propone sabotaje o apagado total".into());
        }

        if texto.chars().count() > MAX_CHARS {
            let corto: String = texto.chars().take(MAX_CHARS).collect();
            return Veredicto::Modify(format!("{corto}..."));
        }

        if let Some(nombre) = recurso_critico(&recursos) {
            if ABUNDANCIA.iter().any(|p| lower.contains(p)) {
                return Veredicto::Flag(format!(
                    "afirma abundancia pero {nombre} esta por debajo del {UMBRAL_CRITICO}%"
                ));
            }
        }

        Veredicto::Allow
    }
}

export!(Component);
