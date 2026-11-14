import QtQuick
import CaptainFM

// "Copy to docs", "Move to Trash", ...: what releasing the drag here would do.
Rectangle {
    property var hint: ({ kind: "", text: "" })
    width: content.implicitWidth + Style.pad * 2.5
    height: Math.round(Style.fontSize * 1.9)
    radius: height / 2
    color: Style.accent
    Row {
        id: content
        anchors.centerIn: parent
        spacing: Style.pad / 1.5
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: Dnd.glyph(hint.kind)
            color: Style.bg
            font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.15) }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: hint.text
            color: Style.bg
            font { family: Style.font; pixelSize: Style.smallFont; bold: true }
        }
    }
}
