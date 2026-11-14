# Roadmap

[Index](README.md) · [OS model](os-model.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

## Suggested order of work

1. **Envelope and record.** Build the envelope at every import boundary in the
   native host and the browser kernel, and write the common record. No behaviour
   change; it makes everything after it observable.
2. **The `kernel` interface and the `process` world.** One system call surface
   for new components; existing worlds keep working through the same path.
3. **Resource manifest and default-deny egress.** One loader for both modes;
   translate `grants` to policies. Add `verbs` and `where`.
4. **Ingress and controls in the harness.** Run the control chain on harness
   calls; controls receive the envelope.
5. **Delegation with capability handles, and budgets.** Let a process call
   another; enforce lending, depth, cycles and shared limits.
6. **HTTP egress.** Filter `llm-call`'s outgoing requests with the Wasmtime HTTP
   hook against `host` services.
7. **Workspace as a component.** Move file access into a WASM service over a
   preopened folder; keep only program start in the kernel.
8. **Pools on one kernel.** `Pool` with `dispatch`, `maxFanout` and divided
   budgets. This gives swarms and expert pools without any networking.
9. **Realms and the gateway.** Realm keys, signed envelopes, a gateway
   component, default-deny ingress from other kernels.
10. **Capability tokens and adverts.** Lend across kernels; discover pool
    members in other realms.
11. **Module mobility.** Fetch an expert by digest and run it locally.

Steps 1 to 8 need one machine. They are ordered so that the distributed steps
add proof (signatures, tokens) to structures that already exist, instead of
adding new structures.

Each step is useful on its own.

## Deliberately left out

| Left out | Why | When it would be needed |
|---|---|---|
| A global naming authority | Realm keys are self-certifying; names are local | If human-readable global names are wanted |
| Consensus between kernels | Each kernel decides for itself and records it | Shared state that several realms must agree on |
| Reputation, payment, contracts | Application semantics; adverts and records give them something to build on | Open marketplaces of experts |
| Deny rules, rule priority | Order-dependent results; controls cover the need | Probably never |
| Persistent policy store, reconciliation | The manifest is the state | Long-running, reconfigurable deployments |
| Placement and load balancing beyond `dispatch` | `any` is enough to start | Large pools with uneven members |
| NAT traversal, relays | A gateway concern, replaceable | Kernels without a public endpoint |

The token format, the signature scheme and the transport are named but not
fixed. They sit behind the gateway and the capability interface, so choosing or
changing them does not alter the components.

## Open questions

1. Should `where` patterns be globs only, or is a small expression language
   worth its cost? Globs are proposed.
2. Is the user a party with policies, or always trusted? Proposed: a party, so a
   kiosk or shared deployment can restrict it.
3. Should limits be hard stops or reported overruns? Proposed: hard stops, with
   a record.
4. JSON keeps the host free of a YAML dependency. Is YAML worth it for
   readability of larger manifests?
5. Which capability token design: macaroons, Biscuit, UCAN, or a minimal own
   format? They differ in how attenuation is expressed and verified.
6. Should adverts be pushed between peered realms, or pulled on demand?
   Proposed: pulled, with expiry, so a kernel never holds stale members.
7. Is "run it here" (module mobility) allowed by default for a pool, or opt-in
   per advert? Proposed: opt-in by the owner, since it hands over the code.
8. Should existing components move to the `process` world, or should the typed
   worlds stay as a compatibility layer indefinitely? Proposed: new components
   use `process`; old ones move when they are next touched.
