# Dim IPC + State Contract v1

The stable boundary between `dimd` (any implementation — Python
reference or `dimd-rs`) and UI shells (Omarchy quickshell plugin,
future tray apps). Shells MUST only depend on this document.

## Transport

- Linux/macOS: unix domain socket at
  `$XDG_RUNTIME_DIR/dim-agent/dimd.sock` (fallback `/tmp/dim-agent/`).
- Windows (future): named pipe `\\.\pipe\dim-agent`, same framing.
- Framing: one JSON object per connection, newline-terminated request
  and newline-terminated reply. Request: `{"cmd": <string>, ...}`.
- Reply envelope: `{"ok": bool, ...}`; on failure `ok=false` plus
  `{"error": <string>}`.

## Commands

| cmd | extra fields | reply | effect |
|---|---|---|---|
| `status` | — | `{ok, state}` | `state` = full state.json snapshot |
| `listen` | — | `{ok}` or `{ok:false,error:"busy"}` | starts a listen cycle async |
| `choice` | `pick: string` | `{ok}` | resolves a pending choice/confirm |
| `task_status` | `name: string` | `{ok, result: string}` | named-agent status |
| `task_cancel` | `name: string` | `{ok, result: string}` | cancel named agent |
| `stop` | — | `{ok}` | daemon exits, socket removed |
| unknown/malformed | — | `{ok:false, error}` | — |

## state.json (atomically rewritten, tmp+rename)

```json
{
  "status": "idle|listening|transcribing|deciding|awaiting_choice|acting|speaking|done|error",
  "transcript": "string",
  "answer": "string",
  "result": "string",
  "choices": ["string"],
  "points": [{"x": 0, "y": 0, "label": "string", "step": 1}],
  "level": 0.0,
  "tasks": {"name": "running|done|failed|cancelled"},
  "error": "string",
  "started_at": "ISO-8601"
}
```

Status vocabulary is closed; shells must treat unknown statuses as
`idle`-equivalent rather than failing. `points` may be absent/empty on
older cores — default `[]`.

## Data files (shared by both cores, never versioned differently)

- `decisions.jsonl` — one JSON per turn: `ts`, `transcript`, `answers`,
  `result`, `timing_ms{record,stt,jev,act}`, `corrected`.
- `session.jsonl` — turns for follow-up context.
- `corrections.jsonl` — user picks on ambiguous turns.
- `tasks.jsonl` — agent registry; `tasks/<id>.log` per-agent output.
- `MEMORY.md`, `USER.md` — curated bounded memory (frozen snapshot).
- `skills/*/SKILL.md` — self-authored skills (progressive disclosure).
- `recall.db` — sqlite-vec/FTS5 long-term recall.

## Versioning

Contract version = top of this file. Shells read
`state.contract_version` when present; absent ⇒ v1. Additive fields are
allowed without a bump; changed/removed semantics bump the version.
