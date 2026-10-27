#!/usr/bin/env python3
"""Prove a packaged install runs without its source executable or checkout paths."""
import hashlib
import json
import os
import platform
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

from integration import main as verify
from support.lsp import Lsp

ROOT = Path(__file__).resolve().parents[1]


def main():
    with tempfile.TemporaryDirectory(prefix='sendline-install-') as temporary:
        data = Path(temporary) / 'Zed data with spaces'
        subprocess.run([sys.executable, str(ROOT / 'scripts/install.py'), '--data-dir', str(data)], check=True)
        installed = data / 'extensions/installed/sendline'
        assert (installed / 'extension.wasm').is_file()
        assert (installed / 'grammars/http.wasm').is_file()
        task = json.loads((installed / 'languages/http/tasks.json').read_text())[0]
        assert str(ROOT) not in task['command']
        executable = next((data / 'extensions/work/sendline').glob('*/sendline'))
        # The installed binary and generated task are exercised through the full
        # integration suite, with separate state and no source binary on PATH.
        sys.argv = [sys.argv[0], str(executable)]
        verify()
        env = {**os.environ, 'SENDLINE_HOME': str(Path(temporary) / 'private cache')}
        lsp = Lsp(executable, env)
        try:
            lsp.request('initialize', {'capabilities': {}})
            lsp.notify('initialized', {})
            runners = list((Path(env['SENDLINE_HOME']) / 'runners').iterdir())
            assert len(runners) == 1 and runners[0].resolve() == executable.resolve()
        finally:
            lsp.close()
        subprocess.run([sys.executable, str(ROOT / 'scripts/release_asset.py'), str(executable),
                        ({'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64'}[platform.machine()]
                         + ('-apple-darwin' if sys.platform == 'darwin' else '-unknown-linux-gnu')), '--output', str(data / 'assets')], check=True)
        archive = next((data / 'assets').glob('*.tar.gz'))
        with tarfile.open(archive) as packaged:
            contents = packaged.extractfile('sendline').read()
            assert packaged.getnames() == ['sendline', 'LICENSE', 'THIRD_PARTY_NOTICES.txt']
            assert 'Fabrice Bellard' in packaged.extractfile('THIRD_PARTY_NOTICES.txt').read().decode()
        checksum = archive.with_name(archive.name.removesuffix('.tar.gz') + '.sha256').read_text().split()[0]
        assert hashlib.sha256(contents).hexdigest() == checksum
        archive_checksum = archive.with_name(archive.name + '.sha256').read_text().split()[0]
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == archive_checksum
        print('PASS: standalone installed client, versioned launcher and release archive checksum')


if __name__ == '__main__':
    main()
