// The page of one worker realm. Everything below runs in this tab: the kernel,
// the worker agent (a WASM component) and this realm's ledger.
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import { coreModuleLoader, wasiImports } from "../../../../web/kernel/kernel.js";
import { fingerprint } from "../../../../web/kernel/realm.js";
import { instantiate as instantiateWorker } from "../../gen/worker/worker.js";
import { startTabRealm } from "./tab-realm.js";

const $ = (id) => document.getElementById(id);
const MAX_LEDGER_ROWS = 60;

function el(tag, className, children = []) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  for (const c of [].concat(children)) n.append(c);
  return n;
}

function setStatus(text) {
  $("status").textContent = text;
  $("status").className = "pill " + (text === "connected" ? "ok" : text === "starting" ? "" : "bad");
}

/** A name people can tell apart: the browser, plus a few random characters. */
function realmName() {
  const ua = navigator.userAgent;
  const browser = /Firefox/.test(ua) ? "firefox" : /Edg\//.test(ua) ? "edge" : /Chrome/.test(ua) ? "chrome" : /Safari/.test(ua) ? "safari" : "tab";
  return `${browser}-${Math.random().toString(36).slice(2, 6)}`;
}

const stats = { pieces: 0, numbers: 0, primes: 0 };
function showWork(entry, body, result) {
  stats.pieces += 1;
  stats.numbers += body.to - body.from;
  stats.primes += result.count;
  $("pieces").textContent = stats.pieces.toLocaleString();
  $("numbers").textContent = stats.numbers.toLocaleString();
  $("primes").textContent = stats.primes.toLocaleString();
  $("last").textContent = `Last piece: ${body.from.toLocaleString()} to ${body.to.toLocaleString()}, ${result.count.toLocaleString()} primes.`;
  const card = $("last").parentElement;
  card.classList.remove("working");
  void card.offsetWidth;
  card.classList.add("working");
}

function showLedger(entry) {
  const list = $("ledger");
  if (list.firstElementChild?.classList.contains("muted")) list.replaceChildren();
  const detail = `${entry.from} -> ${entry.to}  ${entry.verb}  (${entry.by})` +
    (entry.ms !== undefined ? `  ${entry.ms} ms` : "") + (entry.error ? `  error: ${entry.error}` : "");
  list.prepend(el("li", "", [el("span", "tag " + entry.decision, entry.decision), entry.at.slice(11, 19) + "  " + detail]));
  while (list.children.length > MAX_LEDGER_ROWS) list.lastElementChild.remove();
}

function showPeers(peers, me) {
  $("peers").replaceChildren(...peers.map((p) =>
    el("li", p.name === me ? "me" : "", `${p.name}${p.name === me ? " (this tab)" : ""}  ·  ${p.labels?.role ?? "?"}  ·  key ${fingerprint(p.key)}`)));
}

async function main() {
  const assets = new URL(".", import.meta.url);
  const worker = (await instantiateWorker(coreModuleLoader(new URL("wasm/worker/", assets)), wasiImports({ cli, clocks, io, random }))).tool;

  // Wrap the agent so the page can show each piece of work as it happens.
  const observed = {
    invoke(call) {
      const text = worker.invoke(call);
      showWork(null, JSON.parse(call.argsJson), JSON.parse(text));
      return text;
    },
  };

  const token = new URLSearchParams(location.search).get("token") ?? "";
  const name = realmName();
  const { realm, identity } = await startTabRealm({
    url: `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/relay`,
    token,
    name,
    worker: observed,
    faulty: () => $("faulty").checked,
    onLedger: showLedger,
    onPeers: (peers) => showPeers(peers, name),
    onStatus: setStatus,
  });
  $("name").textContent = name;
  $("key").textContent = identity.key
    ? `${fingerprint(identity.key)}… (Ed25519, made in this tab)`
    : "none: this page is not on localhost or https, so it cannot sign";

  // This realm's policy lets nothing out. The button shows the kernel refusing.
  $("reach").addEventListener("click", () => {
    realm.ask("lead", "files.read", { path: "/etc/passwd" }).catch(() => {});
  });
}

main().catch((e) => {
  setStatus("error");
  $("last").textContent = String(e.message ?? e);
  console.error(e);
});
