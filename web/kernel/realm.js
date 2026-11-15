// AgentWOS realm: one kernel's view of a distributed system. A realm has an
// identity (a key pair), a policy (default deny), a ledger, and a transport to
// reach other realms. It runs unchanged in Node and in a browser tab.
//
// This is the first implemented slice of design/distribution.md: signed
// intents, adverts, ingress and egress rules, and pools with `all`, `any`,
// `first` and `quorum` dispatch. Grants and mandates are not here yet: a remote
// agent works with its own authority only.

const encoder = new TextEncoder();
const INTENT_TTL_MS = 30_000;

const b64 = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes)));
const unb64 = (text) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));

/** JSON with sorted keys, so two kernels sign and verify the same bytes. */
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  }
  return JSON.stringify(value) ?? "null";
}

/**
 * A realm's identity. The public key is the identity; the name is a label for
 * people. Where WebCrypto is unavailable (a page on plain http that is not
 * localhost), the identity is unsigned and peers decide whether to admit it.
 */
export async function createIdentity(name, labels = {}) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return { name, labels, key: null, sign: async () => null };
  const pair = await subtle.generateKey("Ed25519", false, ["sign", "verify"]);
  const key = b64(await subtle.exportKey("raw", pair.publicKey));
  return {
    name,
    labels,
    key,
    sign: async (payload) => b64(await subtle.sign("Ed25519", pair.privateKey, encoder.encode(canonical(payload)))),
  };
}

export async function verify(key, signature, payload) {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle || !key || !signature) return false;
  try {
    const publicKey = await subtle.importKey("raw", unb64(key), "Ed25519", false, ["verify"]);
    return await subtle.verify("Ed25519", publicKey, unb64(signature), encoder.encode(canonical(payload)));
  } catch {
    return false;
  }
}

/** Short form of a key for people to compare. */
export const fingerprint = (key) => (key ? key.slice(0, 8) : "unsigned");

/** A selector matches a party by name, by labels, or everyone when empty. */
export function matches(selector, party) {
  if (!party) return false;
  if (selector.name !== undefined && selector.name !== party.name) return false;
  return Object.entries(selector.labels ?? {}).every(([k, v]) => party.labels?.[k] === v);
}

const verbMatches = (patterns, verb) =>
  patterns.some((p) => p === "*" || p === verb || (p.endsWith(".*") && verb.startsWith(p.slice(0, -1))));

/** The rule that allows this, or null. Rules only allow; no rule means deny. */
function allowedBy(rules, peerField, peer, verb) {
  const index = (rules ?? []).findIndex((r) => matches(r[peerField], peer) && verbMatches(r.verbs, verb));
  return index < 0 ? null : index;
}

/**
 * @param identity   from createIdentity
 * @param policy     { ingress: [{ from, verbs }], egress: [{ to, verbs }], allowUnsigned }
 * @param serve      { verb: async (body, intent) => result } for this realm's agents
 * @param send       (peerName, message) => void, the transport
 * @param onLedger   called with every ledger entry
 */
export function createRealm({ identity, policy, serve = {}, send, onLedger = () => {} }) {
  const peers = new Map();      // name -> advert { name, labels, key, serves }
  const pending = new Map();    // intent id -> { resolve, reject, timer }
  const seen = new Map();       // intent id -> expiry, to refuse replays
  const ledger = [];
  let counter = 0;
  let turn = 0;

  function record(entry) {
    const full = { at: new Date().toISOString(), ...entry };
    ledger.push(full);
    onLedger(full);
    return full;
  }

  const describe = (intent) => ({
    intent: intent.id, parent: intent.parent ?? null, from: intent.from, to: intent.to, verb: intent.verb,
  });

  /** Sends one intent to one peer and waits for its result. */
  async function ask(to, verb, body, { parent = null, timeoutMs = INTENT_TTL_MS } = {}) {
    const intent = {
      id: `${identity.name}-${++counter}`,
      parent,
      from: identity.name,
      to,
      verb,
      body,
      expires: Date.now() + timeoutMs,
    };
    const peer = peers.get(to);
    const rule = allowedBy(policy.egress, "to", peer, verb);
    if (!peer || rule === null) {
      record({ ...describe(intent), decision: "deny", by: peer ? "policy:egress" : "unknown party" });
      throw new Error(peer ? `policy does not allow ${verb} to ${to}` : `unknown party ${to}`);
    }
    const started = Date.now();
    const signature = await identity.sign(intent);
    if (!peers.has(to)) {   // it may have left while the intent was being signed
      record({ ...describe(intent), decision: "allow", by: `policy:egress[${rule}]`, ms: 0, error: `${to} left` });
      throw new Error(`${to} left`);
    }
    const reply = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(intent.id);
        reject(new Error(`${to} did not answer in time`));
      }, timeoutMs);
      pending.set(intent.id, { resolve, reject, timer, to });
    });
    send(to, { type: "intent", intent, signature });
    try {
      const result = await reply;
      record({ ...describe(intent), decision: "allow", by: `policy:egress[${rule}]`, ms: Date.now() - started });
      return result;
    } catch (error) {
      record({ ...describe(intent), decision: "allow", by: `policy:egress[${rule}]`, ms: Date.now() - started, error: error.message });
      throw error;
    }
  }

  /** A signed message arrived from the transport. */
  async function receive(message) {
    if (message.type === "result") {
      const waiting = pending.get(message.result.id);
      const peer = waiting && peers.get(waiting.to);
      if (!waiting || message.result.from !== waiting.to) return;
      const trusted = peer?.key ? await verify(peer.key, message.signature, message.result) : policy.allowUnsigned;
      if (!trusted) return;   // a forged result is dropped; the ask times out
      clearTimeout(waiting.timer);
      pending.delete(message.result.id);
      if (message.result.ok) waiting.resolve(message.result.value);
      else waiting.reject(new Error(message.result.error));
      return;
    }
    if (message.type !== "intent") return;

    const { intent, signature } = message;
    const peer = peers.get(intent.from);
    const refuse = async (by) => {
      record({ ...describe(intent), decision: "deny", by });
      await answer(intent, { ok: false, error: `refused: ${by}` });
    };

    // Checks that need no policy come first: who is this, is it fresh.
    if (!peer) return refuse("unknown realm");
    if (intent.to !== identity.name) return refuse("not addressed to this realm");
    if (peer.key ? !(await verify(peer.key, signature, intent)) : !policy.allowUnsigned) return refuse("bad signature");
    if (!(intent.expires > Date.now())) return refuse("expired");
    if (seen.has(intent.id)) return refuse("replayed");
    seen.set(intent.id, intent.expires);
    for (const [id, expiry] of seen) if (expiry < Date.now()) seen.delete(id);

    const rule = allowedBy(policy.ingress, "from", peer, intent.verb);
    if (rule === null) return refuse("policy:ingress");
    const handler = serve[intent.verb];
    if (!handler) return refuse("no agent serves this verb");

    const started = Date.now();
    try {
      const value = await handler(intent.body, intent);
      record({ ...describe(intent), decision: "allow", by: `policy:ingress[${rule}]`, ms: Date.now() - started });
      await answer(intent, { ok: true, value });
    } catch (error) {
      record({ ...describe(intent), decision: "allow", by: `policy:ingress[${rule}]`, ms: Date.now() - started, error: String(error.message ?? error) });
      await answer(intent, { ok: false, error: String(error.message ?? error) });
    }
  }

  async function answer(intent, outcome) {
    const result = { id: intent.id, from: identity.name, ...outcome };
    send(intent.from, { type: "result", result, signature: await identity.sign(result) });
  }

  /**
   * Sends work to a pool: the peers a selector matches.
   * @param bodies   one body per piece of work
   * @param dispatch "all": each body to one member, spread across the pool
   *                 "any": every body to the same single member
   *                 "first": each body to every member; the first answer wins
   *                 "quorum": each body to `replicas` members; `need` equal answers win
   */
  async function askPool(selector, verb, bodies, { dispatch = "all", replicas = 3, need = 2, maxFanout = 64, parent = null } = {}) {
    const members = [...peers.values()].filter((p) => matches(selector, p)).map((p) => p.name);
    if (!members.length) throw new Error("the pool has no members");
    const copies = dispatch === "quorum" ? Math.min(replicas, members.length)
      : dispatch === "first" ? members.length : 1;
    if (bodies.length * copies > maxFanout) {
      record({ intent: null, parent, from: identity.name, to: "pool", verb, decision: "deny", by: `maxFanout ${maxFanout}` });
      throw new Error(`${bodies.length * copies} intents would exceed maxFanout ${maxFanout}`);
    }
    const offset = turn++;
    const sent = new Map();
    const pick = (i, k) => {
      const member = members[(offset + i + k) % members.length];
      sent.set(member, (sent.get(member) ?? 0) + 1);
      return member;
    };
    const dissent = new Map();

    const values = await Promise.all(bodies.map(async (body, i) => {
      if (dispatch === "any") return ask(pick(0, 0), verb, body, { parent });
      if (dispatch === "all") return ask(pick(i, 0), verb, body, { parent });
      const asked = Array.from({ length: copies }, (_, k) => pick(i, k));
      const attempts = asked.map((to) => ask(to, verb, body, { parent }).then((value) => ({ to, value })));
      if (dispatch === "first") return (await Promise.any(attempts)).value;

      const answers = (await Promise.allSettled(attempts)).filter((a) => a.status === "fulfilled").map((a) => a.value);
      const votes = new Map();
      for (const a of answers) votes.set(canonical(a.value), [...(votes.get(canonical(a.value)) ?? []), a]);
      const winner = [...votes.values()].sort((x, y) => y.length - x.length)[0];
      if (!winner || winner.length < Math.min(need, copies)) throw new Error(`no quorum for piece ${i + 1}`);
      for (const a of answers) {
        if (!winner.includes(a)) {
          dissent.set(a.to, (dissent.get(a.to) ?? 0) + 1);
          record({ intent: null, parent, from: a.to, to: identity.name, verb, decision: "flag", by: "quorum: answer differs from the majority" });
        }
      }
      return winner[0].value;
    }));
    return { values, members, sent: Object.fromEntries(sent), dissent: Object.fromEntries(dissent) };
  }

  return {
    identity,
    ledger,
    peers,
    ask,
    askPool,
    receive,
    /** The advert other realms see for this one. */
    advert: () => ({ name: identity.name, labels: identity.labels, key: identity.key, serves: Object.keys(serve) }),
    /** Replaces the list of known peers, as announced by the relay. */
    setPeers(adverts) {
      peers.clear();
      for (const a of adverts) if (a.name !== identity.name) peers.set(a.name, a);
      // A peer that left cannot answer: fail what was waiting on it now, not at the timeout.
      for (const [id, waiting] of pending) {
        if (!peers.has(waiting.to)) {
          clearTimeout(waiting.timer);
          pending.delete(id);
          waiting.reject(new Error(`${waiting.to} left`));
        }
      }
    },
  };
}
