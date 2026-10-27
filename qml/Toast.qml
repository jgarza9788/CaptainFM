import QtQuick
import CaptainFM

// Short-lived message in the bottom-right corner.
Rectangle {
    id: toast
    property color tint: Style.accent
    opacity: 0
    visible: opacity > 0
    radius: Style.radius
    color: Style.surface
    border { width: 1; color: tint }
    width: label.implicitWidth + Style.pad * 3
    height: label.implicitHeight + Style.pad * 1.6

    function show(message, kind) {
        tint = kind === "error" ? Style.red : (kind === "warn" ? Style.yellow : Style.accent)
        label.text = message
        fade.restart()
    }

    Text {
        id: label
        anchors.centerIn: parent
        textFormat: Text.PlainText
        color: Style.fg
        font { family: Style.font; pixelSize: Style.fontSize }
    }

    SequentialAnimation {
        id: fade
        NumberAnimation { target: toast; property: "opacity"; to: 1; duration: Style.anim(120) }
        PauseAnimation { duration: 2600 }
        NumberAnimation { target: toast; property: "opacity"; to: 0; duration: Style.anim(400) }
    }
}
