pragma Singleton
import QtQuick
import CaptainFM

// Drag and drop onto folders, panes and sidebar places: what a drop would do, and doing it.
QtObject {
    // "copy" | "move" | "link", the way Windows does it: Ctrl copies, Shift moves, Alt (or Ctrl+Shift) links, and with
    // no key a drop moves within one drive and copies between drives. The keys reach us as the drag's proposed
    // action; Settings can fix the action instead.
    //
    // The held keys are read directly (`mods`), because the proposed action cannot tell a plain drop from a Shift
    // one: both come as a move. Keys win over the proposed action; without them it decides, as before.
    property int mods: 0                    // Qt.KeyboardModifier flags held now
    property var seen: ({ list: [], web: [], proposed: Qt.MoveAction })    // the drag last asked about

    function sample() { const m = Backend.heldModifiers(); if (m !== mods) mods = m }

    function action(proposed, list, destination) {
        const fixed = Prefs.v.dropAction
        if (fixed !== "auto") return fixed
        const ctrl = (mods & Qt.ControlModifier) !== 0, shift = (mods & Qt.ShiftModifier) !== 0, alt = (mods & Qt.AltModifier) !== 0
        if (alt || (ctrl && shift)) return "link"
        if (ctrl) return "copy"
        if (shift) return "move"        // a move even between drives
        if (proposed === Qt.LinkAction) return "link"
        if (proposed === Qt.CopyAction) return "copy"
        return list.length > 0 && !Backend.sameDevice(JSON.stringify(list), destination) ? "copy" : "move"
    }

    function parentOf(p) { return p.substring(0, Math.max(1, p.lastIndexOf("/"))) }

    function urlsOf(drag) { return drag.hasUrls ? drag.urls.map(u => String(u)) : [] }
    function textOf(drag) { return drag.hasText ? drag.text : "" }

    // Local files in the drop (from this app or another); empty for a link or image from a browser.
    function paths(drag) { return JSON.parse(Backend.dropPaths(JSON.stringify(urlsOf(drag)), textOf(drag))) }
    function webUrls(drag) { return JSON.parse(Backend.dropUrls(JSON.stringify(urlsOf(drag)), textOf(drag))) }

    // Moving something into the folder it already lives in does nothing.
    function alreadyThere(list, destination) {
        return list.length > 0 && list.every(p => parentOf(p) === destination || p === destination)
    }

    // Icon for a kind of drop (an arrow for move, a copy sheet for copy, ...), with a plain fallback.
    // `pointing` is where the arrow looks: toward the destination's name, so "left" when the icon is drawn
    // to the right of that name, "right" (the default) when it is to the left of it.
    function glyph(kind, pointing) {
        switch (kind) {
        case "copy": return Style.pick("\uf0c5", "+")
        case "move": return pointing === "left" ? Style.pick("\uf060", "←") : Style.pick("\uf061", "→")
        case "link": return Style.pick("\uf0c1", "↪")
        case "trash": return Style.pick("\uf1f8", "✕")
        case "download": return Style.pick("\uf019", "↓")
        case "none": return Style.pick("\uf00c", "✓")
        }
        return ""
    }

    // What the drop target says while dragging over it: `{ kind, text }`, kind "" when a drop would do nothing useful.
    function hint(drag, destination, name, trash) {
        sample()
        seen = { list: paths(drag), web: webUrls(drag), proposed: drag.proposedAction }
        return again(destination, name, trash)
    }

    // The hint for the drag last seen, with the keys held now.
    function again(destination, name, trash) {
        const list = seen.list
        if (list.length === 0) return seen.web.length > 0 ? { kind: "download", text: "Download to " + name } : { kind: "", text: "" }
        if (trash) return { kind: "trash", text: "Move to Trash" }
        const act = action(seen.proposed, list, destination)
        if (act === "move" && alreadyThere(list, destination)) return { kind: "none", text: "Already here" }
        return { kind: act, text: (act === "copy" ? "Copy to " : act === "move" ? "Move to " : "Link in ") + name }
    }

    // Do what the hint says. `trash` sends the files to the trash instead of into `destination`.
    function accept(drop, destination, trash) {
        const list = paths(drop)
        if (list.length === 0) {
            // A link or image from a browser: fetch it into the folder.
            const urls = webUrls(drop)
            if (urls.length === 0 || trash) return
            drop.accept(Qt.CopyAction)
            Backend.downloadUrls(JSON.stringify(urls), destination)
            return
        }
        const json = JSON.stringify(list)
        if (trash) {
            drop.accept(Qt.MoveAction)
            Backend.trash(json)
            return
        }
        sample()
        const act = action(drop.proposedAction, list, destination)
        // Tell the source what really happens, so it only removes its own copy after a move.
        drop.accept(act === "move" ? Qt.MoveAction : Qt.CopyAction)
        if (act === "move") {
            if (!alreadyThere(list, destination)) Backend.paste(json, destination, true)
        } else if (act === "link") Backend.link(json, destination)
        else Backend.paste(json, destination, false)
    }
}
