import QtQuick
import CaptainFM

// Jump bar: path completion plus pinned and frecent folders.
Rectangle {
    id: jump
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(680 * Style.zoom))
    height: Math.min(parent.height - y - Style.pad * 2, Math.round(44 * Style.zoom) + list.contentHeight + Style.pad)
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.12)
    radius: Style.radius + 2
    color: Style.bgDark
    border { width: 1; color: Style.surface }

    onVisibleChanged: if (visible) { opacity = 0; fadeIn.restart() }
    NumberAnimation { id: fadeIn; target: jump; property: "opacity"; to: 1; duration: Style.anim(110); easing.type: Easing.OutCubic }

    property var results: []
    property bool picked: false
    readonly property string home: Backend.home()
    signal chosen(string path)
    signal closed()

    function pretty(p) {
        if (p === home) return "~"
        return p.indexOf(home + "/") === 0 ? "~" + p.substring(home.length) : p
    }

    function open(initial) {
        picked = false
        input.text = initial ?? ""
        refresh()
        visible = true
        input.forceActiveFocus()
    }

    function close() { visible = false; closed() }

    function refresh() {
        results = JSON.parse(Backend.jump(input.text))
        list.currentIndex = 0
    }

    function submit() {
        const typed = Backend.expandPath(input.text.trim())
        let target = ""
        if (!picked && typed !== "" && Backend.isDir(typed)) target = typed
        else if (list.currentIndex >= 0 && list.currentIndex < results.length) target = results[list.currentIndex].path
        else if (typed !== "" && Backend.isDir(typed)) target = typed
        if (target === "") return
        close()
        chosen(target)
    }

    function complete() {
        if (list.currentIndex < 0 || list.currentIndex >= results.length) return
        input.text = pretty(results[list.currentIndex].path) + "/"
        input.cursorPosition = input.text.length
        picked = false
    }

    function step(d) {
        picked = true
        list.currentIndex = Math.max(0, Math.min(list.count - 1, list.currentIndex + d))
    }

    Item {
        id: inputRow
        width: parent.width; height: Math.round(40 * Style.zoom)
        Text {
            id: glyph
            anchors { left: parent.left; leftMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            text: "→"
            color: Style.accent
            font { family: Style.font; pixelSize: Style.fontSize + 2; bold: true }
        }
        TextInput {
            id: input
            anchors { left: glyph.right; leftMargin: Style.pad; right: parent.right; rightMargin: Style.pad
                      verticalCenter: parent.verticalCenter }
            color: Style.fg
            selectionColor: Style.accent
            selectedTextColor: Style.bgDarker
            clip: true
            font { family: Style.font; pixelSize: Style.fontSize }
            onTextChanged: { jump.picked = false; jump.refresh() }
            onAccepted: jump.submit()
            Keys.onReturnPressed: (e) => { e.accepted = true; jump.submit() }
            Keys.onEnterPressed: (e) => { e.accepted = true; jump.submit() }
            Keys.onEscapePressed: jump.close()
            Keys.onDownPressed: jump.step(1)
            Keys.onUpPressed: jump.step(-1)
            Keys.onTabPressed: jump.complete()
            Keys.onPressed: (e) => {
                const ctrl = e.modifiers & Qt.ControlModifier
                if (e.key === Qt.Key_PageDown || e.key === Qt.Key_PageUp) {
                    const page = Math.max(1, Math.floor(list.height / Math.max(1, list.currentItem ? list.currentItem.height : Math.round(30 * Style.zoom))) - 1)
                    jump.step(e.key === Qt.Key_PageDown ? page : -page); e.accepted = true
                }
                else if (ctrl && (e.key === Qt.Key_N || e.key === Qt.Key_J)) { jump.step(1); e.accepted = true }
                else if (ctrl && (e.key === Qt.Key_P || e.key === Qt.Key_K)) { jump.step(-1); e.accepted = true }
            }
        }
        Text {
            visible: input.text === ""
            anchors.fill: input
            text: "Jump to a folder: path, pinned, recent, words in order, or re:regex"
            color: Style.fgDim
            font: input.font
            verticalAlignment: Text.AlignVCenter
        }
    }

    Rectangle { y: inputRow.height; width: parent.width; height: 1; color: Style.surface }

    ListView {
        id: list
        x: Style.pad / 2; y: inputRow.height + 2
        width: parent.width - Style.pad; height: parent.height - inputRow.height - 4
        clip: true
        model: jump.results
        boundsBehavior: Flickable.StopAtBounds
        highlightMoveDuration: 0
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
        delegate: Item {
            id: row
            required property int index
            required property var modelData
            width: ListView.view.width
            height: Math.round(28 * Style.zoom)
            readonly property bool current: ListView.isCurrentItem
            Rectangle { anchors.fill: parent; radius: 4; color: row.current ? Style.selection : "transparent" }
            Text {
                id: kindIcon
                x: Style.pad
                anchors.verticalCenter: parent.verticalCenter
                width: Style.fontSize * 1.5
                horizontalAlignment: Text.AlignHCenter
                text: row.modelData.kind === "pinned" ? "★" : (row.modelData.kind === "recent" ? "◷" : (row.modelData.kind === "place" ? "⌂" : "▸"))
                color: row.modelData.kind === "pinned" ? Style.yellow : Style.fgDim
                font { family: Style.font; pixelSize: Style.fontSize }
            }
            Text {
                anchors { left: kindIcon.right; leftMargin: Style.pad; right: parent.right; rightMargin: Style.pad
                          verticalCenter: parent.verticalCenter }
                text: jump.pretty(row.modelData.path)
                elide: Text.ElideLeft
                color: Style.fg
                font { family: Style.font; pixelSize: Style.fontSize }
            }
            MouseArea {
                anchors.fill: parent
                onClicked: { list.currentIndex = row.index; jump.picked = true; jump.submit() }
            }
        }
    }
}
