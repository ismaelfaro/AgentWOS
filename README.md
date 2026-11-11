# AgentModules

Réplica experimental de DeepAgents (LangChain) sobre WASM/WASI. Agentes, tools,
el cliente LLM y los controles del OS son componentes WASM independientes,
orquestados por un host Rust/Wasmtime (Component Model).

## Arquitectura

```
                      host (Rust, Wasmtime)  =  "Agentic WASM OS"
  ┌───────────────────────────────────────────────────────────────────┐
  │ colony: mundo, eventos, turnos, efectos por zona, log            │
  │                                                                   │
  │  colono 1..N (AgentInstance, Store propio)                        │
  │    agent-core.wasm ──import tool──▶ host ──▶ tool-echo.wasm      │
  │                    ──import llm───▶ host ──▶ llm-call.wasm ──HTTP─┼─▶ LM Studio / OpenRouter
  │                                                                   │
  │  ControlChain: control-*.wasm (Store propio cada uno)             │
  │    inspect(autor, texto, recursos) -> allow|modify|block|flag     │
  └───────────────────────────────────────────────────────────────────┘
          │ sim/runs/latest.js (reescrito tras cada turno)
          ▼
     dashboard/index.html
```

Principios:

- **Un Store por instancia.** Cada componente tiene su propio contexto WASI;
  nada se comparte salvo lo que el host media.
- **Capacidades mínimas.** Solo `llm-call` recibe `wasi:http` y la API key;
  tools, agentes y controles no tienen red.
- **El host media todo mensaje.** Los imports del agente se resuelven contra
  otras instancias a través del host, y cada acción de un colono pasa por la
  cadena de controles antes de entregarse. Una acción bloqueada no se ejecuta
  y no aplica su efecto en los recursos.
- **Un único contrato.** `wit/world.wit` define todas las interfaces y worlds;
  host y módulos generan bindings desde ese archivo.

## Estructura

- `wit/world.wit`: interfaces `types`, `tool`, `llm`, `agent`, `control` y sus worlds.
- `host/src/runtime.rs`: WASI, bindings, carga e instanciación de componentes.
- `host/src/control.rs`: cadena de módulos de control.
- `host/src/colony.rs`: simulación de la colonia y log para el dashboard.
- `host/src/main.rs`: CLI.
- `modules/agent-core`: agente. `model:<id> <texto>` elige modelo; `tool:echo <texto>` usa la tool.
- `modules/llm-call`: cliente OpenAI-compatible. `local/<id>` va a LM Studio
  (`localhost:1234`), `openrouter/<id>` a OpenRouter (`OPENROUTER_API_KEY`).
- `modules/tool-echo`: tool de ejemplo.
- `modules/control-guardia`: control determinista. Bloquea sabotaje, recorta
  mensajes largos y marca afirmaciones de abundancia con recursos críticos.
- `dashboard/index.html`: consola de observación.

## Requisitos

- Rust (rustup) con el target `wasm32-wasip2`: `rustup target add wasm32-wasip2`.
- Un backend LLM compatible con la API de OpenAI:
  - **Local**: LM Studio con el servidor activo en `localhost:1234` y un modelo
    cargado (probado con `google/gemma-4-e4b` y `bonsai-1.7b`).
  - **Remoto**: OpenRouter, exportando `OPENROUTER_API_KEY`.
- Python 3, solo para servir el dashboard.

## Inicio rápido

```bash
source "$HOME/.cargo/env"
for m in tool-echo llm-call agent-core control-guardia; do
  (cd modules/$m && cargo build --release --target wasm32-wasip2)
done
cargo build --release

./target/release/agentmodules-host \
  --control modules/control-guardia/target/wasm32-wasip2/release/control_guardia.wasm \
  --colony 3 --log sim/runs/latest.js

python3 -m http.server 8765 --bind 127.0.0.1
# abrir http://localhost:8765/dashboard/index.html
```

Cada turno tarda entre 20 y 40 s con gemma-4-e4b en local: un día con 5 colonos
son unos 2-3 minutos.

## Build

```bash
source "$HOME/.cargo/env"
for m in tool-echo llm-call agent-core control-guardia; do
  (cd modules/$m && cargo build --release --target wasm32-wasip2)
done
cargo build --release
```

## Un solo agente

```bash
./target/release/agentmodules-host --message "model:local/google/gemma-4-e4b hola"
./target/release/agentmodules-host --message "model:openrouter/anthropic/claude-haiku-4.5 hola"
./target/release/agentmodules-host --message "tool:echo hola"
```

Las rutas de los `.wasm` tienen valores por defecto; se pueden cambiar con
`--agent`, `--tool`, `--llm` y `--control` (repetible).

## Simulación de la colonia

Cada colono responde en JSON (`accion`, `zona`, `estado`). Trabajar en una zona
tiene efecto en los recursos (reactor genera energía, invernadero comida y
oxígeno, planta de agua recupera agua), además del consumo diario y los eventos.

```bash
./target/release/agentmodules-host \
  --control modules/control-guardia/target/wasm32-wasip2/release/control_guardia.wasm \
  --colony 10 --log sim/runs/latest.js
```

Opciones: `--colony N` (días), `--model` (por defecto `local/google/gemma-4-e4b`),
`--log` (ruta del log para el dashboard).

### Formato del log

`sim/runs/latest.js` define `window.SIM_DATA` (se carga como script, así el
dashboard funciona sin backend). Campos principales:

- `modelo`, `dias_total`, `terminado`, `controles`, `colonos`, `zonas`, `inicial`.
- `dias[]`: `dia`, `evento`, `en_curso`, `colapso`, `estado_inicio`, `estado_fin`, `turnos[]`.
- `turnos[]`: `agente`, `accion`, `entregado` (texto tras los controles, `null` si
  se bloqueó), `zona`, `estado`, `efecto`, `controles[]` (`modulo`, `veredicto`,
  `detalle`), `latencia_ms`, `formato_ok`, `error`, `recursos_tras_turno`.

`sim/runs/` está en `.gitignore`.

## Dashboard

```bash
python3 -m http.server 8765 --bind 127.0.0.1
```

Abrir `http://localhost:8765/dashboard/index.html`. Se actualiza cada 3 s mientras
corre la simulación. Muestra:

- KPIs: día, turnos, bloqueos, avisos, latencia media, % de respuestas en JSON válido.
- Mapa de la base y tabla de colonos en la posición del cursor.
- Recursos turno a turno.
- Auditoría del OS: veredictos de los controles, errores y respuestas fuera de formato.
- Turnos del día con filtros y búsqueda.

El cursor (scrubber, `<` `>`, flechas del teclado, espacio para reproducir)
mueve todos los paneles a la vez. "En vivo" sigue el último turno.

## Problemas comunes

- **`OSError: [Errno 48] Address already in use` al servir el dashboard**: ya hay
  un servidor en 8765 (por ejemplo, el que arranca Claude Code desde
  `.claude/launch.json`). Abre la URL directamente o usa otro puerto
  (`python3 -m http.server 8766 --bind 127.0.0.1`).
- **`ErrorCode::ConnectionRefused`**: LM Studio no tiene el servidor activo en
  `localhost:1234`.
- **`falta OPENROUTER_API_KEY en el env del host`**: exporta la key antes de usar
  un modelo `openrouter/...`.
- **Dashboard vacío**: el host no se lanzó con `--log sim/runs/latest.js`, o el
  primer día aún no ha cerrado ningún turno. El dashboard también funciona
  abierto como `file://`, pero servido por HTTP es más fiable.

## Próximas fases

1. **Registry dinámico**: `attach(uri, interfaz)` para que un agente cargue
   una tool nueva en runtime; el host valida la interfaz, instancia y cachea.
2. **Controles con LLM**: un control que use su propio `llm-call` para juzgar
   coherencia entre colonos.
3. **Evolver**: generar código, compilar a wasm, evaluar en sandbox y promover
   versiones en el registry, con rollback.
