"""Where the tools look for game data. Both locations come from environment
variables so nothing machine-specific lives in the repository.

  ER_FILES     folder holding the unpacked files (`chr/`, `regulation-bin/`)
  ER_GAME_DIR  the game's own `Game` folder, for its Oodle library
  ER_OODLE_LIB full path to an Oodle library, overriding the search (Linux)
"""
import os
import sys
from pathlib import Path

DEFAULT_GAME_DIR = r"C:\Program Files (x86)\Steam\steamapps\common\ELDEN RING\Game"

# Proton installs the game as plain files on Linux; these are checked in order.
LINUX_GAME_DIRS = [
    "~/.steam/steam/steamapps/common/ELDEN RING/Game",
    "~/.local/share/Steam/steamapps/common/ELDEN RING/Game",
    "~/Games/ELDEN RING/Game",
]


def _default_game_dir() -> Path:
    if os.name == "nt":
        return Path(DEFAULT_GAME_DIR)
    for candidate in LINUX_GAME_DIRS:
        path = Path(candidate).expanduser()
        if path.is_dir():
            return path
    return Path(LINUX_GAME_DIRS[0]).expanduser()


def _folder(variable, default, must_contain):
    path = Path(os.environ.get(variable, default))
    if not (path / must_contain).exists():
        sys.exit(
            f"{variable} is {'set to' if variable in os.environ else 'not set; tried'} {path}\n"
            f"but there is no {must_contain} in it. Point {variable} at the right folder (see README)."
        )
    return path


def er_files():
    return _folder("ER_FILES", "er-files", "chr")


def game_dir():
    return _folder("ER_GAME_DIR", _default_game_dir(), "oo2core_6_win64.dll")
