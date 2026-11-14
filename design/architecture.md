# Architecture

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

## Layers

```
   people            other realms (kernels on other machines)
      │                        │
 ┌────┴────────────────────────┴─────────────────────────────────┐
 │  agents                                                       │
 │  workers · models · tools · guards · gateways                 │
 │  every one a WebAssembly component, built against one world   │
 ├───────────────────────────────────────────────────────────────┤
 │  AgentWOS kernel                                              │
 │  carries intents · checks policy · tracks grants and budgets  │
 │  runs guards · writes the ledger                              │
 ├───────────────────────────────────────────────────────────────┤
 │  WebAssembly runtime (Wasmtime, a browser engine)             │
 ├───────────────────────────────────────────────────────────────┤
 │  host: Linux, macOS, Windows · a browser · a cloud runtime    │
 └───────────────────────────────────────────────────────────────┘
```

The host keeps doing what it is good at: processor scheduling, memory, files,
sockets. AgentWOS does not redo any of it. It adds the layer the host does not
have: who may ask whom for what, with which authority, at what cost.

## Everything that runs is an agent

There is one kind of running thing. What differs is the job:

| Agent | Serves | Holds |
|---|---|---|
| **Worker** | A task: `agent.run` | Whatever it is granted |
| **Model** | Inference: `model.complete` | A way to one inference backend, local or remote |
| **Tool** | An action on something outside: `files.read`, `program.run`, `web.fetch` | One narrow way out: a folder, a list of programs, named hosts |
| **Guard** | A verdict on an intent: `guard.inspect` | Nothing; it only reads what the kernel shows it |
| **Gateway** | Carrying intents to and from other realms | HTTP to named hosts |
| **Memory** | What a plane knows: `memory.publish`, `memory.recall` | The plane's lessons |
| **Observer, Scout, Proposer, Evaluator, Registry** | The evolution loop | The ledger to read; revisions to propose and promote |

The first rows are **user agents** or services for them. The last two rows are
**system agents**: they keep the system running and improving
([Evolution](evolution.md), [Hive](hive.md)). The difference is a label and the
policies that select on it, not a different kind of thing.

**The rule: everything that runs is an agent, and every agent is a WebAssembly
component. The kernel only loads, links, checks tables and writes the ledger.**

Three consequences:

- **Models and tools are agents, not kernel features.** An agent that needs the
  outside gets one narrow way out from the kernel. This is what makes a local
  tool safe to offer: the tool agent holds the folder, and other agents hold
  only a grant to ask it.
- **The kernel does not judge.** Policy is table lookup: selectors, verbs,
  patterns, budgets. Anything that needs judgment is a guard, and a guard is an
  agent.
- **One world.** Every agent is built against the same interface
  ([Protocol](protocol.md)). A new kind of agent is new verbs, not a new world.

One exception is stated, not hidden: starting a program on the host cannot be
done from inside WebAssembly, so the kernel offers it as a built-in tool agent.

## What is borrowed

AgentWOS is its own design, and it borrows mechanisms that have proven
themselves.

| From | Mechanism | Becomes |
|---|---|---|
| Operating systems | Mediation by a kernel; file descriptors as unforgeable handles | The kernel; grants |
| Object-capability systems, macaroons | A holder can pass on a weaker right, never a stronger one | Attenuation of grants |
| Kubernetes | `kind` / `metadata` / `spec`; labels and selectors | The manifest; policy selectors |
| Kubernetes NetworkPolicy | Separate ingress and egress lists | Policy, with deny as the default from the start |
| Kubernetes admission webhooks | Programmable checks that reject or rewrite | Guards |
| Kubernetes Service | A stable name over a changing set | Pools |
| OAuth token exchange | A request names who acts and for whom | The mandate chain |
| WebAssembly Component Model | Typed interfaces; resources a guest cannot forge | The agent world; grants as resources |

One thing does not transfer from Kubernetes: its network policy works on
addresses and ports because it cannot see inside the traffic. This kernel sees
every intent with its verb and arguments, so policy is about meaning ("may run
`python3` in this folder"), not addresses.

Relation to agent protocols: MCP describes how an agent reaches tools, and A2A
how agents on different systems find and call each other. Both are about
reaching a peer. AgentWOS is about what a kernel permits, bounds and records
once a peer can be reached, so it can sit underneath either.

## Where the code is today

The code predates this vocabulary. The table uses today's names and says which
component each will become.

| Area | Today | Gap | Becomes |
|---|---|---|---|
| Manifest | Two unrelated shapes: scenario and harness | A new mode means a new shape | One manifest |
| Egress | `grants: ["llm", "workspace"]` per plugin, all or nothing | Cannot say "this model only" or "read but not run" | Policy and grants |
| Ingress | Implicit: the router can call every plugin | Cannot say who may ask an agent | Policy |
| External network | `llm-call` gets unrestricted `wasi:http` | Any host is reachable; the Wasmtime hook to filter it is unused | Policy on a model agent |
| Working for another | Router calls a plugin through its `tool` import | The plugin does not know for whom it works; no depth or budget | Mandate |
| Checks | `control` components: `allow / modify / block / flag` | Not run by the harness; they only see `author` and `text` | Guards |
| Log | Scenario log and Tab Jam audit, each its own format | Denied calls are not written anywhere | Ledger |
| Limits | Workspace timeout and output cap | No budget for calls, tokens or time | Budget |
| Local tools | The `workspace` interface, kernel code | Not an agent | Tool agents |
| Kinds of component | Eight WIT worlds | Each new kind adds wiring in the host | One agent world |

The strongest existing piece is mediation: plugin agents never get the network,
the file system or programs directly, and an import that was not granted returns
an error. The design builds on that.
