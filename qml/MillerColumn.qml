import QtQuick
import CaptainFM

// A read-only neighbouring column in Miller view: the parent folder or the folder under the cursor.
Item {
    id: col

    property string path: ""
    property string highlightName: ""
    property bool showHidden: false
    clip: true
    signal dragRequested(string path, bool isDir)
    signal openRequested(string path)

    function childPath(name) { return col.path === "/" ? "/" + name : col.path + "/" + name }
    function baseName(p) { return p === "/" ? "/" : p.substring(p.lastIndexOf("/") + 1) }

    onPathChanged: if (path !== "") model.load(path)
    onShowHiddenChanged: model.setShowHiddenFiles(showHidden)

    DirModel {
        id: model
        lite: true
        onLoaded: highlightTimer.restart()
    }

    Timer {
        id: highlightTimer
        interval: 0
        onTriggered: {
            const row = col.highlightName !== "" ? model.indexOf(col.highlightName) : -1
            view.currentIndex = row
            if (row >= 0) view.positionViewAtIndex(row, ListView.Center)
        }
    }
    onHighlightNameChanged: highlightTimer.restart()

    Rectangle { anchors.fill: parent; color: Style.bgDark }
    Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: 1; color: Style.surface }

    // Dropping on empty space (or a file) puts the files in this column's folder.
    DropArea {
        id: colDrop
        anchors.fill: parent
        enabled: col.path !== ""
        property var hint: ({ kind: "", text: "" })
        onEntered: (drag) => hint = Dnd.hint(drag, col.path, col.baseName(col.path), false)
        onPositionChanged: (drag) => hint = Dnd.hint(drag, col.path, col.baseName(col.path), false)
        onDropped: (drop) => Dnd.accept(drop, col.path, false)
        Rectangle {
            anchors.fill: parent
            visible: colDrop.containsDrag
            color: "transparent"
            border { width: 2; color: Style.accent }
            z: 5
        }
        Chip { visible: colDrop.containsDrag && colDrop.hint.kind !== ""; hint: colDrop.hint
               anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Style.pad } }
    }

    component Chip: Rectangle {
        property var hint: ({ kind: "", text: "" })
        z: 8
        width: chipRow.implicitWidth + Style.pad * 2.5
        height: Math.round(Style.fontSize * 1.9)
        radius: height / 2
        color: Style.accent
        Row {
            id: chipRow
            anchors.centerIn: parent
            spacing: Style.pad / 1.5
            Text { anchors.verticalCenter: parent.verticalCenter; text: Dnd.glyph(hint.kind); color: Style.bg
                   font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.15) } }
            Text { anchors.verticalCenter: parent.verticalCenter; text: hint.text; color: Style.bg
                   font { family: Style.font; pixelSize: Style.smallFont; bold: true } }
        }
    }

    ListView {
        id: view
        anchors.fill: parent
        model: model
        visible: col.path !== ""
        boundsBehavior: Flickable.StopAtBounds
        currentIndex: -1
        interactive: true
        delegate: Item {
            id: row
            required property int index
            required property string name
            required property bool isDir
            required property bool isLink
            required property bool hidden
            width: ListView.view.width
            height: Style.rowHeight
            readonly property bool current: ListView.isCurrentItem
            Rectangle {
                anchors { fill: parent; leftMargin: 3; rightMargin: 3 }
                radius: 4
                color: row.current ? Style.selection : "transparent"
            }
            Text {
                id: ic
                x: Style.pad
                width: Style.fontSize * 1.5
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignHCenter
                text: Style.icon(row.name, row.isDir, row.isLink)
                color: Style.kindColor(Style.kind(row.name, row.isDir, row.isLink))
                font { family: Style.font; pixelSize: Style.fontSize }
            }
            Text {
                anchors { left: ic.right; leftMargin: Style.pad / 2; right: parent.right; rightMargin: Style.pad
                          verticalCenter: parent.verticalCenter }
                text: row.name
                elide: Text.ElideRight
                color: row.isDir ? Style.accent : (row.hidden ? Style.fgDim : Style.fgLight)
                font { family: Style.font; pixelSize: Style.fontSize - 1 }
            }

            DragHandler {
                target: null
                onActiveChanged: if (active) col.dragRequested(col.childPath(row.name), row.isDir)
            }

            DropArea {
                anchors.fill: parent
                enabled: row.isDir
                z: 2
                property var hint: ({ kind: "", text: "" })
                onEntered: (drag) => hint = Dnd.hint(drag, col.childPath(row.name), row.name, false)
                onPositionChanged: (drag) => hint = Dnd.hint(drag, col.childPath(row.name), row.name, false)
                onDropped: (drop) => Dnd.accept(drop, col.childPath(row.name), false)
                HoverOpen {
                    hovering: parent.containsDrag
                    onOpened: col.openRequested(col.childPath(row.name))
                }
                Rectangle {
                    anchors.fill: parent
                    visible: parent.containsDrag
                    color: Style.alpha(Style.accent, 0.25)
                }
                Chip {
                    visible: parent.containsDrag && parent.hint.kind !== ""
                    hint: parent.hint
                    anchors { right: parent.right; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
                }
            }
        }
    }
}
