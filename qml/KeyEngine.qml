import QtQuick
import CaptainFM

// Modal key handling: counts, multi-key sequences (including the "," leader) and the
// which-key hints. Bindings map a space-separated key sequence to a command id.
QtObject {
    id: eng

    property var commands            // Commands object
    // fp = File Pilot style (Ctrl shortcuts, arrows, type-ahead); vim = modal keys with a leader.
    property string profile: "fp"
    property var userKeys: ({})      // the user's keys.json overrides
    // Rebuild here, after the profile has its new value: a handler on the owner's `keymap` can run before this
    // property is updated and would build the bindings of the old profile (Vim keys dead when started with Vim).
    onProfileChanged: { reset(); rebuild(userKeys) }
    property var bindings: ({})      // "g g" -> "cursor.top"
    property var prefixes: ({})      // "g" -> true (proper prefix of some binding)
    property var seq: []
    property string count: ""
    property var hintItems: []
    property bool hintVisible: false
    readonly property string pending: (count !== "" ? count + " " : "") + seq.join(" ")
    readonly property string hintPrefix: seq.join(" ")

    readonly property var groups: ({
        ",": "leader",
        ", f": "find",
        ", p": "pins",
        ", t": "tabs",
        ", w": "panes",
        ", o": "order by",
        ", v": "view",
        ", l": "layouts",
        ", g": "go to",
        ", y": "yank",
        ", r": "rename / run",
        "g": "go to",
        "y": "yank",
        "d": "delete / cut",
        "<C-w>": "panes",
        "z": "view"
    })

    readonly property var special: ({
        [Qt.Key_Escape]: "Esc", [Qt.Key_Return]: "CR", [Qt.Key_Enter]: "CR", [Qt.Key_Tab]: "Tab",
        [Qt.Key_Backtab]: "S-Tab", [Qt.Key_Backspace]: "BS", [Qt.Key_Delete]: "Del", [Qt.Key_Space]: "Space", [Qt.Key_Insert]: "Insert",
        [Qt.Key_Up]: "Up", [Qt.Key_Down]: "Down", [Qt.Key_Left]: "Left", [Qt.Key_Right]: "Right",
        [Qt.Key_Home]: "Home", [Qt.Key_End]: "End", [Qt.Key_PageUp]: "PageUp", [Qt.Key_PageDown]: "PageDown",
        [Qt.Key_F1]: "F1", [Qt.Key_F2]: "F2", [Qt.Key_F3]: "F3", [Qt.Key_F4]: "F4", [Qt.Key_F5]: "F5",
        [Qt.Key_F6]: "F6", [Qt.Key_F7]: "F7", [Qt.Key_F8]: "F8", [Qt.Key_F9]: "F9", [Qt.Key_F10]: "F10"
    })

    // Key event -> token such as "j", "J", "<C-d>", "<A-1>", "<Space>", "<S-Tab>"; "" for bare modifiers.
    function tokenFor(e) {
        const k = e.key
        if (k === Qt.Key_Shift || k === Qt.Key_Control || k === Qt.Key_Alt || k === Qt.Key_Meta
            || k === Qt.Key_AltGr || k === Qt.Key_CapsLock || k === Qt.Key_unknown) return ""
        const ctrl = (e.modifiers & Qt.ControlModifier) !== 0
        const alt = (e.modifiers & Qt.AltModifier) !== 0
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        const prefix = (ctrl ? "C-" : "") + (alt ? "A-" : "")

        // Keypad keys are distinct from the main row so they can carry their own bindings.
        if ((e.modifiers & Qt.KeypadModifier) && ((k >= Qt.Key_0 && k <= Qt.Key_9) || k === Qt.Key_Plus
            || k === Qt.Key_Minus || k === Qt.Key_Asterisk || k === Qt.Key_Slash || k === Qt.Key_Period))
            return "<" + prefix + "KP-" + String.fromCharCode(k) + ">"

        const name = special[k]
        if (name !== undefined) {
            const shifted = shift && name !== "S-Tab" ? "S-" : ""
            return "<" + prefix + shifted + name + ">"
        }
        let ch = ""
        if (k >= Qt.Key_A && k <= Qt.Key_Z) {
            ch = String.fromCharCode(k + 32)
            if (shift && !ctrl && !alt) return ch.toUpperCase()
            if (shift) return "<" + prefix + "S-" + ch + ">"
        } else if (ctrl || alt) {
            // Text is a control character with Ctrl held, so go by the key code.
            if (k >= 0x20 && k <= 0x7e) ch = String.fromCharCode(k)
            else return ""
        } else {
            ch = e.text
            if (ch.length !== 1 || ch < " ") return ""
            return ch
        }
        return prefix !== "" ? "<" + prefix + ch + ">" : ch
    }

    function rebuild(userKeys) {
        const b = {}
        const list = commands.all()
        for (let i = 0; i < list.length; i++) {
            const keys = (profile === "fp" ? list[i].fp : list[i].keys) ?? []
            for (let j = 0; j < keys.length; j++) b[keys[j]] = list[i].id
        }
        // keys.json holds per-profile overrides; a flat file from before profiles belongs to vim.
        const structured = userKeys.vim !== undefined || userKeys.fp !== undefined
        const overrides = structured ? (userKeys[profile] ?? {}) : (profile === "vim" ? userKeys : {})
        for (const k in overrides) {
            if (overrides[k] === "" || overrides[k] === null) delete b[k]
            else b[k] = overrides[k]
        }
        const p = {}
        for (const k in b) {
            const parts = k.split(" ")
            for (let n = 1; n < parts.length; n++) p[parts.slice(0, n).join(" ")] = true
        }
        bindings = b
        prefixes = p
    }

    function keysFor(id) {
        const out = []
        for (const k in bindings) if (bindings[k] === id) out.push(k)
        return out
    }

    function prettyToken(t) {
        const m = /^<((?:[CAS]-)+)(.+)>$/.exec(t)
        if (m) {
            const mods = m[1].replace("C-", "Ctrl+").replace("A-", "Alt+").replace("S-", "Shift+")
            return mods + (m[2].length === 1 ? m[2].toUpperCase() : m[2])
        }
        return t.replace("<Space>", "SPC").replace("<CR>", "⏎").replace("<Esc>", "ESC")
                .replace("<BS>", "⌫").replace("<Tab>", "TAB").replace("<Del>", "DEL")
                .replace("<Left>", "←").replace("<Right>", "→").replace("<Up>", "↑").replace("<Down>", "↓")
    }
    function prettySeq(s) { return s.split(" ").map(prettyToken).join(" ") }

    function describe() {
        const pre = seq.join(" ")
        const found = {}
        for (const k in bindings) {
            if (k.indexOf(pre + " ") !== 0) continue
            const rest = k.substring(pre.length + 1).split(" ")
            const next = rest[0]
            if (rest.length === 1) {
                const cmd = commands.byId(bindings[k])
                found[next] = { key: prettyToken(next), label: cmd ? cmd.title : bindings[k], group: false }
            } else if (!found[next]) {
                found[next] = { key: prettyToken(next), label: groups[pre + " " + next] ?? "more", group: true }
            }
        }
        const items = Object.keys(found).map(k => found[k])
        items.sort((a, b) => (a.group - b.group) || a.key.localeCompare(b.key))
        return items
    }

    function reset() {
        seq = []
        count = ""
        hintTimer.stop()
        hintVisible = false
        hintItems = []
    }

    property Timer hintTimer: Timer {
        interval: eng.seq.length > 0 && eng.seq[0] === "," ? 120 : 450
        onTriggered: {
            if (eng.seq.length === 0) return
            eng.hintItems = eng.describe()
            eng.hintVisible = eng.hintItems.length > 0
        }
    }

    function feedToken(t) {
        if (t === "") return false
        if (profile === "vim" && seq.length === 0 && t.length === 1 && t >= "0" && t <= "9" && (t !== "0" || count !== "")
            && bindings[t] === undefined) {
            count += t
            return true
        }
        const candidate = seq.concat([t])
        const key = candidate.join(" ")
        const exact = bindings[key]
        if (exact !== undefined && prefixes[key] !== true) {
            const n = count === "" ? 1 : parseInt(count)
            const had = count !== ""
            reset()
            commands.run(exact, n, had)
            return true
        }
        if (prefixes[key] === true) {
            seq = candidate
            hintVisible = false
            hintTimer.restart()
            return true
        }
        if (seq.length > 0 || count !== "") { reset(); return true }
        // File Pilot style: plain letters search by name as you type.
        if (profile === "fp" && t.length === 1 && t > " ") { commands.typeahead(t); return true }
        return false
    }

    function handle(e) { return feedToken(tokenFor(e)) }
}
