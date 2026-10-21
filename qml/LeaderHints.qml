import QtQuick
import CaptainFM

// Which-key popup: what can follow the keys typed so far, in the accent-bordered style of the mockups.
Rectangle {
    id: hints
    property var items: []        // [{key, label, group}]
    property string prefix: ""
    visible: false
    opacity: visible ? 1 : 0
    color: Style.bgDark
    radius: 4
    border { width: 1; color: Style.accent }
    width: Math.min(parent.width - Style.pad * 4, Math.round(Style.fontSize * 24))
    height: header.height + list.implicitHeight + Style.pad * 3

    Behavior on opacity { NumberAnimation { duration: Style.anim(100) } }

    Text {
        id: header
        x: Style.pad * 1.5; y: Style.pad * 1.2
        textFormat: Text.RichText
        text: "<font color=\"" + Style.accent + "\">" + hints.prefix + " › " + (hints.prefix === "space" ? "leader" : "keys") + "</font>"
        font { family: Style.font; pixelSize: Style.fontSize }
    }

    Column {
        id: list
        x: Style.pad * 1.5
        y: header.y + header.height + Style.pad
        spacing: 2
        Repeater {
            model: hints.items.slice(0, 12)
            delegate: Row {
                required property var modelData
                spacing: Style.pad * 2
                Text {
                    width: Style.fontSize * 3.2
                    text: parent.modelData.key
                    color: Style.yellow
                    font { family: Style.font; pixelSize: Style.fontSize; bold: true }
                }
                Text {
                    text: (parent.modelData.group ? "+ " : "") + parent.modelData.label
                    color: Style.fgLight
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
            }
        }
    }
}
