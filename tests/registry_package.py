#!/usr/bin/env python3
"""Check the actual Zed-built artifact, including its resource ownership boundary."""
import argparse
import json
from pathlib import Path
import re
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--language-provider', type=Path, help='Verify the actual published HTTP provider archive')
    args = parser.parse_args()
    metadata = json.loads((args.directory / 'manifest.json').read_text())
    source = (ROOT / 'extension.toml').read_text()
    version = re.search(r'^version = "([^"]+)"', source, re.M)[1]
    assert metadata['version'] == version
    assert metadata['provides'] == ['language-servers'], metadata['provides']
    with tarfile.open(args.directory / 'archive.tar.gz') as archive:
        files = {member.name.removeprefix('./'): member for member in archive if member.isfile()}
        assert set(files) == {'extension.toml', 'extension.wasm'}, set(files)
        manifest = archive.extractfile(files['extension.toml']).read().decode()
        wasm = archive.extractfile(files['extension.wasm']).read()
        assert 'id = "sendline-http-lsp"' in manifest
        assert '[grammars.' not in manifest and 'languages = ["languages/' not in manifest
        assert wasm.startswith(b'\0asm'), 'Not a compiled WebAssembly artifact'
    if args.language_provider:
        with tarfile.open(args.language_provider) as provider:
            config = next(member for member in provider if member.name.endswith('languages/http/config.toml'))
            language = re.search(r'^name = "([^"]+)"', provider.extractfile(config).read().decode(), re.M)[1]
            assert f'languages = ["{language}"]' in manifest, 'Server language does not match the published provider'
    print('PASS: official registry artifact contains only the LSP manifest and compiled WASM adapter')


if __name__ == '__main__':
    main()
