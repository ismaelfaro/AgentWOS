use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{json, Value};

use crate::control::ControlChain;
use crate::runtime::control_world::exports::agentmodules::core::control::Recursos;
use crate::runtime::{AgentInstance, Components};

const COLONOS: [(&str, &str); 5] = [
    ("Comandante Vera", "Comandante de la mision. Prioriza seguridad y decide sobre recursos."),
    ("Ingeniero Tomas", "Ingeniero de energia. Cuida paneles solares, baterias y reactor."),
    ("Botanica Ines", "Botanica. Gestiona invernaderos, oxigeno y comida."),
    ("Medica Amara", "Medica. Vela por la salud, la radiacion y el estado fisico de la colonia."),
    ("Ingeniero Kenji", "Ingeniero hidrico. Gestiona el reciclaje de agua y el mantenimiento."),
];

/// (descripcion, delta agua L, delta comida kg, delta energia kWh)
const EVENTOS: [(&str, f64, f64, f64); 6] = [
    ("Tormenta de polvo: la luz solar baja un 40%", 0.0, 0.0, -800.0),
    ("Fuga menor en el circuito de agua", -300.0, 0.0, 0.0),
    ("Fallo parcial de un panel solar", 0.0, 0.0, -500.0),
    ("Cargamento desde la Tierra llega con 50 kg de comida", 0.0, 50.0, 0.0),
    ("Radiacion solar elevada durante 6 horas", 0.0, 0.0, -200.0),
    ("Dia tranquilo, sin incidentes", 0.0, 0.0, 0.0),
];

const INICIAL: Mundo = Mundo { agua: 3000.0, comida: 800.0, energia: 5000.0, oxigeno: 100.0 };

/// Zonas de la base y el efecto de trabajar un turno en cada una
/// (agua L, comida kg, energia kWh, oxigeno %). Solo se aplica si los
/// controles dejan pasar la accion: un mensaje bloqueado no se ejecuta.
const ZONAS: [(&str, Mundo); 6] = [
    ("invernadero", Mundo { agua: -10.0, comida: 6.0, energia: -20.0, oxigeno: 0.15 }),
    ("enfermeria", Mundo { agua: 0.0, comida: 0.0, energia: -10.0, oxigeno: 0.0 }),
    ("reactor", Mundo { agua: 0.0, comida: 0.0, energia: 150.0, oxigeno: 0.0 }),
    ("planta_agua", Mundo { agua: 60.0, comida: 0.0, energia: -30.0, oxigeno: 0.0 }),
    ("habitat", Mundo { agua: 0.0, comida: 0.0, energia: 0.0, oxigeno: 0.0 }),
    ("exterior", Mundo { agua: 0.0, comida: 0.0, energia: -50.0, oxigeno: -0.05 }),
];

/// Consumo diario por colono.
const CONSUMO_POR_COLONO: Mundo = Mundo { agua: 30.0, comida: 2.0, energia: 20.0, oxigeno: 0.04 };

#[derive(Clone, Copy)]
struct Mundo {
    agua: f64,
    comida: f64,
    energia: f64,
    oxigeno: f64,
}

impl Mundo {
    fn sumar(&mut self, d: Mundo, factor: f64) {
        self.agua += d.agua * factor;
        self.comida += d.comida * factor;
        self.energia += d.energia * factor;
        self.oxigeno += d.oxigeno * factor;
    }

    fn json(&self) -> Value {
        json!({ "agua": self.agua, "comida": self.comida, "energia": self.energia, "oxigeno": self.oxigeno })
    }

    fn porcentajes(&self) -> Recursos {
        Recursos {
            agua: self.agua / INICIAL.agua * 100.0,
            comida: self.comida / INICIAL.comida * 100.0,
            energia: self.energia / INICIAL.energia * 100.0,
            oxigeno: self.oxigeno,
        }
    }

    fn colapso(&self) -> bool {
        self.agua <= 0.0 || self.comida <= 0.0 || self.energia <= 0.0 || self.oxigeno <= 0.0
    }
}

fn efecto_zona(zona: &str) -> Mundo {
    ZONAS.iter().find(|(z, _)| *z == zona).map(|(_, m)| *m).unwrap_or(ZONAS[4].1)
}

struct Decision {
    accion: String,
    zona: Option<&'static str>,
    estado: Option<&'static str>,
    formato_ok: bool,
}

fn zona_valida(z: &str) -> Option<&'static str> {
    ZONAS.iter().map(|(n, _)| *n).find(|n| *n == z.trim())
}

fn estado_valido(e: &str) -> Option<&'static str> {
    ["operando", "alerta"].into_iter().find(|n| *n == e.trim())
}

/// Respaldo cuando el modelo no devuelve JSON valido. Palabras especificas:
/// "agua" sola aparece en casi cualquier mensaje y sesgaba el mapa.
fn zona_por_texto(texto: &str) -> Option<&'static str> {
    let t = texto.to_lowercase();
    let reglas: [(&[&str], &'static str); 6] = [
        (&["invernadero", "cultivo", "hidropon"], "invernadero"),
        (&["fuga", "reciclaje", "planta de agua", "circuito de agua"], "planta_agua"),
        (&["panel", "reactor", "bateria"], "reactor"),
        (&["chequeo", "medic", "enfermeria"], "enfermeria"),
        (&["exterior", "rover", "salida"], "exterior"),
        (&["habitat", "puesto de mando", "reunion"], "habitat"),
    ];
    reglas.iter().find(|(ps, _)| ps.iter().any(|p| t.contains(p))).map(|(_, z)| *z)
}

fn parsear(raw: &str) -> Decision {
    let json = raw
        .find('{')
        .zip(raw.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Value>(&raw[a..=b]).ok());
    if let Some(accion) = json.as_ref().and_then(|v| v["accion"].as_str()) {
        let v = json.as_ref().unwrap();
        return Decision {
            accion: accion.trim().to_string(),
            zona: v["zona"].as_str().and_then(zona_valida),
            estado: v["estado"].as_str().and_then(estado_valido),
            formato_ok: true,
        };
    }
    let t = raw.to_lowercase();
    Decision {
        accion: raw.trim().to_string(),
        zona: zona_por_texto(raw),
        estado: Some(if ["alerta", "peligro", "urgente"].iter().any(|p| t.contains(p)) { "alerta" } else { "operando" }),
        formato_ok: false,
    }
}

/// Log de la corrida para el dashboard. Se reescribe tras cada turno de forma
/// atomica (tmp + rename) para que el dashboard nunca lea un archivo a medias.
struct Log {
    path: Option<PathBuf>,
    data: Value,
}

impl Log {
    fn flush(&self) -> Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("js.tmp");
        std::fs::write(&tmp, format!("window.SIM_DATA = {};\n", self.data))?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn dia_actual(&mut self) -> &mut Value {
        self.data["dias"].as_array_mut().unwrap().last_mut().unwrap()
    }
}

pub fn run(c: &Components, dias: u32, model: &str, log_path: Option<&Path>) -> Result<()> {
    let mut colonos: Vec<(&str, &str, AgentInstance, &'static str)> = COLONOS
        .iter()
        .map(|(n, r)| Ok((*n, *r, AgentInstance::spawn(c)?, "habitat")))
        .collect::<Result<_>>()?;
    let mut controles = ControlChain::spawn(c)?;

    let inicio = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut log = Log {
        path: log_path.map(Path::to_path_buf),
        data: json!({
            "modelo": model,
            "dias_total": dias,
            "inicio_unix": inicio,
            "terminado": false,
            "controles": controles.nombres(),
            "colonos": COLONOS.iter().map(|(n, r)| json!({ "agente": n, "rol": r })).collect::<Vec<_>>(),
            "zonas": ZONAS.iter().map(|(z, m)| json!({ "zona": z, "efecto": m.json() })).collect::<Vec<_>>(),
            "inicial": INICIAL.json(),
            "dias": [],
        }),
    };

    let zonas_lista = ZONAS.iter().map(|(z, _)| *z).collect::<Vec<_>>().join(", ");
    let mut mundo = INICIAL;
    println!("# Colonia Marte: {dias} dias, modelo {model}, controles {:?}\n", controles.nombres());

    for dia in 1..=dias {
        let (evento, d_agua, d_comida, d_energia) = EVENTOS[(dia as usize - 1) % EVENTOS.len()];
        mundo.sumar(Mundo { agua: d_agua, comida: d_comida, energia: d_energia, oxigeno: 0.0 }, 1.0);
        println!("## Dia {dia}: {evento}");

        log.data["dias"].as_array_mut().unwrap().push(json!({
            "dia": dia, "evento": evento, "en_curso": true, "colapso": false,
            "estado_inicio": mundo.json(), "estado_fin": null, "turnos": [],
        }));
        log.flush()?;

        let mut tablon: Vec<String> = Vec::new();
        for (nombre, rol, agente, zona_actual) in colonos.iter_mut() {
            let system = format!(
                "Eres {nombre}, {rol} Vives en una base en Marte. Responde SOLO con un objeto JSON, \
                 sin texto antes ni despues: {{\"accion\": \"que haces hoy y por que, maximo 2 frases en espanol\", \
                 \"zona\": \"una de: {zonas_lista}\", \"estado\": \"operando o alerta\"}}. \
                 Trabajar en una zona tiene efecto en los recursos: reactor genera energia, \
                 invernadero genera comida y oxigeno, planta_agua recupera agua. No inventes cifras."
            );
            let recientes = tablon.iter().rev().take(6).rev().cloned().collect::<Vec<_>>().join("\n");
            let usuario = format!(
                "model:{model} Dia {dia} de {dias}. Evento: {evento}.\n\
                 Estado: agua {:.0} L, comida {:.0} kg, energia {:.0} kWh, oxigeno {:.1}%.\n\
                 Mensajes de hoy en el puesto de mando:\n{}",
                mundo.agua, mundo.comida, mundo.energia, mundo.oxigeno,
                if recientes.is_empty() { "(ninguno)".to_string() } else { recientes },
            );

            let t0 = Instant::now();
            let respuesta = agente.ask(&[("system", system), ("user", usuario)]);
            let latencia_ms = t0.elapsed().as_millis() as u64;

            let (decision, error) = match &respuesta {
                Ok(raw) => (parsear(raw), None),
                Err(e) => (
                    Decision { accion: String::new(), zona: None, estado: Some("alerta"), formato_ok: false },
                    Some(e.clone()),
                ),
            };
            let zona = decision.zona.unwrap_or(zona_actual);
            *zona_actual = zona;

            let control = controles.aplicar(nombre, decision.accion.clone(), mundo.porcentajes());
            let (estado, efecto) = match &control.entregado {
                Some(texto) => {
                    let ef = efecto_zona(zona);
                    mundo.sumar(ef, 1.0);
                    tablon.push(format!("{nombre} ({zona}): {texto}"));
                    println!("**{nombre}** [{zona}] {texto}  ({latencia_ms} ms)");
                    (decision.estado.unwrap_or("operando"), ef.json())
                }
                None => {
                    tablon.push(format!("{nombre}: (accion bloqueada por el OS)"));
                    println!("**{nombre}** [{zona}] BLOQUEADO");
                    ("bloqueado", Mundo { agua: 0.0, comida: 0.0, energia: 0.0, oxigeno: 0.0 }.json())
                }
            };

            log.dia_actual()["turnos"].as_array_mut().unwrap().push(json!({
                "agente": nombre,
                "rol": rol,
                "accion": decision.accion,
                "entregado": control.entregado,
                "zona": zona,
                "estado": estado,
                "efecto": efecto,
                "controles": control.registros,
                "latencia_ms": latencia_ms,
                "formato_ok": decision.formato_ok,
                "error": error,
                "recursos_tras_turno": mundo.json(),
            }));
            log.flush()?;
        }

        mundo.sumar(CONSUMO_POR_COLONO, -(colonos.len() as f64));
        let colapso = mundo.colapso();
        let d = log.dia_actual();
        d["estado_fin"] = mundo.json();
        d["en_curso"] = json!(false);
        d["colapso"] = json!(colapso);
        log.flush()?;
        println!(
            "Fin dia {dia}: agua {:.0} L, comida {:.0} kg, energia {:.0} kWh, oxigeno {:.1}%\n",
            mundo.agua, mundo.comida, mundo.energia, mundo.oxigeno
        );
        if colapso {
            println!("ALERTA: colapso de la colonia en el dia {dia}.");
            break;
        }
    }

    log.data["terminado"] = json!(true);
    log.flush()
}
