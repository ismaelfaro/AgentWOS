// A worker realm: one kernel that joins a relay and serves one WASM agent.
// The page uses it, and so does the test, which runs the same code in Node.
import { createIdentity, createRealm } from "../../../../web/kernel/realm.js";

/** What a worker realm lets in and out. Nothing else is allowed. */
export const WORKER_POLICY = {
  ingress: [{ from: { name: "lead" }, verbs: ["primes.count"] }],
  egress: [],
};

/**
 * @param url      WebSocket URL of the relay
 * @param token    room token from the lead's terminal
 * @param worker   the `tool` interface of the worker component
 * @param faulty   () => boolean; when true this realm returns wrong answers
 */
export async function startTabRealm({ url, token, name, worker, faulty = () => false, onLedger, onPeers, onStatus = () => {} }) {
  const identity = await createIdentity(name, { role: "worker" });
  const socket = new WebSocket(url);
  let realm;

  const serve = {
    "primes.count": (body) => {
      const result = JSON.parse(worker.invoke({ name: "primes.count", argsJson: JSON.stringify(body) }));
      // A deliberately wrong node, to show what quorum is for.
      return faulty() ? { ...result, count: result.count + 1 } : result;
    },
  };

  realm = createRealm({
    identity,
    policy: WORKER_POLICY,
    serve,
    send: (to, message) => socket.send(JSON.stringify({ type: "relay", to, message })),
    onLedger,
  });

  socket.addEventListener("message", async (event) => {
    const data = JSON.parse(event.data);
    if (data.type === "challenge") {
      const proof = await identity.sign({ nonce: data.nonce });
      socket.send(JSON.stringify({ type: "join", token, advert: realm.advert(), proof }));
    } else if (data.type === "welcome") {
      onStatus("connected");
    } else if (data.type === "refused") {
      onStatus("refused: " + data.reason);
    } else if (data.type === "roster") {
      realm.setPeers(data.peers);
      onPeers?.(data.peers);
    } else if (data.type === "relay") {
      realm.receive(data.message);
    }
  });
  socket.addEventListener("close", () => onStatus("disconnected"));
  socket.addEventListener("error", () => onStatus("cannot reach the relay"));

  return { realm, identity, close: () => socket.close() };
}
