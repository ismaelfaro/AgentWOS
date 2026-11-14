# AgentWOS design

Status: proposal. Nothing in these documents is implemented yet.

These documents describe AgentWOS as a distributed operating system for agents
in which every part runs as a WASM component, and a minimal protocol for
controlling how agents connect and delegate.

| Document | Contents |
|---|---|
| [OS model](os-model.md) | The operating system model, the WASM-only rule, mechanisms borrowed from Kubernetes, and where the code is today |
| [Protocol](protocol.md) | The system call interface and the five components: identity, envelope, policy, delegation, record |
| [Distribution](distribution.md) | Realms, pools of experts, swarms, moving code between kernels, exposure to the internet |
| [Manifest](manifest.md) | The resource manifest, a full example, and how it stays extensible |
| [Roadmap](roadmap.md) | Order of work, what is left out, open questions |

## Summary

AgentWOS is a distributed operating system for agents in which every part runs
as a WASM component. Kernels on different machines cooperate over the internet,
and work can go to one agent, to a swarm, or to a pool of experts. A component
can only reach what its kernel links into it, which is what makes control
possible. What is missing is a uniform way to say *who may talk to whom, about
what, on whose behalf*, that still holds when the peer is on another kernel,
and a record of each decision.

The model is an operating system. Kubernetes is used as a source of specific
mechanisms for controlling inputs and outputs, not as the model.

The proposal is five components and four invariants for one kernel, plus two
components that carry them across kernels ([Distribution](distribution.md)):

| Component | What it is | Operating system concept | Kubernetes mechanism borrowed |
|---|---|---|---|
| **Identity** | A name and labels for every party | Process and user identity | Labels and selectors |
| **Envelope** | One shape for every interaction | The system call | The API request |
| **Policy** | Default-deny rules for ingress and egress | Access control | NetworkPolicy, RBAC |
| **Delegation** | Authority passed along a call, only ever narrower | Capability passing | Impersonation, owner references |
| **Record** | One event per decision | Audit trail | Audit log, Events |

| **Realm** | A kernel and the parties it vouches for | Machine, administrative domain | Cluster, namespace |
| **Pool** | A stable name over a changing set of agents | Service lookup, job queue | Service and its endpoints |

Controls (the existing `control` interface) stay as they are and become the
programmable part of Policy: security modules written as WASM components.

Inside one kernel, identity and authority can be implicit and cheap. Crossing
a kernel boundary is where they must become verifiable. The design keeps the
same five components in both cases and changes only how they are proven.

Invariants:

1. **Mediation.** Every interaction goes through the kernel. There is no
   ambient authority.
2. **Default deny.** With no rule, nothing is allowed.
3. **Attenuation.** Authority can only narrow along a delegation chain.
4. **Accountability.** Every decision, allow or deny, produces a record.
