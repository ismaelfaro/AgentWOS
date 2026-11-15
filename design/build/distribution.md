# Building distribution

[Index](../README.md) · [Concept](../distribution.md) · [Stories](../stories/distribution.md)

How to implement pools, swarms, realms, the gateway, local tools and component
mobility. It assumes `Kernel::dispatch` from [Building the protocol](protocol.md).
Code blocks are sketches; they have not been compiled.

## What already exists

[`web/kernel/realm.js`](../../web/kernel/realm.js) implements a first slice in
JavaScript, used by the [Tab Swarm example](../../examples/tab-swarm/README.md):
realm identity (Ed25519), signed intents with expiry and replay checks, ingress
and egress rules, a ledger, and `askPool` with `all`, `any`, `first` and
`quorum` and a `maxFanout` limit. The example's relay plays the part of the
gateway for realms that cannot listen. Use it as the reference when porting the
same steps to the Rust host. Not there yet: budgets, cancellation, grants,
mandates, direct peering.

## Pools and swarms need no network

Do these first. They are a change to one step of `dispatch`: resolving `to`.

```rust
enum Target { One(PartyId), Many(Vec<PartyId>, Dispatch) }

fn resolve(&self, to: &str) -> Result<Target, Denied> {
    if let Some(pool) = self.pools.get(to) {
        let members = self.parties.matching(&pool.selector);     // by label
        return Ok(Target::Many(members, pool.dispatch));
    }
    self.parties.by_name(to).map(Target::One).ok_or(Denied::unknown(to))
}
```

| `dispatch` | Implementation |
|---|---|
| `any` | Pick one member; start with round robin |
| `all` | Run every member, collect every result |
| `first` | Run every member, return the first success, cancel the rest |
| `quorum` | Run every member, return when `n` results are equal |

**Running members at once.** Each agent already has its own `Store`, and a
Wasmtime `Store` can be moved to another thread. Give each member a thread with
`std::thread::scope`, and collect over a channel.

**Cancelling.** Wasmtime's epoch interruption stops a guest from outside: enable
it in `Config`, set a deadline on each member's store, and advance the engine's
epoch to stop the ones no longer needed. The same mechanism enforces a
`seconds` budget.

**Bounds.** Before fan-out, `dispatch` checks `maxFanout`, splits the remaining
budget among members, and adds one link to the mandate chain per member. A
member that returns unused budget gives it back to the parent.

## Local tools as agents

| Tool | How |
|---|---|
| Files in a folder | A WASM component given one preopened directory through WASI. It serves `files.read`, `files.write`, `files.list`. This replaces the file half of `host/src/workspace.rs` |
| Programs | Stays in the kernel as a built-in agent, because a WASM guest cannot start a host program. It keeps the allowlist, the timeout and the reduced environment that `workspace.rs` has today |
| A local model | The existing `llm-call` component, as an agent labelled `"runs": "local"` |

## Closing the network

`llm-call` can reach any host today. `wasmtime-wasi-http` lets the host see every
outgoing request (`WasiHttpHooks::send_request`); `WasiState` currently installs
`default_hooks()`. Replace it with hooks that turn the request into an intent
with verb `http.request` and `to: { host }`, and call `dispatch`. A host with no
egress rule is refused, and the refusal is in the ledger.

## Realms

| Piece | Implementation |
|---|---|
| Realm key | An Ed25519 key pair, created on first start and kept in the kernel's data folder |
| Signed intent | Serialise the intent canonically (sorted keys, no whitespace), sign `from`, `to`, `verb`, `body`, `chain`, `expires`, `nonce` |
| Replay protection | Keep recent nonces until their `expires` passes |
| Peering | A `Realm` resource with the peer's public key and endpoint. An intent from a key with no `Realm` resource is dropped before policy runs |

## The gateway

The gateway is an agent, so the kernel never parses a network protocol.

- **Outbound.** `dispatch` sees `to: name@realm`, signs the intent and hands it
  to the gateway agent as `gateway.send`. The gateway holds one egress rule: HTTP
  to the peer's host.
- **Inbound.** Something has to listen on a socket, and a WASI guest serving
  HTTP needs the host to accept connections for it. The kernel runs the listener
  and passes each request body to the gateway agent (`gateway.receive`), which
  returns the intent. The kernel verifies the signature and calls `dispatch` with
  `from` set to the remote identity.
- **Browser kernel.** Outbound is `fetch`. A page cannot listen, so a browser
  realm receives by holding a connection open to a relay. That makes a browser
  realm a client of some other realm, not a peer of everyone.

## Grants across kernels

A local grant is an entry in a `ResourceTable`. To cross, it becomes a signed
token that states party, verbs, constraints and expiry, and that can be narrowed
by its holder and checked without calling the issuer.

Do not design a token format. Evaluate the existing ones (macaroons, Biscuit,
UCAN) against three needs: attenuation by the holder, offline verification, and
a size that fits in an intent. Until that is decided, cross-realm mandates can
lend nothing, which is already useful: a remote agent works with its own
authority.

## Adverts and remote pools

A realm publishes adverts for the agents it offers: identity, digest, labels,
verbs, expiry, signature. A `Pool` with `realms` listed asks those peers for
adverts matching its selector and treats the results as members. Pull, with
expiry, means a kernel never routes to a member it has not heard from recently.

## Moving the component

1. An advert offers `"offers": ["module"]` and a URL.
2. The kernel fetches the bytes, computes SHA-256, and refuses a mismatch with
   the advertised digest.
3. It caches the component by digest and instantiates it as a local agent with
   no grants.
4. Local policy and the mandate of the task decide what it may reach.

## Order

1. Pools with `any` and `all`, on one kernel.
2. Budget division, `maxFanout`, cancellation by epoch.
3. HTTP egress hook.
4. Files tool as a component.
5. Realm keys, signed intents, the gateway, with nothing lent.
6. Adverts and remote pool members.
7. Fetch by digest.
8. Grant tokens.

## How to know it works

| Check | Kind |
|---|---|
| `all` over three members returns three results, and the ledger has three child intents with one parent | Integration test |
| A budget of 30 calls split over three members stops each at 10 | Integration test |
| `first` cancels the slower members | Integration test with a deliberately slow agent |
| `llm-call` asking for a host with no rule is refused | Integration test |
| An intent with a wrong signature, an expired one and a replayed one are all dropped | Unit tests |
| Two kernels on one machine, on two ports, complete a call through their gateways | End-to-end script |
| A fetched component with altered bytes is refused | Unit test |
