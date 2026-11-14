#!/usr/bin/env python3
"""Static server for the examples that tells the browser not to cache.

`python3 -m http.server` sends no cache headers, so after a rebuild a browser
can keep showing the old page. Usage: tools/serve.py [port]  (default 8765)
"""
import sys
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


class NoCacheHandler(SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8765
    root = Path(__file__).resolve().parent.parent
    handler = partial(NoCacheHandler, directory=str(root))
    print(f"serving {root} at http://127.0.0.1:{port} (no caching)")
    ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()
