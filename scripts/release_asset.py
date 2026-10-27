#!/usr/bin/env python3
"""Package a native release with the checksum consumed by the WASM launcher."""
import argparse
import hashlib
from pathlib import Path
import re
import tarfile
import tempfile

from license_notices import collect

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('target', choices=['aarch64-apple-darwin', 'x86_64-apple-darwin',
                                         'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu'])
    parser.add_argument('--output', type=Path, default=ROOT / 'dist/assets')
    args = parser.parse_args()
    version = re.search(r'^version = "([^"]+)"', (ROOT / 'Cargo.toml').read_text(), re.M)[1]
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output / f'sendline-{version}-{args.target}.tar.gz'
    with tempfile.TemporaryDirectory() as temporary:
        notices = Path(temporary) / 'THIRD_PARTY_NOTICES.txt'
        notices.write_text(collect(args.target))
        with tarfile.open(archive, 'w:gz') as output:
            output.add(args.binary, arcname='sendline')
            output.add(ROOT / 'LICENSE', arcname='LICENSE')
            output.add(notices, arcname='THIRD_PARTY_NOTICES.txt')
    # Hash the extracted executable so the WASM adapter verifies what it launches.
    digest = hashlib.sha256(args.binary.read_bytes()).hexdigest()
    archive.with_name(archive.name.removesuffix('.tar.gz') + '.sha256').write_text(f'{digest}  sendline\n')
    archive.with_name(archive.name + '.sha256').write_text(f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n')
    print(archive)


if __name__ == '__main__':
    main()
