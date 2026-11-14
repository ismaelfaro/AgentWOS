# Distribution: realms, pools and swarms

[Index](README.md) · [OS model](os-model.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

AgentWOS is meant to run as many kernels cooperating over the internet. This
document adds two components to the five in [Protocol](protocol.md), and says
how the five are proven across a kernel boundary.

## Realm

A realm is one kernel and everything it loads. It has a key pair, and its public
key is its identity, so no global naming authority is needed.

```json
{ "kind": "Realm", "metadata": { "name": "lab", "labels": { "trust": "partner" } },
  "spec": { "key": "ed25519:9f2c...", "endpoint": "https://lab.example/agentwos" } }
```

Declaring a `Realm` resource is how two kernels peer. With no `Realm` resource
for a sender, its envelopes are dropped: default deny applies between kernels as
it does between agents.

A full identity is then `name@realm`, plus the **digest of the component**
(`sha256:...`). Because every party is a WASM component, the same bytes behave
the same on any kernel, so a policy can say "accept calls only from this exact
program", not only "from this name".

## What changes at a kernel boundary

| Component | Inside one kernel | Across kernels |
|---|---|---|
| Identity | Name assigned by the manifest | `name@realm`, realm key, component digest |
| Envelope | An in-memory value | The wire unit, signed by the sending kernel, with `expires` and a nonce |
| Capability | A WIT resource handle | A signed token that can be attenuated and checked offline |
| Delegation chain | Kernel bookkeeping | Each link signed by the kernel that added it |
| Policy | Selectors on names and labels | The same, plus selectors on realm and digest |
| Record | One log | One log per kernel, joined by envelope `id` and `parent` |

A local capability handle and a remote token are the same thing in two forms.
The kernel converts between them at the boundary: a handle lent to a remote
party leaves as a token, and a token that arrives is checked and becomes a
handle. A component never sees the difference, so the same agent code runs alone
or in a swarm.

Attenuable, offline-verifiable capability tokens are an existing technique
(macaroons, Biscuit and UCAN are three designs). Choosing one is an open
question; the protocol only needs the two properties.

## Transport is a component

The kernel does not speak a network protocol. A **gateway** is a service
component with one narrow capability, HTTP to and from named hosts. The kernel
hands it signed envelopes to send and takes back the ones it receives. An
inbound envelope is verified and then goes through the same system call path as
a local call, with `from` set to the remote identity.

This keeps the rule of the [OS model](os-model.md) (only the kernel is not WASM) and makes the
transport replaceable: HTTP today, something else later, without touching
policy or agents.

## Pool

A pool is a stable name over a set of agents that changes, as a Kubernetes
Service is over its pods. It is how a caller reaches "an expert in X" without
knowing who or where.

```json
{ "kind": "Pool", "metadata": { "name": "rust-experts" },
  "spec": { "selector": { "labels": { "expertise": "rust" } },
            "realms": ["local", "lab"],
            "dispatch": "any",
            "serves": ["agent.run"] } }
```

Members are found through **adverts**: a signed, expiring statement by a realm
that one of its agents exists and what it serves.

```json
{ "agent": "borrow-checker@lab", "digest": "sha256:41be...",
  "labels": { "expertise": "rust", "tier": "senior" },
  "serves": ["agent.run"], "expires": "2026-11-14T11:00:00Z", "sig": "..." }
```

A pool is the adverts that match its selector and pass policy. `dispatch` says
how a call to the pool is delivered:

| `dispatch` | Meaning |
|---|---|
| `any` | One member, chosen by the kernel |
| `all` | Every member; the caller gets every result |
| `first` | Every member; the first result wins and the rest are cancelled |
| `quorum` | Every member; returns when `n` results agree |

A pool is addressed like any party (`"to": "rust-experts"`), so ingress, egress
and delegation rules apply unchanged.

## Swarm

A swarm is not a separate concept: it is a call with `dispatch: all` (or `first`,
or `quorum`) whose members may delegate again. What a swarm needs from the
protocol is bounds, because fan-out multiplies cost:

- **`maxFanout`** in a delegation block limits how many members one call reaches.
- **Budgets divide.** A budget lent to `n` members is split among them; the sum
  can never exceed what the delegator held.
- **`maxDepth`** already bounds how far the tree grows.
- **Cancellation** travels down the chain: when the delegator stops waiting,
  members are told and their remaining budget returns.

With those four, the worst case of a swarm is known before it starts:
`maxFanout ^ maxDepth` calls within the root budget.

## Move the code or move the call

Because an expert is a WASM component identified by its digest, a pool member
can be used in two ways:

- **Call it there.** Send the envelope to its realm. The expert keeps its data
  and its realm's policies.
- **Run it here.** Fetch the component by digest and instantiate it locally,
  under local policy and with only the capabilities lent. The owner's data stays
  with the caller, and there is no round trip per call.

The second option is specific to an operating system built on WASM: a foreign
expert is safe to run because it has no authority except what it is handed. An
advert states which of the two an agent offers (`"offers": ["call", "module"]`).

## Exposure to the internet

A kernel reachable from the internet treats every remote party as untrusted
until a rule says otherwise:

- Unknown realm: drop.
- Known realm: ingress rules select by realm, labels and digest.
- Rate and budget limits apply to ingress as well as egress, per realm.
- Expired or replayed envelopes (by `expires` and nonce) are rejected before
  any policy runs.
- A remote party never gets a capability that reaches another remote party
  unless a delegation rule lends it.
