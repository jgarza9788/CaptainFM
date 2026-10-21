import QtQuick
import CaptainFM

// The picture that follows the pointer while dragging files: the first name, and "+N" for the rest.
// It lives off screen and is grabbed into the drag image.
Rectangle {
    id: ghost
    property string name: ""
    property int count: 1
    property string glyph: ""
    x: -4000
    y: -4000
    width: row.implicitWidth + Style.pad * 2
    height: Math.round(Style.rowHeight * 1.1)
    radius: 6
    color: Style.alpha(Style.bgDark, 0.94)
    border { width: 1; color: Style.accent }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: Style.pad
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: ghost.glyph
            color: Style.accent
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Math.round(260 * Style.zoom))
            elide: Text.ElideMiddle
            text: ghost.name
            color: Style.fg
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        Rectangle {
            visible: ghost.count > 1
            anchors.verticalCenter: parent.verticalCenter
            width: badge.implicitWidth + Style.pad
            height: badge.implicitHeight + 2
            radius: height / 2
            color: Style.accent
            Text {
                id: badge
                anchors.centerIn: parent
                text: "+" + (ghost.count - 1)
                color: Style.bg
                font { family: Style.font; pixelSize: Style.smallFont; bold: true }
            }
        }
    }
}
