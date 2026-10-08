#!/usr/bin/env python3
"""Turns a game written by `nullnet-sim --json` into a replay page.

    cargo run --release -p nullnet-sim -- --crews 2 --days 1000 --json \
        | python3 tools/replay.py > replay.html

The page lets you step through the game turn by turn: every crew's orders,
what happened in the turn's days and where each crew stood afterwards.
"""

import json
import sys
from pathlib import Path

TEMPLATE = Path(__file__).with_name("replay-template.html")


def main() -> None:
    game = json.load(sys.stdin)
    # The game data is embedded in a <script> block; a closing tag inside a
    # string would end the block early.
    data = json.dumps(game, separators=(",", ":"), ensure_ascii=False).replace("</", "<\\/")
    body = TEMPLATE.read_text(encoding="utf-8").replace("__GAME_JSON__", data)
    sys.stdout.write(
        '<!doctype html>\n<html lang="da">\n<head>\n<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
        "</head>\n<body>\n" + body + "\n</body>\n</html>\n"
    )


if __name__ == "__main__":
    main()
