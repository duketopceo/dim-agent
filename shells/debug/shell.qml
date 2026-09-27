import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// Dim — management app. Open/close from the launcher; the daemon stays
// resident. Tabs: Status (live state + controls), Logs (decisions with
// per-stage timing bars + session/corrections tails), Settings (live
// config editing via `dimd config`), Tasks (agent registry).
// Everything reads files/IPC — zero coupling to daemon internals.

FloatingWindow {
  id: win
  title: "Dim"
  minimumSize: Qt.size(820, 620)
  color: "#16161e"

  readonly property string rtDir: {
    var rd = Quickshell.env("XDG_RUNTIME_DIR");
    if (!rd || rd.length === 0) rd = "/tmp";
    return rd + "/dim-agent";
  }
  readonly property string dataDir: Quickshell.env("HOME") + "/.local/share/dim-agent"

  property var stateObj: ({})
  property var decisions: []
  property var sessionLines: []
  property var corrections: []
  property var cfgObj: ({})
  property int tab: 0

  function dimd(args) {
    cmdProc.command = ["dimd"].concat(args);
    cmdProc.running = true;
  }

  Process { id: cmdProc; command: ["dimd", "status"] }

  // config fetch: dimd config prints JSON when daemon up; parse stdout
  Process {
    id: cfgProc
    command: ["dimd", "config"]
    stdout: StdioCollector {
      onStreamFinished: {
        try { win.cfgObj = JSON.parse(this.text); } catch (e) {}
      }
    }
  }

  FileView {
    id: stateView
    path: win.rtDir + "/state.json"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      try { win.stateObj = JSON.parse(stateView.text()); } catch (e) {}
    }
  }
  FileView {
    id: decisionsView
    path: win.dataDir + "/decisions.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var out = [];
      var lines = decisionsView.text().split("\n");
      for (var i = lines.length - 1; i >= 0 && out.length < 40; i--) {
        if (!lines[i].trim()) continue;
        try { out.push(JSON.parse(lines[i])); } catch (e) {}
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
      for (var i = lines.length - 1; i >= 0 && out.length < 40; i--) {
        if (!lines[i].trim()) continue;
        try { out.push(JSON.parse(lines[i])); } catch (e) {}
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
      var out = [];
      var lines = corrView.text().split("\n");
      for (var i = lines.length - 1; i >= 0 && out.length < 40; i--) {
        if (!lines[i].trim()) continue;
        try { out.push(JSON.parse(lines[i])); } catch (e) {}
      }
      win.corrections = out;
    }
  }

  Timer {
    interval: 500; running: true; repeat: true
    onTriggered: { stateView.reload(); decisionsView.reload();
                   sessionView.reload(); corrView.reload(); }
  }
  Timer {
    interval: 5000; running: true; repeat: true
    onTriggered: { cfgProc.running = false; cfgProc.running = true; }
  }
  Component.onCompleted: cfgProc.running = true

  readonly property color fg: "#c0caf5"
  readonly property color dim: "#9aa5ce"
  readonly property color faint: "#565f89"
  readonly property color accent: "#7aa2f7"
  readonly property color card: "#1f2335"
  readonly property color border: "#292e42"

  function statusColor(s) {
    if (s === "error") return "#e05555";
    if (s === "listening") return accent;
    if (["transcribing","deciding"].indexOf(s) >= 0) return "#bb9af7";
    if (["acting","awaiting_choice","speaking"].indexOf(s) >= 0) return "#9ece6a";
    return faint;
  }

  RowLayout {
    anchors.fill: parent
    spacing: 0

    // ── Sidebar ──
    Rectangle {
      Layout.fillHeight: true
      width: 150
      color: card
      Column {
        anchors.fill: parent
        anchors.topMargin: 12
        spacing: 2
        Text { text: "  DIM"; color: accent; font.pixelSize: 15
               font.bold: true; bottomPadding: 12 }
        Repeater {
          model: ["Status", "Logs", "Session", "Tasks", "Settings"]
          Rectangle {
            width: 150; height: 34
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

      // STATUS
      Item {
        Column {
          anchors.fill: parent; anchors.margins: 16; spacing: 12
          Row {
            spacing: 14
            Rectangle { width: 18; height: 18; radius: 9
                        color: win.statusColor(win.stateObj.status || "offline") }
            Text { text: win.stateObj.status || "offline"
                   color: fg; font.pixelSize: 20; font.bold: true }
            Item { width: 20 }
            Repeater {
              model: ["listen", "stop", "learn", "harness"]
              Rectangle {
                height: 30; width: bl.implicitWidth + 20; radius: 7
                color: "#283457"
                Text { id: bl; anchors.centerIn: parent; text: modelData
                       color: fg; font.pixelSize: 12 }
                MouseArea { anchors.fill: parent
                            onClicked: win.dimd([modelData]) }
              }
            }
          }
          Rectangle { width: parent.width; height: 1; color: border }
          Grid {
            columns: 2; spacing: 8; width: parent.width
            Repeater {
              model: [
                ["transcript", win.stateObj.transcript || "—"],
                ["answer", win.stateObj.answer || "—"],
                ["result", win.stateObj.result || "—"],
                ["error", win.stateObj.error || "—"],
                ["choices", JSON.stringify(win.stateObj.choices || [])],
                ["level", (win.stateObj.level || 0).toFixed(3)],
                ["started", win.stateObj.started_at || "—"],
              ]
              Rectangle {
                width: (parent.width - 8) / 2
                height: kv.implicitHeight + 16
                color: card; radius: 8
                Column {
                  id: kv; anchors.fill: parent; anchors.margins: 8
                  spacing: 2
                  Text { text: modelData[0]; color: faint; font.pixelSize: 10 }
                  Text { text: modelData[1]; color: fg; font.pixelSize: 12
                         width: kv.width; wrapMode: Text.Wrap }
                }
              }
            }
          }
        }
      }

      // LOGS — decisions with per-stage timing bars
      Flickable {
        contentHeight: logCol.implicitHeight; clip: true
        Column {
          id: logCol; width: parent.width; padding: 16; spacing: 6
          Text { text: "decisions.jsonl — newest first"
                 color: faint; font.pixelSize: 11; bottomPadding: 6 }
          Repeater {
            model: win.decisions
            Rectangle {
              width: logCol.width - 32
              height: dcol.implicitHeight + 12
              color: card; radius: 6
              Column {
                id: dcol; anchors.fill: parent; anchors.margins: 6; spacing: 3
                Text {
                  text: (modelData.ts || "").slice(11, 19) + "  " +
                        (modelData.transcript || "") + "  →  " +
                        (modelData.result || "")
                  color: fg; font.pixelSize: 11; width: dcol.width
                  wrapMode: Text.Wrap
                }
                Row {
                  spacing: 4; visible: !!modelData.timing_ms
                  Repeater {
                    model: modelData.timing_ms ?
                      [["rec", modelData.timing_ms.record_ms, "#7aa2f7"],
                       ["stt", modelData.timing_ms.stt_ms, "#bb9af7"],
                       ["jev", modelData.timing_ms.jev_ms, "#e0af68"],
                       ["act", modelData.timing_ms.act_ms, "#9ece6a"]] : []
                    Rectangle {
                      width: Math.max(14, Math.min(120,
                        (modelData[1] || 0) / 80))
                      height: 14; radius: 3; color: modelData[2]
                      Text { anchors.centerIn: parent
                             text: modelData[0] + " " + (modelData[1]||0) + "ms"
                             color: "#16161e"; font.pixelSize: 8 }
                    }
                  }
                }
              }
            }
          }
        }
      }

      // SESSION + CORRECTIONS
      Flickable {
        contentHeight: sessCol.implicitHeight; clip: true
        Column {
          id: sessCol; width: parent.width; padding: 16; spacing: 6
          Text { text: "session.jsonl"; color: faint; font.pixelSize: 11 }
          Repeater {
            model: win.sessionLines
            Text {
              text: (modelData.ts || "").slice(5, 16) + "  " +
                    (modelData.transcript || "") +
                    (modelData.reply ? "  → " + modelData.reply : "")
              color: dim; font.pixelSize: 11; width: sessCol.width - 32
              wrapMode: Text.Wrap
            }
          }
          Text { text: "corrections.jsonl"; color: faint
                 font.pixelSize: 11; topPadding: 12 }
          Repeater {
            model: win.corrections
            Text {
              text: "heard " + (modelData.heard || "") + " → picked " +
                    (modelData.picked || "")
              color: "#e0af68"; font.pixelSize: 11
              width: sessCol.width - 32; wrapMode: Text.Wrap
            }
          }
        }
      }

      // TASKS
      Flickable {
        contentHeight: taskCol.implicitHeight; clip: true
        Column {
          id: taskCol; width: parent.width; padding: 16; spacing: 6
          Text { text: "agent tasks (from state.json)"; color: faint
                 font.pixelSize: 11 }
          Repeater {
            model: Object.keys(win.stateObj.tasks || {})
            Rectangle {
              width: taskCol.width - 32; height: tl.implicitHeight + 14
              color: card; radius: 6
              Column {
                id: tl; anchors.fill: parent; anchors.margins: 7; spacing: 2
                Text { text: modelData + "  [" +
                       (win.stateObj.tasks[modelData].status || "?") + "]"
                       color: fg; font.pixelSize: 12; font.bold: true }
                Text { text: win.stateObj.tasks[modelData].task || ""
                       color: dim; font.pixelSize: 11; width: tl.width
                       wrapMode: Text.Wrap }
              }
            }
          }
          Text { visible: Object.keys(win.stateObj.tasks || {}).length === 0
                 text: "no agent tasks yet — say \"Dim, agent …\""
                 color: faint; font.pixelSize: 11 }
        }
      }

      // SETTINGS
      Flickable {
        contentHeight: setCol.implicitHeight; clip: true
        Column {
          id: setCol; width: parent.width; padding: 16; spacing: 10
          Text { text: "config.toml — edits apply live via IPC"
                 color: faint; font.pixelSize: 11 }
          Repeater {
            model: [
              ["agent.answer_model", "answer model"],
              ["agent.model", "jev model"],
              ["brain.router", "router (jev|chat|off)"],
              ["agent.voice_out", "voice_out (true|false)"],
              ["audio.seconds", "record seconds"],
              ["agent.risk_threshold", "risk threshold"],
              ["agent.allow_shell", "allow_shell (true|false)"],
              ["agent.screenshots", "screenshots (true|false)"],
            ]
            Rectangle {
              width: setCol.width - 32; height: 40
              color: card; radius: 6
              Row {
                anchors.fill: parent; anchors.margins: 8; spacing: 10
                Text { text: modelData[1]; color: dim; font.pixelSize: 12
                       width: 190; anchors.verticalCenter: parent.verticalCenter }
                Rectangle {
                  width: 300; height: 26; radius: 4; color: "#16161e"
                  anchors.verticalCenter: parent.verticalCenter
                  border.color: border
                  TextInput {
                    id: input
                    anchors.fill: parent; anchors.margins: 5
                    color: fg; font.pixelSize: 12; clip: true
                    text: {
                      var parts = modelData[0].split(".");
                      var v = (win.cfgObj[parts[0]] || {})[parts[1]];
                      v === undefined ? "" : String(v);
                    }
                  }
                }
                Rectangle {
                  width: 60; height: 26; radius: 4; color: "#283457"
                  anchors.verticalCenter: parent.verticalCenter
                  Text { anchors.centerIn: parent; text: "set"
                         color: fg; font.pixelSize: 11 }
                  MouseArea {
                    anchors.fill: parent
                    onClicked: win.dimd(["config", "set", modelData[0],
                                         input.text])
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
