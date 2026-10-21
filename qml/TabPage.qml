import QtQuick
import CaptainFM
import QtQuick.Layouts

// The contents of one tab: 1-4 panes in a row, column or grid layout.
Item {
    id: page

    property var startPaths: []            // initial pane folders (read once at creation)
    property var startViews: []            // matching view modes (list | tree | miller)
    property string layoutMode: "columns"  // columns | rows | grid
    property int activeIndex: 0
    property bool current: false
    property bool optHidden: false
    property string optGroupBy: "none"
    property string optSortKey: "name"
    property bool optSortDesc: false
    property var paneItems: []
    readonly property alias panes: panesModel
    readonly property int paneCount: panesModel.count
    readonly property Item activePane: paneItems.length > activeIndex ? paneItems[activeIndex] : null
    readonly property string title: {
        const p = activePane
        if (!p || !p.dir.path) return "…"
        if (p.deep) return "search: " + p.searchText
        return p.dir.path === "/" ? "/" : p.baseName(p.dir.path)
    }

    signal paneActivated()
    signal contextRequested()
    signal sortRequested(string key)
    signal scopeToggled()
    signal zoomRequested(real steps)
    signal dropped(var paths, string destination, bool move)

    readonly property var layouts: ["columns", "rows", "grid"]

    // A ListModel (not an integer model) so adding or removing a pane keeps the other panes alive.
    ListModel { id: panesModel }

    Component.onCompleted: {
        const paths = startPaths.length > 0 ? startPaths : [""]
        for (let i = 0; i < paths.length; i++)
            panesModel.append({ startPath: paths[i], startView: startViews[i] ?? "list" })
    }

    function addPane(path) {
        if (paneCount >= 4) return false
        panesModel.append({ startPath: path, startView: activePane ? activePane.viewMode : "list" })
        activeIndex = paneCount - 1
        if (paneCount > 2 && layoutMode === "columns") layoutMode = "grid"
        return true
    }

    function closePane(i) {
        if (paneCount <= 1) return false
        panesModel.remove(i)
        activeIndex = Math.min(activeIndex > i ? activeIndex - 1 : activeIndex, paneCount - 1)
        return true
    }

    function onlyPane() {
        const keep = activePane
        for (let i = paneCount - 1; i >= 0; i--) if (i !== activeIndex) panesModel.remove(i)
        activeIndex = 0
        return keep
    }

    // Swap two panes (their state travels with them) and keep the focus on the moved one.
    function swapPanes(a, b) {
        if (a === b || a < 0 || b < 0 || a >= paneCount || b >= paneCount) return
        const lo = Math.min(a, b), hi = Math.max(a, b)
        panesModel.move(hi, lo, 1)
        if (hi - lo > 1) panesModel.move(lo + 1, hi, 1)
        Qt.callLater(() => {
            const items = []
            for (let i = 0; i < paneCount; i++) items.push(paneRepeater.itemAt(i))
            paneItems = items
            activeIndex = b
        })
    }

    function cycleLayout() {
        layoutMode = layouts[(layouts.indexOf(layoutMode) + 1) % layouts.length]
    }

    function focusPane(i) {
        if (paneCount === 0) return
        activeIndex = ((i % paneCount) + paneCount) % paneCount
    }

    function otherPane() {
        return paneCount > 1 ? paneItems[(activeIndex + 1) % paneCount] : null
    }

    GridLayout {
        anchors.fill: parent
        columns: page.layoutMode === "columns" ? Math.max(1, page.paneCount)
                 : page.layoutMode === "rows" ? 1 : Math.min(2, page.paneCount)
        columnSpacing: 1
        rowSpacing: 1

        Repeater {
            id: paneRepeater
            model: panesModel
            delegate: Pane {
                required property int index
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredWidth: 100
                Layout.preferredHeight: 100
                Layout.minimumWidth: 120
                active: page.current && page.activeIndex === index
                optHidden: page.optHidden
                optGroupBy: page.optGroupBy
                optSortKey: page.optSortKey
                optSortDesc: page.optSortDesc
                onActivated: { page.activeIndex = index; page.paneActivated() }
                onZoomRequested: (s) => page.zoomRequested(s)
                paneIndex: index
                showHeader: page.paneCount > 1
                onSortRequested: (key) => page.sortRequested(key)
                onScopeToggled: page.scopeToggled()
                onSwapRequested: (from, to) => page.swapPanes(from, to)
                onContextRequested: page.contextRequested()
                onDropped: (paths, destination, move) => page.dropped(paths, destination, move)
            }
            onItemAdded: (i, item) => {
                const a = page.paneItems.slice()
                a.splice(i, 0, item)
                page.paneItems = a
            }
            onItemRemoved: (i, item) => {
                page.paneItems = page.paneItems.filter(p => p !== item)
            }
        }
    }
}
