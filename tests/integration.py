#!/usr/bin/env python3
"""Check the real header selection -> native client -> HTTP -> saved response flow."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from support.lsp import Lsp

ROOT = Path(__file__).resolve().parents[1]



def main():
    received = []

    class Handler(BaseHTTPRequestHandler):
        def execute(self):
            body = self.rfile.read(int(self.headers.get('Content-Length', 0))).decode()
            received.append((self.command, self.path, dict(self.headers), body))
            if self.path == '/redirect':
                self.send_response(302)
                self.send_header('Location', '/destination?token=redirect-secret')
                self.end_headers()
                return
            if self.path == '/cross-origin':
                self.send_response(302)
                self.send_header('Location', f'http://localhost:{server.server_port}/must-not-receive-api-key')
                self.end_headers()
                return
            if self.path == '/slow':
                time.sleep(3)
            self.send_response(422 if self.path == '/invalid' else 200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('X-Response-Test', 'yes')
            self.end_headers()
            if self.command != 'HEAD':
                self.wfile.write(json.dumps({'method': self.command, 'body': body, 'unicode': 'नमस्ते',
                                            'items': [{'id': 1}], 'token': 'handler-secret'}).encode())

        do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = do_OPTIONS = execute

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    executable = (Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'bin/sendline').resolve()
    lsp = None
    try:
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            environment = {**os.environ, 'SENDLINE_HOME': str(directory / 'private-cache')}
            project = directory / 'project'
            project.mkdir()
            (project / 'http-client.env.json').write_text(json.dumps({
                '$shared': {'host': f'http://127.0.0.1:{server.server_port}'},
                'local': {'token': 'public'}, 'staging': {'token': 'stage'}
            }))
            (project / 'http-client.private.env.json').write_text(json.dumps({'local': {'token': 'test-secret'}}))
            path = project / "requests $(touch SHOULD_NOT_EXIST) 'quoted'.http"
            methods = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']
            text = '@baseUrl = {{host}}\n' + '\n'.join(
                f'### {method}\n{method} {{{{baseUrl}}}}/test\nAuthorization: Bearer {{{{token}}}}\nContent-Type: application/json\n\n'
                + ('{"name":"नमस्ते"}\n' if method in ('POST', 'PUT', 'PATCH') else '')
                for method in methods)
            path.write_text(text)

            def start_lsp():
                instance = Lsp(executable, environment)
                instance.request('initialize', {'processId': os.getpid(), 'rootUri': None,
                    'capabilities': {'workspace': {'codeLens': {'refreshSupport': True}}}})
                instance.notify('initialized', {})
                return instance

            lsp = start_lsp()
            lenses = lsp.open(path, text)
            assert len(lenses) == 1 + len(methods) and 'Environment: local' in lenses[0]['command']['title']
            actions = lsp.request('textDocument/codeAction', {'textDocument': {'uri': path.as_uri()},
                'range': {'start': {'line': 0, 'character': 0}, 'end': {'line': 0, 'character': 0}},
                'context': {'diagnostics': []}})
            assert {action['arguments'][1] for action in actions if action['command'] == 'sendline.selectEnvironment'} == {'local', 'staging'}, 'Environments must be usable with CodeLens disabled'

            assert lenses[0]['range']['start'] == {'line': 0, 'character': 1}, lenses
            lsp.execute(lenses[0]['command'])
            choices = lsp.lenses(path)
            assert len(choices) == 3 + len(methods)
            staging = next(lens['command'] for lens in choices if lens['command']['title'] == 'staging')
            lsp.execute(staging)
            assert 'Environment: staging' in lsp.lenses(path)[0]['command']['title']
            other = project / 'other.http'
            assert 'Environment: staging' in lsp.open(other, 'GET {{host}}/test')[0]['command']['title']
            # Selecting an environment never sends a request or opens a distant dialog.
            assert not received
            lsp.close()
            lsp = start_lsp()
            lenses = lsp.open(path, text)
            assert 'Environment: staging' in lenses[0]['command']['title'], 'Selection lost across restart'

            def run(file, line=1, success=True):
                result = subprocess.run([str(executable), 'run', '--file', str(file), '--line', str(line)],
                    env=environment, capture_output=True, text=True, timeout=15)
                assert (result.returncode == 0) == success, result.stdout + result.stderr
                return result.stdout

            output = run(path, 3)
            assert received[-1][2]['authorization'] == 'Bearer stage'
            lsp.execute(lenses[0]['command'])
            local = next(lens['command'] for lens in lsp.lenses(path) if lens['command']['title'] == 'local')
            lsp.execute(local)
            outputs = []
            for method in methods:
                row = next(index + 1 for index, line in enumerate(text.splitlines()) if line.startswith(method + ' '))
                output = run(path, row)
                outputs.append(output)
                actual_method, actual_path, headers, body = received[-1]
                assert actual_method == method and actual_path == '/test'
                assert headers['authorization'] == 'Bearer test-secret'
                if method in ('POST', 'PUT', 'PATCH'):
                    assert json.loads(body) == {'name': 'नमस्ते'}
                assert '200 OK' in output and 'ms' in output and 'application/json' in output
                assert 'test-secret' not in output and 'नमस्ते' not in output
                response_path = Path(next(line.removeprefix('Response  ') for line in output.splitlines() if line.startswith('Response  ')))
                header_path = Path(next(line.removeprefix('Headers   ') for line in output.splitlines() if line.startswith('Headers   ')))
                assert response_path.exists() and header_path.exists()
                assert response_path.stat().st_mode & 0o077 == 0
                assert response_path.parent.stat().st_mode & 0o077 == 0
                assert 'x-response-test: yes' in header_path.read_text()
                if method != 'HEAD':
                    assert json.loads(response_path.read_text())['method'] == method

            for suffix, expected, success in [('/invalid', '422 Unprocessable Entity', False),
                                               ('/test?token=query-secret', '200 OK', True),
                                               ('/{{missing}}', 'Undefined variable: missing', False)]:
                other.write_text(f'GET {{{{host}}}}{suffix}')
                result = subprocess.run([str(executable), 'run', '--file', str(other), '--line', '1'],
                    env=environment, capture_output=True, text=True, timeout=15)
                assert (result.returncode == 0) == success
                assert expected in result.stdout + result.stderr
                assert 'query-secret' not in result.stdout
            assert len(received) == 10, 'Unexpected request or unresolved variables were sent'
            lsp.change(path, 'GET http://localhost\ninvalid header')
            assert 'Environment: local' in lsp.lenses(path)[0]['command']['title']

            # Real JetBrains response handlers execute locally, never as an HTTP body.
            real_layout = ('@tenant = test\n### List\nGET {{host}}/api/{{tenant}}\n'
                           '    ?use_cache=false\n    &limit=100\nX-API-KEY: {{token}}\n\n'
                           '> {%\nclient.test("List All Client Catalogs", function () {\n'
                           'client.assert(response.status === 200, "Response status is not 200");\n'
                           'client.global.set("clientCatalogs", response.body["items"]);\n'
                           'client.global.set("savedToken", response.body.token);\n'
                           '});\nclient.log(response.body.token);\n%}\n')
            other.write_text(real_layout)
            lenses = lsp.open(other, real_layout)
            assert 'Environment: local' in lenses[0]['command']['title']
            before = len(received)
            lsp.execute(lenses[0]['command'])
            choices = lsp.lenses(other)
            staging = next(item['command'] for item in choices if item['command']['title'] == 'staging')
            lsp.execute(staging)
            assert 'Environment: staging' in lsp.lenses(other)[0]['command']['title']
            dependent = project / 'dependent.http'
            dependent.write_text('GET {{host}}/items/{{clientCatalogs[0].id}}')
            lsp.open(dependent, dependent.read_text())
            output = run(other)
            lsp.wait_diagnostics(dependent, lambda diagnostics: not diagnostics)
            assert len(received) == before + 1
            assert received[-1][1] == '/api/test?use_cache=false&limit=100' and received[-1][3] == ''
            assert '1 passed · 0 failed' in output
            assert 'handler-secret' not in output
            report_path = Path(next(line.removeprefix('Report    ') for line in output.splitlines() if line.startswith('Report    ')))
            report = json.loads(report_path.read_text())
            assert report['tests'][0]['name'] == 'List All Client Catalogs' and report['tests'][0]['passed']
            assert report['logs'] == ['handler-secret']
            assert report_path.stat().st_mode & 0o077 == 0

            # New CLI processes reuse structured globals, without leaking across environments.
            other.write_text('POST {{host}}/chained\nAuthorization: Bearer {{savedToken}}\nContent-Type: application/json\n\n{{clientCatalogs}}\n\n> {% client.test("sent", () => client.assert(response.status === 200)); %}')
            output = run(other)
            assert received[-1][2]['authorization'] == 'Bearer handler-secret'
            assert json.loads(received[-1][3]) == [{'id': 1}], 'Handler text was sent as a body'
            assert 'handler-secret' not in output
            other.write_text('GET {{host}}/items/{{clientCatalogs[0]["id"]}}?\n    use_cache=false\nAuthorization: Bearer {{savedToken}}')
            run(other)
            assert received[-1][1] == '/items/1?use_cache=false'
            lsp.execute(lsp.lenses(other)[0]['command'])
            local = next(item['command'] for item in lsp.lenses(other) if item['command']['title'] == 'local')
            lsp.execute(local)
            before = len(received)
            run(other, success=False)
            assert len(received) == before, 'A staging response variable leaked into local'

            # Script syntax errors are diagnosed at the handler and block sending.
            broken = 'GET {{host}}/test\n\n> {% client.test("bad", function( { %}'
            other.write_text(broken)
            lsp.change(other, broken)
            assert lsp.lenses(other), 'Script errors hid environment controls'
            diagnostics = lsp.wait_diagnostics(other, lambda values: bool(values))
            assert diagnostics[0]['range']['start']['line'] == 2
            run(other, success=False)
            assert len(received) == before

            # Failed assertions and runtime errors preserve the HTTP response and private report.
            for source, failed, error in [
                ('client.test("fails", () => client.assert(false, response.body.token)); client.test("next", () => client.assert(true));', 1, False),
                ('throw new Error(response.body.token);', 0, True),
                ('while (true) {}', 0, True),
            ]:
                other.write_text('GET {{host}}/test\n\n> {% ' + source + ' %}')
                output = run(other, success=False)
                assert '200 OK' in output and 'handler-secret' not in output
                response_path = Path(next(line.removeprefix('Response  ') for line in output.splitlines() if line.startswith('Response  ')))
                report_path = Path(next(line.removeprefix('Report    ') for line in output.splitlines() if line.startswith('Report    ')))
                report = json.loads(report_path.read_text())
                assert response_path.exists()
                assert sum(not test['passed'] for test in report['tests']) == failed
                assert bool(report['error']) == error

            # The supported multiline URL reaches the server with all query parameters.
            other.write_text(real_layout.split('\n> {%')[0])
            run(other)
            assert received[-1][1] == '/api/test?use_cache=false&limit=100'
            assert received[-1][2]['x-api-key'] == 'test-secret'

            # The preview must use the already-built request, including dynamic values.
            other.write_text('### Broken\nGET {{host}}/must-not-send\ninvalid header\n### Valid\nGET {{host}}/independent')
            before = len(received)
            run(other, line=5)
            assert len(received) == before + 1 and received[-1][1] == '/independent'
            run(other, line=2, success=False)
            assert len(received) == before + 1, 'A malformed request sent or changed the selected endpoint'

            other.write_text('GET {{host}}/id/{{$uuid}}')
            output = run(other)
            assert received[-1][1] in output, 'Displayed URL differs from the request sent'
            other.write_text('GET {{host}}/redirect')
            output = run(other)
            assert 'Final URL' in output and '/destination?token=' in output
            assert 'redirect-secret' not in output

            # Custom API keys must never follow a redirect to another origin.
            other.write_text('GET {{host}}/cross-origin\nX-API-KEY: {{token}}')
            before = len(received)
            output = run(other, success=False)
            assert '302 Found' in output
            assert len(received) == before + 1, 'Cross-origin redirect forwarded credentials'

            # Structured environment values support the same paths as response globals.
            values = json.loads((project / 'http-client.env.json').read_text())
            values['local']['account'] = {'ids': [42]}
            (project / 'http-client.env.json').write_text(json.dumps(values))
            other.write_text('GET {{host}}/accounts/{{account.ids[0]}}')
            run(other)
            assert received[-1][1] == '/accounts/42'

            # Removing an environment requires a new choice. Never silently switch servers.
            (project / 'http-client.env.json').write_text(json.dumps({'staging': {'host': f'http://127.0.0.1:{server.server_port}'}}))
            (project / 'http-client.private.env.json').write_text('{}')
            other.write_text('GET {{host}}/test')
            lenses = lsp.open(other, other.read_text())
            assert 'Environment: Select' in lenses[0]['command']['title']
            before = len(received)
            output = run(other, success=False)
            assert len(received) == before
            lsp.execute(lenses[0]['command'])
            staging = next(item['command'] for item in lsp.lenses(other) if item['command']['title'] == 'staging')
            lsp.execute(staging)

            # Invalid variables are diagnosed on their request, before sending.
            lsp.change(other, '### Broken\nGET {{host}}/{{missing}}')
            lsp.lenses(other)
            diagnostics = lsp.wait_diagnostics(other, lambda values: bool(values))
            assert diagnostics[0]['range']['start']['line'] == 1
            assert 'Undefined variable: missing' in diagnostics[0]['message']

            # Rapid edits and a delayed old notification must not restore stale text.
            lsp.change(other, 'GET {{host}}/{{olderMissing}}')
            old_version = lsp.versions[other.as_uri()]
            lsp.change(other, 'GET {{host}}/{{latestMissing}}')
            lsp.notify('textDocument/didChange', {'textDocument': {'uri': other.as_uri(), 'version': old_version},
                'contentChanges': [{'text': 'GET {{host}}/stale-text'}]})
            diagnostics = lsp.wait_diagnostics(other, lambda values: any('latestMissing' in value['message'] for value in values))
            assert all('olderMissing' not in value['message'] for value in diagnostics)

            # Ctrl-C cancels the real request future and leaves no partial response files.
            other.write_text('GET {{host}}/slow')
            saved = set((directory / 'private-cache/responses').iterdir())
            slow = subprocess.Popen([str(executable), 'run', '--file', str(other)],
                                    env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            deadline = time.monotonic() + 5
            while received[-1][1] != '/slow' and time.monotonic() < deadline:
                time.sleep(.02)
            assert received[-1][1] == '/slow'
            slow.send_signal(signal.SIGINT)
            stdout, stderr = slow.communicate(timeout=3)
            assert 'Cancelled.' in stdout.decode(), stdout + stderr
            assert set((directory / 'private-cache/responses').iterdir()) == saved
            print('PASS: CLI and LSP, safe filenames, 7 HTTP methods, private responses, Unicode, HTTP errors, URL redaction, exact dynamic preview, redirects, cancellation, request diagnostics, inline environments, stale-selection recovery, JavaScript handlers, assertions, private reports, bounded execution and environment-scoped variable chaining')
    finally:
        if lsp:
            lsp.close()
        server.shutdown()
        server.server_close()


if __name__ == '__main__':
    main()
