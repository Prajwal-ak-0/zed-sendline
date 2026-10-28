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
        installed = data / 'extensions/installed/sendline-http-lsp'
        assert (installed / 'extension.wasm').is_file()
        assert not (installed / 'grammars').exists()
        assert not (installed / 'languages').exists()
        executable = next((data / 'extensions/work/sendline-http-lsp').glob('*/sendline'))
        # Exercise the installed executable with separate state and no source
        # binary on PATH, through both CLI and LSP entry points.
        sys.argv = [sys.argv[0], str(executable)]
        verify()
        subprocess.run([sys.executable, str(ROOT / 'tests/editor_execution.py'), str(executable)], check=True)
        env = {**os.environ, 'SENDLINE_HOME': str(Path(temporary) / 'private cache')}
        lsp = Lsp(executable, env)
        try:
            lsp.request('initialize', {'capabilities': {}})
            lsp.notify('initialized', {})
            assert not (Path(env['SENDLINE_HOME']) / 'runners').exists()
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
        print('PASS: standalone installed client, LSP-only frontend and release archive checksum')


if __name__ == '__main__':
    main()
