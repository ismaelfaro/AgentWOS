# AgentWOS design

Status: proposal. Nothing in these documents is implemented yet, except where a
document says "today".

## A new kind of operating system

A classic operating system exists so that programs can share a machine. It
turns hardware into processes, files and sockets, and its scarce resources are
processor time and memory.

AgentWOS exists so that **agents can share a world**: models, tools, data,
people and each other. It does not replace the classic operating system. It
runs on top of one, or on a browser, or on a cloud runtime, and it treats all
three as the same thing: a place where WebAssembly runs.

What it manages is different:

| | Classic operating system | AgentWOS |
|---|---|---|
| Runs on | Hardware | An operating system, a browser, a cloud runtime |
| Unit that runs | Process | **Agent** |
| What a unit asks for | A system call on a resource | An **intent**: someone wants something from someone |
| Scarce resources | Processor time, memory | Model calls, tokens, time, money, **authority** |
| Who decides | Deterministic code | Models, which can be wrong |
| Trust | Users and permissions | **Grants** that can be lent and only ever narrowed |
| Scope | One machine | Many kernels, on laptops, browsers and clouds |

Four stances shape every decision.

### AI first

The system assumes that the things it runs think with models, and that models
are uncertain.

- **Inference is a system resource.** A model is something the kernel hands out,
  meters and routes, the way a classic kernel hands out processor time. Agents
  ask for a completion; where it runs, local or remote, is policy.
- **Every action is bounded and observed.** A model can be wrong, so nothing an
  agent does is unlimited or unrecorded. Budgets, guards and the ledger are not
  add-ons; they are the system.
- **Agents describe themselves to models.** What an agent can do is published
  as verbs with descriptions and schemas, so another agent's model can choose
  among them. That description is the manual page of this system.
- **Uncertainty is data.** A reply can carry a confidence, and guards and the
  ledger can use it.
- **People are parties.** A person can be asked, can approve, and can be
  delegated to, through the same protocol as an agent.

### Cloud native

The system is declared, not configured by hand, and it is built to spread.

- **Declarative.** A manifest states what should run and what each part may do.
- **Immutable and content-addressed.** An agent is a WebAssembly component
  identified by its digest. The same bytes behave the same anywhere.
- **Labels and selectors.** Rules target sets of agents, not names.
- **Default deny.** With no rule, nothing may talk to anything.
- **Scale by pools.** A name stands for a changing set of agents.
- **The same manifest runs on a laptop, in a browser and in a cloud.**

### Local first

Being built for the cloud does not mean giving up the machine in front of you.

- **Local tools are first-class.** Files in a folder, programs you allow and
  models running on your machine are reachable as agents of your own kernel.
- **Your kernel decides.** A remote swarm can be lent the use of one local tool
  for one task. It never gets the machine.
- **Works offline.** One kernel with local models is a complete system.

### Self-improving

The system is meant to get better on its own, all the time.

- **System agents improve the rest.** Some agents exist to watch the ledger,
  look for what is new outside, propose better revisions, test them and promote
  them. They improve user agents and the system's own agents.
- **Inside and outside.** Improvement comes from the system's own history and
  from what appears elsewhere: new models, better experts, newer components.
- **Swapped while running.** A better revision replaces the one in use without
  stopping the system or breaking work in progress.
- **What one learns, all have.** Agents that work for the same person or team
  share what they learn at once.
- **Within limits it cannot move.** The kernel and the invariants are outside
  the reach of the part that improves.

## The components

AgentWOS has its own vocabulary. Each term has a relative in classic operating
systems or in cloud platforms, given here as an aid, not as a definition.

| Component | What it is | Classic relative | Cloud relative |
|---|---|---|---|
| **Kernel** | The one part that is not an agent. Loads agents, carries intents, checks policy, writes the ledger | Kernel | Control plane |
| **Agent** | The unit that runs: a WebAssembly component with a name, labels and grants | Process | Pod |
| **Intent** | One request from a party to a party: who, to whom, which verb, with what, on whose behalf | System call | API request |
| **Verb** | Something an agent can be asked to do, with a description a model can read | System call number | API method |
| **Grant** | An unforgeable right to use some verbs on some party. Can be weakened, never strengthened | File descriptor | Scoped token |
| **Mandate** | Grants lent for one task, with a budget and a depth. How one agent works on behalf of another | (none) | Impersonation |
| **Policy** | Who may send what to whom. Default deny | Permissions | NetworkPolicy, RBAC |
| **Guard** | An agent that inspects intents and may allow, modify, block or flag them | Security module | Admission webhook |
| **Budget** | A limit on calls, tokens, time or money that only goes down | Quota | ResourceQuota |
| **Ledger** | One entry per decision, allowed or denied | Audit log | Traces, events |
| **Model** | An agent that serves inference. A system resource | Processor | Managed service |
| **Tool** | An agent that acts on something outside: a folder, a program, a web service | Device driver | Sidecar |
| **Realm** | One kernel and everything it vouches for | Machine | Cluster |
| **Pool** | A stable name over a changing set of agents | (none) | Service |
| **Swarm** | An intent sent to many agents at once, within bounds | (none) | Parallel job |
| **Manifest** | The declaration of a system: its agents, policies, pools and realms | Boot configuration | Manifests |
| **System agent** | An agent that keeps the system running or improving, as opposed to a **user agent** that does work people ask for | System service | Operator, controller |
| **Revision** | One version of an agent: a component digest and its settings | Program version | Image tag |
| **Registry** | The system agent that maps each name to its current revision and keeps the earlier ones | Package manager | Image registry, rollout |
| **Plane** | A scope of sharing: everything that works for one person, team or project, across realms | User session | Tenant |
| **Lesson** | Something one agent learned, published to its plane with author, evidence and expiry | (none) | (none) |
| **Hive memory** | What a plane knows: its lessons, served by a memory agent | Shared memory | Shared state store |

Not everything in a classic operating system has a counterpart, and that is
intended. There is no scheduler of processor time, no virtual memory and no
file system here; the host underneath does those.

## Documents

Each feature has three documents: the concept, how to build it in the code that
exists, and stories that show it working.

| Feature | Concept | Build | Stories |
|---|---|---|---|
| Intents, policy, grants, mandates, guards, ledger | [Protocol](protocol.md) | [Build](build/protocol.md) | [Stories](stories/protocol.md) |
| Realms, pools, swarms, local tools | [Distribution](distribution.md) | [Build](build/distribution.md) | [Stories](stories/distribution.md) |
| System agents that improve the rest | [Evolution](evolution.md) | [Build](build/evolution.md) | [Stories](stories/evolution.md) |
| Replacing agents and workflows while they run | [Hot swapping](hot-swap.md) | [Build](build/hot-swap.md) | [Stories](stories/hot-swap.md) |
| Planes, shared memory, shared skills (under discussion) | [Hive](hive.md) | [Build](build/hive.md) | [Stories](stories/hive.md) |
| Choosing the model for a coding agent: benchmark and knowledge table (plan) | [Benchmark](benchmark.md) | [Build](build/benchmark.md) | [Stories](stories/benchmark.md) |
| Sending each piece of work to the right expert, with decision models (plan) | [Router](router.md) | [Build](build/router.md) | [Stories](stories/router.md) |

Supporting documents:

| Document | Contents |
|---|---|
| [Architecture](architecture.md) | The layers, the rule that only the kernel is not an agent, and where the code is today |
| [Manifest](manifest.md) | The resource format, a full example, and how it stays extensible |
| [State of the art](state-of-the-art.md) | Published work on agents and models that change and improve, and what this design takes from it |
| [Roadmap](roadmap.md) | Order of work, what is left out, open questions |

## Invariants

1. **Mediation.** Every intent goes through a kernel. No agent holds ambient
   authority.
2. **Default deny.** With no rule, nothing is allowed.
3. **Attenuation.** Authority can only narrow as it is lent.
4. **Accountability.** Every decision produces a ledger entry.
5. **Portability.** An agent runs unchanged on any kernel: local, browser or
   cloud.
