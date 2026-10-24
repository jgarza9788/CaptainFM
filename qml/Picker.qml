import QtQuick
import CaptainFM

// Fuzzy list picker: command palette, context menu, layouts, devices. Matches the palette mockup:
// ":" prompt, category column, highlighted matches, key badges ("+ bind" when unbound), footer hints.
Rectangle {
    id: picker
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(860 * Style.zoom))
    height: Math.min(parent.height - y - Style.pad * 2,
                     Math.round(48 * Style.zoom) + list.contentHeight + footer.height + Style.pad)
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.1)
    radius: 6
    color: Style.bgDark
    border { width: 2; color: Style.accent }

    onVisibleChanged: if (visible) { opacity = 0; fadeIn.restart() }
    NumberAnimation { id: fadeIn; target: picker; property: "opacity"; to: 1; duration: Style.anim(110); easing.type: Easing.OutCubic }

    property string placeholder: "Type to search"
    property string promptGlyph: ":"
    property string hintRight: "actions"
    property var items: []          // [{title, hint, tag, category, keys, bindable, search}]
    property var baseItems: []
    property bool predict: false    // rank with guesses (word forms, synonyms, typos) and learned picks
    property var fallback: null     // (query) => items shown when nothing in baseItems matches
    property bool fallbackActive: false
    property var shown: []
    property var onChoose: null
    property var onRebind: null     // (item, sequence) => void ; enables Tab / Ctrl+K rebinding
    property var onUnbind: null
    property var keyHook: null      // (event, item) => bool ; lets callers add Ctrl+<key> actions
    property bool capturing: false
    property var captured: []
    property var tokenFor: null
    readonly property bool hasCategories: items.length > 0 && items[0].category !== undefined

    signal closed()

    function open(opts) {
        placeholder = opts.placeholder ?? "Type to search"
        promptGlyph = opts.prompt ?? ":"
        hintRight = opts.hintRight ?? "actions"
        baseItems = opts.items ?? []
        items = baseItems
        fallback = opts.fallback ?? null
        predict = opts.predict ?? false
        fallbackActive = false
        onChoose = opts.choose ?? null
        onRebind = opts.rebind ?? null
        onUnbind = opts.unbind ?? null
        keyHook = opts.keyHook ?? null
        tokenFor = opts.tokenFor ?? null
        capturing = false
        input.text = opts.query ?? ""
        refresh()
        visible = true
        input.forceActiveFocus()
    }

    function close() { visible = false; capturing = false; closed() }

    function refresh() {
        const q = input.text.trim()
        items = baseItems
        fallbackActive = false
        // Path-like input is about folders, not commands.
        const pathLike = fallback && (q.charAt(0) === "/" || q.charAt(0) === "~" || q.indexOf("/") > 0)
        if (pathLike) shown = []
        else if (predict) shown = JSON.parse(Backend.rankCommands(JSON.stringify(items.map(i => i.search ?? i.title)), JSON.stringify(items.map(i => typeof i.tag === "string" ? i.tag : "")), q, Prefs.v.learnPalette))
        else if (q === "") { shown = items.map((_, i) => i) }
        else {
            const texts = JSON.stringify(items.map(i => i.search ?? i.title))
            // With a folder fallback a miss must be a real miss, so skip the scattered-letters match.
            shown = JSON.parse(fallback ? Backend.rankStrict(texts, q) : Backend.rank(texts, q))
        }
        if (shown.length === 0 && q !== "" && fallback) {
            const extra = fallback(q)
            if (extra.length > 0) { items = extra; shown = extra.map((_, i) => i); fallbackActive = true }
        }
        // An exact alias always wins.
        const exact = shown.filter(i => (items[i].aliasList ?? []).indexOf(q) >= 0)
        if (exact.length > 0) shown = exact.concat(shown.filter(i => exact.indexOf(i) < 0))
        list.currentIndex = 0
    }

    function choose() {
        if (list.currentIndex < 0 || list.currentIndex >= shown.length) return
        const item = items[shown[list.currentIndex]]
        const typed = input.text.trim()
        close()
        if (onChoose) onChoose(item, typed)
    }

    function startCapture() {
        captured = []
        capturing = true
        capture.forceActiveFocus()
    }

    Item {
        id: inputRow
        x: 0; y: 0; width: parent.width; height: Math.round(46 * Style.zoom)
        Text {
            id: prompt
            anchors { left: parent.left; leftMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            text: picker.capturing ? "⌨" : picker.promptGlyph
            color: Style.accent
            font { family: Style.font; pixelSize: Style.fontSize + 1; bold: true }
        }
        TextInput {
            id: input
            anchors { left: prompt.right; leftMargin: Style.pad * 1.2; right: rightHint.left; rightMargin: Style.pad
                      verticalCenter: parent.verticalCenter }
            color: Style.fg
            selectionColor: Style.accent
            selectedTextColor: Style.bgDarker
            clip: true
            font { family: Style.font; pixelSize: Style.fontSize + 1 }
            enabled: !picker.capturing
            onTextChanged: picker.refresh()
            onAccepted: picker.choose()
            Keys.onReturnPressed: (e) => { e.accepted = true; picker.choose() }
            Keys.onEnterPressed: (e) => { e.accepted = true; picker.choose() }
            Keys.onEscapePressed: picker.close()
            Keys.onDownPressed: list.currentIndex = Math.min(list.count - 1, list.currentIndex + 1)
            Keys.onUpPressed: list.currentIndex = Math.max(0, list.currentIndex - 1)
            Keys.onTabPressed: if (picker.onRebind && !picker.fallbackActive) picker.startCapture()
            Keys.onPressed: (e) => {
                const ctrl = e.modifiers & Qt.ControlModifier
                if (picker.keyHook && list.currentIndex >= 0 && list.currentIndex < picker.shown.length
                    && picker.keyHook(e, picker.items[picker.shown[list.currentIndex]])) { e.accepted = true; return }
                if (ctrl && (e.key === Qt.Key_N || e.key === Qt.Key_J)) { list.currentIndex = Math.min(list.count - 1, list.currentIndex + 1); e.accepted = true }
                else if (ctrl && e.key === Qt.Key_P) { list.currentIndex = Math.max(0, list.currentIndex - 1); e.accepted = true }
                else if (ctrl && e.key === Qt.Key_K && picker.onRebind) { picker.startCapture(); e.accepted = true }
                else if (ctrl && e.key === Qt.Key_X && picker.onUnbind) {
                    if (list.currentIndex >= 0) picker.onUnbind(picker.items[picker.shown[list.currentIndex]])
                    e.accepted = true
                }
            }
        }
        Text {
            visible: input.text === "" && !picker.capturing
            anchors.fill: input
            text: picker.placeholder
            color: Style.fgDim
            font: input.font
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Text {
            visible: picker.capturing
            anchors { left: prompt.right; leftMargin: Style.pad * 1.2; verticalCenter: parent.verticalCenter }
            text: picker.captured.length === 0 ? "press the new keys, Enter to save, Esc to cancel" : picker.captured.join(" ")
            color: Style.yellow
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        Text {
            id: rightHint
            anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            text: picker.fallbackActive ? "folders" : picker.hintRight
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont + 1 }
        }
    }

    Item {
        id: capture
        focus: picker.capturing
        Keys.onPressed: (e) => {
            e.accepted = true
            if (e.key === Qt.Key_Escape) { picker.capturing = false; input.forceActiveFocus(); return }
            if ((e.key === Qt.Key_Return || e.key === Qt.Key_Enter) && picker.captured.length > 0) {
                const item = picker.items[picker.shown[list.currentIndex]]
                const seq = picker.captured.join(" ")
                picker.capturing = false
                input.forceActiveFocus()
                if (picker.onRebind) picker.onRebind(item, seq)
                return
            }
            const t = picker.tokenFor ? picker.tokenFor(e) : ""
            if (t !== "") picker.captured = picker.captured.concat([t])
        }
    }

    Rectangle { x: 0; y: inputRow.height; width: parent.width; height: 1; color: Style.surface }

    ListView {
        id: list
        x: 0; y: inputRow.height + 1
        width: parent.width; height: parent.height - inputRow.height - footer.height - 2
        clip: true
        model: picker.shown.length
        boundsBehavior: Flickable.StopAtBounds
        highlightMoveDuration: 0
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
        delegate: Item {
            id: row
            required property int index
            readonly property var item: picker.items[picker.shown[index]]
            width: ListView.view.width
            height: Math.round(Style.rowHeight * 1.25)
            readonly property bool current: ListView.isCurrentItem
            Rectangle { anchors.fill: parent; color: row.current ? Style.alpha(Style.accent, 0.14) : "transparent" }
            Rectangle { visible: row.current; width: 3; height: parent.height; color: Style.accent }

            Text {
                id: category
                visible: picker.hasCategories
                x: Style.pad * 2
                width: picker.hasCategories ? Style.fontSize * 6.5 : 0
                anchors.verticalCenter: parent.verticalCenter
                text: row.item && row.item.category ? row.item.category : ""
                color: Style.fgDim
                elide: Text.ElideRight
                font { family: Style.font; pixelSize: Style.smallFont + 1 }
            }
            Text {
                anchors { left: category.right; leftMargin: picker.hasCategories ? Style.pad : Style.pad * 2
                          right: badge.left; rightMargin: Style.pad; verticalCenter: parent.verticalCenter }
                textFormat: Text.RichText
                text: row.item ? Style.highlightHtml(row.item.title, input.text.trim(), Style.accent) : ""
                color: Style.fg
                // Folder results keep their end (the interesting part) visible.
                elide: row.item && typeof row.item.tag === "object" && row.item.tag.folder !== undefined ? Text.ElideLeft : Text.ElideRight
                font { family: Style.font; pixelSize: Style.fontSize + 1 }
            }
            // Current binding as a badge; unbound commands get a dashed "+ bind".
            Item {
                id: badge
                readonly property var keys: row.item && row.item.keys ? row.item.keys : []
                readonly property bool unbound: row.item && row.item.bindable === true && keys.length === 0
                anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
                width: badgeInner.implicitWidth
                height: Math.round(22 * Style.zoom)
                Row {
                    id: badgeInner
                    spacing: Style.pad / 2
                    Text {
                        visible: row.item && row.item.hint && badge.keys.length === 0 && !badge.unbound
                        anchors.verticalCenter: parent.verticalCenter
                        text: row.item ? row.item.hint : ""
                        color: Style.yellow
                        font { family: Style.font; pixelSize: Style.smallFont }
                    }
                    Repeater {
                        model: badge.keys.slice(0, 2)
                        delegate: Rectangle {
                            required property string modelData
                            height: badge.height
                            width: keyText.implicitWidth + Style.pad * 1.6
                            radius: 4
                            color: "transparent"
                            border { width: 1; color: Style.surface }
                            Text {
                                id: keyText
                                anchors.centerIn: parent
                                text: parent.modelData
                                color: Style.fgLight
                                font { family: Style.font; pixelSize: Style.smallFont + 1 }
                            }
                        }
                    }
                    Rectangle {
                        visible: badge.unbound
                        height: badge.height
                        width: bindText.implicitWidth + Style.pad * 1.6
                        radius: 4
                        color: "transparent"
                        border { width: 1; color: Style.fgDim }
                        Text {
                            id: bindText
                            anchors.centerIn: parent
                            text: "+ bind"
                            color: Style.fgDim
                            font { family: Style.font; pixelSize: Style.smallFont + 1 }
                        }
                    }
                }
            }
            MouseArea {
                anchors.fill: parent
                onClicked: { list.currentIndex = row.index; picker.choose() }
            }
        }
    }

    Item {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Math.round(28 * Style.zoom)
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Style.surface }
        Text {
            anchors { left: parent.left; leftMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            textFormat: Text.RichText
            text: {
                const k = s => "<font color=\"" + Style.fgLight + "\">" + s + "</font>"
                return k("↑↓") + " move   " + k("enter") + " run" + (picker.onRebind ? "   " + k("tab") + " bind key" : "") + "   " + k("esc") + " close"
            }
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont + 1 }
        }
        Text {
            anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            text: picker.items.length + (picker.fallbackActive ? " folders" : (picker.hasCategories ? " actions" : " items"))
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont + 1 }
        }
    }
}
