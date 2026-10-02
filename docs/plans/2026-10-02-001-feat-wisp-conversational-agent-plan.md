# feat: Wisp conversational agent — goals, planning, look-before-ask

Date: 2026-10-02
Status: planned
Source: live dogfood session 2026-10-02 (Robinhood/GDX sequence + workspace fail)

## Problem

The Robinhood sequence shows Wisp is not an agent, it's a slot-filler:

```
03:08 act "check on my robinhood account"  → "not a recognized application"
03:09 act "open it up in a browser"        → tried chromium (wrong browser)
03:10 act "just do command t for a new tab" → "shell command was declined"
03:10 act "type in robinhood.com"           → "I cannot proceed without a click"
03:11 act "click on gdx"                    → clicked wrong coordinates
03:12 act "nope, you didn't"               → apologized, corrected
```

Every utterance was a fresh 1–2 step act with no shared goal. Wisp never
held "get me to GDX on Robinhood" — it asked *whether* to act (clarify
cards: "what app?") instead of *how* (screenshot, look, click). The same
turn-burn pattern killed "Workspace 4" (route tool → SKIP, workspace arg
was the whole sentence) and the earlier "Amache/omarchy" miss.

Three root causes:

1. **Jev decides *if*, not *what*.** The `route`/`app`/`action` questions
   make Jev classify intent; the clarify route exists precisely so it can
   punt. Jev should emit a plan (goal + first step), and safety lives
   in the per-call tool tiers — not in whether Jev is sure.
2. **No goal state.** `session.as_text` is transcript history, not a
   goal. "it's open right here just do command t" should continue the
   active goal, not start a new act.
3. **Clarify is the default failure mode.** Hey Clicky never asks "what
   app" — it screenshots, reads, and acts. We have screenshot+vision
   wired (act loop already attaches images); the default should be
   look-then-act, clarify only after genuine ambiguity survives a look.

Non-goals: no new model calls per turn (Jev stays the one decision call),
no auto-weight learning, no confirm removal for real shell.

## Design

### U1 — Jev emits a plan, not a verdict

Replace `app`/`action`/`tool` questions with a single `plan` slot on the
act/tool routes: Jev answers `"goal"` (one sentence) + `"first_step"`
(tool name + arg from the registry — same vocabulary the tool question
already carries). The act loop gets `task = goal` with `first_step` as a
hint message; execution unchanged.

- `route` stays (act/tool/answer/dictation/learn/spawn/clarify).
- `app`/`action` questions deleted — targets resolve inside the act loop
  via screenshot, not via Jev guessing "browser" vs "browser_new_tab".
- `risk` kept but log-only (threshold 9 already neutered the gate).
- Clarify is demoted from a route to an escape hatch (U3/U4).

### U2 — Goal memory (`wisp/goals.py`)

`state.goal = {text, app, started_ts, steps: [], status}`:

- New utterance joins the active goal when (a) same focused app, or
  (b) topic similarity (token overlap ≥0.5 vs goal text — reuse
  `trajectories._related`), and goal is <10 min old.
- Act loop receives the running goal + last 4 steps as the user message,
  so "no, wrong link" retries with everything it just did in context.
- Goal closes when the model returns `DONE:`/goal-complete text, a new
  unrelated utterance arrives, or 10 min idle. Persist to state.json so
  the panel shows "working on: robinhood — GDX quote".

### U3 — Look before ask

Act route preamble: `screenshot` is step 0 when the provider supports
vision (already capped at 3 images). Focus context block is already
injected (context.py). Jev's clarify route keeps working but the
pipeline only honors it when the transcript is genuinely unparseable
(short/empty post-STT) — "open X" never clarifies again.

### U4 — Confirm once per (tool, app) per session

`state.confirmed: set` — when the user confirms a mutating/shell call,
record `(tool, focus_app)`; subsequent calls with the same pair skip the
prompt until daemon restart or a denylisted arg. "click the URL bar,
type robinhood.com, press enter" becomes three silent steps after one
yes. `allow_shell=false` still hard-blocks; denylist still hard-blocks.

### U5 — Act loop talks back

If the model replies `ASK_USER: <question>`, wisp speaks it via TTS and
parks status=`awaiting_voice` — the next utterance resumes the same
goal with the answer as context. Conversation without a modal card.

## Units

| # | Unit | Files |
|---|------|-------|
| U1 | Jev plan question | `pipeline.py` JEV_QUESTIONS + dispatch |
| U2 | Goal memory | `wisp/goals.py`, `state.py`, `pipeline.py`, `act.py` |
| U3 | Screenshot-first act preamble | `act.py` |
| U4 | Session confirm map | `state.py`, `act.py` `_gate` |
| U5 | ASK_USER voice backchannel | `act.py`, `pipeline.py`, `state.py` |
| U6 | Tests + deploy | `tests/test_goals.py`, `test_pipeline.py` |

## Verification

Replay the Robinhood sequence as a scripted scenario:
`"check my robinhood account"` → `"open it in a browser"` →
`"command t for a new tab"` → all three join one goal, zero clarify
cards, zero repeat confirmations.

## Open questions

- `first_step` hint vs free planning — hint is safer (same vocabulary),
  free text is more flexible; start with hint, measure.
- Goal TTL: 10 min feels right for voice; make it `[agent] goal_ttl_s`.
