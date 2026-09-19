"""Voice pipeline: record -> transcribe -> Jev -> route -> act.

Runs inside the daemon on a worker thread; every stage transitions State
so widgets see listening/deciding/awaiting_choice/done in real time.
U2 replaces the flat app/action questions with the route schema; the
confidence policy here already implements gate-on-target (launch executes
when app confidence is high even if action confidence is low).
"""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

from . import config

JEV_QUESTIONS = {
    "app": {
        "type": "choice",
        "instructions": "Which application is the user asking about? "
                        "Choose 'none' if the user is asking a question, "
                        "chatting, or not requesting an app.",
        "criteria": {
            "none": "no application — the user is asking a question, "
                    "chatting, or the request is unclear",
            "browser": "user wants a web browser or a website",
            "terminal": "user wants a terminal or shell",
            "files": "user wants a file manager",
            "vscode": "user wants the code editor",
            "music": "user wants a music player",
            "settings": "user wants system settings",
            "browser_new_tab": "user wants a new browser tab",
        },
    },
    "action": {
        "type": "choice",
        "instructions": "What should be done?",
        "criteria": {
            "launch": "open or start it",
            "close": "close or quit it",
            "type_text": "type some text",
            "run_shell": "run a shell command",
            "answer": "respond to the user in text — questions, chat, "
                      "or anything that is not a desktop action",
        },
    },
    "risk": {
        "type": "score",
        "instructions": "0 read-only launch, 2 mutating",
        "criteria": ["read-only", "navigational", "mutating"],
    },
}


def hypr_env() -> dict:
    """Hyprland tooling needs its instance signature when run from a daemon."""
    env = dict(os.environ)
    if not env.get("HYPRLAND_INSTANCE_SIGNATURE"):
        rd = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
        hypr = pathlib.Path(rd) / "hypr"
        if hypr.is_dir():
            env["HYPRLAND_INSTANCE_SIGNATURE"] = sorted(hypr.iterdir())[0].name
    return env


def notify(msg: str) -> None:
    subprocess.run(["notify-send", "Dim", msg], env=hypr_env())


def speak(msg: str, cfg: dict) -> None:
    if cfg.get("voice", {}).get("enabled", "false") != "true":
        return
    bin_ = shutil.which("espeak-ng") or shutil.which("espeak")
    if bin_:
        subprocess.Popen([bin_, msg], stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL)


def record(seconds: int, state=None) -> pathlib.Path:
    out = config.CFG_DIR / "utterance.wav"
    config.RUN_DIR.mkdir(parents=True, exist_ok=True)
    sampler = threading.Thread(target=_amplitude_sampler,
                               args=(seconds, state), daemon=True)
    sampler.start()
    rec = shutil.which("pw-record") or shutil.which("arecord")
    if rec is None:
        raise RuntimeError("no pw-record or arecord found")
    if rec.endswith("pw-record"):
        cmd = [rec, "--rate", "16000", "--channels", "1", "--format", "s16",
               "--sample-count", str(16000 * seconds), str(out)]
    else:
        cmd = [rec, "-D", "default", "-r", "16000", "-c", "1", "-f", "S16_LE",
               "-d", str(seconds), str(out)]
    # pw-record exits 1 on clean --sample-count shutdown (pipewire 1.6.x);
    # trust the output file, not the exit code.
    subprocess.run(cmd, timeout=seconds + 10, env=hypr_env())
    if not (out.exists() and out.stat().st_size > 44):
        raise RuntimeError(f"recording produced no audio: {out}")
    time.sleep(0.3)
    if state:
        state.set_level(0.0)
    else:
        config.LEVEL_FILE.write_text("0.0")
    return out


def _amplitude_sampler(seconds: int, state=None) -> None:
    """Breathing darkness: sample mic RMS, publish level for the overlay."""
    arec = shutil.which("arecord")
    if not arec:
        return

    def publish(v: float) -> None:
        if state:
            state.set_level(v)
        else:
            config.LEVEL_FILE.write_text(f"{v:.3f}")

    try:
        proc = subprocess.Popen(
            [arec, "-D", "default", "-f", "U8", "-r", "200", "-c", "1",
             "-d", str(seconds)],
            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=hypr_env())
        while True:
            chunk = proc.stdout.read(200)
            if not chunk:
                break
            step = 20
            for i in range(0, len(chunk) - step, step):
                w = chunk[i:i + step]
                rms = sum(abs(b - 128) for b in w) / (len(w) * 128)
                publish(min(1.0, rms * 6))
        proc.wait(timeout=5)
    except Exception:
        try:
            publish(0.0)
        except Exception:
            pass


def transcribe(wav: pathlib.Path, cfg: dict) -> str:
    model = config.whisper_model(cfg)
    if not (config.WHISPER_BIN.exists() and model.exists()):
        raise RuntimeError(f"whisper.cpp missing: {config.WHISPER_BIN} / {model}")
    r = subprocess.run(
        [str(config.WHISPER_BIN), "-m", str(model), "-nt", "-f", str(wav)],
        capture_output=True, text=True, timeout=120)
    return " ".join(r.stdout.split())


def build_questions(harness: dict | None) -> dict:
    """Jev questions with the app catalog rebuilt from the local harness."""
    if not harness or not harness.get("apps"):
        return JEV_QUESTIONS
    q = json.loads(json.dumps(JEV_QUESTIONS))
    criteria = {
        name: f"{a.get('cues', name)}"
        + (f" (frequently used: {a['seen']}x)" if a.get("seen", 0) >= 5 else "")
        for name, a in harness["apps"].items()
    }
    criteria["none"] = JEV_QUESTIONS["app"]["criteria"]["none"]
    q["app"]["criteria"] = criteria
    return q


def active_window() -> dict:
    r = subprocess.run(["hyprctl", "activewindow", "-j"],
                       capture_output=True, text=True, env=hypr_env())
    try:
        w = json.loads(r.stdout)
        return {"class": w.get("class", ""), "title": w.get("title", "")}
    except json.JSONDecodeError:
        return {}


def ask_jev(transcript: str, model: str, questions: dict,
            context: str = "") -> dict:
    state_txt = f"{context}\n\nThe user said: \"{transcript}\"" \
        if context else f'The user said: "{transcript}"'
    payload = {"model": model, "state": state_txt, "questions": questions}
    req = urllib.request.Request(
        config.JEV_ENDPOINT, data=json.dumps(payload).encode(),
        headers={
            "Authorization": f"Bearer {config.load_api_key()}",
            "Content-Type": "application/json",
            "HTTP-Referer": "https://github.com/duketopceo/dim-agent",
            "X-Title": "Dim",
        }, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"Jev HTTP {e.code}: {e.read().decode()[:200]}") \
            from None


def is_low_confidence(answers: dict, cfg: dict) -> bool:
    """Gate on app/target confidence — action confidence no longer kills
    correct launches (the 'retro-large' bug). Launch is a whitelisted
    action: high app confidence + acceptable risk executes regardless of
    how Jev scored the action question."""
    app_conf = answers.get("app", {}).get("confidence", 1)
    thresh = float(cfg.get("agent", {}).get("confidence_ambiguous", "0.8"))
    return app_conf < thresh


def execute(answers: dict, cfg: dict, harness: dict | None = None) -> str:
    apps = dict(cfg.get("apps", {}))
    if harness:
        apps.update({n: a["launch"] for n, a in harness.get("apps", {}).items()
                     if a.get("launch")})
    agent = cfg.get("agent", {})
    threshold = float(agent.get("risk_threshold", "1.5"))
    action = answers.get("action", {}).get("choice")
    app = answers.get("app", {}).get("choice")
    risk = float(answers.get("risk", {}).get("score", 2))

    if action == "answer" or app == "none":
        return "ANSWERED"
    if risk > threshold:
        return f"BLOCKED (risk={risk:.2f} > {threshold})"
    if action != "launch":
        return f"SKIP (action={action!r} not auto-executed)"
    binname = apps.get(app)
    if not binname:
        return f"SKIP (unknown app {app!r})"
    binary = binname.split()[0]
    if not (shutil.which(binary)
            or (config.HOME / ".local" / "bin" / binary).exists()):
        return f"SKIP ({app} -> {binary!r} not installed)"
    # Hyprland 0.56: hyprctl dispatch exec <cmd> hits a Lua parse bug
    # (hyprwm/Hyprland#16224); the working path is the Lua dispatcher.
    r = subprocess.run(["hyprctl", "eval", f'hl.dsp.exec_cmd("{binname}")'],
                       capture_output=True, text=True, env=hypr_env())
    if r.returncode != 0 or "ok" not in r.stdout:
        subprocess.run(["hyprctl", "dispatch", "exec", binname],
                       capture_output=True, env=hypr_env())
    return f"LAUNCHED {app} -> {binname}"


def log_decision(record_dict: dict, log_file=config.DECISIONS) -> None:
    try:
        log_file.parent.mkdir(parents=True, exist_ok=True)
        with log_file.open("a") as f:
            f.write(json.dumps(record_dict) + "\n")
    except Exception:
        pass


def ambiguous_choices(answers: dict, cfg: dict) -> list:
    """Choice labels for the clarify widget: top app/action candidates."""
    app_probs = sorted(answers.get("app", {}).get("probabilities", {}).items(),
                       key=lambda kv: -kv[1])[:3]
    act_probs = sorted(answers.get("action", {}).get("probabilities", {}).items(),
                       key=lambda kv: -kv[1])[:3]
    return [f"app:{k}" for k, _ in app_probs] \
        + [f"action:{k}" for k, _ in act_probs]


def apply_choice(answers: dict, picked: str) -> dict:
    """User picked a clarify option — rewrite the decision accordingly."""
    kind, _, value = picked.partition(":")
    corrected = json.loads(json.dumps(answers))
    if kind == "app":
        corrected["app"]["choice"] = value
    else:
        corrected["action"]["choice"] = value
    corrected["corrected_by_user"] = True
    return corrected


def run_listen(cfg: dict, state, wait_for_choice=None) -> int:
    """One push-to-talk cycle inside the daemon.

    `wait_for_choice(timeout)` -> picked label or None; injected so U5 can
    swap the GTK overlay path for IPC choices without touching this flow.
    Defaults to the legacy GTK button overlay.
    """
    secs = int(cfg.get("audio", {}).get("seconds", "5"))
    model = cfg.get("agent", {}).get("model", "typesafe/jev-1.13")
    overlay = None
    try:
        state.transition("listening", transcript="", result="", answer="",
                         choices=[], error="")
        if config.OVERLAY_BIN.exists():
            overlay = subprocess.Popen([str(config.OVERLAY_BIN)], env=hypr_env(),
                                       stdout=subprocess.DEVNULL,
                                       stderr=subprocess.DEVNULL)
        wav = record(secs, state)
        state.transition("transcribing")
        text = transcribe(wav, cfg)
        state.transition("deciding", transcript=text)
        if not text or "[BLANK" in text:
            state.transition("done", result="heard nothing")
            notify("heard nothing")
            return 0
        harness = config.load_harness()
        win = active_window()
        context = (harness or {}).get("context", "")
        if win.get("title"):
            context += f"\nActive window: {win['class']} — {win['title']}"
        resp = ask_jev(text, model, build_questions(harness), context=context)
        answers = resp.get("answers", {})
        low_conf = is_low_confidence(answers, cfg)
        corrected = None
        if low_conf:
            labels = ambiguous_choices(answers, cfg)
            if wait_for_choice:
                state.transition("awaiting_choice", choices=labels)
                picked = wait_for_choice(60)
                state.transition("acting", choices=[])
                if picked:
                    corrected = apply_choice(answers, picked)
            else:
                corrected = _gtk_choice(answers, cfg)
        if low_conf and not corrected:
            result = "CANCELLED (low confidence, no pick made)"
            state.transition("done", result=result)
            notify(result)
            log_decision({"ts": datetime.now(timezone.utc).isoformat(),
                          "transcript": text, "answers": answers,
                          "result": result, "corrected": False})
            return 0
        if corrected:
            answers = corrected
        state.transition("acting")
        result = execute(answers, cfg, harness)
        state.transition("done", result=result)
        notify(result)
        log_decision({"ts": datetime.now(timezone.utc).isoformat(),
                      "transcript": text, "answers": answers, "result": result,
                      "corrected": bool(answers.get("corrected_by_user"))})
        return 0
    except Exception as e:
        state.transition("error", error=str(e))
        notify(f"error: {e}")
        print(f"error: {e}", file=sys.stderr)
        return 1
    finally:
        if overlay:
            overlay.terminate()
        if state:
            state.set_level(0.0)


def _gtk_choice(answers: dict, cfg: dict) -> dict | None:
    """Legacy path: ambiguous choices as clickable GTK overlay buttons."""
    labels = ambiguous_choices(answers, cfg)
    if not labels:
        return None
    try:
        r = subprocess.run([str(config.OVERLAY_BIN), "--buttons"] + labels,
                           capture_output=True, text=True, timeout=60,
                           env=hypr_env())
    except Exception:
        return None
    picked = r.stdout.strip()
    return apply_choice(answers, picked) if picked else None
