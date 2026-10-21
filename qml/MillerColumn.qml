import QtQuick
import CaptainFM

// A read-only neighbouring column in Miller view: the parent folder or the folder under the cursor.
Item {
    id: col

    property string path: ""
    property string highlightName: ""
    property bool showHidden: false
    clip: true

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
        }
    }
}
