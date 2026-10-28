#!/usr/bin/env python3
"""Exercise grammar-free LSP execution against a disposable local HTTP server."""
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote, urlparse

from support.lsp import Lsp

ROOT = Path(__file__).resolve().parents[1]


def main():
    received = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            received.append({'path': self.path, 'token': self.headers.get('X-API-Key')})
            if self.path == '/slow':
                time.sleep(2)
            self.send_response(422 if self.path == '/invalid' else 200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            try:
                self.wfile.write(json.dumps({'token': 'response-secret', 'path': self.path}).encode())
            except (BrokenPipeError, ConnectionResetError):
                pass

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    executable = (Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'bin/sendline').resolve()
    try:
        with tempfile.TemporaryDirectory(prefix='sendline-lsp execution ') as temporary:
            directory = Path(temporary)
            project = directory / 'project'
            project.mkdir()
            home = directory / 'extension work/state'
            env = {**os.environ, 'SENDLINE_HOME': str(home)}
            (project / 'http-client.env.json').write_text(json.dumps({
                'local': {'baseUrl': f'http://127.0.0.1:{server.server_port}', 'token': 'request-secret'},
                'preview': {'baseUrl': f'http://127.0.0.1:{server.server_port}', 'token': 'preview-secret'},
            }))
            path = project / "requests $(touch MUST_NOT_EXIST) 'quoted'.http"
            path.write_text('GET {{baseUrl}}/on-disk-must-not-run')
            lsp = Lsp(executable, env)
            lsp.request('initialize', {'capabilities': {
                'window': {'showDocument': {'support': True}},
                'workspace': {'codeLens': {'refreshSupport': True}},
            }})
            lsp.notify('initialized', {})

            def text(endpoint):
                return f'### Editor request\nGET {{{{baseUrl}}}}{endpoint}\nX-API-Key: {{{{token}}}}'

            def sends():
                return [lens['command'] for lens in lsp.lenses(path)
                        if lens['command']['command'] == 'sendline.sendRequest']

            def finished(expected):
                deadline = time.monotonic() + 10
                while time.monotonic() < deadline:
                    lenses = lsp.lenses(path)
                    if any(item['command']['command'] == 'sendline.openResult'
                           and expected in item['command']['title'] for item in lenses):
                        return lenses
                    time.sleep(.02)
                raise AssertionError(f'No completed request control: {expected}')

            def console_path():
                request = lsp.server_requests[-1]
                assert request['params']['external'] is False
                assert request['params']['takeFocus'] is False, 'Sending stole editor focus'
                return Path(unquote(urlparse(request['params']['uri']).path))

            try:
                source = text('/unsaved?token=query-secret') + '\n\n> {% client.log(response.body.token); client.test("200", () => client.assert(response.status === 200)); %}'
                lsp.open(path, source)
                assert any(item['command'] == 'sendline.selectEnvironment' for item in
                           lsp.request('textDocument/codeAction', {'textDocument': {'uri': path.as_uri()},
                           'range': {'start': {'line': 1, 'character': 0}, 'end': {'line': 1, 'character': 0}},
                           'context': {'diagnostics': []}}))
                assert not received, 'Opening or validating a document sent a request'
                old = sends()[0]
                lsp.change(path, source + '\n')
                assert 'changed' in lsp.execute(old, error=True)['message']
                assert not received, 'A stale CodeLens sent an edited request'
                lsp.execute(sends()[0])
                lenses = finished('200 OK')
                assert received == [{'path': '/unsaved?token=query-secret', 'token': 'request-secret'}]
                assert 'on-disk-must-not-run' not in str(received), 'Execution ignored the editor snapshot'
                console = console_path()
                assert console.is_relative_to(home)
                contents = console.read_text()
                assert '200 OK' in contents and '1 passed' in contents and 'Open response' in contents
                assert all(secret not in contents for secret in ['query-secret', 'request-secret', 'response-secret'])
                assert console.stat().st_mode & 0o277 == 0, 'Console is writable or public'
                assert not (home / 'runners').exists(), 'LSP created a global task launcher'
                response = next(item['command'] for item in lenses if item['command']['title'] == 'Response')
                lsp.execute(response)
                opened = lsp.server_requests[-1]['params']
                assert opened['takeFocus'] is True, 'Explicit Open Response did not request focus'
                response_file = Path(unquote(urlparse(opened['uri']).path))
                assert response_file.is_relative_to(home) and json.loads(response_file.read_text())['token'] == 'response-secret'
                assert response_file.stat().st_mode & 0o077 == 0
                lsp.execute({'command': 'sendline.openResult', 'arguments': [path.as_uri(), 'file:///etc/passwd']}, error=True)

                # A client that refuses the console must not trigger a hidden HTTP request.
                before = len(received)
                lsp.change(path, text('/not-sent'))
                lsp.open_document_success = False
                lsp.execute(sends()[0], error=True)
                assert len(received) == before
                lsp.open_document_success = True

                # HTTP and assertion failures preserve response artifacts and metadata.
                lsp.change(path, text('/invalid'))
                lsp.execute(sends()[0])
                failure = finished('422')
                assert any(item['command']['title'] == 'Response' for item in failure)
                assert 'request or its response tests failed' in console_path().read_text()
                lsp.change(path, text('/test-failure') + '\n\n> {% client.test("fails", () => client.assert(false, response.body.token)); %}')
                lsp.execute(sends()[0])
                finished('200 OK')
                assert '1 failed' in console_path().read_text() and 'response-secret' not in console_path().read_text()

                # Cancellation is responsive and a duplicate click cannot send another request.
                lsp.change(path, text('/slow'))
                button = sends()[0]
                lsp.execute(button)
                assert 'running' in lsp.execute(button, error=True)['message']
                cancel = next(item['command'] for item in lsp.lenses(path) if item['command']['command'] == 'sendline.cancelRequest')
                started = time.monotonic()
                lsp.execute(cancel)
                finished('Cancelled')
                assert time.monotonic() - started < 1.5

                # Closing/reopening a document cannot let its old run overwrite a new one.
                lsp.execute(sends()[0])
                lsp.notify('textDocument/didClose', {'textDocument': {'uri': path.as_uri()}})
                lsp.versions.pop(path.as_uri())
                lsp.open(path, text('/reopened'))
                lsp.execute(sends()[0])
                finished('200 OK')
                assert '/reopened' in console_path().read_text()

                # Invalid scripts fail before sending, without executing the response handler.
                before = len(received)
                lsp.change(path, text('/bad-script') + '\n\n> {% function( { %}')
                lsp.execute(sends()[0], error=True)
                assert len(received) == before
                assert not (directory / 'MUST_NOT_EXIST').exists()
            finally:
                lsp.close()

            unsupported = Lsp(executable, env)
            try:
                unsupported.request('initialize', {'capabilities': {}})
                unsupported.notify('initialized', {})
                lenses = unsupported.open(path, text('/unsupported'))
                command = next(lens['command'] for lens in lenses if lens['command']['command'] == 'sendline.sendRequest')
                assert 'does not support' in unsupported.execute(command, error=True)['message']
            finally:
                unsupported.close()
        print('PASS: grammar-free LSP send, unsaved snapshots, stale-action rejection, private consoles, explicit response opening, HTTP/test failures, cancellation, duplicate prevention and close/reopen lifecycle')
    finally:
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    main()
