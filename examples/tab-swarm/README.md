# Tab Swarm

One agent on your machine, many agents in browsers, working together. Each
browser tab is its own AgentWOS realm: a kernel with its own key, its own policy
and its own ledger, running a WASM agent. The lead agent on your machine splits
a job among them and puts the answers together.

```
 your machine                                  any browser, any tab
 ┌──────────────────────────────┐              ┌───────────────────────────┐
 │ realm "lead"                 │              │ realm "chrome-3isv"       │
 │  lead.wasm   plans, reduces  │   signed     │  worker.wasm  counts      │
 │  kernel      policy, ledger  │◀─ intents ──▶│  kernel       policy,     │
 │  relay       WebSocket       │   results    │               ledger      │
 └──────────────────────────────┘              └───────────────────────────┘
                 ▲                              ┌───────────────────────────┐
                 └─────────────────────────────▶│ realm "firefox-9k2a"  ... │
                                                └───────────────────────────┘
```

The job is counting prime numbers below a limit. It is chosen because it splits
cleanly and has one right answer, so a realm that lies can be caught.

## Run it

Requires Rust with the `wasm32-wasip2` target and Node.js 20 or later.

```bash
examples/tab-swarm/build.sh
node examples/tab-swarm/local/realm.mjs
```

The terminal prints a URL with a room token. Open it in two or three browsers
(Chrome, Firefox, Safari, or several tabs of one), then type in the terminal:

```
> primes 20000000
There are 1270607 primes below 20000000, counted in 4 pieces.
  4 pieces, 4 intents to 2 realms, 44 ms, dispatch all
  chrome-3isv: 2  chrome-620z: 2
```

Each tab shows the pieces it worked on and its own ledger.

| Command | What it does |
|---|---|
| `primes <n>` | The lead splits the range across the worker realms |
| `/who` | Realms in the room, with the start of each key |
| `/mode all` | Each piece goes to one realm (default) |
| `/mode first` | Each piece goes to every realm; the first answer wins |
| `/mode quorum` | Each piece goes to three realms; two equal answers win |
| `/probe` | The lead asks a worker to read a file. Its own policy refuses |
| `/ledger [n]` | The last decisions of the local kernel |

No browser at hand: `node examples/tab-swarm/local/realm.mjs --local-workers 3`
starts three worker realms in the same process, over the same WebSocket path.

## Things to try

- **Catch a liar.** In one tab, tick "Return wrong answers". With `/mode all` the
  total comes out wrong and nothing notices. With three or more tabs and
  `/mode quorum`, the total is right and the terminal names the realm that
  disagreed.
- **Watch a refusal.** `/probe` in the terminal, or "Try to reach out" in a tab.
  Each kernel refuses by its own policy, and the denial is in its ledger.
- **Close a tab mid-job.** The lead is told at once that the realm left, instead
  of waiting for a timeout.
- **Join with the wrong token.** Change the token in the URL. The relay refuses
  the realm before it sees any intent.

## How it works

| Piece | Where | What it is |
|---|---|---|
| Realm | [`web/kernel/realm.js`](../../web/kernel/realm.js) | Identity, policy, ledger and pools. The same file runs in Node and in the tab |
| Lead agent | [`agents/lead`](agents/lead/src/lib.rs) | A WASM component. It turns a request into pieces of work, and results into an answer. It has no network, model or tool |
| Worker agent | [`agents/worker`](agents/worker/src/lib.rs) | A WASM component that counts primes in a range |
| Relay | [`local/relay.mjs`](local/relay.mjs) | Admits realms and forwards messages. A browser cannot accept connections, so tabs connect out to it |
| Local realm | [`local/realm.mjs`](local/realm.mjs) | The lead, the relay and the prompt |
| Tab realm | [`web/src/tab-realm.js`](web/src/tab-realm.js), [`web/src/main.js`](web/src/main.js) | The worker and its page |

1. **A realm is a key.** Each realm makes an Ed25519 key pair when it starts.
   The private key never leaves the tab or the process.
2. **Joining.** The relay sends a challenge. The realm signs it and presents the
   room token. The relay then tells everyone who is present: name, labels, key,
   verbs served.
3. **An intent.** The lead's kernel checks its egress rule, signs the intent and
   sends it through the relay.
4. **Receiving.** The tab's kernel checks, in order: known realm, signature, not
   expired, not seen before, ingress rule. Only then does the worker run.
5. **The result** comes back signed by the tab. A result that fails its
   signature is dropped.
6. **Every decision**, on both sides, is a ledger entry.

The policies are a few lines each, and deny everything else:

```js
// the lead
{ egress: [{ to: { labels: { role: "worker" } }, verbs: ["primes.count"] }], ingress: [] }
// a tab
{ ingress: [{ from: { name: "lead" }, verbs: ["primes.count"] }], egress: [] }
```

## Test

```bash
(cd examples/tab-swarm && node test/smoke.mjs)
```

It runs the example with workers on this machine, including a faulty one, and
checks each refusal: a verb outside the policy, an intent or result changed on
the wire, a replay, an expired intent, an unknown realm, and a pool larger than
its fan-out limit.

## Limits

- **The relay is trusted to introduce realms.** It tells each realm the others'
  keys. It cannot forge an intent, because it has no private key, but a dishonest
  relay could introduce a realm under a false name. Peering by exchanging keys
  directly is in the design and not built.
- **The local realm runs on the JavaScript kernel in Node,** not on the Rust
  host. The agents are the same WASM components either way; wiring the Rust host
  to a relay is the next step.
- **No grants or mandates across realms yet.** A worker acts with its own
  authority only, which here is none beyond counting.
- **Other devices need https.** A page that is not on `localhost` or `https`
  cannot use WebCrypto, so it cannot sign. `--allow-unsigned` admits such realms
  for a demo on a local network; do not use it anywhere else.
- **`first` does not cancel the losers.** They finish their piece and the result
  is ignored.
- **Tested with Chromium tabs and Node workers.** Firefox and Safari support
  Ed25519 in WebCrypto in current versions, but this example has not been run in
  them.
