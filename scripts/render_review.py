#!/usr/bin/env python3
"""Render the review diagram. Requires Pillow only for documentation authoring."""
import argparse
from pathlib import Path
import re
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
WIDTH, HEIGHT = 1440, 1020
INK = '#102231'
MUTED = '#516472'
BLUE = '#155ec4'
GREEN = '#147456'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--font', type=Path, default=Path('/System/Library/Fonts/Supplemental/Arial.ttf'))
    parser.add_argument('--bold-font', type=Path, default=Path('/System/Library/Fonts/Supplemental/Arial Bold.ttf'))
    args = parser.parse_args()
    regular = lambda size: ImageFont.truetype(str(args.font), size)
    bold = lambda size: ImageFont.truetype(str(args.bold_font), size)
    image = Image.new('RGB', (WIDTH, HEIGHT), '#f5f7fa')
    draw = ImageDraw.Draw(image)
    version = re.search(r'^version = "([^"]+)"', (ROOT / 'extension.toml').read_text(), re.M)[1]

    def label(x, y, value, size=25, color=INK, strong=False):
        draw.text((x, y), value, font=bold(size) if strong else regular(size), fill=color)

    def card(box, color='#ffffff'):
        draw.rounded_rectangle(box, radius=22, fill=color, outline='#dce3eb', width=2)

    label(64, 47, 'SENDLINE HTTP CLIENT', 20, BLUE, True)
    label(64, 90, 'One HTTP language. A dedicated request engine.', 44, strong=True)
    label(64, 153, 'A language-server-only extension for existing .http files', 27, MUTED)
    draw.rounded_rectangle((64, 207, 1376, 256), radius=12, fill='#e6eefb')
    label(84, 219, f'ID  sendline-http-lsp     |     Version {version}     |     macOS + GNU/Linux', 22, BLUE)

    columns = [64, 520, 976]
    cards = [
        ('01', 'HTTP extension', ['Recognizes .http files', 'Owns grammar + highlighting', 'Installed from Zed Extensions']),
        ('02', 'Sendline HTTP LSP', ['Send Request + Cancel Request', 'Inline environment selection', 'Native HTTP + response tests']),
        ('03', 'Response console', ['Status, timing, size, content type', 'Body, header, and report links', 'Private editor document']),
    ]
    for x, (number, title, lines) in zip(columns, cards):
        card((x, 298, x + 400, 548))
        label(x + 25, 322, number, 20, BLUE, True)
        label(x + 25, 365, title, 29, strong=True)
        for index, line in enumerate(lines):
            label(x + 25, 424 + 33 * index, line, 22, MUTED)
    for x in [484, 940]:
        draw.line((x, 419, x + 27, 419), fill=BLUE, width=3)
        draw.polygon([(x + 27, 419), (x + 17, 412), (x + 17, 426)], fill=BLUE)

    label(64, 590, 'The execution workflow', 30, strong=True)
    features = [
        ('Automatic backend', 'Versioned native download with SHA-256 verification.'),
        ('Current editor text', 'Unsaved edits are included. Stale controls are rejected.'),
        ('Bounded response tests', 'Embedded JavaScript. No Node runtime or host IO.'),
    ]
    for index, (title, description) in enumerate(features):
        y = 646 + 68 * index
        draw.ellipse((66, y + 6, 78, y + 18), fill=GREEN)
        label(96, y, title, 25, strong=True)
        label(450, y + 1, description, 24, MUTED)

    card((64, 870, 1376, 938), '#e8f4ef')
    label(85, 892, 'No bundled grammar   |   No project tasks   |   State stays in Zed\'s extension work directory', 23, GREEN)
    label(64, 967, 'Architecture diagram, not an editor screenshot. The console replaces automatic terminal output.', 20, MUTED)
    output = ROOT / 'docs/images/sendline-workflow.png'
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output, optimize=True)
    print(output)


if __name__ == '__main__':
    main()
