// The relay: lets realms that cannot accept connections (browser tabs) reach
// each other and the local realm. It admits realms, announces who is present,
// and forwards messages. It does not read or decide on intents; each realm
// checks signatures and its own policy.
import { randomUUID } from "node:crypto";
import { WebSocketServer } from "ws";
import { verify } from "../../../web/kernel/realm.js";

/**
 * @param server         a node:http server to attach to, at path /relay
 * @param token          realms must present it to join
 * @param allowUnsigned  admit realms with no key (pages on plain http off localhost)
 * @param local          { advert, receive(message) } for the realm in this process
 */
export function createRelay({ server, token, allowUnsigned, local, onRoster = () => {} }) {
  const sockets = new Map();   // name -> { socket, advert }
  const wss = new WebSocketServer({ server, path: "/relay" });

  const roster = () => [local.advert, ...[...sockets.values()].map((s) => s.advert)];
  function announce() {
    const peers = roster();
    for (const { socket } of sockets.values()) socket.send(JSON.stringify({ type: "roster", peers }));
    onRoster(peers);
  }

  function deliver(from, to, message) {
    // The relay cannot vouch for content, but it can refuse a realm speaking as another.
    const claimed = message.intent?.from ?? message.result?.from;
    if (claimed !== from) return;
    if (to === local.advert.name) return local.receive(message);
    sockets.get(to)?.socket.send(JSON.stringify({ type: "relay", message }));
  }

  wss.on("connection", (socket) => {
    const nonce = randomUUID();
    let name = null;
    socket.send(JSON.stringify({ type: "challenge", nonce }));

    socket.on("message", async (raw) => {
      let data;
      try { data = JSON.parse(raw); } catch { return; }

      if (data.type === "join" && !name) {
        const advert = data.advert ?? {};
        const refuse = (reason) => { socket.send(JSON.stringify({ type: "refused", reason })); socket.close(); };
        if (data.token !== token) return refuse("wrong token");
        if (typeof advert.name !== "string" || !/^[a-z0-9-]{1,24}$/.test(advert.name)) return refuse("invalid name");
        if (advert.name === local.advert.name || sockets.has(advert.name)) return refuse("name already taken");
        // A realm must prove it holds the key it advertises.
        if (advert.key ? !(await verify(advert.key, data.proof, { nonce })) : !allowUnsigned) {
          return refuse(advert.key ? "key proof failed" : "unsigned realms are not admitted");
        }
        name = advert.name;
        sockets.set(name, { socket, advert: { name, labels: advert.labels ?? {}, key: advert.key ?? null, serves: advert.serves ?? [] } });
        socket.send(JSON.stringify({ type: "welcome", name }));
        announce();
      } else if (data.type === "relay" && name) {
        deliver(name, data.to, data.message);
      }
    });

    socket.on("close", () => {
      if (name && sockets.delete(name)) announce();
    });
  });

  return {
    roster,
    /** Sends a message from the local realm. */
    send: (to, message) => deliver(local.advert.name, to, message),
    close: () => wss.close(),
  };
}
