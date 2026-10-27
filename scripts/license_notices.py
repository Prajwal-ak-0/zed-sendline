#!/usr/bin/env python3
"""Collect license and bundled-vendor notices from Cargo's resolved dependencies."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[1]


def repository_key(url):
    parsed = urlsplit(url or '')
    if parsed.hostname == 'github.com':
        return '/'.join(parsed.path.strip('/').split('/')[:2]).removesuffix('.git').lower()
    return (url or '').rstrip('/')


def collect(target=None):
    cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
    command = [cargo, 'metadata', '--locked', '--format-version', '1']
    metadata = json.loads(subprocess.check_output(command, cwd=ROOT))
    packages = {package['id']: package for package in metadata['packages']}
    platforms = [target] if target else [
        'aarch64-apple-darwin', 'x86_64-apple-darwin',
        'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu', 'wasm32-wasip2',
    ]
    selected = set()
    for platform in platforms:
        filtered = json.loads(subprocess.check_output(command + ['--filter-platform', platform], cwd=ROOT))
        nodes = {node['id']: node for node in filtered['resolve']['nodes']}
        name = 'sendline-extension' if platform == 'wasm32-wasip2' else 'sendline'
        pending = [next(package['id'] for package in packages.values() if package['name'] == name)]
        visited = set()
        while pending:
            identifier = pending.pop()
            if identifier not in visited:
                visited.add(identifier)
                pending.extend(nodes[identifier]['dependencies'])
        selected.update(visited)
    by_repository = {}
    resources = {}
    for identifier, package in packages.items():
        if not package.get('source'):
            continue
        directory = Path(package['manifest_path']).parent
        files = set()
        for path in directory.rglob('*'):
            if not path.is_file() or path.suffix.lower() in {'.rs', '.c', '.h', '.cc', '.cpp', '.py', '.js', '.go'}:
                continue
            relative = path.relative_to(directory)
            if (path.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'COPYRIGHT'))
                    or any(part.upper() == 'LICENSES' for part in relative.parts[:-1])):
                files.add(path)
        if package.get('license_file'):
            files.add(directory / package['license_file'])
        resources[identifier] = sorted(files)
        repository = package.get('repository')
        if repository:
            by_repository.setdefault(repository_key(repository), set()).update(files)
    digest = hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest()
    sections = [f'Third-party dependency notices\n\nCargo.lock SHA-256: {digest}\nGenerated from the dependency sources for the supported native targets and WASM adapter.\n']
    missing = []
    unique = {}
    for identifier in sorted(selected, key=lambda item: (packages[item]['name'], packages[item]['version'])):
        package = packages[identifier]
        if not package.get('source'):
            continue
        files = resources[identifier]
        if not files:
            # Some workspace members ship their license only in a sibling crate
            # from the same upstream repository, such as rquickjs-core.
            files = sorted(by_repository.get(repository_key(package.get('repository')), set()))
        if not files:
            missing.append(package['name'] + ' ' + package['version'])
            continue
        sections.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nSPDX: {package.get('license')}\nUpstream: {package.get('repository') or 'see crates.io'}\n")
        for file in files:
            text = file.read_text(errors='replace')
            key = hashlib.sha256(text.encode()).hexdigest()
            unique[key] = text
            sections.append(f'  {file.name}: notice {key}\n')
    if missing:
        raise SystemExit('Missing dependency notices: ' + ', '.join(missing))
    for key, text in sorted(unique.items()):
        sections.append(f'\n{"=" * 72}\nNotice {key}\n\n{text}\n')
    return ''.join(sections)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(collect(args.target))
    print(f'Collected dependency notices: {args.output}')


if __name__ == '__main__':
    main()
