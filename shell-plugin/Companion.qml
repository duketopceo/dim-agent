import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

// Companion — the Clicky-style orb for io.github.duketopceo.dim.
// A small floating orb anchored bottom-right that breathes while Dim
// works; clicking expands a card with transcript, answer, and choice
// buttons. Bound to $XDG_RUNTIME_DIR/dim-agent/state.json like
// DimService, so it works whether or not the bar widget is placed.
Item {
  id: root

  property var shell: null
  property var manifest: null
  property bool opened: true   // orb is persistent; expanded card toggles
  property bool expanded: false

  property string status: "offline"
  property string transcript: ""
  property string answer: ""
  property string result: ""
  property var choices: []
  property real level: 0.0
  property string error: ""

  readonly property string stateFile: {
    var rd = Quickshell.env("XDG_RUNTIME_DIR");
    if (!rd || rd.length === 0) rd = "/tmp";
    return rd + "/dim-agent/state.json";
  }

  function open(payload) { root.opened = true; }
  function close() { root.opened = false; }
  function toggle() { root.opened ? root.close() : root.open(""); }
  function dismiss() {
    root.expanded = false;
    root.close();
  }

  function sendChoice(pick) {
    choiceProc.command = ["dimd", "choice", pick];
    choiceProc.running = true;
  }

  readonly property color orbColor: {
    if (root.status === "error" || root.error.length > 0) return "#e05555";
    if (root.status === "listening") return "#7aa2f7";
    if (["transcribing", "deciding"].indexOf(root.status) >= 0) return "#bb9af7";
    if (["acting", "awaiting_choice"].indexOf(root.status) >= 0) return "#9ece6a";
    if (root.status === "offline") return "#565f89";
    return "#3b4261";
  }

  FileView {
    id: stateView
    path: root.stateFile
    watchChanges: true
    onFileChanged: reload()
    onLoadFailed: root.status = "offline"
    onLoaded: {
      try {
        var s = JSON.parse(stateView.text());
        root.status = s.status || "idle";
        root.transcript = s.transcript || "";
        root.answer = s.answer || "";
        root.result = s.result || "";
        root.choices = s.choices || [];
        root.level = s.level || 0.0;
        root.error = s.error || "";
      } catch (e) { root.status = "offline"; }
    }
  }

  Timer {
    interval: 500
    running: true
    repeat: true
    onTriggered: stateView.reload()
  }

  Process {
    id: choiceProc
    command: ["dimd", "choice", ""]
  }

  // Orb
  Rectangle {
    id: orb
    visible: root.opened
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 24
    width: root.expanded ? 0 : 44
    height: root.expanded ? 0 : 44
    radius: 22
    color: root.orbColor
    opacity: 0.9

    SequentialAnimation on scale {
      running: root.status === "listening"
      loops: Animation.Infinite
      NumberAnimation { to: 1.15; duration: 700; easing.type: Easing.InOutSine }
      NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutSine }
    }

    Text {
      anchors.centerIn: parent
      text: "◉"
      font.pixelSize: 18
      color: "#c0caf5"
    }

    MouseArea {
      anchors.fill: parent
      onClicked: root.expanded = true
    }
  }

  // Expanded card
  Rectangle {
    visible: root.opened && root.expanded
    anchors.right: parent.right
    anchors.bottom: parent.bottom
    anchors.margins: 24
    width: 340
    height: cardCol.implicitHeight + 28
    radius: 12
    color: "#1a1b26"
    border.color: "#3b4261"
    border.width: 1

    MouseArea {
      anchors.fill: parent
      onClicked: root.expanded = false
    }

    Column {
      id: cardCol
      anchors.fill: parent
      anchors.margins: 14
      spacing: 8

      Row {
        spacing: 8
        Rectangle {
          width: 10; height: 10; radius: 5
          color: root.orbColor
          anchors.verticalCenter: parent.verticalCenter
        }
        Text {
          text: "Dim — " + root.status
          color: "#c0caf5"
          font.pixelSize: 13
          font.bold: true
        }
      }

      Text {
        visible: root.transcript.length > 0
        width: parent.width
        wrapMode: Text.Wrap
        text: "“" + root.transcript + "”"
        color: "#9aa5ce"
        font.pixelSize: 12
        font.italic: true
      }

      Text {
        visible: root.answer.length > 0
        width: parent.width
        wrapMode: Text.Wrap
        text: root.answer
        color: "#c0caf5"
        font.pixelSize: 13
      }

      Text {
        visible: root.answer.length === 0 && root.result.length > 0
        width: parent.width
        wrapMode: Text.Wrap
        text: root.result
        color: "#9aa5ce"
        font.pixelSize: 12
      }

      Text {
        visible: root.error.length > 0
        width: parent.width
        wrapMode: Text.Wrap
        text: root.error
        color: "#e05555"
        font.pixelSize: 12
      }

      Flow {
        width: parent.width
        spacing: 6
        visible: root.choices.length > 0
        Repeater {
          model: root.choices
          Rectangle {
            height: 28
            width: Math.min(160, choiceLabel.implicitWidth + 18)
            radius: 6
            color: "#283457"
            Text {
              id: choiceLabel
              anchors.centerIn: parent
              text: modelData
              color: "#c0caf5"
              font.pixelSize: 11
              elide: Text.ElideRight
              width: 140
            }
            MouseArea {
              anchors.fill: parent
              onClicked: {
                root.sendChoice(modelData);
                root.expanded = false;
              }
            }
          }
        }
      }

      Text {
        text: "click to collapse · Super+D to talk"
        color: "#565f89"
        font.pixelSize: 10
      }
    }
  }
}
