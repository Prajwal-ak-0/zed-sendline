#!/usr/bin/env python3
"""Generate a portable task using the versioned runner registered by the LSP."""
import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def task():
    version = re.search(r'^version = "([^"]+)"', (ROOT / 'Cargo.toml').read_text(), re.M)[1]
    command = (
        'if [ -n "${SENDLINE_HOME:-}" ]; then sendline_home="$SENDLINE_HOME"; '
        'elif [ "$(uname -s)" = Darwin ]; then sendline_home="$HOME/Library/Caches/sendline"; '
        'else sendline_home="${XDG_CACHE_HOME:-$HOME/.cache}/sendline"; fi; '
        f'exec "$sendline_home/runners/{version}" run --file "$ZED_FILE" --line "$ZED_ROW"'
    )
    return {
        'label': 'Sendline', 'command': command, 'tags': ['http-request'],
        'reveal': 'no_focus', 'hide': 'never', 'save': 'current',
        'use_new_terminal': False, 'allow_concurrent_runs': False,
        'show_summary': False, 'show_command': False,
        'shell': {'program': '/bin/sh'},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    path = ROOT / 'languages/http/tasks.json'
    text = json.dumps([task()], indent=2) + '\n'
    if args.check:
        if path.read_text() != text:
            raise SystemExit('Run python3 scripts/generate_tasks.py to update the portable task.')
    else:
        path.write_text(text)


if __name__ == '__main__':
    main()
