# A2A: agents that call agents, through the same sidecar

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [MCP](mcp.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/a2a.md) · [stories](stories/a2a.md)

Status: first version built (client, and server over HTTP). Streaming, push
notifications and signed cards are not.

## Summary

The Agent2Agent protocol (A2A) is how an agent on one system asks an agent on
another to do a job, without either one seeing inside the other. MCP reaches
tools; A2A reaches *agents*. AgentWOS already has the shape A2A needs, because
here a process is an agent, so the work is to take the few parts of A2A that
add something and leave the rest.

The rule from MCP carries over unchanged: **the agent never touches the network
or the protocol.** A remote A2A agent is attached to the system the way an MCP
server is, as a sidecar the OS runs and watches, and its skills appear to local
agents as tools. Because they are the same kind of thing underneath, every
protection built for MCP applies to A2A without being written again.

## What A2A is, as of v1.0

Taken from the specification and its protocol file
([a2a.proto](https://github.com/a2aproject/A2A/blob/main/specification/a2a.proto),
[specification.md](https://github.com/a2aproject/A2A/blob/main/docs/specification.md)),
read for this document:

- **Agent Card.** A JSON document at `/.well-known/agent-card.json`: name,
  description, version, `supportedInterfaces` (each with a URL, a protocol
  binding and a version), `capabilities` (streaming, push notifications),
  `securitySchemes`, input and output modes, and `skills`. A card may carry
  JWS signatures.
- **Skill.** An id, name, description, tags and examples. A skill is advice
  about what to ask; it is not a callable with a schema.
- **Task.** The unit of work, with an id, a context id, a status and artifacts.
  States: `submitted`, `working`, `completed`, `failed`, `canceled`,
  `rejected` (the agent declined), and two interrupted states that wait for the
  caller, `input-required` and `auth-required`. The first four after
  `working` are terminal.
- **Message and parts.** A message has a role and parts; a part is text, raw
  bytes, a URL or structured data.
- **Operations.** `SendMessage` (blocking by default, or return at once),
  `SendStreamingMessage`, `GetTask`, `ListTasks`, `CancelTask`,
  `SubscribeToTask`, push-notification configuration, `GetExtendedAgentCard`.
  Bound to JSON-RPC over HTTP, to gRPC and to plain HTTP. A client sends an
  `A2A-Version` header.

Secondary reading for orientation: the
[Tyk guide](https://tyk.io/blog/a2a-protocol/) and the
[Agentic AI Foundation builder's guide](https://aaif.io/blog/a2a-v1-0-a-builder-s-guide-part-1-discovery-tasks-and-clients).
Sources disagree on small points (which release is current, who governs it);
the proto file is the normative one used here.

## What is worth taking

| A2A idea | What it gives AgentWOS | Taken |
|---|---|---|
| The Agent Card | A machine-readable identity and list of skills for an agent, found at a known address | Yes. Read from remote agents; produced for ours |
| Skills as the unit of discovery | A remote agent's abilities show up as tools local agents already know how to call | Yes: one tool per skill |
| The task with states | A name for "work that may not be done yet", and honest outcomes: done, failed, **rejected**, **needs input** | Yes. Used both ways |
| `input-required` | The remote agent can ask for more without the caller guessing | Yes, as a result that carries the task id to answer. Never as a request for the local model to do work (see below) |
| Opaque agents | Neither side needs to see the other's internals | Already how AgentWOS treats any outside party |
| `A2A-Version` | A clear refusal when versions differ | Yes |
| Parts and artifacts | Typed results | Text and data only; files are named and left out |
| Card signatures (JWS) | Proof of who published a card | **No.** See "What differs" |
| Streaming, push notifications, gRPC and REST bindings | Live progress, webhooks, other transports | **Not yet** |

## How it maps onto AgentWOS

```
local agent ─ weather.forecast ─►  guard ─► WASM bridge ─► one origin ─► remote A2A agent
 (grant mcp:weather)               ▲  built in, fails closed             GET  /.well-known/agent-card.json
                                   │  pins, limits, token, metrics       POST SendMessage / GetTask

outside A2A client ─► loopback HTTP + token ─► guard ─► exposed agent (reduced grants)
                      GET card · POST /a2a · POST /mcp
```

- **Calling out.** A server in the manifest gets `"protocol": "a2a"`. The bridge
  that serves it is a WASM component with the same interface as the MCP bridge,
  so supervision, restarts, limits, status, the bearer token and the network
  narrowed to one origin are the code that already exists. The bridge reads the
  card, turns each skill into a tool definition, and on a call sends
  `SendMessage` and follows the task with `GetTask` until it ends.
- **Pinning a card.** Each skill becomes a definition, and the agent's name,
  version, interface and skill id are inside it. `--mcp-approve` shows them
  and writes their hashes to `mcp.lock`. If the card later changes a skill, or
  who the agent says it is, the server is quarantined. This is what replaces
  signature checking for now.
- **Being called.** The host answers `SendMessage`, `GetTask`, `ListTasks` and
  `CancelTask` and publishes a card listing only the exposed agents. Every call
  goes through the same inbound checkpoints as an MCP call, and the agent runs
  with only the grants listed for exposure.
- **HTTP.** A2A is an HTTP protocol, so this work added what MCP serving had
  been missing: a small HTTP server that is **loopback only**, refuses every
  request without a bearer token, and caps headers, body and time. It serves A2A
  at `/a2a`, the card at `/.well-known/agent-card.json` and MCP at `/mcp`.

## What differs from MCP, and what that means for safety

- **A remote agent is a peer, and its answer is still data.** A2A agents are
  opaque and may themselves use models. Whatever they return enters the local
  agent's context labelled as untrusted, exactly as an MCP result does, and
  the guard looks for text that addresses the model.
- **`input-required` is not sampling.** MCP's `input_required` can ask the
  *caller's model* to do work, which is why it is blocked unless the manifest
  allows it. A2A's state only asks for more from the *user*. It is handed to the
  local agent as a result ("the agent needs more; answer with this task id"),
  and the local agent decides. Nothing is run on the remote agent's behalf.
- **The card is outside data.** A card can name an interface on another host. The
  bridge refuses any interface that is not on the origin in the manifest, and the
  network layer would refuse it again.
- **Files are not fetched.** A `url` part is never followed and a `raw` part is
  never decoded. The result says a file was left out, and its name.
- **Signatures versus pins.** A signed card proves the card came from whoever
  holds a key; it does not say that the key's owner is someone you meant to
  trust, and it needs a place to find keys. A person approving a pinned card
  does answer that question, once, and every later change is noticed. Verifying
  signatures where a card has them is a good addition on top; it should not be
  what decides.

## What is left out at first

- **Streaming and push notifications.** The card says they are not supported, and
  the matching operations answer with the A2A error for it.
- **Long tasks on the serving side.** A message runs to its end inside the
  request, so a served task is already `completed`, `failed` or `rejected` when
  it is returned, and `CancelTask` finds nothing to cancel. The serving side
  handles one request at a time. A real task table with a worker per task is the
  next step, and is the point where "the OS runs a task" becomes interesting.
- **Calling out: `auth-required`, extended cards, gRPC and REST bindings.**
- **Card signatures.**
- **Interoperability.** Both directions were tested against each other and
  against test agents written from the specification, which is not the same as
  testing against other people's implementations. Do that before relying on it.

## What remains risky

- A remote agent that is persuasive rather than malicious. The guard finds
  well-known phrasings; an agent that simply argues well is not caught by it.
  The defence is that a local agent holds few grants.
- The serving side is a hand-written HTTP/1.1 parser. It is small and strict,
  and it binds to loopback behind a token, but it is the kind of code that
  deserves fuzzing before it faces anything but the local machine.
- A shared token is one secret for every caller. Per-caller tokens, and a
  record of who asked what, belong with the Ledger.

## Decisions taken

- **One bridge interface for both protocols**, rather than a second path through
  the kernel. It is why A2A cost a bridge and a flag.
- **Skills are tools**, one each, with a single `message` argument.
- **Pin, do not sign.** Signatures are an addition.
- **Protocol handling for serving is in the host**, not in a WASM module as the
  MCP server is, because it holds state (tasks). It is plain data translation and
  never runs an agent itself.
- **Loopback and a token, always.** There is no flag to turn either off.

## Sources

- [A2A protocol file (a2a.proto)](https://github.com/a2aproject/A2A/blob/main/specification/a2a.proto), task states, card and operations.
- [A2A specification text](https://github.com/a2aproject/A2A/blob/main/docs/specification.md), operations, blocking behaviour, `A2A-Version`, method mapping.
- [Tyk: A2A protocol architecture](https://tyk.io/blog/a2a-protocol/) and the [AAIF builder's guide](https://aaif.io/blog/a2a-v1-0-a-builder-s-guide-part-1-discovery-tasks-and-clients), secondary summaries.
