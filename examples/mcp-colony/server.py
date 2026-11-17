#!/usr/bin/env python3
"""A small, honest MCP server: the colony's reference desk. Current protocol
(2026-07-28), Streamable HTTP, standard library only.

    server.py [--port 8801]

Tools:  lookup(topic)  convert(value, from, to)
"""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

VERSION = "2026-07-28"
FACTS = {
    "gravity": "Mars surface gravity is 3.71 m/s^2, about 38% of Earth's.",
    "day": "A sol, the Martian day, lasts 24 h 39 min 35 s.",
    "pressure": "Surface pressure is about 600 Pa, under 1% of Earth's.",
    "distance": "Earth is 55 to 400 million km away, depending on the orbits.",
}
UNITS = {("km", "mi"): 0.621371, ("mi", "km"): 1.609344, ("kg", "lb"): 2.204623, ("lb", "kg"): 0.453592,
         ("c", "f"): None, ("f", "c"): None}

TOOLS = [
    {"name": "lookup", "description": "Looks up a fact about Mars: gravity, day, pressure or distance.",
     "inputSchema": {"type": "object", "properties": {"topic": {"type": "string"}}, "required": ["topic"]}},
    {"name": "convert", "description": "Converts km/mi, kg/lb or c/f.",
     "inputSchema": {"type": "object", "properties": {"value": {"type": "number"}, "from": {"type": "string"},
                                                       "to": {"type": "string"}}, "required": ["value", "from", "to"]}},
]


def call(name, args):
    if name == "lookup":
        topic = str(args.get("topic") or args.get("input") or "").strip().lower()
        return FACTS.get(topic, "No fact for '%s'. Topics: %s." % (topic, ", ".join(FACTS)))
    if name == "convert":
        v, a, b = float(args["value"]), str(args["from"]).lower(), str(args["to"]).lower()
        if (a, b) == ("c", "f"):
            return "%.1f f" % (v * 9 / 5 + 32)
        if (a, b) == ("f", "c"):
            return "%.1f c" % ((v - 32) * 5 / 9)
        k = UNITS.get((a, b))
        return "%.2f %s" % (v * k, b) if k else "Cannot convert %s to %s." % (a, b)
    raise KeyError(name)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def reply(self, status, payload=None):
        body = b"" if payload is None else json.dumps(payload).encode()
        self.send_response(status)
        if payload is not None:
            self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        if "chunked" in self.headers.get("Transfer-Encoding", "").lower():
            raw = b""
            while True:
                size = int(self.rfile.readline().split(b";")[0].strip() or b"0", 16)
                if size == 0:
                    self.rfile.readline()
                    break
                raw += self.rfile.read(size)
                self.rfile.readline()
        else:
            raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        msg = json.loads(raw or b"{}")
        method, rid, params = msg.get("method"), msg.get("id"), msg.get("params") or {}
        ok = lambda r: self.reply(200, {"jsonrpc": "2.0", "id": rid, "result": r})
        err = lambda c, m: self.reply(200, {"jsonrpc": "2.0", "id": rid, "error": {"code": c, "message": m}})
        if method == "server/discover":
            return ok({"supportedVersions": [VERSION], "capabilities": {"tools": {}},
                       "serverInfo": {"name": "colony-desk", "version": "1"}})
        if (params.get("_meta") or {}).get("io.modelcontextprotocol/protocolVersion") != VERSION:
            return self.reply(400, {"error": "missing or unsupported protocol version"})
        if method == "tools/list":
            return ok({"tools": TOOLS, "resultType": "complete"})
        if method == "tools/call":
            try:
                text = call(params.get("name"), params.get("arguments") or {})
            except KeyError:
                return err(-32602, "Unknown tool")
            except (KeyError, ValueError, TypeError) as e:
                return ok({"content": [{"type": "text", "text": "bad arguments: %s" % e}], "isError": True, "resultType": "complete"})
            return ok({"content": [{"type": "text", "text": text}], "isError": False, "resultType": "complete"})
        return err(-32601, "Method not found")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8801)
    args = ap.parse_args()
    print("colony desk on http://127.0.0.1:%d/mcp" % args.port, flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
