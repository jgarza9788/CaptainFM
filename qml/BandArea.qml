import QtQuick
import CaptainFM

// Empty space in a view: a click drops the selection, a drag draws a rubber band (Ctrl+drag adds to the
// selection). It sits under the rows, so it only gets presses that no row took.
MouseArea {
    id: band
    property Flickable view: null
    property var host: null     // the Pane
    property var model: null    // its DirModel
    anchors.fill: parent
    z: -1
    acceptedButtons: Qt.LeftButton | Qt.RightButton
    preventStealing: true
    property point start: Qt.point(0, 0)      // content coordinates
    property point end: Qt.point(0, 0)
    property point pointer: Qt.point(0, 0)    // own coordinates, for scrolling at the edges
    property bool banding: false
    property var baseRows: []
    readonly property bool outside: pointer.x < 0 || pointer.y < 0 || pointer.x > width || pointer.y > height
    // Declared inside the view, which would put it in the scrolling content: it belongs to the view itself.
    Component.onCompleted: parent = view

    onPressed: (m) => {
        host.activated()
        start = mapToItem(view.contentItem, m.x, m.y)
        end = start
        banding = false
        const rows = []
        if (m.button === Qt.LeftButton && (m.modifiers & Qt.ControlModifier))
            for (const p of JSON.parse(model.selectedPaths())) { const r = model.rowOfPath(p); if (r >= 0) rows.push(r) }
        baseRows = rows
    }
    onPositionChanged: (m) => {
        if (!(pressedButtons & Qt.LeftButton)) return
        pointer = Qt.point(m.x, m.y)
        const p = mapToItem(view.contentItem, m.x, m.y)
        if (!banding && Math.abs(p.x - start.x) + Math.abs(p.y - start.y) < 6) return
        if (!banding) host.endVisual()
        banding = true
        end = p
        host.bandSelect(view, start, end, baseRows)
    }
    onReleased: (m) => {
        if (banding) { banding = false; return }
        host.clickEmpty(m.modifiers)
    }
    onCanceled: banding = false

    // Holding the band past an edge scrolls the view.
    Timer {
        interval: 30; repeat: true
        running: band.banding && band.outside
        onTriggered: {
            const v = band.view, q = band.pointer
            const dy = q.y < 0 ? q.y : (q.y > band.height ? q.y - band.height : 0)
            const dx = q.x < 0 ? q.x : (q.x > band.width ? q.x - band.width : 0)
            if (v.contentHeight > v.height)
                v.contentY = Math.max(v.originY, Math.min(v.originY + v.contentHeight - v.height, v.contentY + dy / 2))
            if (v.contentWidth > v.width)
                v.contentX = Math.max(v.originX, Math.min(v.originX + v.contentWidth - v.width, v.contentX + dx / 2))
            band.end = band.mapToItem(v.contentItem, q.x, q.y)
            host.bandSelect(v, band.start, band.end, band.baseRows)
        }
    }
    Rectangle {
        Component.onCompleted: parent = band.view
        z: 20
        visible: band.banding
        x: Math.min(band.start.x, band.end.x) + band.view.contentItem.x
        y: Math.min(band.start.y, band.end.y) + band.view.contentItem.y
        width: Math.abs(band.end.x - band.start.x)
        height: Math.abs(band.end.y - band.start.y)
        color: Style.alpha(Style.accent, 0.14)
        border { width: 1; color: Style.accent }
    }
}
