# Operating system model

[Index](README.md) · [OS model](os-model.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

## Everything is a WASM process

| Operating system | AgentWOS | State today |
|---|---|---|
| Kernel | The host: Wasmtime natively, the JS kernel in a browser | Exists; the only code that is not WASM |
| Process | An instance of a WASM component, with its own memory and Store | Exists |
| Program | A `.wasm` component | Exists |
| System call | A call through an import, mediated by the kernel | Exists, but each interface is wired by hand |
| Capability, file descriptor | The right to use some verbs on some party | Only as all-or-nothing `grants` |
| IPC | One agent calling another through the kernel | Only router to plugin |
| Driver | A service that touches the outside: LLM, workspace, network | LLM is a WASM component; the workspace is kernel code |
| Security module | A control | Exists, as WASM |
| Init table | The manifest | Two unrelated formats |
| Audit trail | The record | Per-example formats |

**The rule this document follows: everything that makes a decision or holds
state for an agent is a WASM component. The kernel only loads, links, checks
tables and records.** Three consequences:

- **Services are components.** `llm-call` already is. A service that needs the
  outside world gets a narrow WASI capability from the kernel (HTTP to named
  hosts, a preopened folder) rather than being written into the kernel. The
  workspace is the one service that is kernel code today; a WASM workspace
  component over a preopened directory would cover files, while starting
  programs stays a kernel service because WASI has no process API.
- **Policy checks are table lookups in the kernel; judgment is in controls.**
  The kernel never runs policy code of its own beyond matching selectors, verbs
  and patterns. Anything smarter is a control, which is WASM.
- **One process world.** Today there are eight worlds, one per kind of
  component, and each new kind adds wiring in the host. An operating system has
  one system call interface ([Protocol](protocol.md)).

## Mechanisms borrowed from Kubernetes

| Kubernetes | Idea | Use here |
|---|---|---|
| `apiVersion` / `kind` / `metadata` / `spec` | One envelope for every declared object | One manifest shape for agents, services, policies and controls |
| Labels and selectors | Rules target sets, not names | Policies select agents by label, so adding an agent needs no policy edit |
| NetworkPolicy | Pods are open until selected, then default deny; separate ingress and egress lists | Same split, but **deny is the default from the start** |
| RBAC (Role, RoleBinding) | Verbs on resources, bound to a subject | Rule = selector + verbs |
| Admission (validating and mutating webhooks) | Programmable checks that can reject or rewrite a request | Already present as controls |
| ResourceQuota / LimitRange | Budgets enforced by the platform | Per-agent and per-delegation limits |
| Impersonation, owner references | "Acting as", and who created what | Delegation chain |
| Audit log, Events | Structured record of every request | Record |
| CustomResourceDefinition | New kinds without changing the core | Extension by `kind` |

What does not transfer: Kubernetes policy works on IP addresses and ports
because it cannot see inside the traffic. The kernel here sees every call with
its interface, function and arguments, so policy can be about *meaning*
("may call `workspace.exec` with program `python3`"), not addresses. Kubernetes
is also a reconciling control plane with persistent state; none of that is
needed for the minimal protocol.

Two ideas come from outside Kubernetes, because it has no equivalent:

- **Capability attenuation** (object-capability systems, macaroons): a holder
  can hand on a weaker version of what it holds, never a stronger one. This is
  the delegation invariant.
- **On-behalf-of chains** (the `act` claim in OAuth token exchange): a request
  names both who is acting and for whom. This is the delegation chain.

Relation to existing agent protocols: MCP describes how an agent reaches tools,
and A2A describes how agents on different systems find and call each other.
Both are about *reaching* a peer. This proposal is about what the kernel
*permits and records* once peers can be reached, so it sits underneath either.

## Where AgentWOS is today

| Area | Today | Gap |
|---|---|---|
| Manifest | Two unrelated shapes: scenario (`scenario`, `controls`, `days`) and harness (`router`, `plugins`, `grants`) | No shared vocabulary; a new mode means a new shape |
| Egress | `grants: ["llm", "workspace"]` per plugin, all or nothing | Cannot say "this model only" or "read but not exec" |
| Ingress | Implicit: the router can call every plugin | Cannot say who may call an agent |
| External network | `llm-call` gets unrestricted `wasi:http` | Any host is reachable; the hook to filter it (`WasiHttpHooks::send_request`) is unused |
| Delegation | Router calls a plugin through its `tool` import | The plugin does not know on whose behalf it runs; no depth or budget limit; a plugin cannot delegate further |
| Controls | `allow / modify / block / flag`, run by the scenario runner and the browser kernel | Not run by the harness; only see `author` and `text` |
| Record | Scenario log and Tab Jam audit, each in its own format | No common event; denied capability calls are not recorded |
| Limits | Workspace timeout and output cap | No per-agent budget for calls, tokens or time |

The strongest existing piece is mediation: plugins never get `wasi:http`, the
filesystem or processes, and an ungranted import returns an error. The proposal
builds on that rather than replacing it.
