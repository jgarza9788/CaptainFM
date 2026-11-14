import QtQuick
import CaptainFM

// Test automation: CFM_KEYS="j j @nav.jump" feeds keys and commands, CFM_SIZE=WxH sizes the window, and CFM_SHOT=file.png saves
// a screenshot and quits. scripts/ui-tests.sh and scripts/selection-tests.sh show the key language.
QtObject {
    id: automation
    required property var app            // the Window (Main.qml)
    // Items inside it that the key language drives.
    required property var engine
    required property var keyRoot
    required property var sidebar
    required property var promptBar
    required property var clashBox
    required property var inspectorPane
    property var queue: []
    property Timer stepper: Timer {
        interval: 250
        repeat: true
        onTriggered: automation.step()
    }
    property Timer shooter: Timer {
        interval: 1500
        onTriggered: automation.shoot()
    }

    function start() {
        const size = Backend.env("CFM_SIZE")
        if (size !== "") { const wh = size.split("x"); app.width = parseInt(wh[0]); app.height = parseInt(wh[1]) }
        const keys = Backend.env("CFM_KEYS")
        const shot = Backend.env("CFM_SHOT")
        if (keys !== "") { queue = keys.split(" ").filter(k => k !== ""); stepper.start() }
        else if (shot !== "") shooter.start()
    }

    function step() {
        if (queue.length === 0) {
            stepper.stop()
            if (Backend.env("CFM_SHOT") !== "") shooter.start()
            return
        }
        const t = queue.shift()
        // CFM_DEBUG=1 logs when each key was fed, so a recording can be matched to the keys (see scripts/record-demo03.sh).
        if (app.debugStartup && t !== "<F9>") console.log("[keys] " + Backend.sinceStartMs() + " " + t)
        if (t[0] === "@") app.commands.run(t.substring(1), 1, false)
        else if (t[0] === "=") {
            const f = app.activeFocusItem
            if (f && f.text !== undefined) f.text = f.text + t.substring(1).replace(/_/g, " ")
        } else if (t === "!accept") {
            const f = app.activeFocusItem
            if (promptBar.visible && promptBar.confirmMode) promptBar.accept()     // y on a y/n question
            else if (f && f.accepted) f.accepted()
        } else if (t.indexOf("!bind:") === 0) {
            const [cid, seq] = t.substring(6).split("=")
            app.bindKey(cid, seq.replace(/_/g, " "))
        } else if (t.indexOf("!mode:") === 0) app.lazyBatch().setMode(t.substring(6))
        else if (t.indexOf("!fold:") === 0) sidebar.toggleFold(t.substring(6))
        else if (t.indexOf("!clash:") === 0) clashBox.press(t.substring(7))
        else if (t.indexOf("!set:") === 0) app.lazySettings().press(t.substring(5))
        else if (t.indexOf("!conn:") === 0) app.lazyConnect().press(t.substring(6))
        else if (t.indexOf("!click:pin:") === 0) {
            // Test automation: left-click the pinned sidebar entry whose name contains the text.
            const it = sidebar.pinned.find(x => x.label.indexOf(t.substring(11)) >= 0)
            if (!it) app.toast("No pin “" + t.substring(11) + "”", "error")
            else if (it.offline) sidebar.connectPathRequested(it.path, it.home)
            else sidebar.navigate(it.path)
        }
        else if (t.indexOf("!rclick:") === 0) {
            // Test automation for the double right-click: !rclick:row, !rclick:tab:N, !rclick:pin:/path, !rclick:fake-remote, !rclick:root
            const w = t.substring(8)
            if (w === "row") app.rowRightClicked()
            else if (w.indexOf("tab:") === 0) app.tabRightClicked(parseInt(w.substring(4)), "tab " + w.substring(4))
            else if (w.indexOf("pin:") === 0) app.togglePinned(w.substring(4), "pin")
            else if (w.indexOf("side:") === 0) {
                // a real sidebar drive or remote whose name contains the text
                const found = sidebar.drives.concat(sidebar.remotes).find(x => x.label.indexOf(w.substring(5)) >= 0 || (x.path || "").indexOf(w.substring(5)) >= 0)
                if (found) app.deviceRightClicked(found); else app.toast("No sidebar entry “" + w.substring(5) + "”", "error")
            }
            else if (w === "root") app.deviceRightClicked({ label: "root", path: "/", dev: { kind: "disk" }, verb: "unmount" })
            else app.deviceRightClicked({ label: w, path: "/fake/" + w, dev: { kind: "smb", path: "/fake/" + w }, verb: "disconnect" })
        }
        else if (t.indexOf("!space:") === 0) {
            // Test automation for the remote capacity lookup (a hover cannot be sent): !space:<part of the name>
            const found = sidebar.remotes.find(x => x.label.indexOf(t.substring(7)) >= 0 || (x.path || "").indexOf(t.substring(7)) >= 0)
            if (found) sidebar.askSpace(found); else app.toast("No remote “" + t.substring(7) + "”", "error")
        }
        else if (t.indexOf("!click:") === 0 && t.indexOf("!click:pin:") !== 0) {
            // Test automation for the mouse selection protocol: !click:<row>[:C|S|CS] (Ctrl, Shift, both)
            const [row, mods] = t.substring(7).split(":")
            const m = (mods ?? "").indexOf("C") >= 0 ? Qt.ControlModifier : 0
            if (app.pane) app.pane.clickRow(parseInt(row), m | ((mods ?? "").indexOf("S") >= 0 ? Qt.ShiftModifier : 0))
        }
        else if (t.indexOf("!rclickrow:") === 0) {
            // A right-click on row N: it selects only that row when it is not part of the selection.
            const row = parseInt(t.substring(11))
            if (app.pane) app.pane.rightClickRow(row, JSON.parse(app.pane.dir.selectedPaths()).indexOf(app.pane.dir.pathAt(row)) >= 0)
        }
        else if (t === "!clickempty") { if (app.pane) app.pane.clickEmpty(0) }
        else if (t.indexOf("!tear:") === 0) {
            // Test automation for tearing a tab off (a drag cannot be sent): !tear:<tab index>:<left|right|top|bottom>
            const [ti, zone] = t.substring(6).split(":")
            app.tearTab(parseInt(ti), zone || "right")
        }
        else if (t === "!play") inspectorPane.togglePlayback()
        else if (t === "!clear") { const f = app.activeFocusItem; if (f && f.text !== undefined) f.text = "" }
        else if (t === "!next") app.lazyBatch().nextField(false)
        else if (t === "!zoom") Style.zoomTarget = 1.4
        else engine.feedToken(t)
    }

    function shoot() {
        keyRoot.grabToImage(function (result) {
            result.saveToFile(Backend.env("CFM_SHOT"))
            Qt.quit()
        })
    }
}
