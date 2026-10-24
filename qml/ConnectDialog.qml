import QtQuick
import CaptainFM

// Connect to a server or cloud drive. Up/Down (or Tab) pick a row, Left/Right change a choice, Space flips a
// switch, Enter connects, Esc closes. The password is only kept when "Remember password" is on, and then
// only in the system keyring.
Rectangle {
    id: dlg
    visible: false
    width: Math.min(parent.width - Style.pad * 4, Math.round(620 * Style.zoom))
    height: Math.min(parent.height - y - Style.pad * 2, head.height + body.implicitHeight + foot.height + Style.pad)
    x: (parent.width - width) / 2
    y: Math.round(parent.height * 0.1)
    radius: 6
    color: Style.bgDark
    border { width: 2; color: Style.accent }
    focus: visible

    property var app
    property int current: 0
    property int kindIndex: 0
    property var values: ({})
    property bool save: true
    property bool remember: false
    property string note: ""
    // Id of the saved connection being edited; empty for a new one.
    property string editId: ""

    signal closed()

    readonly property var kinds: [
        { id: "sftp", label: "SFTP (SSH)", fields: ["host", "port", "path", "user", "password"],
          labels: { path: "Folder", password: "Password (empty: SSH key)" } },
        { id: "smb", label: "SMB / Windows share", fields: ["host", "path", "user", "password"],
          labels: { path: "Share", user: "User (DOMAIN;user)" } },
        { id: "ftp", label: "FTP", fields: ["host", "port", "path", "user", "password"], labels: { path: "Folder" } },
        { id: "davs", label: "WebDAV (https)", fields: ["host", "port", "path", "user", "password"], labels: { path: "Path" } },
        { id: "dav", label: "WebDAV (http)", fields: ["host", "port", "path", "user", "password"], labels: { path: "Path" } },
        { id: "nfs", label: "NFS", fields: ["host", "path"], labels: { path: "Export" } },
        { id: "gdrive", label: "Google Drive", fields: [], cloud: true, labels: {} },
        { id: "gcs", label: "Google Cloud Storage", fields: ["host", "path"], cloud: true,
          labels: { host: "Project number", path: "Bucket (empty: all)" } },
        { id: "dropbox", label: "Dropbox", fields: [], cloud: true, labels: {} },
        { id: "onedrive", label: "OneDrive", fields: [], cloud: true, labels: {} },
        { id: "box", label: "Box", fields: [], cloud: true, labels: {} },
        { id: "s3", label: "S3 or compatible", fields: ["host", "path", "user", "password"], cloud: true,
          labels: { host: "Endpoint", path: "Bucket (empty: all)", user: "Access key", password: "Secret key" } }
    ]
    // The first kinds are the ones most people use; the other cloud services appear after "More cloud services…".
    readonly property int commonKinds: 7
    property bool moreServices: false
    readonly property bool onMoreRow: !moreServices && kindIndex === commonKinds
    readonly property var kind: onMoreRow ? ({ id: "more", label: "More cloud services…", fields: [], labels: ({}) }) : kinds[kindIndex]
    readonly property bool hasPassword: kind.fields.indexOf("password") >= 0
    readonly property var defaultLabels: ({ host: "Host", port: "Port (optional)", path: "Folder", user: "User", password: "Password" })

    // Rows in order: the kind, its fields, then the save options.
    readonly property var rows: {
        const r = [{ key: "kind", type: "choice", label: "Service" }]
        if (onMoreRow) return r
        for (const f of kind.fields)
            r.push({ key: f, type: f === "password" ? "secret" : "text", label: (f === "password" && editId !== "" && remember) ? "Password (empty: keep)" : (kind.labels[f] ?? defaultLabels[f]) })
        // Cloud drives always need a name: it is the folder they mount in.
        if (!kind.cloud && editId === "") r.push({ key: "save", type: "switch", label: "Save this connection" })
        if (kind.cloud || save) r.push({ key: "name", type: "text", label: kind.cloud ? "Name" : "Name (shown in the sidebar)" })
        if (hasPassword && (kind.cloud || save)) r.push({ key: "remember", type: "switch", label: "Remember password (keyring)" })
        return r
    }

    // `kindId` preselects a service; `prefill` holds field values.
    function open(prefill, kindId) {
        values = prefill ?? ({})
        current = 0
        note = ""
        save = true
        remember = false
        editId = ""
        kindIndex = Math.max(0, kinds.findIndex(k => k.id === kindId))
        moreServices = kindIndex >= commonKinds
        visible = true
        refocus()
    }

    // Change a saved connection; the password stays in the keyring unless a new one is typed.
    function edit(c) {
        open({ host: c.host, port: c.port, path: c.path, user: c.user, name: c.name }, c.kind)
        editId = c.id
        remember = c.remember
    }

    function close() { visible = false; closed() }

    function refocus() { Qt.callLater(() => { const it = rowRepeater.itemAt(current); if (it) it.focusRow(); else dlg.forceActiveFocus() }) }

    function move(d) {
        current = (current + d + rows.length) % rows.length
        refocus()
    }

    function change(d) {
        const row = rows[current]
        if (row.type === "choice") {
            if (onMoreRow && d > 0) moreServices = true
            else {
                const count = moreServices ? kinds.length : commonKinds + 1
                kindIndex = (kindIndex + d + count) % count
            }
            if (current >= rows.length) current = rows.length - 1
        } else if (row.type === "switch") {
            if (row.key === "save") save = !save
            else remember = !remember
        } else return
        refocus()
    }

    function submit() {
        if (onMoreRow) { moreServices = true; refocus(); return }
        const v = values
        const cloud = kind.cloud === true
        const name = (v.name ?? "").trim() || (cloud ? kind.label : "")
        if (!cloud && (v.host ?? "").trim() === "") { note = "Enter a host"; current = rows.findIndex(r => r.key === "host"); refocus(); return }
        if ((cloud || save) && name !== "") {
            const taken = JSON.parse(Backend.savedConnections()).some(c => c.id !== editId && c.name.trim().toLowerCase() === name.toLowerCase())
            if (taken) { note = "A connection named " + name + " already exists"; current = rows.findIndex(r => r.key === "name"); refocus(); return }
        }
        const keep = hasPassword && (cloud || save) && remember && (v.password ?? "") !== ""
        const req = {
            id: editId, kind: kind.id, name: name, host: (v.host ?? "").trim(), port: (v.port ?? "").trim(), path: (v.path ?? "").trim(),
            user: (v.user ?? "").trim(), remember: keep, password: v.password ?? "", save: cloud || save
        }
        close()
        if (req.id !== "") app.runUpdate(req)
        else app.runConnect(req)
    }

    // Test automation: `!conn:down`, `!conn:right`, `!conn:enter` ... (see Main.automation).
    function press(name) {
        switch (name) {
        case "down": move(1); break
        case "up": move(-1); break
        case "left": change(-1); break
        case "right": change(1); break
        case "enter": submit(); break
        case "edit": edit(JSON.parse(Backend.savedConnections())[0]); break
        default: close()
        }
    }

    Keys.onPressed: (e) => {
        switch (e.key) {
        case Qt.Key_Escape: close(); break
        case Qt.Key_Down: case Qt.Key_Tab: move(1); break
        case Qt.Key_Up: case Qt.Key_Backtab: move(-1); break
        case Qt.Key_Left: case Qt.Key_H: change(-1); break
        case Qt.Key_Right: case Qt.Key_L: case Qt.Key_Space: change(1); break
        case Qt.Key_Return: case Qt.Key_Enter: submit(); break
        default: return
        }
        e.accepted = true
    }

    Item {
        id: head
        x: 0; y: 0; width: parent.width; height: Math.round(54 * Style.zoom)
        Text {
            x: Style.pad * 2; y: Style.pad
            text: dlg.editId !== "" ? "Edit connection" : "Connect"
            color: Style.fg
            font { family: Style.font; pixelSize: Math.round(Style.fontSize * 1.25); bold: true }
        }
        Text {
            x: Style.pad * 2; y: Style.pad + Math.round(Style.fontSize * 1.7)
            text: dlg.kind.cloud && dlg.kind.fields.indexOf("password") < 0
                  ? "Your browser opens to sign in the first time" : "↑↓ choose · ←→ change · Enter " + (dlg.editId !== "" ? "save" : "connect") + " · Esc close"
            color: Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
        Rectangle { anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 1; color: Style.surface }
    }

    Column {
        id: body
        anchors { left: parent.left; right: parent.right; top: head.bottom }
        Repeater {
            id: rowRepeater
            model: dlg.rows
            delegate: Item {
                id: row
                required property int index
                required property var modelData
                readonly property bool active: index === dlg.current
                readonly property bool text: modelData.type === "text" || modelData.type === "secret"
                width: body.width
                height: Math.round(38 * Style.zoom)

                function focusRow() { if (text) field.forceActiveFocus(); else dlg.forceActiveFocus() }

                Rectangle { anchors.fill: parent; visible: row.active; color: Style.selection }
                Rectangle { visible: row.active; width: 3; height: parent.height; color: Style.accent }

                Text {
                    x: Style.pad * 2
                    width: parent.width * 0.46 - Style.pad * 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: row.modelData.label
                    elide: Text.ElideRight
                    color: Style.fg
                    font { family: Style.font; pixelSize: Style.fontSize }
                }

                // Choice and switch values
                Text {
                    visible: !row.text
                    anchors { right: parent.right; rightMargin: Style.pad * 2; verticalCenter: parent.verticalCenter }
                    width: parent.width * 0.54 - Style.pad * 2
                    horizontalAlignment: Text.AlignRight
                    elide: Text.ElideRight
                    color: row.active ? Style.accent : Style.fgLight
                    font { family: Style.font; pixelSize: Style.fontSize }
                    text: {
                        const k = row.modelData.key
                        if (k === "kind") return "‹ " + dlg.kind.label + " ›"
                        return (k === "save" ? dlg.save : dlg.remember) ? "● on" : "○ off"
                    }
                }

                TextInput {
                    id: field
                    visible: row.text
                    x: parent.width * 0.46
                    width: parent.width * 0.54 - Style.pad * 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: dlg.values[row.modelData.key] ?? ""
                    echoMode: row.modelData.type === "secret" ? TextInput.Password : TextInput.Normal
                    color: Style.fg
                    selectionColor: Style.accent
                    selectedTextColor: Style.bg
                    font { family: Style.font; pixelSize: Style.fontSize }
                    clip: true
                    onTextChanged: dlg.values[row.modelData.key] = text
                    onActiveFocusChanged: if (activeFocus) dlg.current = row.index
                    Keys.onPressed: (e) => {
                        switch (e.key) {
                        case Qt.Key_Escape: dlg.close(); break
                        case Qt.Key_Down: case Qt.Key_Tab: dlg.move(1); break
                        case Qt.Key_Up: case Qt.Key_Backtab: dlg.move(-1); break
                        case Qt.Key_Return: case Qt.Key_Enter: dlg.submit(); break
                        default: return
                        }
                        e.accepted = true
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    visible: !row.text
                    onClicked: { dlg.current = row.index; dlg.change(1) }
                }
            }
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
            text: dlg.note !== "" ? dlg.note
                  : (dlg.hasPassword && dlg.remember ? "The password is stored encrypted in your keyring, never in a file"
                     : (dlg.hasPassword ? "The password is used now and then forgotten" : "Nothing secret is stored"))
            color: dlg.note !== "" ? Style.red : Style.fgDim
            font { family: Style.font; pixelSize: Style.smallFont }
        }
    }
}
