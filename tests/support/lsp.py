"""A real stdio LSP client with version-aware notification waits."""
import json
from pathlib import Path
import queue
import re
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[2]


class Lsp:
    def __init__(self, executable, environment):
        version = re.search(r'^version = "([^"]+)"', (ROOT / 'extension.toml').read_text(), re.M)[1]
        self.process = subprocess.Popen([str(executable), 'lsp', '--extension-version', version], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=environment)
        self.messages = queue.Queue()
        self.notifications = []
        self.server_requests = []
        self.open_document_success = True
        self.next_id = 0
        self.versions = {}
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        try:
            while True:
                headers = {}
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        raise EOFError('Language server exited')
                    if line == b'\r\n':
                        break
                    name, value = line.decode().split(':', 1)
                    headers[name.lower()] = value.strip()
                self.messages.put(json.loads(self.process.stdout.read(int(headers['content-length']))))
        except Exception as error:
            self.messages.put(error)

    def send(self, value):
        body = json.dumps({'jsonrpc': '2.0', **value}).encode()
        self.process.stdin.write(f'Content-Length: {len(body)}\r\n\r\n'.encode() + body)
        self.process.stdin.flush()

    def notify(self, method, params):
        self.send({'method': method, **({'params': params} if params is not None else {})})

    def request(self, method, params, error=False):
        self.next_id += 1
        request_id = self.next_id
        self.send({'id': request_id, 'method': method, **({'params': params} if params is not None else {})})
        while True:
            message = self.messages.get(timeout=15)
            if isinstance(message, Exception):
                raise message
            if message.get('method') == 'workspace/codeLens/refresh':
                self.send({'id': message['id'], 'result': None})
            elif message.get('method') == 'window/showDocument':
                self.server_requests.append(message)
                self.send({'id': message['id'], 'result': {'success': self.open_document_success}})
            elif message.get('id') == request_id and 'method' not in message:
                assert ('error' in message) == error, message
                return message.get('error' if error else 'result')
            else:
                assert message.get('method') not in ('window/showMessageRequest', 'window/showDocument'), message
                self.notifications.append(message)

    def open(self, path, text):
        uri = path.as_uri()
        if uri in self.versions:
            self.change(path, text)
        else:
            self.versions[uri] = 1
            self.notify('textDocument/didOpen', {'textDocument': {
                'uri': uri, 'languageId': 'http', 'version': 1, 'text': text}})
        return self.lenses(path)

    def change(self, path, text):
        uri = path.as_uri()
        self.versions[uri] += 1
        self.notify('textDocument/didChange', {'textDocument': {
            'uri': uri, 'version': self.versions[uri]}, 'contentChanges': [{'text': text}]})

    def lenses(self, path):
        return self.request('textDocument/codeLens', {'textDocument': {'uri': path.as_uri()}})

    def execute(self, command, error=False):
        return self.request('workspace/executeCommand', command, error=error)

    def wait_diagnostics(self, path, predicate):
        # A CodeLens response is not a barrier for asynchronous diagnostics.
        # Match the current document version, including notifications already read.
        def matching(message):
            params = message.get('params', {})
            return (message.get('method') == 'textDocument/publishDiagnostics'
                    and params['uri'] == path.as_uri()
                    and params.get('version') == self.versions[path.as_uri()]
                    and predicate(params['diagnostics']))

        for message in reversed(self.notifications):
            if matching(message):
                return message['params']['diagnostics']
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            message = self.messages.get(timeout=max(.01, deadline - time.monotonic()))
            if isinstance(message, Exception):
                raise message
            if message.get('method') == 'workspace/codeLens/refresh':
                self.send({'id': message['id'], 'result': None})
                continue
            self.notifications.append(message)
            if matching(message):
                return message['params']['diagnostics']
        raise AssertionError('Current document diagnostics did not arrive')

    def close(self):
        try:
            if self.process.poll() is None:
                self.request('shutdown', None)
                self.notify('exit', None)
                self.process.stdin.close()
                self.process.wait(timeout=5)
        finally:
            if self.process.poll() is None:
                self.process.kill()
                self.process.wait()
            try:
                self.process.stdin.close()
            except BrokenPipeError:
                pass
            self.process.stdout.close()
            errors = self.process.stderr.read().decode()
            self.process.stderr.close()
            assert not errors, errors
