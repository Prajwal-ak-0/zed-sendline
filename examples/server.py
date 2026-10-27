#!/usr/bin/env python3
"""Loopback-only server for trying examples/requests.http."""
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Handler(BaseHTTPRequestHandler):
    def handle_request(self):
        raw = self.rfile.read(int(self.headers.get('Content-Length', 0)))
        body = json.loads(raw) if raw else None
        payload = json.dumps({'method': self.command, 'path': self.path, 'body': body,
                              'user': {'id': 1, 'name': 'Test User'}}, ensure_ascii=False).encode()
        self.send_response(201 if self.command == 'POST' else 200)
        self.send_header('Content-Type', 'application/json; charset=utf-8')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        if self.command != 'HEAD':
            self.wfile.write(payload)

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = do_OPTIONS = handle_request

    def log_message(self, *_):
        pass


if __name__ == '__main__':
    print('Demo API listening on http://127.0.0.1:8787', flush=True)
    ThreadingHTTPServer(('127.0.0.1', 8787), Handler).serve_forever()
