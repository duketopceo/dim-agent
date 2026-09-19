"""Toolbelt: named tools Jev can route to, each with a risk tier.

Tiers:
  safe     — read-only/launch; executes immediately
  mutating — changes desktop state (window ops, typing, close); needs a
             confirmation unless the user enabled elevated mode
  shell    — arbitrary command; always needs per-call confirmation and
             is subject to the denylist
"""
from . import desktop, system

RISK = {"safe": 0, "mutating": 1, "shell": 2}

# hard refusals — checked before any confirmation prompt
SHELL_DENYLIST = ("rm -rf /", "rm -rf ~", "rm -rf $HOME", "mkfs",
                  "dd if=", ":(){ ", "shutdown", "reboot")

REGISTRY = {
    "launch": (desktop.launch, "safe", "open an application"),
    "focus": (desktop.focus, "safe", "focus a window by class"),
    "close": (desktop.close, "mutating", "close the active/matching window"),
    "workspace": (desktop.workspace, "mutating", "switch Hyprland workspace"),
    "notify": (system.notify_tool, "safe", "send a desktop notification"),
    "screenshot": (system.screenshot, "safe", "capture the screen to a file"),
    "type_text": (system.type_text, "mutating", "type text into the focused window"),
    "shell": (system.shell, "shell", "run a shell command"),
    "search_files": (system.search_files, "safe", "find files by name under ~"),
    "agent_spawn": (None, "safe", "spawn a named ori opencode agent"),  # wired in dim.agents
    "task_status": (None, "safe", "report a named agent's status"),
    "task_cancel": (None, "mutating", "cancel a named agent"),
}


def get(name: str):
    return REGISTRY.get(name)


def describe() -> dict:
    """Criteria text for Jev's tool question."""
    return {name: desc for name, (_, _, desc) in REGISTRY.items()}


def risk_of(name: str) -> str:
    entry = REGISTRY.get(name)
    return entry[1] if entry else "shell"


def run(name: str, arg: str, cfg: dict, harness: dict | None = None) -> str:
    """Execute a tool by name. Confirm-gating happens in the pipeline
    before run() is called — this is the bare executor."""
    from .. import agents
    if name == "agent_spawn":
        return agents.spawn(arg, cfg)
    if name == "task_status":
        return agents.status(arg)
    if name == "task_cancel":
        return agents.cancel(arg)
    entry = REGISTRY.get(name)
    if entry is None or entry[0] is None:
        return f"SKIP (tool {name!r} unavailable)"
    fn = entry[0]
    if fn is desktop.launch:
        return fn(arg, cfg, harness)
    return fn(arg)


def denied(cmd: str) -> bool:
    c = cmd.lower()
    return any(d in c for d in SHELL_DENYLIST)
