import QtQuick
import CaptainFM

// Settings overlay (Ctrl-, or "Settings" in the palette). Up/Down pick a row, Left/Right or Enter change it,
// Enter on a text row edits it, Esc closes. Changes apply at once; the ones the app already remembers (view,
// sort, keys ...) are saved with the session, the rest in ~/.config/captainfm/settings.json.
Rectangle {
    id: st
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(760 * Style.zoom))
    height: Math.min(parent.height - y - Style.pad * 2, Math.round(640 * Style.zoom))
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.08)
    radius: 6
    color: Style.bgDark
    border { width: 2; color: Style.accent }
    focus: visible

    property var app
    property int current: 1
    property int revision: 0          // bumped on every change so row values re-read their state
    property bool editing: false
    property string note: ""

    signal closed()

    onVisibleChanged: if (visible) { opacity = 0; fadeIn.restart() }
    NumberAnimation { id: fadeIn; target: st; property: "opacity"; to: 1; duration: Style.anim(110); easing.type: Easing.OutCubic }

    function open() {
        current = 1
        editing = false
        note = ""
        revision++
        visible = true
        forceActiveFocus()
    }

    function close() { visible = false; editing = false; closed() }

    function prefs(key, choices) {
        return { key: key, choices: choices, get: () => Prefs.v[key], set: (v) => Prefs.set(key, v) }
    }

    // A row is a section title, a choice (value -> label), or a text field.
    readonly property var rows: [
        { section: "General" },
        Object.assign({ title: "On start", hint: "restore the last tabs, or always begin in one folder" },
            prefs("startup", [["restore", "restore last session"], ["home", "home folder"], ["folder", "start folder"]])),
        { title: "Start folder", hint: "used when \"On start\" is start folder", kind: "text", key: "startFolder",
          get: () => Prefs.v.startFolder, set: (v) => Prefs.set("startFolder", v) },
        { title: "Key style", hint: "File Pilot keys, or modal Vim keys with a Space leader",
          choices: [["fp", "File Pilot"], ["vim", "Vim"]], get: () => app.keymap, set: (v) => { app.keymap = v; app.markDirty() } },
        Object.assign({ title: "Default view", hint: "for new panes and tabs" },
            prefs("defaultView", [["list", "list"], ["tree", "tree"], ["miller", "Miller"], ["grid", "icons"], ["columns", "columns"]])),
        Object.assign({ title: "Open items with", hint: "a folder or file under the pointer" },
            prefs("openOnClick", [["double", "double click"], ["single", "single click"]])),
        { title: "Terminal", hint: "command that opens in the current folder; empty uses xdg-terminal-exec", kind: "text", key: "terminal",
          get: () => Prefs.v.terminal, set: (v) => Prefs.set("terminal", v) },

        { section: "Appearance" },
        { title: "Font", hint: "family name; empty follows the system and Omarchy font", kind: "text", key: "fontFamily",
          get: () => Prefs.v.fontFamily, set: (v) => Prefs.set("fontFamily", v) },
        { title: "Row spacing", hint: "",
          choices: [[0.8, "compact"], [1.0, "normal"], [1.25, "roomy"]], get: () => Style.density,
          set: (v) => { Style.density = v; app.markDirty() } },
        { title: "Animations", hint: "", choices: [[true, "on"], [false, "off"]], get: () => Style.animations,
          set: (v) => { Style.animations = v; app.markDirty() } },
        Object.assign({ title: "File sizes", hint: "1024 per step (K, M, G) or 1000" },
            prefs("sizeUnits", [["binary", "binary"], ["decimal", "decimal"]])),
        Object.assign({ title: "Dates", hint: "" },
            prefs("dateFormat", [["friendly", "today, yesterday, oct 14"], ["iso", "2026-10-14 07:12"]])),

        { section: "Browsing" },
        { title: "Hidden files", hint: "", choices: [[true, "shown"], [false, "hidden"]], get: () => app.showHidden,
          set: (v) => app.showHidden = v },
        { title: "Preview pane", hint: "", choices: [[true, "on"], [false, "off"]], get: () => app.inspectorOn,
          set: (v) => app.inspectorOn = v },
        { title: "Sidebar", hint: "", choices: [["open", "open"], ["icons", "icons only"], ["hidden", "hidden"]],
          get: () => app.sidebarMode, set: (v) => app.sidebarMode = v },
        { title: "Sort by", hint: "", choices: [["name", "name"], ["size", "size"], ["mtime", "modified"], ["ext", "extension"]],
          get: () => app.sortKey, set: (v) => app.sortKey = v },
        { title: "Sort order", hint: "", choices: [[false, "ascending"], [true, "descending"]], get: () => app.sortDesc,
          set: (v) => app.sortDesc = v },
        { title: "Group by", hint: "", choices: [["none", "nothing"], ["type", "type"], ["date", "date"], ["size", "size"]],
          get: () => app.groupBy, set: (v) => app.groupBy = v },

        { section: "Files" },
        Object.assign({ title: "Dropping files", hint: "when files are dropped on a folder or pane" },
            prefs("dropAction", [["auto", "as the drag asks (Ctrl copies)"], ["copy", "always copy"], ["move", "always move"], ["link", "always link (symlink)"]])),
        Object.assign({ title: "Ask before trashing", hint: "Delete permanently always asks" },
            prefs("confirmTrash", [[true, "yes"], [false, "no"]])),

        { section: "System" },
        { title: "File picker for other apps", hint: "Open and Save dialogs of other programs use CaptainFM",
          choices: [[true, "on"], [false, "off"]], get: () => Backend.systemPicker(),
          set: (v) => { const r = JSON.parse(Backend.setSystemPicker(v)); st.note = r.message; if (!r.ok) app.toast(r.message, "error") } }
    ]

    function isRow(i) { return i >= 0 && i < rows.length && rows[i].section === undefined }

    function labelOf(row) {
        const cur = row.get()
        for (const c of row.choices ?? []) if (c[0] === cur) return c[1]
        return String(cur)
    }

    function step(dir) {
        if (!isRow(current)) return
        const row = rows[current]
        if (row.kind === "text") { startEdit(); return }
        const cs = row.choices
        let i = cs.findIndex(c => c[0] === row.get())
        i = ((i < 0 ? 0 : i) + dir + cs.length) % cs.length
        row.set(cs[i][0])
        revision++
    }

    function move(dir) {
        let i = current
        do { i += dir } while (i >= 0 && i < rows.length && !isRow(i))
        if (isRow(i)) { current = i; list.positionViewAtIndex(i, ListView.Contain) }
    }

    function startEdit() {
        editing = true
        editor.text = rows[current].get()
        editor.forceActiveFocus()
        editor.selectAll()
    }

    function endEdit(save) {
        if (save) { rows[current].set(editor.text.trim()); revision++ }
        editing = false
        st.forceActiveFocus()
    }

    // Test automation: `!set:down`, `!set:right`, `!set:enter` ... (see Main.automation).
    function press(name) {
        if (editing) { if (name === "enter") endEdit(true); else if (name === "esc") endEdit(false); return }
        switch (name) {
        case "esc": close(); break
        case "down": move(1); break
        case "up": move(-1); break
        case "left": step(-1); break
        default: step(1)
        }
    }

    Keys.onPressed: (e) => {
        if (editing) return
        switch (e.key) {
        case Qt.Key_Escape: close(); break
        case Qt.Key_Down: case Qt.Key_J: move(1); break
        case Qt.Key_Up: case Qt.Key_K: move(-1); break
        case Qt.Key_Left: case Qt.Key_H: step(-1); break
        case Qt.Key_Right: case Qt.Key_L: case Qt.Key_Return: case Qt.Key_Enter: case Qt.Key_Space: step(1); break
        default: return
        }
        e.accepted = true
    }

    Item {
        id: head
        x: 0; y: 0; width: parent.width; height: Math.round(54 * Style.zoom)
        Text {
            x: Style.pad * 2; y: Style.pad
            text: "Settings"
            color: Style.fg
            font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.25); bold: true }
        }
        Text {
            x: Style.pad * 2; y: Style.pad + Math.round(Style.fontSize * 1.7)
            text: "↑↓ choose · ←→ or Enter change · Esc close"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }
    }

    ListView {
        id: list
        anchors { left: parent.left; right: parent.right; top: head.bottom; bottom: foot.top }
        clip: true
        model: st.rows
        boundsBehavior: Flickable.StopAtBounds
        delegate: Item {
            id: item
            required property int index
            required property var modelData
            readonly property bool section: modelData.section !== undefined
            readonly property bool active: index === st.current
            width: list.width
            height: section ? Math.round(36 * Style.zoom) : Math.round(40 * Style.zoom)

            Rectangle { anchors.fill: parent; visible: item.active; color: Style.selection }
            Rectangle { visible: item.active; width: 3; height: parent.height; color: Style.accent }

            Text {
                visible: item.section
                x: Style.pad * 2; anchors.bottom: parent.bottom; anchors.bottomMargin: Style.pad / 2
                text: item.section ? item.modelData.section.toUpperCase() : ""
                color: Style.accent
                font { family: Style.font; pixelSize: Style.smallFont; bold: true; letterSpacing: 1 }
            }

            Column {
                visible: !item.section
                x: Style.pad * 2
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width * 0.5 - Style.pad * 2
                spacing: 1
                Text {
                    text: item.section ? "" : item.modelData.title
                    color: Style.fg
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
                Text {
                    width: parent.width
                    visible: !item.section && item.modelData.hint !== ""
                    text: item.section ? "" : item.modelData.hint
                    elide: Text.ElideRight
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
            }

            Text {
                id: valueText
                visible: !item.section && !(item.active && st.editing)
                anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
                width: parent.width * 0.5 - Style.pad * 2
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideLeft
                color: item.active ? Style.accent : Style.fgLight
                font { family: Style.font; pixelSize: Style.fontSize }
                text: {
                    st.revision
                    if (item.section) return ""
                    const row = item.modelData
                    if (row.kind === "text") { const t = row.get(); return t === "" ? "(default)" : t }
                    return "‹ " + st.labelOf(row) + " ›"
                }
            }

            MouseArea {
                anchors.fill: parent
                visible: !item.section
                onClicked: { st.current = item.index; if (st.editing) st.endEdit(true); st.step(1) }
            }
        }
    }

    // Inline editor for text rows, drawn over the selected row's value.
    TextInput {
        id: editor
        visible: st.editing
        x: parent.width * 0.5
        y: {
            // Row positions: sections are shorter than rows, so ask the list.
            const it = list.itemAtIndex(st.current)
            return it ? list.y + it.y - list.contentY + (it.height - height) / 2 : 0
        }
        width: parent.width * 0.5 - Style.pad * 2
        color: Style.fg
        selectionColor: Style.accent
        selectedTextColor: Style.bg
        horizontalAlignment: TextInput.AlignRight
        font { family: Style.font; pixelSize: Style.fontSize }
        clip: true
        Keys.onPressed: (e) => {
            if (e.key === Qt.Key_Return || e.key === Qt.Key_Enter) { st.endEdit(true); e.accepted = true }
            else if (e.key === Qt.Key_Escape) { st.endEdit(false); e.accepted = true }
        }
    }

    Item {
        id: foot
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Math.round(30 * Style.zoom)
        Rectangle { anchors { left: parent.left; right: parent.right; top: parent.top } height: 1; color: Style.surface }
        Text {
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter; leftMargin: Style.pad * 2; rightMargin: Style.pad * 2 }
            elide: Text.ElideRight
            text: st.note !== "" ? st.note : "Saved in ~/.config/captainfm/settings.json and session.json"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
    }
}
