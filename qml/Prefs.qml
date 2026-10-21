pragma Singleton
import QtQuick
import CaptainFM

// User preferences from ~/.config/captainfm/settings.json. Only values that differ from the defaults are
// written. `v` is replaced on every change, so bindings like `Prefs.v.dropAction` update live.
QtObject {
    readonly property var defaults: ({
        startup: "restore",        // restore | home | folder
        startFolder: "",
        defaultView: "list",       // list | tree | miller | grid | columns
        openOnClick: "double",     // double | single
        dropAction: "auto",        // auto (what the source and modifier keys ask for) | copy | move
        dragOpenDelay: 5,          // seconds a drag rests on a folder before it opens; 0 = disabled
        confirmTrash: false,
        terminal: "",              // command line; empty = xdg-terminal-exec
        fontFamily: "",            // empty = the system font
        sizeUnits: "binary",       // binary (1024) | decimal (1000)
        dateFormat: "friendly"     // friendly | iso
    })

    property var v: defaults

    function load() {
        let saved = {}
        try { saved = JSON.parse(Backend.readConfig("settings")) } catch (e) { saved = ({}) }
        v = Object.assign({}, defaults, saved)
    }

    function set(key, value) {
        v = Object.assign({}, v, { [key]: value })
        const out = {}
        for (const k in v) if (JSON.stringify(v[k]) !== JSON.stringify(defaults[k])) out[k] = v[k]
        Backend.writeConfig("settings", JSON.stringify(out))
    }
}
