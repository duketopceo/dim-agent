"""Paths, config file, secrets, and the local harness catalog."""
import json
import os
import pathlib

HOME = pathlib.Path.home()
CFG_DIR = HOME / ".config" / "dim-agent"
CFG_FILE = CFG_DIR / "config.toml"
ENV_FILE = CFG_DIR / ".env"
DATA_DIR = pathlib.Path(os.environ.get("XDG_DATA_HOME", HOME / ".local" / "share")) / "dim-agent"
CORRECTIONS = DATA_DIR / "corrections.jsonl"
DECISIONS = DATA_DIR / "decisions.jsonl"
RUN_DIR = pathlib.Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "dim-agent"
LEVEL_FILE = RUN_DIR / "level"
STATE_FILE = RUN_DIR / "state.json"
SOCK_FILE = RUN_DIR / "dimd.sock"
TASKS_FILE = DATA_DIR / "tasks.jsonl"
TASK_LOGS = DATA_DIR / "tasks"
HARNESS_FILE = CFG_DIR / "harness.json"
WHISPER_HOME = HOME / "src" / "whisper.cpp"
WHISPER_BIN = WHISPER_HOME / "build" / "bin" / "whisper-cli"
OVERLAY_BIN = HOME / ".local" / "opt" / "dim-agent" / "dim-overlay"

JEV_ENDPOINT = "https://openrouter.ai/api/alpha/decisions"

DEFAULT_CONFIG = """\
[hotkey]
# Super + D = push-to-talk. mod is a Hyprland modmask token (SUPER, ALT,
# SHIFT, CTRL) or a keysym like ALT_R (multi-key bind, side-specific).
mod = "SUPER"
key = "D"

[audio]
seconds = 5
# whisper.cpp model filename under ~/src/whisper.cpp/models/
whisper_model = "ggml-small.en.bin"

[agent]
model = "typesafe/jev-1.13"
# Auto-execute when risk <= threshold. Measured Jev scores: "open terminal"
# launch scores ~1.2 (navigational band), so 1.5 = launch/close allowed,
# mutating (>=2) always requires explicit confirmation.
risk_threshold = 1.5
confidence_instant = 0.95
confidence_ambiguous = 0.8

[voice]
# spoken replies via espeak/espeak-ng when true; missing binary = silent no-op
enabled = false

[apps]
browser = "chromium"
terminal = "ghostty"
files = "nautilus"
vscode = "code"
music = "spotify"
settings = "gnome-control-center"
browser_new_tab = "chromium"
"""


def _default_cfg_dict() -> dict:
    return {
        "hotkey": {"mod": "SUPER", "key": "D"},
        "audio": {"seconds": "5", "whisper_model": "ggml-small.en.bin"},
        "agent": {
            "model": "typesafe/jev-1.13", "risk_threshold": "1.5",
            "confidence_instant": "0.95", "confidence_ambiguous": "0.8",
        },
        "voice": {"enabled": "false"},
        "apps": {
            "browser": "chromium", "terminal": "ghostty", "files": "nautilus",
            "vscode": "code", "music": "spotify",
            "settings": "gnome-control-center", "browser_new_tab": "chromium",
        },
    }


def load_config() -> dict:
    """Parse a flat [section] key = "value" TOML subset without deps."""
    cfg = {}
    if CFG_FILE.exists():
        section = None
        for raw in CFG_FILE.read_text().splitlines():
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("["):
                section = line.strip("[]")
                cfg[section] = {}
            elif "=" in line and section:
                k, v = (p.strip() for p in line.split("=", 1))
                cfg[section][k] = v.strip('"')
    else:
        CFG_DIR.mkdir(parents=True, exist_ok=True)
        CFG_FILE.write_text(DEFAULT_CONFIG)
        cfg = _default_cfg_dict()
    return cfg


def whisper_model(cfg: dict) -> pathlib.Path:
    name = cfg.get("audio", {}).get("whisper_model", "ggml-small.en.bin")
    return WHISPER_HOME / "models" / name


def load_api_key() -> str:
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            if line.startswith("OPENROUTER_API_KEY="):
                return line.split("=", 1)[1].strip()
    key = os.environ.get("OPENROUTER_API_KEY", "")
    if not key:
        raise RuntimeError(f"No OPENROUTER_API_KEY in {ENV_FILE} or environment")
    return key


def load_harness() -> dict | None:
    """Local personalization built by scripts/build_harness.py.
    Never committed — lives in ~/.config/dim-agent/."""
    try:
        return json.loads(HARNESS_FILE.read_text())
    except Exception:
        return None
