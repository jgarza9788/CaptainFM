import QtQuick
import CaptainFM

// Global search, pinned items, places, drives with usage and remotes. Collapses to glyphs when narrow.
Rectangle {
    id: side

    property string currentPath: ""
    property bool compact: false
    signal navigate(string path)
    signal searchChanged(string text)
    signal searchAccepted()
    signal searchCancelled()
    signal searchRequested()          // icon-only mode: open the search prompt instead
    property string searchValue: ""
    signal connectRequested()

    color: Style.bgDark
    clip: true

    property var places: []
    property var pinned: []
    property var drives: []
    property var remotes: []

    readonly property string home: Backend.home()

    // Which sections are folded: { places: true, ... }. Saved with the session.
    property var folded: ({})
    signal foldsChanged()
    function toggleFold(key) {
        const f = Object.assign({}, folded)
        f[key] = !f[key]
        folded = f
        foldsChanged()
    }

    function loadPlaces() {
        const icons = {
            Projects: ["\uf121", "▸"], Documents: ["\uf15c", "▸"], Downloads: ["\uf019", "▸"], Pictures: ["\uf03e", "▸"],
            Music: ["\uf001", "▸"], Videos: ["\uf03d", "▸"], Desktop: ["\uf108", "▸"]
        }
        const out = [{ label: "home", glyph: Style.pick("\uf015", "~"), path: home }]
        for (const n of Object.keys(icons)) {
            const p = home + "/" + n
            if (Backend.isDir(p)) out.push({ label: n.toLowerCase(), glyph: Style.pick(icons[n][0], icons[n][1]), path: p })
        }
        // Dropping files here sends them to the trash.
        const trash = home + "/.local/share/Trash/files"
        if (Backend.isDir(trash)) out.push({ label: "trash", glyph: Style.pick("\uf1f8", "✕"), path: trash, trash: true })
        places = out
    }

    function refreshPinned() {
        try {
            pinned = JSON.parse(Backend.bookmarks()).filter(p => Backend.exists(p)).map(p => {
                const dir = Backend.isDir(p)
                const label = p.substring(p.lastIndexOf("/") + 1) || "/"
                // Pinned files show their own file-type icon; folders and the rest get a star.
                return { label: label, glyph: dir ? Style.pick("\uf005", "★") : Style.glyph(label, false, false),
                         path: p, file: !dir, pin: true }
            })
        } catch (e) { pinned = [] }
    }

    function refresh() {
        loadPlaces()
        refreshPinned()
        Backend.listSidebarDevices()
    }

    Component.onCompleted: refresh()

    // Pins and unpins show up immediately, wherever they were made (keys, menu, other windows).
    Connections {
        target: Backend
        function onBookmarksChanged() { side.refreshPinned() }
    }
    Timer { interval: 30000; running: true; repeat: true; onTriggered: Backend.listSidebarDevices() }

    Connections {
        target: Backend
        function onSidebarDevices(json) {
            let list = []
            try { list = JSON.parse(json) } catch (e) { list = [] }
            const d = [], r = []
            for (const x of list) {
                if (!x.mounted) continue
                if (x.kind === "disk") {
                    const mp = x.path
                    const wanted = mp === "/" || mp.indexOf("/mnt") === 0 || mp.indexOf("/media") === 0 || mp.indexOf("/run/media") === 0
                    if (!wanted) continue
                    const label = mp === "/" ? "root" : (x.label || mp.substring(mp.lastIndexOf("/") + 1))
                    d.push({ label: label.toLowerCase(), glyph: Style.pick("\uf0a0", "■"), path: mp, usage: x.usage, used: x.used, total: x.total })
                } else {
                    const u = x.label.replace(/^[a-z0-9+.-]+:\/\//, "").replace(/^.*@/, "")
                    r.push({ label: u.split("/")[0].toLowerCase(), glyph: Style.pick("\uf0ac", "⇅"), path: x.path })
                }
            }
            drives = d
            remotes = r
        }
    }

    // The sidebar item that best contains the current folder (longest path wins).
    readonly property string activePath: {
        let best = "", len = -1
        for (const list of [places, pinned, drives, remotes])
            for (const it of list)
                if ((currentPath === it.path || currentPath.indexOf(it.path + "/") === 0) && it.path.length > len) {
                    best = it.path; len = it.path.length
                }
        return best
    }

    Rectangle { anchors { top: parent.top; bottom: parent.bottom; right: parent.right } width: 1; color: Style.surface }

    // Section title with a fold chevron; click to fold or unfold.
    component Section: Item {
        id: section
        property string key: ""
        property string title: ""
        readonly property bool folded: side.folded[key] === true
        visible: !side.compact
        width: side.width - 1
        height: visible ? Math.round(Style.smallFont * 1.5 + Style.pad * 1.2) : 0
        Text {
            x: Style.pad * 1.6
            anchors { bottom: parent.bottom; bottomMargin: Style.pad / 3 }
            textFormat: Text.RichText
            text: "<font color=\"" + Style.fgDim + "\">" + (section.folded ? Style.pick("\uf054", "▸") : Style.pick("\uf078", "▾")) + "</font>  " + section.title
            color: sectionMouse.containsMouse ? Style.fgLight : Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont - 1; letterSpacing: 1 }
        }
        MouseArea {
            id: sectionMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: side.toggleFold(section.key)
        }
    }

    component Entry: Item {
        id: entry
        property var item
        property bool dashed: false
        width: side.width - 1
        // Drives show how full they are as a thin bar under the name.
        readonly property bool hasBar: item && item.usage !== undefined && item.usage >= 0 && item.total > 0
        readonly property int barHeight: Math.max(3, Math.round(4 * Style.zoom))
        readonly property int barShift: hasBar ? Math.round((barHeight + Style.pad / 2) / 2) : 0
        height: Math.round(Style.rowHeight * 1.05) + (hasBar ? barHeight + Math.round(Style.pad / 2) : 0)
        readonly property bool active: item && item.path !== "" && item.path === side.activePath
        Rectangle {
            anchors { fill: parent; leftMargin: Style.pad / 2; rightMargin: Style.pad / 2 }
            radius: 5
            color: entry.active ? Style.selection : (hover.containsMouse ? Style.alpha(Style.selection, 0.4) : "transparent")
        }
        Text {
            id: glyph
            x: Style.pad * 1.6
            width: Style.fontSize * 1.2
            anchors { verticalCenter: parent.verticalCenter; verticalCenterOffset: -entry.barShift }
            horizontalAlignment: Text.AlignHCenter
            text: entry.item ? entry.item.glyph : ""
            color: entry.active ? Style.accent : Style.fgDim
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        Text {
            visible: !side.compact
            anchors { left: glyph.right; leftMargin: Style.pad; right: dropIcon.visible ? dropIcon.left : usage.left; rightMargin: Style.pad; verticalCenter: parent.verticalCenter; verticalCenterOffset: -entry.barShift }
            text: entry.item ? entry.item.label : ""
            elide: Text.ElideRight
            color: entry.active ? Style.accent : (entry.dashed ? Style.fgDim : Style.fgLight)
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        Text {
            id: usage
            visible: !side.compact && !dropHere.containsDrag && entry.item && entry.item.usage !== undefined && entry.item.usage >= 0
            anchors { right: parent.right; rightMargin: Style.pad * 1.6; verticalCenter: parent.verticalCenter; verticalCenterOffset: -entry.barShift }
            // Hover shows what is left instead of the percentage.
            text: entry.item && entry.item.usage >= 0
                  ? (hover.containsMouse && entry.item.total > 0 ? Style.humanSize(entry.item.total - entry.item.used) + " free" : entry.item.usage + "%") : ""
            color: entry.item && entry.item.usage >= 90 ? Style.red : (entry.item && entry.item.usage >= 80 ? Style.yellow : Style.fgDim)
            font { family: Style.font; pixelSize: Style.smallFont }
        }
        Rectangle {
            id: track
            visible: entry.hasBar
            x: Style.pad * 1.6
            width: parent.width - Style.pad * 3.2
            height: entry.barHeight
            y: parent.height - height - Style.pad / 2
            radius: height / 2
            color: Style.surface
            Rectangle {
                width: Math.max(parent.height, parent.width * Math.min(100, Math.max(0, entry.item ? entry.item.usage : 0)) / 100)
                height: parent.height
                radius: height / 2
                color: entry.item && entry.item.usage >= 90 ? Style.red : (entry.item && entry.item.usage >= 80 ? Style.yellow : Style.accent)
                Behavior on width { NumberAnimation { duration: Style.anim(200); easing.type: Easing.OutCubic } }
            }
        }
        // Files dropped on a place are copied or moved there (or trashed, for the trash); the row says which.
        DropArea {
            id: dropHere
            anchors.fill: parent
            enabled: entry.item && entry.item.path !== "" && !entry.item.file
            property var hint: ({ kind: "", text: "" })
            function update(drag) { hint = Dnd.hint(drag, entry.item.path, entry.item.label, entry.item.trash === true) }
            onEntered: (drag) => update(drag)
            onPositionChanged: (drag) => update(drag)
            onDropped: (drop) => Dnd.accept(drop, entry.item.path, entry.item.trash === true)
            Rectangle {
                anchors { fill: parent; leftMargin: Style.pad / 2; rightMargin: Style.pad / 2 }
                visible: dropHere.containsDrag
                radius: 5
                color: Style.alpha(Style.accent, 0.25)
                border { width: 1; color: Style.accent }
            }
        }
        // Just the icon (an arrow for "move to"): a long label would run into the place's name.
        Text {
            id: dropIcon
            visible: dropHere.containsDrag && dropHere.hint.kind !== ""
            width: Math.round(Style.fontSize * 1.8)
            horizontalAlignment: Text.AlignRight
            anchors { right: parent.right; rightMargin: Style.pad * 1.6; verticalCenter: parent.verticalCenter; verticalCenterOffset: -entry.barShift }
            text: Dnd.glyph(dropHere.hint.kind)
            color: Style.accent
            font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.25); bold: true }
        }
        MouseArea {
            id: hover
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onClicked: (m) => {
                if (m.button === Qt.RightButton) {
                    // Right-click a pinned entry to unpin it.
                    if (entry.item && entry.item.pin) Backend.toggleBookmark(entry.item.path)
                    return
                }
                entry.item.path === "" ? side.connectRequested() : side.navigate(entry.item.path)
            }
        }
    }

    Flickable {
        anchors.fill: parent
        contentHeight: col.implicitHeight + Style.pad
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Column {
            id: col
            width: parent.width
            topPadding: Style.pad / 2
            // Global search: everything on this drive, results shown in the pane.
            Item {
                width: side.width - 1
                height: Math.round(Style.rowHeight * 1.5)
                Rectangle {
                    anchors { fill: parent; leftMargin: Style.pad; rightMargin: Style.pad; topMargin: Style.pad / 2; bottomMargin: Style.pad / 4 }
                    radius: 5
                    color: Style.bgDarker
                    border { width: 1; color: searchInput.activeFocus ? Style.accent : Style.surface }
                    Text {
                        id: searchGlyph
                        x: Style.pad * 0.9
                        anchors.verticalCenter: parent.verticalCenter
                        text: Style.pick("\uf002", "⌕")
                        color: searchInput.activeFocus ? Style.accent : Style.fgDim
                        font { family: Style.font; pixelSize: Style.fontSize }
                    }
                    TextInput {
                        id: searchInput
                        visible: !side.compact
                        anchors { left: searchGlyph.right; leftMargin: Style.pad; right: parent.right; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
                        clip: true
                        color: Style.fg
                        selectionColor: Style.accent
                        selectedTextColor: Style.bgDarker
                        font { family: Style.font; pixelSize: Style.fontSize }
                        onTextEdited: side.searchChanged(text)
                        Keys.onReturnPressed: (e) => { e.accepted = true; side.searchAccepted() }
                        Keys.onEnterPressed: (e) => { e.accepted = true; side.searchAccepted() }
                        Keys.onDownPressed: side.searchAccepted()
                        Keys.onEscapePressed: { text = ""; side.searchCancelled() }
                        // Mirror the pane's query when it changes elsewhere (Ctrl+F, Esc).
                        Binding on text { when: !searchInput.activeFocus; value: side.searchValue }
                        Text {
                            visible: searchInput.text === "" && !searchInput.activeFocus
                            anchors.fill: parent
                            verticalAlignment: Text.AlignVCenter
                            text: "search everywhere"
                            color: Style.fgDim
                            font: searchInput.font
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: side.compact
                        cursorShape: Qt.PointingHandCursor
                        onClicked: side.searchRequested()
                    }
                }
            }

            Section { key: "pinned"; title: "PINNED" }
            Repeater { model: side.folded.pinned ? [] : side.pinned; delegate: Entry { required property var modelData; item: modelData } }
            Text {
                visible: !side.compact && !side.folded.pinned && side.pinned.length === 0
                x: Style.pad * 1.6
                height: visible ? implicitHeight + Style.pad : 0
                text: "nothing pinned yet"
                color: Style.fgDim
                font { family: Style.font; pixelSize: Style.smallFont }
            }
            Section { key: "places"; title: "PLACES" }
            Repeater { model: side.folded.places ? [] : side.places; delegate: Entry { required property var modelData; item: modelData } }
            Section { key: "drives"; visible: !side.compact && side.drives.length > 0; title: "DRIVES" }
            Repeater { model: side.folded.drives ? [] : side.drives; delegate: Entry { required property var modelData; item: modelData } }
            Section { key: "remote"; title: "REMOTE" }
            Repeater { model: side.folded.remote ? [] : side.remotes; delegate: Entry { required property var modelData; item: modelData } }
            Entry { visible: !side.folded.remote; height: visible ? Math.round(Style.rowHeight * 1.05) : 0
                    item: ({ label: "connect…", glyph: Style.pick("\uf067", "+"), path: "" }); dashed: true }
        }
    }
}
