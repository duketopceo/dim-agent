# dim-agent — agent notes

A voice computer-use agent for Omarchy (Hyprland on Asahi Linux):
push-to-talk → PipeWire capture → whisper.cpp → Jev decision via OpenRouter →
guarded Hyprland launch, with a dim overlay. `README.md` is the user doc.

## Commands

```bash
# Tests — 12 tests, zero third-party dependencies (pyproject: dependencies = [])
python3 -m unittest discover -s tests -v

# Exactly what CI runs (.github/workflows/test.yml, push to master + PR):
python -m py_compile dimd dim-overlay scripts/propose_criteria.py tests/test_dimd.py
python -m unittest discover -s tests -v
```

CI matrix is Python 3.11 and 3.13; `requires-python = ">=3.11"`.

**The default branch is `master`, not `main`.** `.github/workflows/test.yml`
has `push: branches: [master]` with an **unfiltered** `pull_request:` trigger.
So a rename to `main` must update that `push` filter in the same change:
without it, PRs still get CI but direct pushes to `main` run nothing at all.

## `dimd` and `dim-overlay` are extensionless scripts, on purpose

Both are executable Python files with **no `.py` extension**, and they are not
importable as packages. `tests/test_dimd.py` loads `dimd` with
`importlib.machinery.SourceFileLoader` and monkeypatches its filesystem
constants so no real config, overlay, or `hyprctl` is touched. `dimd install`
copies them to `~/.local/opt/dim-agent/` under those exact names and writes
`~/.local/bin/dim-agent-trigger` to match.

Renaming them to `dimd.py` / `dim-overlay.py` breaks the test loader, the
install path, and the Hyprland bind. That is the single most likely
"harmless cleanup" to wreck this repo.

## Layout

| Path | What it is |
|---|---|
| `dimd` | The daemon: hotkey trigger, record, transcribe, decide, act. Also `dimd install` and `dimd harness`. |
| `dim-overlay` | Fullscreen PyGObject + `gtk-layer-shell` overlay whose opacity tracks live mic amplitude. |
| `scripts/build_harness.py` | Same harness builder as `dimd harness`; mines the local dayflow DB for real launch commands. |
| `scripts/propose_criteria.py` | Manual, weekly clustering of `corrections.jsonl` into proposed `JEV_QUESTIONS`. Not cron-scheduled. |
| `tests/test_dimd.py` | Headless unit tests over `dimd`'s pure decision logic. |
| `tests/stage_test.sh` | Stage-by-stage end-to-end, **not** in CI. |

`tests/stage_test.sh` needs espeak-ng, a whisper.cpp build at
`~/src/whisper.cpp` with `models/ggml-base.en.bin`, a real
`~/.config/dim-agent/.env`, and a live Hyprland session. Do not add it to CI
and do not treat a failure there as a unit-test failure.

## Environment and secrets

- User config: `~/.config/dim-agent/config.toml` (hotkey, apps, risk threshold).
- `~/.config/dim-agent/.env` holds `OPENROUTER_API_KEY`, chmod 600, never
  committed. The harness output `~/.config/dim-agent/harness.json` is local
  only, also never committed.
- Hyprland is configured in **Lua** on Omarchy — `hyprland.conf` is not
  sourced, and `dimd install` strips any legacy `bind =` line it previously
  wrote there. `hyprctl dispatch exec` also hits a known Hyprland Lua
  dispatcher bug; the working path is `hyprctl eval 'hl.dsp.exec_cmd(...)'`.

## Known defect, do not "fix" it silently

`README.md` currently contains **unresolved merge-conflict markers**
(`<<<<<<< HEAD` / `>>>>>>> origin/master`) around the v0.1 hotkey
(`Right Alt + Space` vs `Super + D`) and the Jev model
(`typesafe/jev-1.13` vs `~typesafe/jev-latest`). The conflict is committed on
`master`. CI does not catch it because nothing lints Markdown.

The rest of the README and `tests/stage_test.sh` also disagree about the model
pin (`jev-1.13` in the architecture diagram, `~typesafe/jev-latest` in the
test script). Resolve the conflict deliberately and fix the two disagreeing
model references together; do not pick one side by accident.
