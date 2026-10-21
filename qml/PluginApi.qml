import QtQuick
import CaptainFM

// What plugin QML panels see as `api`: the current state of the active pane plus a few actions.
QtObject {
    id: api
    property var app

    readonly property string path: app && app.pane ? app.pane.dir.path : ""
    readonly property string cursor: app && app.pane ? app.pane.currentPath : ""
    readonly property bool cursorIsDir: app && app.pane ? app.pane.currentIsDir : false
    readonly property int selectedCount: app && app.pane ? app.pane.dir.selectedCount : 0
    // Colours and font of the active theme (updates live when the Omarchy theme changes).
    readonly property var theme: ({
        bg: Style.bg, surface: Style.surface, fg: Style.fg, fgDim: Style.fgDim, accent: Style.accent,
        red: Style.red, yellow: Style.yellow, green: Style.green, blue: Style.blue, cyan: Style.cyan,
        magenta: Style.magenta, orange: Style.orange, font: Style.font, fontSize: Style.fontSize
    })

    function selection() {
        if (!app || !app.pane) return []
        const sel = JSON.parse(app.pane.dir.selectedPaths())
        return sel.length > 0 ? sel : (cursor !== "" ? [cursor] : [])
    }

    function toast(message, kind) { app.toast(message, kind) }
    function navigate(p) { if (app.pane) app.pane.navigate(p) }
    function command(id) { app.commands.run(id, 1, false) }

    // Run a plugin's Lua action; `done(value)` receives the action's returned data, if any.
    function call(plugin, action, done) { app.runPlugin(plugin, action, done) }
}
