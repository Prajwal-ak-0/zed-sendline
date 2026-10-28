#!/usr/bin/env python3
"""Install the compiled LSP-only package into an owned development installation."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys
import uuid

ROOT = Path(__file__).resolve().parents[1]
EXTENSION_ID = 'sendline-http-lsp'
PACKAGE = ROOT / 'dist' / EXTENSION_ID


def data_directory():
    if sys.platform == 'darwin':
        return Path.home() / 'Library/Application Support/Zed'
    if sys.platform == 'linux':
        return Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share')) / 'zed'
    raise SystemExit('Sendline currently supports macOS and Linux.')


def owned(path):
    marker = path / '.local-source'
    return not path.is_symlink() and marker.is_file() and marker.read_text() == str(ROOT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package-only', action='store_true')
    parser.add_argument('--data-dir', type=Path)
    parser.add_argument('--replace-legacy', action='store_true', help='Back up this checkout\'s old Sendline installation')
    parser.add_argument('--enable-buttons', action='store_true', help='Enable HTTP CodeLens in Zed settings')
    args = parser.parse_args()
    # This generated directory belongs to this repository, never to a user profile.
    if PACKAGE.exists():
        shutil.rmtree(PACKAGE)
    PACKAGE.mkdir(parents=True)
    for name in ['extension.wasm', 'LICENSE']:
        shutil.copy2(ROOT / name, PACKAGE / name)
    manifest = (ROOT / 'extension.toml').read_text()
    assert '[grammars.' not in manifest and f'id = "{EXTENSION_ID}"' in manifest
    version = re.search(r'^version = "([^"]+)"', manifest, re.M)[1]
    (PACKAGE / 'extension.toml').write_text(manifest + '\n[lib]\nkind = "Rust"\nversion = "0.1.0"\n')
    if args.package_only:
        print(f'Packaged LSP-only extension: {PACKAGE}')
        return
    data = args.data_dir or data_directory()
    installed = data / 'extensions/installed' / EXTENSION_ID
    if installed.exists() and not owned(installed):
        raise SystemExit(f'Refusing to replace an unowned installation: {installed}')
    for extension_id in ['sendline', 'rest-client']:
        legacy = data / 'extensions/installed' / extension_id
        if legacy.exists() and owned(legacy):
            if not args.replace_legacy:
                raise SystemExit('Use --replace-legacy to back up the old owned extension before installing.')
            backup = data / 'extensions/local-backups' / f'{extension_id}-{uuid.uuid4().hex}'
            backup.parent.mkdir(parents=True, exist_ok=True)
            legacy.rename(backup)
            print(f'Previous development extension preserved: {backup}')
    architecture = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64'}.get(platform.machine())
    if not architecture:
        raise SystemExit('Unsupported CPU architecture')
    target = architecture + ('-apple-darwin' if sys.platform == 'darwin' else '-unknown-linux-gnu')
    binary_dir = data / 'extensions/work' / EXTENSION_ID / f'sendline-v{version}-{target}'
    binary_dir.mkdir(parents=True, exist_ok=True)
    temporary = binary_dir / '.sendline.new'
    shutil.copy2(ROOT / 'bin/sendline', temporary)
    temporary.chmod(0o755)
    temporary.replace(binary_dir / 'sendline')
    shutil.copytree(PACKAGE, installed, dirs_exist_ok=True)
    (installed / '.local-source').write_text(str(ROOT))
    if args.enable_buttons:
        config = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'zed/settings.json'
        try:
            settings = json.loads(config.read_text() if config.exists() else '{}')
        except json.JSONDecodeError:
            raise SystemExit('Installed. Enable languages.http.code_lens in your JSONC settings manually.')
        settings.setdefault('languages', {}).setdefault('http', {})['code_lens'] = 'on'
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text(json.dumps(settings, indent=2) + '\n')
    print(f'Installed: {installed}')
    print('Install the HTTP language extension, open a .http file, then click Send Request.')


if __name__ == '__main__':
    main()
