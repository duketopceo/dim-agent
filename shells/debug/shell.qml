import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io

// dim-debug — standalone Quickshell debug window for the dimd daemon.
// Install: symlink this dir to ~/.config/quickshell/dim-debug and drop
// dim-debug.desktop into ~/.local/share/applications/.
// Shows: daemon status + controls, live state.json, decisions/session/
// corrections tails, agent tasks. Polls on a timer — no daemon coupling.

FloatingWindow {
  id: win
  title: "Dim Debug"
  minimumSize: Qt.size(760, 560)
  color: "#1a1b26"

  readonly property string rtDir: {
    var rd = Quickshell.env("XDG_RUNTIME_DIR");
    if (!rd || rd.length === 0) rd = "/tmp";
    return rd + "/dim-agent";
  }
  readonly property string dataDir: Quickshell.env("HOME") + "/.local/share/dim-agent"

  property var stateObj: ({})
  property string decisionsTail: ""
  property string sessionTail: ""
  property string correctionsTail: ""

  function dimd(cmd) {
    if (cmdProc.running) return;
    cmdProc.command = ["dimd", cmd];
    cmdProc.running = true;
  }

  Process {
    id: cmdProc
    command: ["dimd", "status"]
  }

  FileView {
    id: stateView
    path: win.rtDir + "/state.json"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      try { win.stateObj = JSON.parse(stateView.text()); }
      catch (e) { win.stateObj = {"status": "parse error"}; }
    }
  }
  FileView {
    id: decisionsView
    path: win.dataDir + "/decisions.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var lines = decisionsView.text().split("\n").filter(function(l) {
        return l.trim().length > 0; });
      win.decisionsTail = lines.slice(-30).reverse().join("\n");
    }
  }
  FileView {
    id: sessionView
    path: win.dataDir + "/session.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var lines = sessionView.text().split("\n").filter(function(l) {
        return l.trim().length > 0; });
      win.sessionTail = lines.slice(-20).reverse().join("\n");
    }
  }
  FileView {
    id: corrView
    path: win.dataDir + "/corrections.jsonl"
    watchChanges: true
    onFileChanged: reload()
    onLoaded: {
      var lines = corrView.text().split("\n").filter(function(l) {
        return l.trim().length > 0; });
      win.correctionsTail = lines.slice(-15).reverse().join("\n");
    }
  }

  Timer {
    interval: 500
    running: true
    repeat: true
    onTriggered: {
      stateView.reload();
      decisionsView.reload();
      sessionView.reload();
      corrView.reload();
    }
  }

  ColumnLayout {
    anchors.fill: parent
    anchors.margins: 12
    spacing: 10

    // Header: status orb + controls
    RowLayout {
      spacing: 12
      Rectangle {
        width: 16; height: 16; radius: 8
        color: {
          var s = win.stateObj.status || "offline";
          if (s === "error") return "#e05555";
          if (s === "listening") return "#7aa2f7";
          if (["transcribing","deciding"].indexOf(s) >= 0) return "#bb9af7";
          if (["acting","awaiting_choice"].indexOf(s) >= 0) return "#9ece6a";
          return "#565f89";
        }
      }
      Text {
        text: "dimd — " + (win.stateObj.status || "offline")
        color: "#c0caf5"; font.pixelSize: 16; font.bold: true
      }
      Item { Layout.fillWidth: true }
      Repeater {
        model: ["listen", "status", "learn", "harness", "stop"]
        Rectangle {
          height: 26; width: bLbl.implicitWidth + 16; radius: 6
          color: "#283457"
          Text { id: bLbl; anchors.centerIn: parent; text: modelData
                 color: "#c0caf5"; font.pixelSize: 12 }
          MouseArea { anchors.fill: parent
                      onClicked: win.dimd(modelData) }
        }
      }
    }

    // Live state
    Rectangle {
      Layout.fillWidth: true
      height: stCol.implicitHeight + 16
      color: "#24283b"; radius: 8
      Column {
        id: stCol
        anchors.fill: parent; anchors.margins: 8; spacing: 4
        Text { text: "transcript: " + (win.stateObj.transcript || "—")
               color: "#9aa5ce"; font.pixelSize: 12; width: stCol.width
               wrapMode: Text.Wrap }
        Text { text: "answer: " + (win.stateObj.answer || "—")
               color: "#c0caf5"; font.pixelSize: 12; width: stCol.width
               wrapMode: Text.Wrap }
        Text { text: "result: " + (win.stateObj.result || "—")
               color: "#9aa5ce"; font.pixelSize: 12; width: stCol.width
               wrapMode: Text.Wrap }
        Text { visible: (win.stateObj.error || "") !== ""
               text: "error: " + (win.stateObj.error || "")
               color: "#e05555"; font.pixelSize: 12; width: stCol.width
               wrapMode: Text.Wrap }
        Text { text: "choices: " + JSON.stringify(win.stateObj.choices || [])
               color: "#e0af68"; font.pixelSize: 11; width: stCol.width
               wrapMode: Text.Wrap }
        Text { text: "tasks: " + JSON.stringify(win.stateObj.tasks || {})
               color: "#7dcfff"; font.pixelSize: 11; width: stCol.width
               wrapMode: Text.Wrap }
      }
    }

    // Log tails — three tabs would be nice; stacked sections are enough
    Flickable {
      Layout.fillWidth: true
      Layout.fillHeight: true
      contentHeight: logsCol.implicitHeight
      clip: true
      Column {
        id: logsCol
        width: parent.width
        spacing: 10

        Text { text: "— decisions.jsonl (latest first) —"
               color: "#565f89"; font.pixelSize: 11 }
        Text { text: win.decisionsTail || "(empty)"
               color: "#9aa5ce"; font.pixelSize: 10
               font.family: "monospace"; width: logsCol.width
               wrapMode: Text.WrapAnywhere }
        Text { text: "— session.jsonl —"
               color: "#565f89"; font.pixelSize: 11 }
        Text { text: win.sessionTail || "(empty)"
               color: "#9aa5ce"; font.pixelSize: 10
               font.family: "monospace"; width: logsCol.width
               wrapMode: Text.WrapAnywhere }
        Text { text: "— corrections.jsonl —"
               color: "#565f89"; font.pixelSize: 11 }
        Text { text: win.correctionsTail || "(empty)"
               color: "#9aa5ce"; font.pixelSize: 10
               font.family: "monospace"; width: logsCol.width
               wrapMode: Text.WrapAnywhere }
      }
    }
  }
}
