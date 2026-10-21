import QtQuick
import CaptainFM

// One folder view: header, virtualised list and its own DirModel.
FocusScope {
    id: pane

    property bool active: false
    property bool optHidden: false
    property string optGroupBy: "none"
    property string optSortKey: "name"
    property bool optSortDesc: false
    required property string startPath
    required property string startView
    property string viewMode: startView     // list | tree | miller
    readonly property bool tree: viewMode === "tree"
    readonly property bool miller: viewMode === "miller"
    readonly property bool icons: viewMode === "grid"
    readonly property bool columns: viewMode === "columns"
    readonly property bool grid: icons || columns
    property bool visualMode: false
    property int anchorRow: -1
    property string currentPath: ""       // full path under the cursor ("" when empty)
    property bool currentIsDir: false

    // Where to put the cursor after the next (re)load: a full path wins over a bare name.
    property string pendingPath: ""
    property string pendingName: ""
    property string lastPath: ""
    property var backStack: []
    property var forwardStack: []

    readonly property alias dir: dir
    readonly property alias view: list
    readonly property bool deep: dir.mode === "deep"
    readonly property string home: Backend.home()

    property int paneIndex: 0
    // Picker mode (`--pick`, the file chooser portal): activating a file chooses it instead of opening it.
    readonly property bool pickMode: Qt.application.arguments.indexOf("--pick") >= 0
    signal pickRequested()
    signal activated()
    signal swapRequested(int from, int to)
    signal contextRequested()
    signal zoomRequested(real steps)
    signal cursorMoved()
    signal sortRequested(string key)
    signal scopeToggled()
    property bool showHeader: false
    property string searchText: ""
    readonly property int rowH: Math.round(Style.rowHeight * (deep ? 1.85 : 1))

    function baseName(p) { return p.substring(p.lastIndexOf("/") + 1) }
    function parentOf(p) { const i = p.lastIndexOf("/"); return i <= 0 ? "/" : p.substring(0, i) }
    function pretty(p) {
        if (home !== "" && p === home) return "~"
        if (home !== "" && p.indexOf(home + "/") === 0) return "~" + p.substring(home.length)
        return p
    }

    function syncCurrent() {
        const row = list.currentIndex
        currentPath = row >= 0 ? dir.pathAt(row) : ""
        currentIsDir = row >= 0 && dir.isDirAt(row)
        cursorMoved()
    }

    function ensureVisible(row) {
        // Section headers make row positions non-uniform: let the view place the row.
        if (grid) { grid2.positionViewAtIndex(row, GridView.Contain); return }
        if (optGroupBy !== "none") { list.positionViewAtIndex(row, ListView.Contain); return }
        const top = row * rowH, bottom = top + rowH
        const margin = rowH * 2
        let target = list.contentY
        if (top < list.contentY + margin) target = top - margin
        else if (bottom > list.contentY + list.height - margin) target = bottom - list.height + margin
        target = Math.max(0, Math.min(Math.max(0, list.contentHeight - list.height), target))
        if (Math.abs(target - list.contentY) < 1) return
        scrollAnim.stop()
        scrollAnim.to = target
        scrollAnim.start()
    }

    function setCursor(row) {
        const n = list.count
        if (n === 0) return
        row = Math.max(0, Math.min(n - 1, row))
        list.currentIndex = row
        if (visualMode) {
            dir.clearSelection()
            dir.selectRange(anchorRow, row, true)
        }
        ensureVisible(row)
    }

    // Grid views: vertical / horizontal steps depend on the flow direction.
    readonly property int gridColumns: Math.max(1, Math.floor(grid2.width / grid2.cellWidth))
    readonly property int gridRows: Math.max(1, Math.floor(grid2.height / grid2.cellHeight))
    function moveVertical(sign) { move(sign * (icons ? gridColumns : 1)) }
    function moveHorizontal(sign) { move(sign * (icons ? 1 : gridRows)) }

    function move(delta) { setCursor((list.currentIndex < 0 ? 0 : list.currentIndex) + delta) }
    function pageRows() { return Math.max(1, Math.floor(list.height / rowH)) }

    function startVisual() {
        visualMode = true
        anchorRow = Math.max(0, list.currentIndex)
        dir.selectRange(anchorRow, anchorRow, true)
    }
    function endVisual() { visualMode = false; anchorRow = -1 }

    property double navStart: 0
    readonly property bool debugTiming: Backend.env("CFM_DEBUG") !== ""

    function navigate(path, select) {
        navStart = Date.now()
        if (debugTiming) console.log("[nav] " + path + " startPath=" + startPath)
        if (!path) return
        if (dir.mode === "folder" && dir.path !== "" && path !== dir.path) {
            backStack = backStack.concat([dir.path])
            forwardStack = []
        }
        endVisual()
        pendingName = select ?? ""
        pendingPath = ""
        lastPath = ""
        dir.load(path)
        Backend.visit(path)
    }

    // Tree view: expand a folder, or step into it when already expanded.
    function treeRight() {
        const row = list.currentIndex
        if (row < 0) return
        if (!currentIsDir) { openCurrent(false); return }
        if (!dir.expandRow(row)) setCursor(row + 1)
    }

    // Tree view: collapse, else jump to the parent folder row, else leave the root.
    function treeLeft() {
        const row = list.currentIndex
        if (row >= 0 && dir.collapseRow(row)) return
        const parent = row >= 0 ? dir.parentRow(row) : -1
        if (parent >= 0) setCursor(parent)
        else goUp()
    }

    function goUp() {
        if (deep) { exitDeep(); return }
        if (dir.path === "/") return
        navigate(parentOf(dir.path), baseName(dir.path))
    }

    function goBack() {
        if (backStack.length === 0) return
        const target = backStack[backStack.length - 1]
        forwardStack = forwardStack.concat([dir.path])
        backStack = backStack.slice(0, -1)
        endVisual()
        pendingName = baseName(dir.path)
        dir.load(target)
    }

    function goForward() {
        if (forwardStack.length === 0) return
        const target = forwardStack[forwardStack.length - 1]
        backStack = backStack.concat([dir.path])
        forwardStack = forwardStack.slice(0, -1)
        endVisual()
        pendingName = ""
        dir.load(target)
    }

    function exitDeep() {
        const keep = currentPath
        pendingPath = keep
        dir.deepStop()
    }

    // Enter a folder, or open a file (in deep search, reveal it in its folder instead).
    function openCurrent(reveal) {
        if (currentPath === "") return
        if (currentIsDir && !reveal) { navigate(currentPath); return }
        if (deep && reveal) { navigate(parentOf(currentPath), baseName(currentPath)); return }
        if (pickMode) { pickRequested(); return }
        Backend.openPath(currentPath)
    }

    // ---- drag and drop ---------------------------------------------------------------

    // Paths a drag starting on `row` carries: the whole selection if the row is part of it.
    function dragPaths(row) {
        const path = dir.pathAt(row)
        const sel = JSON.parse(dir.selectedPaths())
        return sel.indexOf(path) >= 0 ? sel : [path]
    }

    Timer { id: restoreTimer; interval: 0; onTriggered: pane.restoreCursor() }

    function restoreCursor() {
        if (debugTiming && navStart > 0) { console.log("[timing] first paint of " + dir.entryCount + " entries in " + (Date.now() - navStart) + " ms"); navStart = 0 }
        let row = -1
        if (pendingPath !== "") row = dir.rowOfPath(pendingPath)
        if (row < 0 && pendingName !== "") row = dir.indexOf(pendingName)
        if (row >= 0) { pendingPath = ""; pendingName = "" }
        else if (lastPath !== "") row = dir.rowOfPath(lastPath)
        list.currentIndex = row >= 0 ? row : (list.count > 0 ? 0 : -1)
        if (row >= 0) ensureVisible(row)
        else list.contentY = 0
        syncCurrent()
    }

    DirModel {
        id: dir
        onLoaded: restoreTimer.restart()
        onEntryCountChanged: pane.syncCurrent()
    }

    NumberAnimation { id: scrollAnim; target: list; property: "contentY"; duration: Style.anim(90); easing.type: Easing.OutCubic }

    onViewModeChanged: dir.setTreeMode(viewMode === "tree")
    onOptHiddenChanged: dir.setShowHiddenFiles(optHidden)
    onOptGroupByChanged: dir.applyGroupBy(optGroupBy)
    onOptSortKeyChanged: dir.setSort(optSortKey, optSortDesc)
    onOptSortDescChanged: dir.setSort(optSortKey, optSortDesc)

    // New panes fade in.
    NumberAnimation on opacity { from: 0; to: 1; duration: Style.anim(160); easing.type: Easing.OutCubic }

    Component.onCompleted: {
        dir.setTreeMode(viewMode === "tree")
        dir.applyGroupBy(optGroupBy)
        dir.setShowHiddenFiles(optHidden)
        dir.setSort(optSortKey, optSortDesc)
        navigate(startPath !== "" ? startPath : Backend.home())
    }

    // ---- look -----------------------------------------------------------------------

    Rectangle {
        anchors.fill: parent
        color: Style.bg
    }

    // Search words to highlight in result names (filters like ext:rs are not words).
    function searchWords() {
        return searchText.split(/\s+/).filter(w => w !== "" && !/^(ext:|type:|newer:|older:|size[<>]|re:)/.test(w))
    }

    // Rich text for `name` with the in-order search words in the accent colour.
    function highlighted(name, color) { return Style.highlightHtml(name, searchWords().join(" "), color) }

    DragGhost { id: ghost }

    // Drags out of the list start from this item, which lives as long as the pane. A row can be destroyed in
    // the middle of a drag (the folder changes under it), and the drag must not hold on to it.
    Item {
        id: dragSource
        width: 1; height: 1
        opacity: 0
        Drag.dragType: Drag.Automatic
        Drag.supportedActions: Qt.CopyAction | Qt.MoveAction | Qt.LinkAction
        Drag.onDragFinished: dragSource.Drag.active = false
    }
    property int dragToken: 0
    property var ghostResult: null      // keeps the grabbed picture alive for as long as the drag uses it

    function beginDrag(rowIndex, fullPath, isDir) {
        const paths = dragPaths(rowIndex)
        const payload = JSON.parse(Backend.dragPayload(JSON.stringify(paths)))
        // The picture under the pointer: the first name, "+N" for the rest.
        ghost.name = baseName(paths[0])
        ghost.count = paths.length
        ghost.glyph = Style.glyph(ghost.name, paths[0] === fullPath && isDir, false)
        const token = dragToken
        ghost.grabToImage((result) => {
            if (token !== dragToken) return
            ghostResult = result
            dragSource.Drag.mimeData = { "text/uri-list": payload.uriList, "text/plain": payload.plain }
            dragSource.Drag.imageSource = result.url
            dragSource.Drag.hotSpot = Qt.point(Style.pad * 2, ghost.height / 2)
            dragSource.Drag.active = true
        })
    }

    // "Copy to docs", "Move to Trash", ...: what releasing the drag here would do.
    component DropChip: Rectangle {
        property var hint: ({ kind: "", text: "" })
        width: content.implicitWidth + Style.pad * 2.5
        height: Math.round(Style.fontSize * 1.9)
        radius: height / 2
        color: Style.accent
        Row {
            id: content
            anchors.centerIn: parent
            spacing: Style.pad / 1.5
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: Dnd.glyph(hint.kind)
                color: Style.bg
                font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.15) }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: hint.text
                color: Style.bg
                font { family: Style.font; pixelSize: Style.smallFont; bold: true }
            }
        }
    }

    // Dragging a pane's grip onto another pane swaps them.
    DropArea {
        id: swapDrop
        anchors.fill: header
        keys: ["captainfm-pane"]
        z: 6
        enabled: pane.showHeader
        onDropped: (drop) => pane.swapRequested(drop.source.fromIndex, pane.paneIndex)
        Rectangle {
            anchors.fill: parent
            visible: swapDrop.containsDrag
            color: Style.alpha(Style.accent, 0.3)
        }
    }

    DropArea {
        id: paneDrop
        anchors.fill: parent
        enabled: !pane.deep
        property var hint: ({ kind: "", text: "" })
        onEntered: (drag) => hint = Dnd.hint(drag, dir.path, pane.baseName(dir.path) || "/", false)
        onPositionChanged: (drag) => hint = Dnd.hint(drag, dir.path, pane.baseName(dir.path) || "/", false)
        onDropped: (drop) => Dnd.accept(drop, dir.path, false)
        Rectangle {
            anchors.fill: parent
            visible: paneDrop.containsDrag
            color: "transparent"
            border { width: 2; color: Style.accent }
            z: 5
        }
        DropChip {
            visible: paneDrop.containsDrag && paneDrop.hint.kind !== ""
            hint: paneDrop.hint
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: Style.pad * 3 }
            z: 7
        }
    }

    // Slim per-pane header, only when several panes share the tab.
    Rectangle {
        id: header
        visible: pane.showHeader
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: pane.showHeader ? Math.round(28 * Style.zoom) : 0
        color: Style.bgDark

        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 2
            color: pane.active ? Style.accent : "transparent"
            Behavior on color { ColorAnimation { duration: Style.anim(120) } }
        }

        Item {
            id: grip
            x: Style.pad / 2
            y: 0
            width: Style.fontSize * 1.2; height: parent.height
            readonly property int fromIndex: pane.paneIndex
            Drag.keys: ["captainfm-pane"]
            Drag.dragType: Drag.Internal
            Drag.active: gripDrag.active
            Drag.hotSpot { x: width / 2; y: height / 2 }
            Text {
                anchors.centerIn: parent
                text: "⠿"
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont }
            }
            DragHandler {
                id: gripDrag
                cursorShape: Qt.ClosedHandCursor
                onActiveChanged: if (!active) { grip.x = Style.pad / 2; grip.y = 0 }
            }
        }

        Breadcrumbs {
            anchors { left: grip.right; leftMargin: Style.pad / 2; verticalCenter: parent.verticalCenter
                      right: info.left; rightMargin: Style.pad }
            height: parent.height
            path: dir.path
            strong: pane.active
            pixelSize: Style.smallFont + 1
            onNavigate: (target) => { pane.activated(); pane.navigate(target) }
        }

        Row {
            id: info
            anchors { right: parent.right; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
            spacing: Style.pad
            Rectangle {
                width: 7; height: 7; radius: 4; anchors.verticalCenter: parent.verticalCenter
                color: Style.yellow
                visible: dir.loading
                SequentialAnimation on opacity {
                    running: dir.loading; loops: Animation.Infinite
                    NumberAnimation { to: 0.2; duration: 450 }
                    NumberAnimation { to: 1.0; duration: 450 }
                }
            }
            Text {
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont }
                text: (dir.selectedCount > 0 ? dir.selectedCount + " selected · " : "")
                      + dir.entryCount + (dir.entryCount === 1 ? " item" : " items")
            }
        }
    }

    // Column titles: name, size, modified (click to sort).
    Item {
        id: colHead
        visible: !pane.deep && !pane.miller && !pane.grid
        anchors { left: parent.left; right: parent.right; top: header.bottom }
        height: visible ? Math.round(26 * Style.zoom) : 0
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }

        component HeadCell: Text {
            property string sortKey: ""
            readonly property bool sorted: sortKey !== "" && pane.optSortKey === sortKey
            color: sorted ? Style.fgLight : Style.fgDim
            text: (label) + (sorted ? (pane.optSortDesc ? " ↑" : " ↓") : "")
            property string label: ""
            font { family: Style.font; pixelSize: Style.smallFont }
            MouseArea {
                anchors { fill: parent; margins: -4 }
                cursorShape: Qt.PointingHandCursor
                onClicked: if (parent.sortKey !== "") pane.sortRequested(parent.sortKey)
            }
        }

        HeadCell { x: Style.pad * 3 + Style.fontSize * 1.6; anchors.verticalCenter: parent.verticalCenter; label: "name"; sortKey: "name" }
        HeadCell {
            visible: list.showSize
            anchors { right: parent.right; rightMargin: (list.showDate ? list.dateWidth + Style.pad * 1.8 : 0) + Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            horizontalAlignment: Text.AlignRight
            width: list.sizeWidth
            label: "size"; sortKey: "size"
        }
        HeadCell {
            visible: list.showDate
            anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            width: list.dateWidth
            label: "modified"; sortKey: "mtime"
        }
    }

    // Search view: scope and filter chips with match statistics.
    Item {
        id: searchBar
        visible: pane.deep
        anchors { left: parent.left; right: parent.right; top: header.bottom }
        height: visible ? Math.round(38 * Style.zoom) : 0
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }

        readonly property var tokens: pane.searchText.split(/\s+/)
        readonly property string extChip: {
            const t = tokens.find(x => x.indexOf("ext:") === 0)
            return t ? t.substring(4).split(",").map(e => "." + e.replace(/^\./, "")).join(" ") : ""
        }
        readonly property string typeChip: {
            const t = tokens.find(x => x.indexOf("type:") === 0)
            return !t ? "all" : (t.endsWith("d") || t.endsWith("dir") ? "folders" : "files")
        }
        readonly property string dateChip: {
            const t = tokens.find(x => x.indexOf("newer:") === 0 || x.indexOf("older:") === 0)
            return t ? t.replace(":", " ") : "any date"
        }

        component Chip: Rectangle {
            id: chip
            property alias text: label.text
            property bool on: false
            signal clicked()
            anchors.verticalCenter: parent.verticalCenter
            height: Math.round(24 * Style.zoom)
            width: label.implicitWidth + Style.pad * 2
            radius: 4
            color: "transparent"
            border { width: 1; color: on ? Style.accent : Style.surface }
            Text {
                id: label
                anchors.centerIn: parent
                color: chip.on ? Style.accent : Style.fgLight
                font { family: Style.font; pixelSize: Style.smallFont + 1 }
            }
            MouseArea { anchors.fill: parent; onClicked: chip.clicked() }
        }

        Row {
            anchors { left: parent.left; leftMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            spacing: Style.pad
            height: parent.height
            Chip { text: pane.pretty(dir.path); on: true; onClicked: pane.scopeToggled() }
            Chip { visible: searchBar.extChip !== ""; text: searchBar.extChip; on: true }
            Chip { text: searchBar.typeChip; on: searchBar.typeChip !== "all" }
            Chip { text: searchBar.dateChip; on: searchBar.dateChip !== "any date" }
        }
        Text {
            anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
            text: dir.entryCount + " matches · " + pane.indexSizeText + " paths · " + dir.searchMs + " ms"
        }
    }

    property string indexSizeText: ""
    function refreshIndexSize() {
        const n = Backend.indexPaths()
        indexSizeText = n >= 1000000 ? (n / 1000000).toFixed(2) + "M" : (n >= 1000 ? (n / 1000).toFixed(1) + "k" : String(n))
    }
    Timer { running: pane.deep; interval: 500; repeat: true; triggeredOnStart: true; onTriggered: pane.refreshIndexSize() }

    MillerColumn {
        id: parentCol
        anchors { left: parent.left; top: colHead.bottom; bottom: parent.bottom }
        width: pane.miller ? Math.round(parent.width * 0.2) : 0
        visible: width > 1
        path: pane.miller && dir.path !== "/" && !pane.deep ? pane.parentOf(dir.path) : ""
        highlightName: pane.baseName(dir.path)
        showHidden: pane.optHidden
        Behavior on width { NumberAnimation { duration: Style.anim(140); easing.type: Easing.OutCubic } }
    }

    MillerColumn {
        id: childCol
        anchors { right: parent.right; top: colHead.bottom; bottom: parent.bottom }
        width: pane.miller && pane.currentIsDir ? Math.round(parent.width * 0.3) : 0
        visible: width > 1
        path: pane.miller && pane.currentIsDir ? pane.currentPath : ""
        showHidden: pane.optHidden
        Behavior on width { NumberAnimation { duration: Style.anim(140); easing.type: Easing.OutCubic } }
    }

    ListView {
        id: list
        // In grid views this list only owns the cursor; it is collapsed so it builds no rows.
        anchors {
            left: parentCol.right; right: childCol.left
            top: pane.deep ? searchBar.bottom : colHead.bottom
        }
        height: pane.grid ? 0 : Math.max(0, pane.height - y)
        visible: !pane.grid
        model: dir
        clip: true
        focus: true
        currentIndex: -1
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        cacheBuffer: pane.rowH * 20

        // Group headers (type / date / size) between rows.
        section.property: pane.optGroupBy !== "none" && !pane.tree ? "groupName" : ""
        section.criteria: ViewSection.FullString
        section.delegate: Item {
            required property string section
            width: ListView.view.width
            height: Math.round(Style.rowHeight * 1.15)
            Text {
                x: Style.pad * 1.5
                anchors { bottom: parent.bottom; bottomMargin: Style.pad / 2 }
                text: parent.section.toUpperCase()
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont; letterSpacing: 1 }
            }
            Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.alpha(Style.surface, 0.7) }
        }

        onCurrentIndexChanged: {
            pane.syncCurrent()
            if (pane.currentPath !== "") pane.lastPath = pane.currentPath
        }

        // Narrow tiles drop columns: no date under ~640 px, no size under ~330 px.
        readonly property bool showSize: width > 330
        readonly property bool showDate: width > 540 && !pane.miller
        readonly property real sizeWidth: Style.fontSize * 4.6
        readonly property real dateWidth: Style.fontSize * 8.6

        delegate: Item {
            id: row
            required property int index
            required property string name
            required property string fullPath
            required property string rel
            required property bool isDir
            required property bool isLink
            required property bool selected
            required property bool hidden
            required property int depth
            required property bool expanded
            required property var size
            required property var mtime

            width: ListView.view ? ListView.view.width : 0
            height: pane.rowH
            readonly property bool current: ListView.isCurrentItem
            readonly property color nameColor: selected ? Style.yellow : (hidden ? Style.fgDim : Style.fg)

            Rectangle {
                anchors { fill: parent; leftMargin: 2; rightMargin: 2 }
                color: row.current ? (pane.active ? Style.selection : Style.alpha(Style.selection, 0.5)) : "transparent"
                Behavior on color { ColorAnimation { duration: Style.anim(80) } }
            }
            // Accent edge on the cursor row.
            Rectangle {
                visible: row.current
                x: 2; width: 2; height: parent.height
                color: pane.active ? Style.accent : Style.fgDim
            }

            readonly property real indent: pane.tree ? row.depth * Style.fontSize * 1.4 : 0

            Text {
                visible: pane.tree && row.isDir && !row.isLink
                x: Style.pad * 1.5 + row.indent - Style.fontSize * 0.1
                anchors.verticalCenter: parent.verticalCenter
                text: row.expanded ? "▾" : "▸"
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont }
            }

            Text {
                id: iconText
                x: Style.pad * 1.5 + row.indent + (pane.tree ? Style.fontSize * 0.9 : 0)
                y: pane.deep ? (parent.height / 2 - height) + height * 0.15 : (parent.height - height) / 2
                width: Style.fontSize * 1.6
                horizontalAlignment: Text.AlignHCenter
                text: pane.dir.mode === "deep" || !Style.nerd ? Style.glyph(row.name, row.isDir, row.isLink) : Style.glyph(row.name, row.isDir, row.isLink)
                color: Style.glyphColor(row.name, row.isDir)
                opacity: row.hidden ? 0.6 : 1
                font { family: Style.font; pixelSize: Style.fontSize }
            }

            Column {
                id: nameCol
                anchors {
                    left: iconText.right; leftMargin: Style.pad * 0.8
                    right: sizeText.visible ? sizeText.left : parent.right; rightMargin: Style.pad
                    verticalCenter: parent.verticalCenter
                }
                clip: true
                Text {
                    width: parent.width
                    textFormat: pane.deep ? Text.RichText : Text.PlainText
                    text: pane.deep ? pane.highlighted(row.name + (row.isDir ? "/" : ""), Style.accent)
                                    : row.name + (row.isDir ? "/" : "")
                    color: row.nameColor
                    elide: Text.ElideRight
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
                Text {
                    visible: pane.deep
                    width: parent.width
                    text: pane.pretty(row.fullPath.substring(0, row.fullPath.lastIndexOf("/"))) + "/"
                    color: Style.fgDim
                    elide: Text.ElideLeft
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
            }

            Text {
                id: sizeText
                visible: list.showSize
                anchors { right: dateText.visible ? dateText.left : parent.right
                          rightMargin: Style.pad * (dateText.visible ? 1.8 : 1.5); verticalCenter: parent.verticalCenter }
                width: list.sizeWidth
                horizontalAlignment: Text.AlignRight
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.fontSize }
                text: row.size >= 0 ? Style.humanSize(row.size) : (dir.loading ? "…" : "")
            }
            Text {
                id: dateText
                visible: list.showDate
                anchors { right: parent.right; rightMargin: Style.pad * 1.5; verticalCenter: parent.verticalCenter }
                width: list.dateWidth
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.fontSize }
                text: Style.humanDate(row.mtime)
            }

            DragHandler {
                id: dragHandler
                target: null
                onActiveChanged: {
                    pane.dragToken++            // a release cancels a drag that is still being prepared
                    if (active) pane.beginDrag(row.index, row.fullPath, row.isDir)
                }
            }

            DropArea {
                anchors.fill: parent
                enabled: row.isDir
                property var hint: ({ kind: "", text: "" })
                onEntered: (drag) => hint = Dnd.hint(drag, row.fullPath, row.name, false)
                onPositionChanged: (drag) => hint = Dnd.hint(drag, row.fullPath, row.name, false)
                onDropped: (drop) => Dnd.accept(drop, row.fullPath, false)
                Rectangle {
                    anchors.fill: parent
                    visible: parent.containsDrag
                    color: Style.alpha(Style.accent, 0.25)
                }
                DropChip {
                    visible: parent.containsDrag && parent.hint.kind !== ""
                    hint: parent.hint
                    anchors { right: parent.right; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
                    z: 8
                }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                onPressed: pane.activated()
                onClicked: (m) => {
                    if (m.button === Qt.RightButton) {
                        // Keep a multi-selection; otherwise act on the row under the pointer.
                        if (!row.selected) pane.setCursor(row.index)
                        else list.currentIndex = row.index
                        pane.contextRequested()
                        return
                    }
                    if (m.button === Qt.MiddleButton) { pane.setCursor(row.index); pane.openCurrent(false); return }
                    if (m.modifiers & Qt.ControlModifier) { dir.toggleSelect(row.index); list.currentIndex = row.index }
                    else if (m.modifiers & Qt.ShiftModifier) {
                        const from = Math.max(0, list.currentIndex)
                        dir.selectRange(from, row.index, true)
                        list.currentIndex = row.index
                    } else {
                        pane.setCursor(row.index)
                        if (Prefs.v.openOnClick === "single") pane.openCurrent(false)
                    }
                }
                onDoubleClicked: if (Prefs.v.openOnClick !== "single") pane.openCurrent(false)
            }
        }

        // Scroll wheel zooms when Ctrl is held.
        WheelHandler {
            acceptedModifiers: Qt.ControlModifier
            onWheel: (e) => pane.zoomRequested(e.angleDelta.y > 0 ? 1 : -1)
        }

        Text {
            anchors.centerIn: parent
            visible: list.count === 0 && !dir.loading
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.fontSize }
            text: dir.filter !== "" || pane.searchText !== "" ? "No matches" : (pane.deep ? "Type to search" : "Empty folder")
        }
    }

    // Icons (thumbnails) and Columns (compact multi-column) views.
    GridView {
        id: grid2
        visible: pane.grid
        anchors {
            left: parentCol.right; right: childCol.left
            top: pane.deep ? searchBar.bottom : colHead.bottom; bottom: parent.bottom
            margins: pane.icons ? Style.pad : 0
        }
        model: pane.grid ? dir : null
        clip: true
        currentIndex: list.currentIndex
        boundsBehavior: Flickable.StopAtBounds
        cellWidth: pane.icons ? Math.round(124 * Style.zoom * Style.density) : Math.round(250 * Style.zoom)
        cellHeight: pane.icons ? Math.round(132 * Style.zoom * Style.density) : Style.rowHeight
        flow: pane.icons ? GridView.FlowLeftToRight : GridView.FlowTopToBottom
        // Distribute spare width so icon cells fill the row evenly.
        highlightFollowsCurrentItem: false

        delegate: Item {
            id: cell
            required property int index
            required property string name
            required property string fullPath
            required property bool isDir
            required property bool isLink
            required property bool selected
            required property bool hidden
            required property int depth
            required property bool expanded
            required property string rel
            required property string groupName
            required property var size
            required property var mtime

            width: grid2.cellWidth
            height: grid2.cellHeight
            readonly property bool current: grid2.currentIndex === index
            readonly property string ext: name.lastIndexOf(".") > 0 ? name.substring(name.lastIndexOf(".") + 1).toLowerCase() : ""
            readonly property bool isImage: !isDir && ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "avif"].indexOf(ext) >= 0
            property string cached: ""
            Component.onCompleted: if (pane.icons && !isDir && !isImage) cached = Backend.thumbnail(fullPath)
            readonly property color nameColor: selected ? Style.yellow : (hidden ? Style.fgDim : Style.fg)

            Rectangle {
                anchors { fill: parent; margins: pane.icons ? 3 : 1 }
                radius: 6
                color: cell.selected ? Style.alpha(Style.yellow, 0.12)
                       : (cell.current ? (pane.active ? Style.selection : Style.alpha(Style.selection, 0.5)) : "transparent")
                border { width: cell.current && pane.icons ? 2 : 0; color: pane.active ? Style.accent : Style.fgDim }
            }
            Rectangle {
                visible: cell.current && pane.columns
                x: 1; width: 2; height: parent.height; color: pane.active ? Style.accent : Style.fgDim
            }

            // Icons view: thumbnail or big glyph, name below.
            Item {
                visible: pane.icons
                anchors.fill: parent
                Item {
                    id: thumbBox
                    anchors { top: parent.top; topMargin: Style.pad; horizontalCenter: parent.horizontalCenter }
                    width: parent.width - Style.pad * 2
                    height: parent.height * 0.6
                    Image {
                        id: thumbImage
                        anchors.fill: parent
                        visible: status === Image.Ready
                        source: cell.isImage ? "file://" + encodeURI(cell.fullPath).replace(/#/g, "%23").replace(/\?/g, "%3F")
                                             : cell.cached
                        asynchronous: true
                        cache: true
                        fillMode: Image.PreserveAspectFit
                        sourceSize.width: 220
                        sourceSize.height: 220
                        autoTransform: true
                    }
                    Text {
                        anchors.centerIn: parent
                        visible: thumbImage.status !== Image.Ready
                        text: Style.glyph(cell.name, cell.isDir, cell.isLink)
                        color: Style.glyphColor(cell.name, cell.isDir)
                        opacity: cell.hidden ? 0.6 : 1
                        font { family: Style.font; pixelSize: Math.round(Style.fontSize * 3.2) }
                    }
                }
                Text {
                    anchors { left: parent.left; right: parent.right; top: thumbBox.bottom; topMargin: Style.pad / 2
                              leftMargin: Style.pad / 2; rightMargin: Style.pad / 2 }
                    text: cell.name + (cell.isDir ? "/" : "")
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WrapAnywhere
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: cell.nameColor
                    font { family: Style.font; pixelSize: Style.smallFont + 1 }
                }
            }

            // Columns view: icon and name in a compact row.
            Item {
                visible: pane.columns
                anchors.fill: parent
                Text {
                    id: colIcon
                    x: Style.pad
                    width: Style.fontSize * 1.6
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignHCenter
                    text: Style.glyph(cell.name, cell.isDir, cell.isLink)
                    color: Style.glyphColor(cell.name, cell.isDir)
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
                Text {
                    anchors { left: colIcon.right; leftMargin: Style.pad * 0.8; right: parent.right; rightMargin: Style.pad
                              verticalCenter: parent.verticalCenter }
                    text: cell.name + (cell.isDir ? "/" : "")
                    elide: Text.ElideRight
                    color: cell.nameColor
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
            }

            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.MiddleButton | Qt.RightButton
                onPressed: pane.activated()
                onClicked: (m) => {
                    if (m.button === Qt.RightButton) {
                        if (!cell.selected) pane.setCursor(cell.index); else list.currentIndex = cell.index
                        pane.contextRequested()
                        return
                    }
                    if (m.button === Qt.MiddleButton) { pane.setCursor(cell.index); pane.openCurrent(false); return }
                    if (m.modifiers & Qt.ControlModifier) { dir.toggleSelect(cell.index); list.currentIndex = cell.index }
                    else if (m.modifiers & Qt.ShiftModifier) {
                        dir.selectRange(Math.max(0, list.currentIndex), cell.index, true)
                        list.currentIndex = cell.index
                    } else pane.setCursor(cell.index)
                }
                onDoubleClicked: if (Prefs.v.openOnClick !== "single") pane.openCurrent(false)
            }
        }

        WheelHandler {
            acceptedModifiers: Qt.ControlModifier
            onWheel: (e) => pane.zoomRequested(e.angleDelta.y > 0 ? 1 : -1)
        }
    }

    TapHandler {
        acceptedButtons: Qt.BackButton
        onTapped: pane.goBack()
    }
    TapHandler {
        acceptedButtons: Qt.ForwardButton
        onTapped: pane.goForward()
    }
    TapHandler {
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onPressedChanged: if (pressed) pane.activated()
    }
}
