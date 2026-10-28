#!/usr/bin/env python3
"""Check public-source invariants before a commit or release."""
import argparse
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def version(path):
    return re.search(r'^version = "([^"]+)"', path.read_text(), re.M)[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag')
    args = parser.parse_args()
    release = version(ROOT / 'extension.toml')
    assert version(ROOT / 'Cargo.toml') == version(ROOT / 'crates/sendline/Cargo.toml') == release, 'Version mismatch'
    assert args.tag in (None, 'v' + release), 'Release tag must match the manifest'
    manifest = (ROOT / 'extension.toml').read_text()
    assert 'id = "sendline-http-lsp"' in manifest
    assert '[grammars.' not in manifest, 'Sendline must not duplicate the HTTP grammar'
    assert not any((ROOT / 'languages').rglob('*.*')), 'Language resources must belong to the HTTP provider'
    assert 'https://github.com/Prajwal-ak-0/zed-sendline' in manifest
    files = [ROOT / name for name in ['Cargo.toml', 'extension.toml', 'README.md', 'CONTRIBUTING.md']]
    for directory in ['src', 'crates', 'tests', 'languages', 'examples', 'scripts', 'docs', '.github']:
        files.extend(path for path in (ROOT / directory).rglob('*') if path.is_file() and '__pycache__' not in path.parts)
    for path in files:
        assert path.name != 'http-client.private.env.json', f'Private environment in source: {path}'
        if path.suffix == '.png':
            assert path.parent == ROOT / 'docs/images', f'Unexpected binary asset: {path}'
            assert path.read_bytes().startswith(b'\x89PNG\r\n\x1a\n') and path.stat().st_size < 2 * 1024 * 1024
            continue
        text = path.read_text()
        # Check source, not example paths such as /Users/you in the README.
        assert all(name == 'you' for name in re.findall(r'/Users/([A-Za-z0-9_.-]+)/', text)), f'Machine-specific path: {path}'
        assert not re.search(r'(?i)(gh[pousr]_[A-Za-z0-9]{30,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)', text), f'Credential in source: {path}'
    print(f'PASS: Sendline {release}, matching versions, LSP-only package, source path and credential checks')


if __name__ == '__main__':
    main()
