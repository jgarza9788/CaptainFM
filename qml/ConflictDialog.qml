import QtQuick
import CaptainFM

// "Some of these names are already there": replace them (the old ones go to the trash), skip them, or keep both.
// R, S and K pick at once; Left and Right move, Enter chooses, Esc cancels the whole paste.
Rectangle {
    id: dlg
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(520 * Style.zoom))
    height: head.height + list.height + buttons.height + Style.pad * 2
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.18)
    radius: 6
    color: Style.bgDark
    border { width: 2; color: Style.yellow }
    focus: visible

    property var request: ({})
    property int current: 2
    readonly property var names: request.names ?? []
    readonly property var choices: [
        { policy: "replace", label: "Replace", key: "R", tip: "old ones go to the trash" },
        { policy: "skip", label: "Skip", key: "S", tip: "leave what is there" },
        { policy: "both", label: "Keep both", key: "K", tip: "new ones get (copy)" }
    ]

    signal closed()

    function ask(req) {
        request = req
        current = 2
        visible = true
        forceActiveFocus()
    }
    function close() { visible = false; closed() }
    function choose(i) {
        const r = request
        close()
        Backend.pasteWith(JSON.stringify(r.paths), r.dest, r.cut, choices[i].policy)
    }
    // Test automation: `!clash:left`, `!clash:right`, `!clash:enter`.
    function press(name) {
        if (name === "left") current = Math.max(0, current - 1)
        else if (name === "right") current = Math.min(2, current + 1)
        else if (name === "enter") choose(current)
        else close()
    }

    Keys.onPressed: (e) => {
        switch (e.key) {
        case Qt.Key_Escape: close(); break
        case Qt.Key_Left: case Qt.Key_H: press("left"); break
        case Qt.Key_Right: case Qt.Key_L: case Qt.Key_Tab: press("right"); break
        case Qt.Key_Return: case Qt.Key_Enter: press("enter"); break
        case Qt.Key_R: choose(0); break
        case Qt.Key_S: choose(1); break
        case Qt.Key_K: choose(2); break
        default: return
        }
        e.accepted = true
    }

    Item {
        id: head
        width: parent.width; height: Math.round(58 * Style.zoom)
        Text {
            x: Style.pad * 2; y: Style.pad
            text: dlg.names.length === 1 ? "“" + dlg.names[0] + "” is already in this folder" : dlg.names.length + " names are already in this folder"
            width: parent.width - Style.pad * 4
            elide: Text.ElideMiddle
            color: Style.fg
            font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.15); bold: true }
        }
        Text {
            x: Style.pad * 2; y: Style.pad + Math.round(Style.fontSize * 1.7)
            text: (dlg.request.cut ? "Moving" : "Copying") + " to " + String(dlg.request.dest ?? "").split("/").filter(s => s !== "").pop()
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
    }

    Column {
        id: list
        anchors { left: parent.left; right: parent.right; top: head.bottom }
        visible: dlg.names.length > 1
        height: visible ? implicitHeight : 0
        Repeater {
            model: dlg.names.slice(0, 5)
            delegate: Text {
                required property string modelData
                x: Style.pad * 2; width: list.width - Style.pad * 4
                text: modelData
                elide: Text.ElideMiddle
                color: Style.fg
                font { family: Style.font; pixelSize: Style.fontSize }
            }
        }
        Text {
            visible: dlg.names.length > 5
            x: Style.pad * 2
            text: "and " + (dlg.names.length - 5) + " more"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
    }

    Row {
        id: buttons
        anchors { top: list.bottom; topMargin: Style.pad; horizontalCenter: parent.horizontalCenter }
        spacing: Style.pad
        height: Math.round(52 * Style.zoom)
        Repeater {
            model: dlg.choices
            delegate: Rectangle {
                id: btn
                required property int index
                required property var modelData
                width: Math.round(150 * Style.zoom); height: Math.round(46 * Style.zoom)
                radius: 4
                color: index === dlg.current ? Style.selection : Style.surface
                border { width: index === dlg.current ? 2 : 0; color: Style.accent }
                Text {
                    anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: Style.pad / 2 }
                    text: btn.modelData.label + "  (" + btn.modelData.key + ")"
                    color: Style.fg
                    font { family: Style.font; pixelSize: Style.fontSize; bold: true }
                }
                Text {
                    anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Style.pad / 2 }
                    text: btn.modelData.tip
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont - 1 }
                }
                MouseArea { anchors.fill: parent; onClicked: dlg.choose(btn.index) }
            }
        }
    }
}
