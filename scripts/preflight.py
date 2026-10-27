#!/usr/bin/env python3
"""Check public-source invariants before a commit or release."""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def version(path):
    return re.search(r'^version = "([^"]+)"', path.read_text(), re.M)[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag')
    args = parser.parse_args()
    release = version(ROOT / 'extension.toml')
    assert version(ROOT / 'Cargo.toml') == version(ROOT / 'client/Cargo.toml') == release, 'Version mismatch'
    assert args.tag in (None, 'v' + release), 'Release tag must match the manifest'
    manifest = (ROOT / 'extension.toml').read_text()
    assert 'id = "sendline"' in manifest
    assert 'https://github.com/Prajwal-ak-0/zed-sendline' in manifest
    subprocess.run(['python3', str(ROOT / 'scripts/generate_tasks.py'), '--check'], check=True)
    task = json.loads((ROOT / 'languages/http/tasks.json').read_text())[0]
    assert task['reveal'] == 'no_focus' and task['save'] == 'current'
    assert task['shell']['program'] == '/bin/sh'
    files = [ROOT / name for name in ['Cargo.toml', 'extension.toml', 'README.md', 'CONTRIBUTING.md']]
    for directory in ['src', 'client', 'languages', 'examples', 'scripts', 'docs', '.github']:
        files.extend(path for path in (ROOT / directory).rglob('*') if path.is_file() and '__pycache__' not in path.parts)
    for path in files:
        assert path.name != 'http-client.private.env.json', f'Private environment in source: {path}'
        text = path.read_text()
        # Check source, not example paths such as /Users/you in the README.
        assert all(name == 'you' for name in re.findall(r'/Users/([A-Za-z0-9_.-]+)/', text)), f'Machine-specific path: {path}'
        assert not re.search(r'(?i)(gh[pousr]_[A-Za-z0-9]{30,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)', text), f'Credential in source: {path}'
    print(f'PASS: Sendline {release}, matching versions, portable task, source path and credential checks')


if __name__ == '__main__':
    main()
