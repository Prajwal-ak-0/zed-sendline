#!/usr/bin/env python3
"""Install a compiled development build without depending on its source checkout."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import sys

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = ROOT / 'dist/sendline'


def data_directory():
    if sys.platform == 'darwin':
        return Path.home() / 'Library/Application Support/Zed'
    if sys.platform == 'linux':
        return Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share')) / 'zed'
    raise SystemExit('Sendline currently supports macOS and Linux.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package-only', action='store_true')
    parser.add_argument('--data-dir', type=Path, help='Use a disposable Zed data directory for verification')
    parser.add_argument('--replace-legacy', action='store_true', help='Remove this checkout\'s old rest-client installation')
    parser.add_argument('--enable-buttons', action='store_true', help='Enable CodeLens in Zed settings')
    args = parser.parse_args()
    PACKAGE.mkdir(parents=True, exist_ok=True)
    for source in ['extension.wasm', 'grammars/http.wasm', 'LICENSE']:
        destination = PACKAGE / source
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / source, destination)
    shutil.copytree(ROOT / 'languages', PACKAGE / 'languages', dirs_exist_ok=True)
    manifest = (ROOT / 'extension.toml').read_text()
    version = re.search(r'^version = "([^"]+)"', manifest, re.M)[1]
    manifest = manifest.replace('schema_version = 1', 'schema_version = 1\nlanguages = ["languages/http"]')
    manifest += '\n[lib]\nkind = "Rust"\nversion = "0.1.0"\n'
    (PACKAGE / 'extension.toml').write_text(manifest)
    if args.package_only:
        print(f'Packaged extension: {PACKAGE}')
        return
    data = args.data_dir or data_directory()
    installed = data / 'extensions/installed/sendline'
    marker = installed / '.local-source'
    if installed.is_symlink() or (installed.exists() and (not marker.is_file() or marker.read_text() != str(ROOT))):
        raise SystemExit(f'Refusing to replace an unowned extension at {installed}. Uninstall it in Zed first.')
    if args.replace_legacy:
        legacy = data / 'extensions/installed/rest-client'
        old_marker = legacy / '.local-source'
        if legacy.exists():
            if legacy.is_symlink() or not old_marker.is_file() or old_marker.read_text() != str(ROOT):
                raise SystemExit('The legacy extension belongs to another installation. Uninstall it in Zed first.')
            shutil.rmtree(legacy)
    # Seed the same versioned cache that the production WASM downloader uses.
    architecture = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64'}.get(platform.machine())
    if not architecture:
        raise SystemExit('Unsupported CPU architecture')
    target = architecture + ('-apple-darwin' if sys.platform == 'darwin' else '-unknown-linux-gnu')
    binary_dir = data / f'extensions/work/sendline/sendline-v{version}-{target}'
    binary_dir.mkdir(parents=True, exist_ok=True)
    temporary = binary_dir / '.sendline.new'
    shutil.copy2(ROOT / 'bin/sendline', temporary)
    temporary.chmod(0o755)
    temporary.replace(binary_dir / 'sendline')
    shutil.copytree(PACKAGE, installed, dirs_exist_ok=True)
    marker.write_text(str(ROOT))
    print(f'Installed compiled extension: {installed}')
    if args.enable_buttons:
        config = Path(os.environ.get('XDG_CONFIG_HOME', Path.home() / '.config')) / 'zed/settings.json'
        try:
            settings = json.loads(config.read_text() if config.exists() else '{}')
        except json.JSONDecodeError:
            raise SystemExit('Installed. Add "code_lens": "on" to Zed settings. Your JSONC settings were preserved.')
        settings['code_lens'] = 'on'
        settings.get('languages', {}).get('HTTP', {}).pop('code_lens', None)
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text(json.dumps(settings, indent=2) + '\n')
    print('Open a .http file, select an environment in the header, then click a gutter play button.')


if __name__ == '__main__':
    main()
