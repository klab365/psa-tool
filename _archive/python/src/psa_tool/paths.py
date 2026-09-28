import os
from pathlib import Path

APP_DIR = Path.home() / ".psa-tool"


def app_dir() -> Path:
    APP_DIR.mkdir(parents=True, exist_ok=True, mode=0o700)
    return APP_DIR


CONFIG_FILE = APP_DIR / "config.json"
TOKEN_CACHE_FILE = APP_DIR / "token_cache.json"
DB_FILE = APP_DIR / "psa.sqlite3"
