import QtQuick
import CaptainFM

// The picture that follows the pointer while dragging files: the first name, and "+N" for the rest.
// It lives off screen and is grabbed into the drag image.
// The card sits down and to the right of the pointer inside a transparent margin, so it never covers the
// row (or the drop icon on it) that the pointer is over.
Item {
    id: ghost
    property string name: ""
    property int count: 1
    property string glyph: ""
    x: -4000
    y: -4000
    width: card.width + card.x
    height: card.height + card.y

    Rectangle {
        id: card
        x: Style.pad * 2
        y: Math.round(Style.rowHeight * 1.1)
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
            textFormat: Text.PlainText
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
}
