import QtQuick
import CaptainFM

// Copies and moves in progress: a bar, speed and time left for each, with pause and cancel. It opens by
// itself when a transfer starts and closes a few seconds after the last one ends, unless it was opened by hand.
Rectangle {
    id: panel
    property var app
    property var list: []
    property bool pinned: false
    property bool lingering: false
    readonly property var active: list.filter(t => t.state === "queued" || t.state === "running")
    readonly property bool wanted: pinned || active.length > 0 || lingering

    visible: wanted && list.length > 0
    width: Math.min(parent.width - Style.pad * 4, Math.round(460 * Style.zoom))
    height: Math.min(parent.height - Style.pad * 6, head.height + col.implicitHeight + Style.pad)
    radius: 6
    color: Style.bgDark
    border { width: 1; color: Style.accent }

    function refresh() {
        let next = []
        try { next = JSON.parse(Backend.transfers()) } catch (e) { return }
        const before = active.length
        list = next
        if (before > 0 && active.length === 0) { lingering = true; hideTimer.restart() }
    }

    function clear() { Backend.transferClear(); pinned = false; lingering = false; refresh() }
    function toggle() {
        if (visible) { pinned = false; lingering = false; if (active.length === 0) Backend.transferClear() }
        else pinned = true
        refresh()
    }
    function pauseAll() {
        const running = active.filter(t => t.state === "running")
        const resume = running.length > 0 && running.every(t => t.paused)
        for (const t of active) Backend.transferPause(t.id, !resume)
        refresh()
    }
    function cancelAll() { for (const t of active) Backend.transferCancel(t.id); refresh() }

    function rate(bytes) { return Style.humanSize(Math.round(bytes)) + "/s" }
    function clock(s) {
        if (s < 0) return ""
        const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60), sec = s % 60
        return (h > 0 ? h + ":" + String(m).padStart(2, "0") : m) + ":" + String(sec).padStart(2, "0")
    }
    function title(t) {
        const verb = t.label === "Moved" ? "Moving" : "Copying"
        return verb + " " + (t.itemsTotal === 1 ? "1 item" : t.itemsTotal + " items") + " to " + t.dest.split("/").filter(s => s !== "").pop()
    }
    function detail(t) {
        if (t.state === "queued") return "Waiting"
        if (t.state === "cancelled") return "Cancelled"
        if (t.state === "done") return "Done"
        if (t.paused) return "Paused · " + Style.humanSize(t.bytesDone) + " of " + Style.humanSize(t.bytesTotal)
        const parts = []
        if (t.bytesTotal > 0) parts.push(Style.humanSize(t.bytesDone) + " of " + Style.humanSize(t.bytesTotal))
        if (t.speed > 0) parts.push(rate(t.speed))
        if (t.eta >= 0) parts.push(clock(t.eta) + " left")
        return parts.join(" · ")
    }

    Timer {
        // Only while something is copying (or just finished); Backend.transferStarted wakes it up.
        interval: 400; running: panel.active.length > 0 || panel.lingering; repeat: true
        onTriggered: panel.refresh()
    }
    Timer {
        id: hideTimer
        interval: 4000
        onTriggered: panel.lingering = false
    }

    Item {
        id: head
        width: parent.width; height: Math.round(34 * Style.zoom)
        Text {
            x: Style.pad * 1.5; anchors.verticalCenter: parent.verticalCenter
            text: panel.active.length > 0 ? "Transfers" : "Transfers · finished"
            color: Style.fg
            font { family: Style.font; pixelSize: Style.fontSize; bold: true }
        }
        Text {
            anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            text: "Clear"
            visible: panel.active.length === 0
            color: closeArea.containsMouse ? Style.accent : Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
            MouseArea { id: closeArea; anchors.fill: parent; anchors.margins: -6; hoverEnabled: true; onClicked: panel.clear() }
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }
    }

    Column {
        id: col
        anchors { left: parent.left; right: parent.right; top: head.bottom; topMargin: Style.pad / 2 }
        Repeater {
            model: panel.list.slice().reverse()
            delegate: Item {
                id: row
                required property var modelData
                readonly property bool over: modelData.state === "done" || modelData.state === "cancelled"
                width: col.width
                height: Math.round(66 * Style.zoom)

                Text {
                    x: Style.pad * 1.5; y: Style.pad / 2
                    width: parent.width - Style.pad * 3 - buttons.width
                    text: panel.title(row.modelData)
                    elide: Text.ElideRight
                    color: row.over ? Style.fgDim : Style.fg
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
                Row {
                    id: buttons
                    anchors { right: parent.right; rightMargin: Style.pad * 1.5; top: parent.top; topMargin: Style.pad / 2 }
                    spacing: Style.pad
                    visible: !row.over
                    Text {
                        text: row.modelData.paused ? "Resume" : "Pause"
                        visible: row.modelData.state === "running"
                        color: pauseArea.containsMouse ? Style.accent : Style.fgDim
                        font { family: Style.font; pixelSize: Style.smallFont }
                        MouseArea { id: pauseArea; anchors.fill: parent; hoverEnabled: true; onClicked: { Backend.transferPause(row.modelData.id, !row.modelData.paused); panel.refresh() } }
                    }
                    Text {
                        text: "Cancel"
                        color: cancelArea.containsMouse ? Style.red : Style.fgDim
                        font { family: Style.font; pixelSize: Style.smallFont }
                        MouseArea { id: cancelArea; anchors.fill: parent; hoverEnabled: true; onClicked: { Backend.transferCancel(row.modelData.id); panel.refresh() } }
                    }
                }
                Text {
                    x: Style.pad * 1.5; y: Math.round(Style.fontSize * 1.6) + Style.pad / 2
                    width: parent.width - Style.pad * 3
                    text: row.modelData.current
                    visible: !row.over
                    elide: Text.ElideMiddle
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
                Rectangle {
                    x: Style.pad * 1.5; y: Math.round(Style.fontSize * 1.6 + Style.smallFont * 1.5) + Style.pad / 2
                    width: parent.width - Style.pad * 3; height: 5; radius: 2
                    color: Style.surface
                    Rectangle {
                        width: parent.width * row.modelData.fraction; height: parent.height; radius: 2
                        color: row.modelData.state === "cancelled" ? Style.red : (row.modelData.paused ? Style.yellow : Style.accent)
                        Behavior on width { NumberAnimation { duration: Style.anim(300) } }
                    }
                }
                Text {
                    x: Style.pad * 1.5
                    anchors { bottom: parent.bottom; bottomMargin: Style.pad / 2 }
                    text: panel.detail(row.modelData)
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
            }
        }
    }
}
