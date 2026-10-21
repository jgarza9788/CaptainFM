import QtQuick

// Side panel: branch and changed files of the folder in the active pane.
// `api` is injected by CaptainFM (see PluginApi.qml).
Item {
    id: root
    property var api
    property var result: ({ repo: false, loading: true })

    function refresh() {
        if (api) api.call("git-status", "status", (value) => { result = value })
    }

    // `api` is injected after the panel is created.
    onApiChanged: {
        if (!api) return
        api.pathChanged.connect(refresh)   // follow the active pane's folder
        refresh()
    }
    Column {
        anchors { fill: parent; margins: 12 }
        spacing: 6
        Text {
            text: root.result.loading ? "…" : (root.result.repo ? "⎇  " + root.result.branch : "Not a git repository")
            color: root.api ? root.api.theme.accent : "white"
            font { pixelSize: 14; bold: true; family: root.api ? root.api.theme.font : "monospace" }
        }
        Repeater {
            model: root.result.repo ? root.result.files : []
            delegate: Text {
                required property var modelData
                text: modelData.code + "  " + modelData.name
                color: modelData.code.indexOf("?") >= 0 ? root.api.theme.yellow : root.api.theme.green
                font { pixelSize: 12; family: root.api.theme.font }
            }
        }
    }
}
