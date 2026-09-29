import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// Wisp — management app. Open/close from the launcher; the daemon stays
// resident. Plain-language tabs:
//   Home     — what Wisp is doing right now + quick controls
//   Activity — every turn: what you said, what it did, how long each stage took
//   Memory   — the notes Wisp reads on every turn (editable)
//   Agents   — background coding-agent tasks (spawn, watch, cancel)
//   Settings — config.toml with descriptions, applied live
// Everything reads files/IPC — zero coupling to daemon internals.

FloatingWindow {
  id: win
  title: "Wisp"
  minimumSize: Qt.size(860, 640)
  color: "#16161e"

  readonly property string rtDir: {
    var rd = Quickshell.env("XDG_RUNTIME_DIR");
    if (!rd || rd.length === 0) rd = "/tmp";
    return rd + "/wisp";
  }
  readonly property string dataDir: Quickshell.env("HOME") + "/.local/share/wisp"

  property var stateObj: ({})
  property var decisions: []
  property var sessionLines: []
  property var corrections: []
  property var cfgObj: ({})
  property var tasks: ({})
  property int tab: 0

  function wispd(args) {
    if (cmdProc.running) return;
    cmdProc.command = ["wispd"].concat(args);
    cmdProc.running = true;
  }
  function svc(args) {
    if (svcProc.running) return;
    svcProc.command = ["systemctl", "--user"].concat(args).concat(["wispd"]);
    svcProc.running = true;
  }

  FileView {
    id: stateView
    path: win.rtDir + "/state.json"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      try { win.stateObj = JSON.parse(stateView.text()) } catch (e) {}
    }
  }
  Timer { interval: 400; running: true; repeat: true
          onTriggered: stateView.reload() }

  FileView {
    id: decisionsView
    path: win.dataDir + "/decisions.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var out = [];
      var lines = decisionsView.text().split("\n");
      for (var i = lines.length - 1; i >= 0 && out.length < 12; i--) {
        var l = lines[i].trim();
        if (!l) continue;
        try { out.unshift(JSON.parse(l)) } catch (e) {}
      }
      win.decisions = out;
    }
  }

  FileView {
    id: sessionView
    path: win.dataDir + "/session.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var out = [];
      var lines = sessionView.text().split("\n");
      for (var i = lines.length - 1; i >= 0 && out.length < 8; i--) {
        var l = lines[i].trim();
        if (!l) continue;
        try { out.unshift(JSON.parse(l)) } catch (e) {}
      }
      win.sessionLines = out;
    }
  }

  FileView {
    id: corrView
    path: win.dataDir + "/corrections.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var lines = corrView.text().split("\n").filter(function(l){return l.trim()});
      win.corrections = lines.slice(-6).reverse();
    }
  }

  FileView { id: memoryView; path: win.dataDir + "/MEMORY.md" }
  FileView { id: userView;    path: win.dataDir + "/USER.md" }

  // Config + tasks refresh via CLI so we exercise the same path users do.
  Process {
    id: cfgProc
    command: ["wispd", "config"]
    stdout: StdioCollector {
      onStreamFinished: {
        try { win.cfgObj = JSON.parse(this.text) } catch (e) {}
      }
    }
  }
  Process {
    id: tasksProc
    command: ["wispd", "task_status"]
    stdout: StdioCollector {
      onStreamFinished: {
        try { win.tasks = JSON.parse(this.text).tasks || {} } catch (e) {}
      }
    }
  }
  Process { id: cmdProc; command: ["wispd"]
            onExited: { cfgProc.running = true; tasksProc.running = true } }
  Process { id: svcProc; command: ["systemctl", "--user", "status", "wispd"] }
  Process { id: writeProc; command: ["wispd"] }

  Component.onCompleted: { cfgProc.running = true; tasksProc.running = true }
  Timer { interval: 3000; running: win.tab === 3; repeat: true
          onTriggered: tasksProc.running = true }

  property string skillsList: ""

  readonly property color fg: "#c0caf5"
  readonly property color dim: "#9aa5ce"
  readonly property color faint: "#565f89"
  readonly property color accent: "#7aa2f7"
  readonly property color card: "#1f2335"
  readonly property color border: "#292e42"

  function statusColor(s) {
    if (s === "listening") return "#7aa2f7";
    if (["transcribing","deciding"].indexOf(s) >= 0) return "#bb9af7";
    if (["acting","awaiting_choice"].indexOf(s) >= 0) return "#9ece6a";
    if (s === "speaking") return "#e0af68";
    if (s === "error" || s === "offline") return "#e05555";
    return "#565f89";
  }
  function statusBlurb(s) {
    if (s === "listening") return "Listening — it's recording you right now";
    if (s === "transcribing") return "Turning your speech into text";
    if (s === "deciding") return "Thinking — the router is picking an action";
    if (s === "acting") return "Running a tool or agent on your desktop";
    if (s === "awaiting_choice") return "It needs you to pick an option";
    if (s === "speaking") return "Speaking the answer out loud";
    if (s === "done") return "Idle — last turn finished";
    if (s === "idle") return "Idle — press " + ((win.cfgObj.hotkey||{}).mod||"SUPER")
                                + "+" + ((win.cfgObj.hotkey||{}).key||"D") + " to talk";
    if (s === "error") return "Something failed — check Activity";
    if (s === "offline") return "Daemon not running";
    return s || "offline";
  }

  RowLayout {
    anchors.fill: parent
    spacing: 0

    // ── Sidebar ──
    Rectangle {
      Layout.fillHeight: true
      width: 148
      color: card
      Column {
        anchors.fill: parent
        anchors.topMargin: 12
        spacing: 2
        Text { text: "  Wisp"; color: accent; font.pixelSize: 16
               font.bold: true; bottomPadding: 12 }
        Repeater {
          model: ["Home", "Activity", "Memory", "Agents", "Settings"]
          Rectangle {
            width: 148; height: 34
            color: win.tab === index ? "#283457" : "transparent"
            Text { anchors.verticalCenter: parent.verticalCenter
                   anchors.left: parent.left; anchors.leftMargin: 14
                   text: modelData
                   color: win.tab === index ? fg : dim; font.pixelSize: 13 }
            MouseArea { anchors.fill: parent
                        onClicked: win.tab = index }
          }
        }
      }
    }

    // ── Content ──
    StackLayout {
      Layout.fillWidth: true
      Layout.fillHeight: true
      currentIndex: win.tab

      // ════ HOME ════
      Item {
        Column {
          anchors.fill: parent; anchors.margins: 20; spacing: 16

          Row {
            spacing: 16
            Rectangle {
              width: 44; height: 44; radius: 22
              color: win.statusColor(win.stateObj.status || "offline")
              Rectangle {
                visible: win.stateObj.status === "listening"
                anchors.centerIn: parent
                width: 44; height: 44; radius: 22
                color: "transparent"; border.color: "#7aa2f7"; border.width: 2
                opacity: 0.4 + (win.stateObj.level || 0) * 0.6
                Behavior on opacity { NumberAnimation { duration: 120 } }
              }
            }
            Column {
              anchors.verticalCenter: parent.verticalCenter
              Text { text: win.stateObj.status || "offline"
                     color: fg; font.pixelSize: 22; font.bold: true }
              Text { text: win.statusBlurb(win.stateObj.status || "offline")
                     color: dim; font.pixelSize: 12 }
            }
          }

          // what it heard / what it said
          Rectangle {
            width: parent.width - 40; height: hearCol.height + 24
            color: card; radius: 8
            Column {
              id: hearCol
              anchors.fill: parent; anchors.margins: 12; spacing: 8
              Text { text: "Last thing it heard"; color: faint; font.pixelSize: 11 }
              Text { text: win.stateObj.transcript || "—"
                     color: fg; font.pixelSize: 14; width: hearCol.width
                     wrapMode: Text.WordWrap }
              Text { text: "What it did / said"; color: faint; font.pixelSize: 11 }
              Text {
                text: win.stateObj.answer || win.stateObj.result || "—"
                color: dim; font.pixelSize: 13; width: hearCol.width
                wrapMode: Text.WordWrap }
              Text {
                visible: (win.stateObj.error || "").length > 0
                text: "Error: " + (win.stateObj.error || "")
                color: "#e05555"; font.pixelSize: 12; width: hearCol.width
                wrapMode: Text.WordWrap }
            }
          }

          // controls
          Row {
            spacing: 10
            Repeater {
              model: [
                ["Talk to it", ["trigger"], "#283457"],
                ["Restart daemon", ["__svc_restart"], "#283457"],
                ["Stop daemon", ["__svc_stop"], "#3a2030"]
              ]
              Rectangle {
                width: 110; height: 30; radius: 6; color: modelData[2]
                Text { anchors.centerIn: parent; text: modelData[0]
                       color: fg; font.pixelSize: 12 }
                MouseArea {
                  anchors.fill: parent
                  onClicked: {
                    if (modelData[1][0] === "__svc_restart") win.svc(["restart"]);
                    else if (modelData[1][0] === "__svc_stop") win.svc(["stop"]);
                    else win.wispd(modelData[1]);
                  }
                }
              }
            }
          }

          // what this app is
          Text {
            width: parent.width - 40
            wrapMode: Text.WordWrap
            color: faint; font.pixelSize: 11
            text: "This is the control room, not the assistant itself. " +
                  "Wisp lives in your bar (the orb, bottom-right) and on " +
                  "your hotkey — press SUPER+D anywhere and speak. " +
                  "Everything here just watches the daemon's files, so you " +
                  "can close this window and Wisp keeps running."
          }
        }
      }

      // ════ ACTIVITY ════
      Item {
        Flickable {
          anchors.fill: parent; anchors.margins: 16
          contentHeight: actCol.height
          clip: true
          Column {
            id: actCol
            width: parent.width; spacing: 10

            Text {
              width: parent.width; wrapMode: Text.WordWrap
              color: faint; font.pixelSize: 11
              text: "Every time you talk, a 'turn' is recorded here: what it " +
                    "heard, what it decided, and how long each step took " +
                    "(record → transcribe → decide → act → speak). If Wisp " +
                    "ever does something weird, this is the replay log."
            }

            Repeater {
              model: win.decisions
              Rectangle {
                width: actCol.width; height: dCol.height + 16
                color: card; radius: 8
                Column {
                  id: dCol
                  anchors.fill: parent; anchors.margins: 8; spacing: 5
                  Row {
                    spacing: 8
                    Text { text: modelData.turn || ""
                           color: accent; font.pixelSize: 12; font.bold: true }
                    Text { text: "route: " + (modelData.route || "?")
                           color: faint; font.pixelSize: 11 }
                    Text {
                      visible: (modelData.ms_total || 0) > 0
                      text: (modelData.ms_total || 0) + " ms total"
                      color: faint; font.pixelSize: 11 }
                  }
                  Text { visible: (modelData.transcript || "").length > 0
                         text: "you: " + (modelData.transcript || "")
                         color: fg; font.pixelSize: 12
                         width: dCol.width; wrapMode: Text.WordWrap }
                  Text { visible: (modelData.reply || modelData.result || "").length > 0
                         text: "wisp: " + (modelData.reply || modelData.result || "")
                         color: dim; font.pixelSize: 12
                         width: dCol.width; wrapMode: Text.WordWrap }
                  Row {
                    spacing: 6
                    Repeater {
                      model: modelData.stages || []
                      Rectangle {
                        width: stTxt.width + 10; height: 16; radius: 3
                        color: "#232946"
                        Text { id: stTxt; anchors.centerIn: parent
                               text: modelData[0] + " " + (modelData[1]||0) + "ms"
                               color: dim; font.pixelSize: 10 }
                      }
                    }
                  }
                }
              }
            }

            Text {
              visible: win.corrections.length > 0
              topPadding: 8
              width: parent.width; wrapMode: Text.WordWrap
              color: faint; font.pixelSize: 11
              text: "Learned corrections — things you told it to do " +
                    "differently:"
            }
            Repeater {
              model: win.corrections
              Text { text: "  • " + modelData
                     color: dim; font.pixelSize: 11
                     width: actCol.width; wrapMode: Text.WordWrap
                     elide: Text.ElideRight; maximumLineCount: 1 }
            }
          }
        }
      }

      // ════ MEMORY ════
      Item {
        Flickable {
          anchors.fill: parent; anchors.margins: 16
          contentHeight: memCol.height
          clip: true
          Column {
            id: memCol
            width: parent.width; spacing: 12

            Text {
              width: parent.width; wrapMode: Text.WordWrap
              color: faint; font.pixelSize: 11
              text: "Wisp reads these files at the start of every turn — " +
                    "they're its long-term memory. MEMORY.md = facts it has " +
                    "learned about you and your machine. USER.md = how you " +
                    "want it to behave. Edit either and it applies next turn. " +
                    "Semantic recall (searches old turns by meaning) is in " +
                    "Settings → Memory."
            }

            Repeater {
              model: [["memory", "MEMORY.md", memoryView],
                      ["user",   "USER.md",   userView]]
              Rectangle {
                width: memCol.width; height: memEditCol.height + 16
                color: card; radius: 8
                property var src: modelData[2]
                property string tgt: modelData[0]
                Column {
                  id: memEditCol
                  anchors.fill: parent; anchors.margins: 8; spacing: 6
                  Row {
                    spacing: 10
                    Text { text: modelData[1]; color: fg; font.pixelSize: 13
                           font.bold: true }
                    Rectangle {
                      width: 44; height: 22; radius: 4; color: "#283457"
                      Text { anchors.centerIn: parent; text: "save"
                             color: fg; font.pixelSize: 11 }
                      MouseArea {
                        anchors.fill: parent
                        onClicked: {
                          writeProc.command = ["wispd", "memory-write",
                                               modelData[0],
                                               Qt.btoa(edit.text)];
                          writeProc.running = true;
                        }
                      }
                    }
                    Text { text: "whole-file edit — it keeps the header"
                           color: faint; font.pixelSize: 10
                           anchors.verticalCenter: parent.verticalCenter }
                  }
                  Rectangle {
                    width: memEditCol.width; height: 140; radius: 4
                    color: "#16161e"; border.color: border
                    Flickable {
                      anchors.fill: parent; anchors.margins: 4
                      contentHeight: edit.height; clip: true
                      TextEdit {
                        id: edit
                        width: parent.width
                        color: dim; font.pixelSize: 11
                        font.family: "monospace"
                        wrapMode: TextEdit.Wrap
                        text: modelData[2].text()
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }

      // ════ AGENTS ════
      Item {
        Column {
          anchors.fill: parent; anchors.margins: 16; spacing: 12

          Text {
            width: parent.width; wrapMode: Text.WordWrap
            color: faint; font.pixelSize: 11
            text: "Background tasks Wisp runs for you (coding agents like " +
                  "opencode/codex/claude — pick which in Settings → Brain). " +
                  "They keep working while you do other things; cancel any " +
                  "one below."
          }

          Row {
            spacing: 8
            Rectangle {
              width: parent.width - 140; height: 30; radius: 5
              color: card; border.color: border
              TextInput {
                id: taskInput
                anchors.fill: parent; anchors.margins: 6
                color: fg; font.pixelSize: 12; clip: true
                property string placeholder: "describe a task…"
                Text { anchors.fill: parent; verticalAlignment: Text.AlignVCenter
                       visible: !taskInput.text
                       text: taskInput.placeholder
                       color: faint; font.pixelSize: 12 }
              }
            }
            Rectangle {
              width: 70; height: 30; radius: 5; color: "#283457"
              Text { anchors.centerIn: parent; text: "spawn"
                     color: fg; font.pixelSize: 12 }
              MouseArea {
                anchors.fill: parent
                onClicked: {
                  if (taskInput.text.trim().length === 0) return;
                  win.wispd(["agent", taskInput.text.trim()]);
                  taskInput.text = "";
                }
              }
            }
          }

          Flickable {
            width: parent.width; height: parent.height - y - 8
            contentHeight: taskCol.height; clip: true
            Column {
              id: taskCol
              width: parent.width; spacing: 8
              Repeater {
                model: Object.keys(win.tasks)
                Rectangle {
                  width: taskCol.width; height: tRow.height + 16
                  color: card; radius: 8
                  property var t: win.tasks[modelData] || {}
                  Row {
                    id: tRow
                    anchors.fill: parent; anchors.margins: 8; spacing: 10
                    Rectangle {
                      width: 8; height: 8; radius: 4
                      anchors.verticalCenter: parent.verticalCenter
                      color: parent.parent.t.status === "running" ? "#9ece6a"
                           : parent.parent.t.status === "failed"  ? "#e05555"
                           : "#565f89"
                    }
                    Column {
                      width: tRow.width - 110
                      Text { text: modelData
                             color: fg; font.pixelSize: 12
                             elide: Text.ElideRight; width: parent.width }
                      Text { text: (parent.parent.t.status || "?") + " · "
                                 + (parent.parent.t.task || "").slice(0, 80)
                             color: faint; font.pixelSize: 10
                             width: parent.width; elide: Text.ElideRight }
                    }
                    Rectangle {
                      visible: parent.parent.t.status === "running"
                      width: 60; height: 24; radius: 4; color: "#3a2030"
                      anchors.verticalCenter: parent.verticalCenter
                      Text { anchors.centerIn: parent; text: "cancel"
                             color: fg; font.pixelSize: 10 }
                      MouseArea {
                        anchors.fill: parent
                        onClicked: win.wispd(["task_cancel", modelData])
                      }
                    }
                  }
                }
              }
              Text {
                visible: Object.keys(win.tasks).length === 0
                text: "No tasks yet."
                color: faint; font.pixelSize: 12
              }
            }
          }
        }
      }

      // ════ SETTINGS ════
      Item {
        Flickable {
          anchors.fill: parent; anchors.margins: 16
          contentHeight: setCol.height
          clip: true
          Column {
            id: setCol
            width: parent.width; spacing: 14

            Text {
              width: parent.width; wrapMode: Text.WordWrap
              color: faint; font.pixelSize: 11
              text: "config.toml, grouped. Edits apply live — no restart " +
                    "needed. Everything has a default; blank means default."
            }

            Repeater {
              model: [
                ["Talking to it",
                 "The hotkey and how long it records after you press.",
                 [["hotkey.mod",          "modifier key",   "SUPER / ALT / CTRL"],
                  ["hotkey.key",          "push-to-talk key", "D (with modifier = SUPER+D)"],
                  ["audio.seconds",       "recording length", "seconds of mic per press"],
                  ["voice.enabled",       "speak answers",  "true/false — spoken replies via TTS"],
                  ["voice.cmd",           "TTS command",    "empty = espeak; set a nicer voice cmd"]]],
                ["Hearing",
                 "Speech-to-text: who transcribes you. 'local' = whisper.cpp " +
                 "on this machine (free, private); 'openai'-compatible = Groq/any " +
                 "API (faster, sends audio out).",
                 [["stt.provider",        "STT provider",   "local | openai"],
                  ["stt.base_url",        "STT endpoint",   "groq/openai/vllm URL"],
                  ["stt.model",           "STT model",      "whisper-large-v3-turbo etc."],
                  ["stt.key_env",         "API key env var","name of env var holding the key"],
                  ["stt.prompt",          "vocab priming",  "names/jargon to recognize better"]]],
                ["Brain",
                 "Who thinks. Router picks what to do with your request " +
                 "(jev = decision API, chat = straight to the answer model, " +
                 "off = ask clarifying). The answer brain writes replies.",
                 [["brain.router",        "router",         "jev | chat | off"],
                  ["brain.default",       "answer brain",   "provider:model, e.g. ollama:llama3"],
                  ["agent.model",         "jev model",      "typesafe/jev-1.13"],
                  ["agent.answer_model",  "answer model",   "e.g. llama-4-maverick"],
                  ["brain.agent_runtime", "agent runtime",  "opencode | codex | claude | devin"]]],
                ["Agents & actions",
                 "What Wisp is allowed to do on your desktop. Risk threshold " +
                 "gates how dangerous an action can be before it needs you " +
                 "to confirm.",
                 [["agent.risk_threshold","risk threshold", "lower = asks you more often"],
                  ["agent.allow_shell",   "allow shell",    "true/false — run shell commands"],
                  ["agent.screenshots",   "screenshots",    "true/false — may see your screen"],
                  ["agents.model",        "agent model",    "model for spawned runtimes"]]],
                ["Memory & recall",
                 "Semantic recall: embeds past turns so 'the thing with the " +
                 "printer' finds old context. 'none' = off (FTS keyword " +
                 "search only, zero keys needed).",
                 [["recall.provider",     "recall embeds",  "none | openai"],
                  ["recall.base_url",     "embed endpoint", "openrouter/openai URL"],
                  ["recall.model",        "embed model",    "text-embedding-3-small"],
                  ["agent.session_turns", "session memory", "turns of chat history kept"]]],
                ["Debugging",
                 "Full-fidelity trace of every turn (action, decision, tool " +
                 "call, timings) for replay and fixing. Never logs keys.",
                 [["debug.trace",         "dev trace",      "true/false — writes trace.jsonl"]]]
              ]

              Column {
                width: setCol.width; spacing: 6
                property string secTitle: modelData[0]
                property string secBlurb: modelData[1]
                property var secRows: modelData[2]

                Text { text: parent.secTitle
                       color: accent; font.pixelSize: 14; font.bold: true }
                Text { text: parent.secBlurb
                       color: faint; font.pixelSize: 11
                       width: setCol.width; wrapMode: Text.WordWrap }

                Repeater {
                  model: parent.secRows
                  Rectangle {
                    width: setCol.width - 16; height: 44
                    color: card; radius: 6
                    Row {
                      anchors.fill: parent; anchors.margins: 8; spacing: 10
                      Column {
                        width: 200
                        anchors.verticalCenter: parent.verticalCenter
                        Text { text: modelData[1]; color: dim; font.pixelSize: 12 }
                        Text { text: modelData[2]; color: faint; font.pixelSize: 9 }
                      }
                      Rectangle {
                        width: setCol.width - 330; height: 28; radius: 4
                        color: "#16161e"; border.color: border
                        anchors.verticalCenter: parent.verticalCenter
                        TextInput {
                          id: sinput
                          anchors.fill: parent; anchors.margins: 5
                          color: fg; font.pixelSize: 12; clip: true
                          text: {
                            var parts = modelData[0].split(".");
                            var sec = win.cfgObj[parts[0]] || {};
                            var v = sec[parts.slice(1).join(".")];
                            if (v === undefined && parts.length > 2)
                              v = (sec[parts[1]] || {})[parts[2]];
                            v === undefined ? "" : String(v);
                          }
                        }
                      }
                      Rectangle {
                        width: 50; height: 28; radius: 4; color: "#283457"
                        anchors.verticalCenter: parent.verticalCenter
                        Text { anchors.centerIn: parent; text: "set"
                               color: fg; font.pixelSize: 11 }
                        MouseArea {
                          anchors.fill: parent
                          onClicked: win.wispd(["config", "set",
                                               modelData[0], sinput.text])
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
