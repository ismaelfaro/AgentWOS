# AgentModules

Replica experimental de DeepAgents (LangChain) sobre WASM/WASI: Agentes y Tools
son componentes WASM independientes, linkeados por un host Rust/Wasmtime.

## Arquitectura

```
host (Rust, Wasmtime, Component Model)
 ├─ instancia tool-echo.wasm   (Store propio)
 ├─ instancia llm-call.wasm    (Store propio)
 └─ instancia agent-core.wasm  (Store propio)
        imports: llm, tool  ──┐
        export:  agent        │
                               └─ el host intercepta cada import y lo
                                  reenvia a la instancia correspondiente
```

Cada módulo vive en su propio `wasmtime::Store` (aislamiento real). El host
actúa de mediador: cuando `agent-core` llama a su import `tool.invoke(...)`,
el host (trait `Host` generado por `bindgen!`) reenvía la llamada a la
instancia `tool-echo` ya cargada en su propio Store. Mismo patrón para `llm`.

Este mediado por el host (en vez de linking estático binario-a-binario) es
deliberado: es el punto donde en fase 2 entra el **registry dinámico**
(attach/detach de módulos en runtime, ver abajo).

## Estructura

- `wit/world.wit` — contrato de interfaces (`agent`, `tool`, `llm`, `types`).
- `host/` — runtime Rust que instancia y linkea componentes.
- `modules/agent-core/` — agente orquestador mínimo: si el mensaje empieza
  con `tool:echo <texto>` llama a la tool; si no, delega al llm.
- `modules/tool-echo/` — tool de ejemplo (uppercase).
- `modules/llm-call/` — **stub v1**, sin HTTP real todavía (ver TODO).

## Build

```bash
source "$HOME/.cargo/env"
for m in tool-echo llm-call agent-core; do
  (cd modules/$m && cargo build --release --target wasm32-wasip2)
done
cargo build --release -p agentmodules-host
```

## Run

```bash
./target/release/agentmodules-host \
  --agent modules/agent-core/target/wasm32-wasip2/release/agent_core.wasm \
  --tool  modules/tool-echo/target/wasm32-wasip2/release/tool_echo.wasm \
  --llm   modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm \
  --message "tool:echo hola mundo"
```

## TODO (próximas fases)

1. **llm-call real**: añadir `include wasi:cli/imports@0.2.0;` +
   `wasi:http/outgoing-handler` al world, usar `cargo-component build`
   (ya instalado) para resolver deps WIT automáticamente, y hacer POST
   real a OpenRouter (remoto) u Ollama (local), eligiendo backend por
   prefijo de `model` (`openrouter/...` vs `local/...`).
2. **Registry dinámico**: interface `registry` (ya en `wit/world.wit`,
   no usada aún) con `attach(uri, expected-interface) -> module-handle`.
   El host resuelve, valida compatibilidad de interfaz, instancia y
   cachea — permite que un agente pida una tool nueva a mitad de tarea
   sin reiniciar el proceso.
3. **Evolver**: ciclo generar código → compilar a wasm → evaluar en
   sandbox (fitness) → promover versión en el registry (content-addressed,
   rollback si falla). Nunca mutar el binario corriendo.
