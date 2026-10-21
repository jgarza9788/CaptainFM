pragma Singleton
import QtQuick
import CaptainFM

// Colours come from the active Omarchy theme (Theme.json); the fallbacks match Catppuccin Mocha.
QtObject {
    readonly property var p: {
        try { return JSON.parse(Theme.json) } catch (e) { return ({}) }
    }

    readonly property color bg: p.background ?? "#1e1e2e"
    readonly property color bgDark: p.dark_background ?? "#181825"
    readonly property color bgDarker: p.darker_background ?? "#11111b"
    readonly property color surface: p.lighter_background ?? "#313244"
    readonly property color selection: p.selection ?? "#45475a"
    readonly property color muted: p.muted ?? "#585b70"
    readonly property color fg: p.foreground ?? "#cdd6f4"
    readonly property color fgDim: p.dark_foreground ?? "#6c7086"
    readonly property color fgLight: p.light_foreground ?? "#bac2de"
    readonly property color accent: p.accent ?? "#89b4fa"
    readonly property color red: p.red ?? "#f38ba8"
    readonly property color yellow: p.yellow ?? "#f9e2af"
    readonly property color orange: p.orange ?? "#fab387"
    readonly property color green: p.green ?? "#a6e3a1"
    readonly property color cyan: p.cyan ?? "#94e2d5"
    readonly property color blue: p.blue ?? "#89b4fa"
    readonly property color magenta: p.magenta ?? "#f5c2e7"

    readonly property string font: Prefs.v.fontFamily !== "" ? Prefs.v.fontFamily : (Theme.fontFamily !== "" ? Theme.fontFamily : "monospace")
    readonly property bool nerd: font.toLowerCase().indexOf("nerd") >= 0

    // Zoom is animated by whoever sets `zoomTarget`; everything else scales from `zoom`.
    // User preferences (saved with the session): animations on/off, and row/padding density.
    property bool animations: true
    property real density: 1.0
    function anim(ms) { return animations ? ms : 0 }

    property real zoomTarget: 1.0
    property real zoom: zoomTarget
    Behavior on zoom { NumberAnimation { duration: anim(140); easing.type: Easing.OutCubic } }

    readonly property int fontSize: Math.round(13 * zoom)
    readonly property int smallFont: Math.round(11 * zoom)
    readonly property int rowHeight: Math.round(26 * zoom * density)
    readonly property int pad: Math.round(8 * zoom * density)
    readonly property int radius: 6

    // Rich text with the query's words highlighted (in-order substrings; fuzzy letters as a fallback).
    function highlightHtml(text, query, color) {
        const esc = t => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        const words = query.split(/\s+/).filter(w => w !== "")
        if (words.length === 0 || /^re:/.test(query)) return esc(text)
        const lower = text.toLowerCase()
        let marks = [], from = 0, ok = true
        for (const w of words) {
            const i = lower.indexOf(w.toLowerCase(), from)
            if (i < 0) { ok = false; break }
            marks.push([i, i + w.length]); from = i + w.length
        }
        if (!ok) {
            // Fuzzy: highlight the letters of the whole query in order.
            marks = []
            const q = words.join("").toLowerCase()
            let qi = 0
            for (let i = 0; i < lower.length && qi < q.length; i++)
                if (lower[i] === q[qi]) { marks.push([i, i + 1]); qi++ }
            if (qi < q.length) return esc(text)
        }
        let out = "", pos = 0
        for (const [a, b] of marks) {
            out += esc(text.substring(pos, a)) + "<b><font color=\"" + color + "\">" + esc(text.substring(a, b)) + "</font></b>"
            pos = b
        }
        return out + esc(text.substring(pos))
    }

    function alpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }
    function mix(a, b, t) { return Qt.rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1) }

    function humanSize(n) {
        if (n === undefined || n < 0) return ""
        const base = Prefs.v.sizeUnits === "decimal" ? 1000 : 1024
        if (n < base) return n + "B"
        const units = ["K", "M", "G", "T", "P"]
        let v = n / base, i = 0
        while (v >= base && i < units.length - 1) { v /= base; i++ }
        return (v < 10 ? v.toFixed(1) : Math.round(v)) + units[i]
    }

    // "today 07:12", "yesterday", "oct 14", "mar 2025" (lower case, like the terminal-tool look).
    function humanDate(t) {
        if (!t || t < 0) return ""
        const d = new Date(t * 1000), now = new Date()
        const pad = n => (n < 10 ? "0" : "") + n
        if (Prefs.v.dateFormat === "iso")
            return d.getFullYear() + "-" + pad(d.getMonth() + 1) + "-" + pad(d.getDate()) + " " + pad(d.getHours()) + ":" + pad(d.getMinutes())
        const months = ["jan","feb","mar","apr","may","jun","jul","aug","sep","oct","nov","dec"]
        const day = x => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
        const diff = Math.round((day(now) - day(d)) / 86400000)
        if (diff === 0) return "today " + pad(d.getHours()) + ":" + pad(d.getMinutes())
        if (diff === 1) return "yesterday"
        if (d.getFullYear() === now.getFullYear()) return months[d.getMonth()] + " " + pad(d.getDate())
        return months[d.getMonth()] + " " + d.getFullYear()
    }

    // Nerd Font icons by extension / well-known name (used when the system font is a Nerd Font).
    readonly property var nerdByExt: ({
        rs: "\ue7a8", py: "\ue73c", js: "\ue74e", mjs: "\ue74e", ts: "\ue628", tsx: "\ue7ba", jsx: "\ue7ba",
        c: "\ue61e", h: "\ue61e", cpp: "\ue61d", hpp: "\ue61d", cc: "\ue61d", go: "\ue627", lua: "\ue620",
        java: "\ue738", rb: "\ue739", php: "\ue73d", swift: "\ue755", kt: "\ue634", nix: "\uf313",
        html: "\ue736", htm: "\ue736", css: "\ue749", scss: "\ue749", qml: "\uf1c9", vim: "\ue62b",
        sh: "\uf489", bash: "\uf489", zsh: "\uf489", fish: "\uf489",
        json: "\ue60b", yml: "\uf013", yaml: "\uf013", toml: "\uf013", ini: "\uf013", conf: "\uf013", xml: "\uf1c9",
        md: "\ue73e", markdown: "\ue73e", txt: "\uf15c", log: "\uf15c", pdf: "\uf1c1", epub: "\uf02d",
        doc: "\uf1c2", docx: "\uf1c2", odt: "\uf1c2", xls: "\uf1c3", xlsx: "\uf1c3", csv: "\uf1c3", ppt: "\uf1c4", pptx: "\uf1c4",
        png: "\uf1c5", jpg: "\uf1c5", jpeg: "\uf1c5", gif: "\uf1c5", webp: "\uf1c5", svg: "\uf1c5", bmp: "\uf1c5", ico: "\uf1c5", avif: "\uf1c5",
        mp3: "\uf1c7", flac: "\uf1c7", ogg: "\uf1c7", wav: "\uf1c7", opus: "\uf1c7", m4a: "\uf1c7",
        mp4: "\uf1c8", mkv: "\uf1c8", webm: "\uf1c8", mov: "\uf1c8", avi: "\uf1c8",
        zip: "\uf1c6", tar: "\uf1c6", gz: "\uf1c6", xz: "\uf1c6", zst: "\uf1c6", "7z": "\uf1c6", rar: "\uf1c6", bz2: "\uf1c6",
        iso: "\uf0a0", img: "\uf0a0", deb: "\uf187", rpm: "\uf187", pkg: "\uf187", appimage: "\uf085",
        tiff: "\uf1c5", tif: "\uf1c5", heic: "\uf1c5", heif: "\uf1c5", jxl: "\uf1c5", psd: "\uf1c5", xcf: "\uf1c5", raw: "\uf1c5",
        cr2: "\uf1c5", nef: "\uf1c5", arw: "\uf1c5", dng: "\uf1c5", ai: "\uf1c5", eps: "\uf1c5",
        m4v: "\uf1c8", wmv: "\uf1c8", flv: "\uf1c8", aac: "\uf1c7", wma: "\uf1c7",
        sqlite: "\uf1c0", sqlite3: "\uf1c0", cbz: "\uf02d", cbr: "\uf02d", crt: "\uf084", cer: "\uf084",
        ttf: "\uf031", otf: "\uf031", woff: "\uf031", lock: "\uf023", pem: "\uf084", key: "\uf084", sql: "\uf1c0", db: "\uf1c0"
    })
    readonly property var nerdByName: ({
        "makefile": "\uf085", "dockerfile": "\uf308", "license": "\uf0a3", "readme": "\ue73e", ".gitignore": "\ue702",
        ".gitconfig": "\ue702", "cargo.lock": "\uf023", "package.json": "\ue71e", ".env": "\uf023", "flake.lock": "\uf023"
    })

    function nerdGlyph(name, isDir, isLink) {
        if (isDir) return "\uf07b"
        const lower = name.toLowerCase()
        if (nerdByName[lower] !== undefined) return nerdByName[lower]
        const dot = lower.lastIndexOf(".")
        const ext = dot > 0 ? lower.substring(dot + 1) : ""
        if (nerdByExt[ext] !== undefined) return nerdByExt[ext]
        return isLink ? "\uf0c1" : "\uf15b"
    }

    // Pick the Nerd Font icon when available, else the plain character.
    function pick(nerdChar, plain) { return nerd ? nerdChar : plain }

    // Single-character markers that work in any font (no Nerd Font needed).
    function glyph(name, isDir, isLink) {
        if (nerd) return nerdGlyph(name, isDir, isLink)
        if (isDir) return "\u25b8"                       // ▸
        if (name.charAt(0) === ".") return "\u00b7"      // ·
        const k = kind(name, false, isLink)
        const dot = name.lastIndexOf(".")
        const ext = dot > 0 ? name.substring(dot + 1).toLowerCase() : ""
        if (ext === "py") return "\u03bb"                // λ
        if (ext === "sh" || ext === "bash" || ext === "zsh") return "$"
        if (ext === "json") return "{"
        switch (k) {
        case "config": return "\u25c6"                   // ◆
        case "text": return "\u2261"                     // ≡
        case "archive": return "\u25a3"                  // ▣
        case "image": return "\u25a4"                    // ▤
        case "video": return "\u25b6"                    // ▶
        case "audio": return "\u266a"                    // ♪
        case "pdf": return "\u25a6"                      // ▦
        case "code": return "\u2039\u203a"              // ‹›
        case "link": return "\u21aa"                     // ↪
        default: return "\u00b7"
        }
    }

    function glyphColor(name, isDir) {
        if (isDir) return accent
        const dot = name.lastIndexOf(".")
        const ext = dot > 0 ? name.substring(dot + 1).toLowerCase() : ""
        const lang = { rs: orange, py: yellow, js: yellow, ts: blue, tsx: blue, go: cyan, c: blue, h: blue, cpp: blue, hpp: blue,
                       html: orange, css: blue, lua: blue, java: red, rb: red, nix: cyan, md: yellow, qml: green }
        if (lang[ext] !== undefined) return lang[ext]
        if (ext === "sh" || ext === "bash" || ext === "zsh") return green
        if (ext === "json") return orange
        switch (kind(name, false, false)) {
        case "config": return yellow
        case "text": return yellow
        case "archive": return magenta
        case "image": return orange
        case "video": case "audio": return orange
        case "code": return green
        default: return fgDim
        }
    }

    // Nerd Font (Font Awesome range) glyphs by file kind; plain markers when the font has none.
    readonly property var kindOfExt: ({
        png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image", svg: "image", bmp: "image", ico: "image",
        mp3: "audio", flac: "audio", ogg: "audio", wav: "audio", opus: "audio", m4a: "audio",
        mp4: "video", mkv: "video", webm: "video", mov: "video", avi: "video",
        zip: "archive", tar: "archive", gz: "archive", xz: "archive", zst: "archive", "7z": "archive", rar: "archive", bz2: "archive",
        pdf: "pdf", md: "text", txt: "text", log: "text",
        rs: "code", py: "code", js: "code", ts: "code", c: "code", h: "code", cpp: "code", go: "code", lua: "code",
        sh: "code", html: "code", css: "code", qml: "code", java: "code", rb: "code",
        json: "config", toml: "config", yaml: "config", yml: "config", conf: "config", ini: "config"
    })
    readonly property var glyphOfKind: ({
        dir: "\uf07b", link: "\uf0c1", file: "\uf15b", image: "\uf1c5", audio: "\uf1c7", video: "\uf1c8",
        archive: "\uf1c6", pdf: "\uf1c1", text: "\uf0f6", code: "\uf1c9", config: "\uf013"
    })

    function kind(name, isDir, isLink) {
        if (isDir) return "dir"
        const dot = name.lastIndexOf(".")
        const ext = dot > 0 ? name.substring(dot + 1).toLowerCase() : ""
        return kindOfExt[ext] ?? (isLink ? "link" : "file")
    }

    function kindColor(k) {
        switch (k) {
        case "dir": return accent
        case "image": return magenta
        case "audio": case "video": return orange
        case "archive": return red
        case "code": return green
        case "config": return yellow
        case "pdf": return red
        default: return fgLight
        }
    }

    function icon(name, isDir, isLink) {
        if (!nerd) return isDir ? "\u25b8" : " "
        return glyphOfKind[kind(name, isDir, isLink)]
    }
}
