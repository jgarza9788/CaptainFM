import QtQuick
import CaptainFM

// A one-line command bar (rename, new file, filter, search) and a y/n confirmation bar.
Rectangle {
    id: bar
    visible: false
    height: Math.round(34 * Style.zoom)
    color: Style.bgDarker

    property string label: ""
    property bool confirmMode: false
    property var opts: ({})
    readonly property bool active: visible
    readonly property alias text: input.text

    signal closed()

    function open(o) {
        opts = o
        label = o.label ?? ""
        confirmMode = o.confirm === true
        input.text = o.text ?? ""
        input.echoMode = o.password === true ? TextInput.Password : TextInput.Normal
        visible = true
        if (confirmMode) { catcher.forceActiveFocus(); return }
        input.forceActiveFocus()
        if (o.selectStem) {
            const dot = input.text.lastIndexOf(".")
            input.select(0, dot > 0 ? dot : input.text.length)
        } else input.cursorPosition = input.text.length
    }

    function close() {
        visible = false
        closed()
    }

    function accept() {
        const o = opts
        const t = input.text
        close()
        if (o.accept) o.accept(t)
    }

    function cancel() {
        const o = opts
        close()
        if (o.cancel) o.cancel()
    }

    Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Style.surface }

    Text {
        id: labelText
        anchors { left: parent.left; leftMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
        text: bar.label
        color: bar.confirmMode ? Style.yellow : Style.accent
        font { family: Style.font; pixelSize: Style.fontSize; bold: true }
    }

    TextInput {
        id: input
        visible: !bar.confirmMode
        anchors { left: labelText.right; leftMargin: Style.pad; right: parent.right; rightMargin: Style.pad
                  verticalCenter: parent.verticalCenter }
        color: Style.fg
        selectionColor: Style.accent
        selectedTextColor: Style.bgDarker
        font { family: Style.font; pixelSize: Style.fontSize }
        clip: true
        onTextChanged: if (bar.visible && bar.opts.live) bar.opts.live(text)
        onAccepted: bar.accept()
        Keys.onReturnPressed: (e) => { e.accepted = true; bar.accept() }
        Keys.onEnterPressed: (e) => { e.accepted = true; bar.accept() }
        Keys.onEscapePressed: bar.cancel()
        Keys.onPressed: (e) => {
            if (bar.opts.onKey && bar.opts.onKey(e)) e.accepted = true
        }
    }

    Item {
        id: catcher
        visible: bar.confirmMode
        focus: bar.confirmMode
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Y || e.key === Qt.Key_Return || e.key === Qt.Key_Enter) bar.accept()
            else if (e.key === Qt.Key_N || e.key === Qt.Key_Escape) bar.cancel()
            e.accepted = true
        }
        Text {
            anchors { left: parent.left; verticalCenter: parent.verticalCenter }
            x: labelText.width
            text: "   y / n"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
    }
}
