#!/usr/bin/env python3
"""Verify public release assets anonymously, then exercise the downloaded host binary."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
import tempfile
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'Prajwal-ak-0/zed-sendline'
TARGETS = {
    'aarch64-apple-darwin': ('macho', 0x0100000C),
    'x86_64-apple-darwin': ('macho', 0x01000007),
    'aarch64-unknown-linux-gnu': ('elf', 183),
    'x86_64-unknown-linux-gnu': ('elf', 62),
}


def fetch(url):
    # Deliberately omit authentication: registry users need public downloads.
    request = Request(url, headers={'User-Agent': 'Sendline-release-verification'})
    with urlopen(request, timeout=60) as response:
        return response.read()


def check_architecture(binary, target):
    kind, cpu = TARGETS[target]
    if kind == 'macho':
        assert binary[:4] == b'\xcf\xfa\xed\xfe', f'Invalid Mach-O binary: {target}'
        assert int.from_bytes(binary[4:8], 'little') == cpu, f'Wrong CPU: {target}'
    else:
        assert binary[:5] == b'\x7fELF\x02', f'Invalid ELF binary: {target}'
        assert int.from_bytes(binary[18:20], 'little') == cpu, f'Wrong CPU: {target}'
        versions = {tuple(map(int, match.split(b'.'))) for match in re.findall(rb'GLIBC_([0-9]+\.[0-9]+)', binary)}
        assert not versions or max(versions) <= (2, 35), f'Release exceeds the documented glibc minimum: {target}'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    version = re.search(r'^version = "([^"]+)"', (ROOT / 'extension.toml').read_text(), re.M)[1]
    parser.add_argument('--tag', default='v' + version)
    args = parser.parse_args()
    assert args.tag == 'v' + version, 'Check out the matching release source before verifying an older version'
    release = json.loads(fetch(f'https://api.github.com/repos/{REPOSITORY}/releases/tags/{args.tag}'))
    assert not release['draft'], 'Release is still a draft'
    assets = {asset['name']: asset['browser_download_url'] for asset in release['assets']}
    architecture = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64'}[platform.machine()]
    host = architecture + ('-apple-darwin' if sys.platform == 'darwin' else '-unknown-linux-gnu')
    with tempfile.TemporaryDirectory(prefix='sendline-public-release-') as temporary:
        directory = Path(temporary)
        host_binary = None
        for target in TARGETS:
            stem = f'sendline-{version}-{target}'
            name = stem + '.tar.gz'
            archive_bytes = fetch(assets[name])
            expected_archive = fetch(assets[name + '.sha256']).decode().split()[0]
            assert hashlib.sha256(archive_bytes).hexdigest() == expected_archive, f'Archive checksum: {target}'
            path = directory / name
            path.write_bytes(archive_bytes)
            with tarfile.open(path) as archive:
                assert archive.getnames() == ['sendline', 'LICENSE', 'THIRD_PARTY_NOTICES.txt']
                member = archive.getmember('sendline')
                assert member.isfile() and member.mode & 0o111, f'Binary is not executable: {target}'
                binary = archive.extractfile(member).read()
                assert 'Fabrice Bellard' in archive.extractfile('THIRD_PARTY_NOTICES.txt').read().decode()
            expected_binary = fetch(assets[stem + '.sha256']).decode().split()[0]
            assert hashlib.sha256(binary).hexdigest() == expected_binary, f'Binary checksum: {target}'
            check_architecture(binary, target)
            if target == host:
                host_binary = directory / 'sendline'
                host_binary.write_bytes(binary)
                host_binary.chmod(0o755)
            print(f'PASS: public archive, executable checksum, CPU and notices: {target}', flush=True)
        assert host_binary is not None, 'This verification host is unsupported'
        result = subprocess.check_output([str(host_binary), '--version'], text=True).strip()
        assert result == f'sendline {version}', result
        subprocess.run([sys.executable, str(ROOT / 'tests/integration.py'), str(host_binary)], check=True)
        subprocess.run([sys.executable, str(ROOT / 'tests/editor_execution.py'), str(host_binary)], check=True)
    print('PASS: anonymous release downloads and full integration flow using the published host executable')


if __name__ == '__main__':
    main()
