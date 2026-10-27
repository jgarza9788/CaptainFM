import QtQuick
import CaptainFM

// Batch rename in four modes (replace, template, regex, case) with a live preview. Colliding names
// are flagged and skipped; the rest is applied as one journaled, undoable operation.
Rectangle {
    id: br
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(860 * Style.zoom))
    height: Math.min(parent.height - y - Style.pad * 2, Math.round(580 * Style.zoom))
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.1)
    radius: 6
    color: Style.bgDark
    border { width: 2; color: Style.accent }

    onVisibleChanged: if (visible) { opacity = 0; fadeIn.restart() }
    NumberAnimation { id: fadeIn; target: br; property: "opacity"; to: 1; duration: Style.anim(110); easing.type: Easing.OutCubic }

    property var app
    property var paths: []
    property var rows: []
    property string error: ""
    property string mode: "replace"          // replace | template | regex | case
    property bool dateSort: false
    property int caseIndex: 1
    readonly property var modes: ["replace", "template", "regex", "case"]
    readonly property var caseModes: ["keep", "lower", "upper", "title", "snake", "kebab"]
    readonly property int okCount: rows.filter(r => r.changed && !r.conflict).length
    readonly property int skipCount: rows.filter(r => r.conflict).length

    signal closed()

    function open() {
        const p = app.pane
        if (!p) return
        let sel = JSON.parse(p.dir.selectedPaths())
        if (sel.length === 0 && p.currentPath !== "") sel = [p.currentPath]
        if (sel.length === 0) return
        paths = sel
        mode = "replace"
        find.text = ""; repl.text = ""
        tpl.text = "{name}{ext}"
        start.text = "1"; step.text = "1"
        dateSort = false
        caseIndex = 1
        refresh()
        visible = true
        find.input.forceActiveFocus()
    }

    function close() { visible = false; closed() }

    // Page Up / Page Down scroll the preview list.
    function scrollPreview(dir) {
        const max = Math.max(0, list.contentHeight - list.height)
        list.contentY = Math.max(0, Math.min(max, list.contentY + dir * list.height * 0.9))
    }

    function setMode(m) {
        mode = m
        refresh()
        Qt.callLater(() => { const f = fields()[0]; if (f) f.input.forceActiveFocus() })
    }

    function refresh() {
        const opts = {
            template: mode === "template" ? tpl.text : "{name}{ext}",
            find: mode === "replace" || mode === "regex" ? find.text : "",
            replace: repl.text,
            regex: mode === "regex",
            case_sensitive: true,
            case_mode: mode === "case" ? caseModes[caseIndex] : "keep",
            start: parseInt(start.text) || 0,
            step: parseInt(step.text) || 1,
            sort: dateSort ? "date" : "name"
        }
        const result = JSON.parse(Backend.renamePreview(JSON.stringify(paths), JSON.stringify(opts)))
        error = result.error ?? ""
        rows = result.rows ?? []
    }

    function apply() {
        if (error !== "") return
        const pairs = rows.filter(r => r.changed && !r.conflict).map(r => [r.from, r.to])
        if (pairs.length === 0) { app.toast("Nothing to rename", "warn"); return }
        close()
        Backend.renameMany(JSON.stringify(pairs))
        app.pane.endVisual()
        app.pane.dir.clearSelection()
    }

    // Fields visible in the current mode, in tab order.
    function fields() {
        const list = []
        if (mode === "replace" || mode === "regex") list.push(find, repl)
        if (mode === "template") list.push(tpl)
        list.push(start, step)
        return list
    }

    function nextField(back) {
        const list = fields()
        const i = list.findIndex(f => f.activeFocus)
        list[((i < 0 ? 0 : i) + (back ? list.length - 1 : 1)) % list.length].input.forceActiveFocus()
    }

    // Rich text for the template: {tokens} in the accent colour (same glyph widths as the input).
    function tokenHtml(text) {
        const esc = t => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        let out = "", pos = 0
        const re = /\{[^}]*\}/g
        let m
        while ((m = re.exec(text)) !== null) {
            out += esc(text.substring(pos, m.index)) + "<span style=\"color:" + Style.accent + "; background-color:" + Style.alpha(Style.accent, 0.18) + ";\">" + esc(m[0]) + "</span>"
            pos = m.index + m[0].length
        }
        return "<span style=\"white-space:pre;\">" + out + esc(text.substring(pos)) + "</span>"
    }

    // The new name with its counter (last digit run before the extension) in the accent colour.
    function newNameHtml(name) {
        const esc = t => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        const dot = name.lastIndexOf(".")
        const stem = dot > 0 ? name.substring(0, dot) : name
        const ext = dot > 0 ? name.substring(dot) : ""
        const m = /(\d+)$/.exec(stem)
        if (mode !== "template" || !m || tpl.text.indexOf("{n") < 0) return esc(name)
        return esc(stem.substring(0, m.index)) + "<font color=\"" + Style.accent + "\">" + m[1] + "</font>" + esc(ext)
    }

    component Field: FocusScope {
        id: field
        property alias text: input.text
        property alias input: input
        property bool rich: false
        property string placeholder: ""
        signal edited()
        height: Math.round(32 * Style.zoom)
        Rectangle {
            anchors.fill: parent
            radius: 4
            color: Style.bgDarker
            border { width: 1; color: input.activeFocus ? Style.accent : Style.surface }
            clip: true
            Text {
                visible: field.rich
                anchors { fill: input }
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.RichText
                text: br.tokenHtml(input.text)
                color: Style.fg
                font: input.font
            }
            TextInput {
                id: input
                anchors { fill: parent; leftMargin: Style.pad; rightMargin: Style.pad }
                verticalAlignment: TextInput.AlignVCenter
                clip: true
                color: field.rich ? "transparent" : Style.fg
                selectionColor: Style.alpha(Style.accent, 0.4)
                selectedTextColor: field.rich ? "transparent" : Style.fg
                cursorDelegate: Rectangle { width: 2; color: Style.accent }
                font { family: Style.font; pixelSize: Style.fontSize }
                onTextChanged: field.edited()
                onAccepted: br.apply()
                Keys.onReturnPressed: (e) => { e.accepted = true; br.apply() }
                Keys.onEnterPressed: (e) => { e.accepted = true; br.apply() }
                Keys.onEscapePressed: br.close()
                Keys.onTabPressed: br.nextField(false)
                Keys.onBacktabPressed: br.nextField(true)
                Keys.onPressed: (e) => {
                    if (e.key === Qt.Key_PageDown || e.key === Qt.Key_PageUp) { br.scrollPreview(e.key === Qt.Key_PageDown ? 1 : -1); e.accepted = true; return }
                    if (e.modifiers & Qt.AltModifier) {
                        const n = e.key - Qt.Key_1
                        if (n >= 0 && n < 4) { br.setMode(br.modes[n]); e.accepted = true }
                    }
                }
            }
            Text {
                visible: input.text === "" && field.placeholder !== ""
                anchors { fill: input }
                verticalAlignment: Text.AlignVCenter
                text: field.placeholder
                color: Style.fgDim
                font: input.font
            }
        }
    }

    component Chip: Rectangle {
        id: chip
        property alias text: label.text
        property bool on: false
        signal clicked()
        height: Math.round(26 * Style.zoom)
        width: label.implicitWidth + Style.pad * 2.4
        radius: 4
        color: "transparent"
        border { width: 1; color: on ? Style.accent : Style.surface }
        Text {
            id: label
            anchors.centerIn: parent
            color: chip.on ? Style.accent : Style.fgLight
            font { family: Style.font; pixelSize: Style.fontSize }
        }
        MouseArea { anchors.fill: parent; onClicked: chip.clicked() }
    }

    // Title row.
    Item {
        id: titleRow
        x: 0; y: 0; width: parent.width; height: Math.round(40 * Style.zoom)
        Text {
            id: title
            anchors { left: parent.left; leftMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            textFormat: Text.RichText
            text: "<b>Batch rename</b> <font color=\"" + Style.fgDim + "\">" + br.paths.length + " selected</font>"
            color: Style.fg
            font { family: Style.font; pixelSize: Style.fontSize + 1 }
        }
        Text {
            anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            text: "floating · enter apply · esc cancel"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont + 1 }
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }
    }

    Column {
        id: form
        x: Style.pad * 2; y: titleRow.height + Style.pad * 1.2
        width: parent.width - Style.pad * 4
        spacing: Style.pad

        // Row helper: label + content.
        Row {
            spacing: Style.pad * 2
            height: Math.round(28 * Style.zoom)
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: "mode"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Row {
                spacing: Style.pad
                Repeater {
                    model: br.modes
                    delegate: Chip { required property string modelData; text: modelData; on: br.mode === modelData; onClicked: br.setMode(modelData) }
                }
            }
        }

        Row {
            visible: br.mode === "replace" || br.mode === "regex"
            spacing: Style.pad * 2
            width: parent.width
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: br.mode === "regex" ? "pattern" : "find"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Field { id: find; width: parent.width - Style.fontSize * 6.5 - Style.pad * 2; onEdited: br.refresh() }
        }
        Row {
            visible: br.mode === "replace" || br.mode === "regex"
            spacing: Style.pad * 2
            width: parent.width
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: "replace"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Field { id: repl; width: parent.width - Style.fontSize * 6.5 - Style.pad * 2; onEdited: br.refresh() }
        }
        Row {
            visible: br.mode === "template"
            spacing: Style.pad * 2
            width: parent.width
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: "template"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Field { id: tpl; rich: true; width: parent.width - Style.fontSize * 6.5 - Style.pad * 2; onEdited: br.refresh() }
        }
        Row {
            visible: br.mode === "case"
            spacing: Style.pad * 2
            height: Math.round(28 * Style.zoom)
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: "convert"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Row {
                spacing: Style.pad
                Repeater {
                    model: br.caseModes
                    delegate: Chip { required property int index; required property string modelData; text: modelData
                                     on: br.caseIndex === index; onClicked: { br.caseIndex = index; br.refresh() } }
                }
            }
        }

        Row {
            spacing: Style.pad * 2
            height: Math.round(32 * Style.zoom)
            Text { width: Style.fontSize * 6.5; anchors.verticalCenter: parent.verticalCenter; text: "counter"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.fontSize } }
            Row {
                spacing: Style.pad
                Text { anchors.verticalCenter: parent.verticalCenter; text: "start"; color: Style.fgDim; font { family: Style.font; pixelSize: Style.fontSize } }
                Field { id: start; width: Style.fontSize * 5; onEdited: br.refresh() }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "step"; color: Style.fgDim; font { family: Style.font; pixelSize: Style.fontSize } }
                Field { id: step; width: Style.fontSize * 5; onEdited: br.refresh() }
                Chip { anchors.verticalCenter: parent.verticalCenter; text: "sort: " + (br.dateSort ? "date" : "name"); on: br.dateSort
                       onClicked: { br.dateSort = !br.dateSort; br.refresh() } }
            }
        }
    }

    Text {
        id: hint
        visible: br.error !== ""
        anchors { left: parent.left; leftMargin: Style.pad * 2; right: parent.right; top: form.bottom; topMargin: Style.pad }
        text: br.error
        color: Style.red
        font { family: Style.font; pixelSize: Style.smallFont + 1 }
    }

    // Preview table.
    Rectangle {
        id: table
        anchors { left: parent.left; right: parent.right; leftMargin: Style.pad * 2; rightMargin: Style.pad * 2
                  top: hint.visible ? hint.bottom : form.bottom; topMargin: Style.pad
                  bottom: footer.top; bottomMargin: Style.pad }
        color: "transparent"
        border { width: 1; color: Style.surface }
        radius: 4
        clip: true

        Item {
            id: tableHead
            width: parent.width; height: Math.round(26 * Style.zoom)
            Text { x: Style.pad * 1.5; anchors.verticalCenter: parent.verticalCenter; text: "current"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.smallFont + 1 } }
            Text { x: parent.width * 0.52; anchors.verticalCenter: parent.verticalCenter; text: "new"; color: Style.fgDim
                   font { family: Style.font; pixelSize: Style.smallFont + 1 } }
            Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }
        }
        ListView {
            id: list
            anchors { left: parent.left; right: parent.right; top: tableHead.bottom; bottom: parent.bottom }
            model: br.rows
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            delegate: Item {
                required property int index
                required property var modelData
                width: ListView.view.width
                height: Math.round(28 * Style.zoom)
                Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.alpha(Style.surface, 0.6) }
                Text {
                    x: Style.pad * 1.5
                    width: parent.width * 0.46
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.name
                    textFormat: Text.PlainText
                    elide: Text.ElideMiddle
                    color: Style.fgLight
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
                Text {
                    x: parent.width * 0.48
                    anchors.verticalCenter: parent.verticalCenter
                    text: "→"
                    color: Style.fgDim
                    font { family: Style.font; pixelSize: Style.smallFont }
                }
                Text {
                    x: parent.width * 0.52
                    width: parent.width * 0.46
                    anchors.verticalCenter: parent.verticalCenter
                    textFormat: Text.RichText
                    text: modelData.conflict ? (modelData.changed ? "exists, skipped" : "") : (modelData.changed ? br.newNameHtml(modelData.newName) : "<font color=\"" + Style.fgDim + "\">unchanged</font>")
                    elide: Text.ElideMiddle
                    color: modelData.conflict ? Style.yellow : Style.fg
                    font { family: Style.font; pixelSize: Style.fontSize }
                }
            }
        }
    }

    Item {
        id: footer
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: Math.round(44 * Style.zoom)
        Text {
            anchors { left: parent.left; leftMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            text: "journaled · undo with u" + (br.skipCount > 0 ? "  ·  " + br.skipCount + " skipped" : "") + "  ·  alt+1…4 mode"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont + 1 }
        }
        Rectangle {
            anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
            height: Math.round(30 * Style.zoom)
            width: applyText.implicitWidth + Style.pad * 3
            radius: 4
            color: br.okCount > 0 ? Style.accent : Style.surface
            Text {
                id: applyText
                anchors.centerIn: parent
                text: "rename " + br.okCount
                color: br.okCount > 0 ? Style.bgDarker : Style.fgDim
                font { family: Style.font; pixelSize: Style.fontSize; bold: true }
            }
            MouseArea { anchors.fill: parent; onClicked: br.apply() }
        }
    }
}
