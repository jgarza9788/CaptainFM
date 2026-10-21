import QtQuick
import CaptainFM

// Put inside a DropArea: while a drag rests on it for Prefs.v.dragOpenDelay seconds, `opened()` fires once.
// A thin bar along the bottom fills up so the wait is visible.
Item {
    id: root
    property bool hovering: false
    readonly property real delay: Number(Prefs.v.dragOpenDelay) * 1000
    signal opened()

    anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
    height: 2

    Rectangle {
        id: bar
        height: parent.height
        color: Style.accent
        visible: width > 0
    }

    NumberAnimation {
        id: fill
        target: bar
        property: "width"
        from: 0
        to: root.width
        duration: root.delay
        running: root.hovering && root.delay > 0
        onRunningChanged: if (!running) bar.width = 0
        onFinished: root.opened()
    }
}
