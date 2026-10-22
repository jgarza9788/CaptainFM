import QtQuick
import CaptainFM

// Every user-facing action. `keys` are the default bindings (space-separated sequences);
// `words` are extra palette search terms; users override them in ~/.config/captainfm/keys.json or from the command palette.
QtObject {
    id: cmds

    property var app    // the main window

    // Commands contributed by plugins (see Main.loadPlugins).
    property var pluginCommands: []

    function all() { return list.concat(pluginCommands) }

    function byId(id) {
        const everything = all()
        for (let i = 0; i < everything.length; i++) if (everything[i].id === id) return everything[i]
        return null
    }

    function run(id, n, has) {
        const c = byId(id)
        if (!c) { app.toast("Unknown command: " + id, "error"); return }
        c.run(n ?? 1, has ?? false)
    }

    // ---- helpers ------------------------------------------------------------------------

    function pane() { return app.pane }

    function targets() {
        const p = pane()
        if (!p) return []
        let sel = JSON.parse(p.dir.selectedPaths())
        if (sel.length === 0 && p.currentPath !== "") sel = [p.currentPath]
        return sel
    }

    function plural(n, word) { return n + " " + word + (n === 1 ? "" : "s") }

    function yank(cut) {
        const paths = targets()
        if (paths.length === 0) return
        app.yankPaths = paths
        app.yankCut = cut
        Backend.clipboardWrite(JSON.stringify(paths), cut)
        const p = pane()
        p.endVisual()
        p.dir.clearSelection()
        app.toast((cut ? "Cut " : "Copied ") + plural(paths.length, "item"))
    }

    function paste(destPane) {
        const p = destPane ?? pane()
        if (!p) return
        let paths = app.yankPaths, cut = app.yankCut
        const sys = JSON.parse(Backend.clipboardRead())
        if (sys.paths && sys.paths.length > 0) { paths = sys.paths; cut = sys.cut }
        if (!paths || paths.length === 0) { app.toast("Nothing to paste", "warn"); return }
        const dest = p.deep ? p.parentOf(p.currentPath) : p.dir.path
        Backend.paste(JSON.stringify(paths), dest, cut)
        if (cut) { app.yankPaths = []; Backend.copyText("") }
    }

    function sendToOther(cut) {
        const other = app.tab ? app.tab.otherPane() : null
        const paths = targets()
        if (!other) { app.toast("Only one pane open", "warn"); return }
        if (paths.length === 0) return
        Backend.paste(JSON.stringify(paths), other.dir.path, cut)
        pane().endVisual()
        pane().dir.clearSelection()
    }

    function trash() {
        const paths = targets()
        if (paths.length === 0) return
        const go = () => { pane().endVisual(); Backend.trash(JSON.stringify(paths)) }
        if (!Prefs.v.confirmTrash) { go(); return }
        app.prompt.open({
            label: "Move " + plural(paths.length, "item") + " to the trash?",
            confirm: true,
            accept: go,
            cancel: () => app.refocus()
        })
    }

    function remove() {
        const paths = targets()
        if (paths.length === 0) return
        app.prompt.open({
            label: "Permanently delete " + plural(paths.length, "item") + "?",
            confirm: true,
            accept: () => { pane().endVisual(); Backend.remove(JSON.stringify(paths)) },
            cancel: () => app.refocus()
        })
    }

    function extract(intoFolder) {
        const paths = targets().filter(p => Style.kind(pane().baseName(p), false, false) === "archive")
        if (paths.length === 0) { app.toast("No archive selected", "warn"); return }
        pane().endVisual()
        Backend.extract(JSON.stringify(paths), intoFolder)
    }

    function rename() {
        const p = pane()
        if (!p || p.currentPath === "") return
        const sel = JSON.parse(p.dir.selectedPaths())
        if (sel.length > 1) { run("file.batchRename", 1, false); return }
        const path = p.currentPath
        app.prompt.open({
            label: "Rename:",
            text: p.baseName(path),
            selectStem: true,
            accept: (t) => {
                p.pendingPath = p.parentOf(path) + "/" + t
                Backend.renameTo(path, t)
            }
        })
    }

    function create(isDir) {
        const p = pane()
        if (!p || p.deep) return
        app.prompt.open({
            label: isDir ? "New folder:" : "New file:",
            text: "",
            accept: (t) => {
                if (t === "") return
                const dirName = isDir || t.endsWith("/")
                const name = t.replace(/\/+$/, "")
                p.pendingPath = p.dir.path + "/" + name
                Backend.create(p.dir.path, name, dirName)
            }
        })
    }

    function filter() {
        const p = pane()
        if (!p || p.deep) return
        app.prompt.open({
            label: "/ (words, or re:regex)",
            text: p.dir.filter,
            live: (t) => p.dir.setFilterText(t),
            accept: () => {},
            cancel: () => { p.dir.setFilterText("") }
        })
    }

    function search(drive) {
        const p = pane()
        if (!p) return
        const root = drive ? Backend.mountRoot(p.dir.path) : (p.deep ? p.dir.path : p.dir.path)
        p.searchText = ""
        p.dir.deepStart(root)
        app.prompt.open({
            label: (drive ? "drive " : "search ") + p.pretty(root) + " ›",
            text: "",
            live: (t) => { p.searchText = t; p.dir.deepQuery(t) },
            accept: () => {},
            cancel: () => { p.searchText = ""; p.exitDeep() }
        })
    }

    function setSort(key) {
        if (app.sortKey === key) app.sortDesc = !app.sortDesc
        else { app.sortKey = key; app.sortDesc = key === "size" || key === "mtime" }
        app.toast("Sort by " + key + (app.sortDesc ? " ↓" : " ↑"))
    }

    function go(path) { const p = pane(); if (p) p.navigate(path) }

    function zoom(delta) {
        Style.zoomTarget = Math.max(0.7, Math.min(2.2, Math.round((Style.zoomTarget + delta) * 100) / 100))
    }

    function moveBy(sign, n, has) {
        const p = pane()
        if (!p) return
        if (p.grid) { if (app.keymap === "fp" && p.visualMode) { p.endVisual(); p.dir.clearSelection() }; for (let i = 0; i < (has ? n : 1); i++) p.moveVertical(sign); return }
        // Windows-style: a plain arrow key drops a Shift-extended selection.
        if (app.keymap === "fp" && p.visualMode) { p.endVisual(); p.dir.clearSelection() }
        p.move(sign * (has ? n : 1))
    }

    // Type-ahead: letters typed in quick succession jump to the item starting with them.
    property string typeBuffer: ""
    property double typeTime: 0

    function typeahead(ch) {
        const p = pane()
        if (!p) return false
        const now = Date.now()
        const cycling = typeBuffer === ch.toLowerCase() && now - typeTime < 900
        typeBuffer = (now - typeTime < 900 && !cycling ? typeBuffer : "") + ch.toLowerCase()
        if (typeBuffer.length > 1 && typeBuffer.split("").every(c => c === typeBuffer[0])) typeBuffer = typeBuffer[0]
        typeTime = now
        const from = typeBuffer.length === 1 ? p.view.currentIndex + 1 : Math.max(0, p.view.currentIndex)
        const row = p.dir.findPrefix(typeBuffer, from)
        if (row >= 0) p.setCursor(row)
        return row >= 0
    }

    // ---- registry -----------------------------------------------------------------------

    property var list: [
        { id: "cursor.down", title: "Cursor down", fp: ["<Down>"], keys: ["j", "<Down>"], run: (n, has) => moveBy(1, n, has) },
        { id: "cursor.up", title: "Cursor up", fp: ["<Up>"], keys: ["k", "<Up>"], run: (n, has) => moveBy(-1, n, has) },
        { id: "cursor.top", title: "Go to first item", fp: ["<Home>", "<C-Home>"], keys: ["g g", "<Home>"],
          run: (n, has) => { const p = pane(); if (p) p.setCursor(has ? n - 1 : 0) } },
        { id: "cursor.bottom", title: "Go to last item (or line N)", fp: ["<End>", "<C-End>"], keys: ["G", "<End>"],
          run: (n, has) => { const p = pane(); if (p) p.setCursor(has ? n - 1 : p.view.count - 1) } },
        { id: "cursor.halfDown", title: "Half page down", keys: ["<C-d>"],
          run: (n) => { const p = pane(); if (p) p.move(Math.max(1, Math.floor(p.pageRows() / 2)) * n) } },
        { id: "cursor.halfUp", title: "Half page up", keys: ["<C-u>"],
          run: (n) => { const p = pane(); if (p) p.move(-Math.max(1, Math.floor(p.pageRows() / 2)) * n) } },
        { id: "cursor.pageDown", title: "Page down", fp: ["<PageDown>"], keys: ["<PageDown>"],
          run: (n) => { const p = pane(); if (p) p.move(p.pageRows() * n) } },
        { id: "cursor.pageUp", title: "Page up", fp: ["<PageUp>"], keys: ["<PageUp>"],
          run: (n) => { const p = pane(); if (p) p.move(-p.pageRows() * n) } },

        { id: "nav.up", title: "Parent folder / collapse", fp: ["<Left>"], keys: ["h", "<Left>"],
          run: (n) => {
              const p = pane(); if (!p) return
              // Icons/columns views: the arrow keys move sideways in File Pilot mode.
              if (p.grid && app.keymap === "fp") { p.moveHorizontal(-1); return }
              for (let i = 0; i < n; i++) { if (p.tree) p.treeLeft(); else p.goUp() }
          } },
        { id: "nav.parent", title: "Go to parent folder", fp: ["<BS>", "<A-Up>"], keys: ["<BS>", "-"],
          run: (n) => { const p = pane(); if (p) for (let i = 0; i < n; i++) p.goUp() } },
        { id: "nav.open", title: "Open / enter / expand", fp: ["<Right>"], keys: ["l", "<Right>"],
          run: () => {
              const p = pane(); if (!p) return
              if (p.tree) { p.treeRight(); return }
              if (p.grid && app.keymap === "fp") { p.moveHorizontal(1); return }
              if (app.pickMode && p.currentPath !== "" && !p.currentIsDir) { app.pickEnterFile(); return }
              p.openCurrent(p.deep && !p.currentIsDir)
          } },
        { id: "nav.enter", title: "Open / enter folder", fp: ["<CR>"], keys: ["<CR>"],
          run: () => {
              const p = pane(); if (!p) return
              if (app.pickMode && p.currentPath !== "" && !p.currentIsDir) { app.pickEnterFile(); return }
              p.openCurrent(p.deep && !p.currentIsDir)
          } },
        { id: "pick.accept", title: "Picker: accept", keys: [], palette: false, run: () => { if (app.pickMode) app.pickAccept() } },
        { id: "pick.filter", title: "Picker: next file type filter", keys: ["<C-j>"], palette: false, run: () => app.pickCycleFilter() },
        { id: "nav.reveal", title: "Reveal in its folder", keys: ["g f", "<A-CR>"], fp: ["<A-CR>"],
          run: () => { const p = pane(); if (p) p.openCurrent(true) } },
        { id: "nav.openExternal", title: "Open with default application", fp: ["<A-CR>", "<C-o>"], keys: ["o"],
          run: () => { const p = pane(); if (p && p.currentPath !== "") Backend.openPath(p.currentPath) } },
        { id: "nav.back", title: "History back", fp: ["<A-Left>", "<XF86Back>"], keys: ["H", "<A-Left>"], run: () => { const p = pane(); if (p) p.goBack() } },
        { id: "nav.forward", title: "History forward", fp: ["<A-Right>", "<XF86Forward>"], keys: ["L", "<A-Right>"], run: () => { const p = pane(); if (p) p.goForward() } },
        { id: "nav.home", title: "Go home", fp: ["<A-Home>"], keys: ["g h", "~"], run: () => go(Backend.home()) },
        { id: "nav.root", title: "Go to /", keys: ["g r"], run: () => go("/") },
        { id: "nav.downloads", title: "Go to Downloads", keys: ["g d"], run: () => go(Backend.home() + "/Downloads") },
        { id: "nav.documents", title: "Go to Documents", keys: ["g D"], run: () => go(Backend.home() + "/Documents") },
        { id: "nav.config", title: "Go to ~/.config", keys: ["g c"], run: () => go(Backend.home() + "/.config") },
        { id: "nav.trash", title: "Go to Trash", keys: ["g b"], run: () => go(Backend.home() + "/.local/share/Trash/files") },
        { id: "nav.jump", title: "Jump to folder…", fp: ["<C-l>", "<C-g>", "<A-d>"], keys: ["z", "<C-l>", "<Space> g j"], run: () => app.jump.open("") },
        { id: "nav.reload", title: "Reload folder", fp: ["<F5>", "<C-r>"], keys: ["<Space> v r", "<C-S-r>"], run: () => { const p = pane(); if (p) p.dir.reload() } },

        { id: "sel.toggle", title: "Toggle mark and move down", fp: ["<Space>", "<C-Space>"], keys: ["m"],
          run: () => { const p = pane(); if (p && p.view.currentIndex >= 0) { p.dir.toggleSelect(p.view.currentIndex); p.move(1) } } },
        { id: "sel.extendDown", title: "Extend selection down", keys: [], fp: ["<S-Down>"],
          run: () => { const p = pane(); if (!p) return; if (!p.visualMode) p.startVisual(); p.move(1) } },
        { id: "sel.extendUp", title: "Extend selection up", keys: [], fp: ["<S-Up>"],
          run: () => { const p = pane(); if (!p) return; if (!p.visualMode) p.startVisual(); p.move(-1) } },
        { id: "sel.extendEnd", title: "Extend selection to the end", keys: [], fp: ["<S-End>"],
          run: () => { const p = pane(); if (!p) return; if (!p.visualMode) p.startVisual(); p.setCursor(p.view.count - 1) } },
        { id: "sel.extendHome", title: "Extend selection to the start", keys: [], fp: ["<S-Home>"],
          run: () => { const p = pane(); if (!p) return; if (!p.visualMode) p.startVisual(); p.setCursor(0) } },
        { id: "nav.newTab", title: "Open in a new tab", keys: ["<C-CR>"], fp: ["<C-CR>"],
          run: () => {
              if (app.pickMode) { app.pickAccept(); return }
              const p = pane(); if (!p || p.currentPath === "") return
              if (p.currentIsDir) { app.newTab(p.currentPath); return }
              // A search result: show its folder in a new tab with the file selected.
              const parent = p.parentOf(p.currentPath)
              const name = p.baseName(p.currentPath)
              app.newTab(parent)
              Qt.callLater(() => { if (app.pane) app.pane.pendingName = name })
          } },
        { id: "keymap.toggle", title: "Switch keymap: File Pilot (Ctrl) / Vim (modal)", keys: ["<Space> k"], fp: ["<C-S-k>"],
          run: () => app.setKeymap(app.keymap === "fp" ? "vim" : "fp") },
        { id: "sel.visual", title: "Visual selection mode", keys: ["v"],
          run: () => { const p = pane(); if (!p) return; if (p.visualMode) p.endVisual(); else p.startVisual() } },
        { id: "sel.all", title: "Select all", fp: ["<C-a>"], keys: ["<C-a>"], run: () => { const p = pane(); if (p) p.dir.selectAll() } },
        { id: "sel.clear", title: "Clear selection / filter / search", fp: ["<Esc>"], keys: ["<Esc>"],
          run: () => {
              const p = pane(); if (!p) return
              if (p.visualMode) p.endVisual()
              else if (p.dir.selectedCount > 0) p.dir.clearSelection()
              else if (p.deep) p.exitDeep()
              else if (p.dir.filter !== "") p.dir.setFilterText("")
              else if (app.pickMode) app.quit()     // nothing left to clear: Esc cancels the file chooser
          } },

        { id: "file.copy", title: "Copy (yank)", fp: ["<C-c>"], keys: ["y y", "<C-c>"], run: () => yank(false) },
        { id: "file.cut", title: "Cut", fp: ["<C-x>"], keys: ["d d", "<C-x>"], run: () => yank(true) },
        { id: "file.paste", title: "Paste", fp: ["<C-v>"], keys: ["p", "<C-v>"], run: () => paste() },
        { id: "file.trash", title: "Move to trash", fp: ["<Del>"], keys: ["d t", "<Del>"], run: () => trash() },
        { id: "file.delete", title: "Delete permanently…", fp: ["<S-Del>"], keys: ["d D", "<S-Del>"], run: () => remove() },
        { id: "file.rename", title: "Rename", fp: ["<F2>"], keys: ["r", "<F2>", "c w"], run: () => rename() },
        { id: "file.newFile", title: "New file", fp: ["<C-n>"], keys: ["a"], run: () => create(false) },
        { id: "file.newDir", title: "New folder", fp: ["<C-S-n>"], keys: ["A", "<F7>"], run: () => create(true) },
        { id: "file.extractHere", title: "Extract archive here", words: ["unzip", "uncompress", "unpack", "untar"], run: () => extract(false) },
        { id: "file.extractFolder", title: "Extract archive to ./<name>/", words: ["unzip", "uncompress", "unpack", "untar"], run: () => extract(true) },
        { id: "file.undo", title: "Undo last file operation", fp: ["<C-z>"], keys: ["u", "<C-z>"], run: () => Backend.undo() },
        { id: "file.redo", title: "Redo file operation", fp: ["<C-y>", "<C-S-z>"], keys: ["<C-r>", "<C-S-z>", "U"], run: () => Backend.redo() },
        { id: "file.copyPath", title: "Copy path to clipboard", fp: ["<C-S-c>"], keys: ["y p"],
          run: () => { const p = pane(); if (p && p.currentPath) { Backend.copyText(p.currentPath); app.toast("Copied path") } } },
        { id: "file.copyName", title: "Copy name to clipboard", keys: ["y n"],
          run: () => { const p = pane(); if (p && p.currentPath) { Backend.copyText(p.baseName(p.currentPath)); app.toast("Copied name") } } },
        { id: "file.copyDir", title: "Copy folder path to clipboard", keys: ["y d"],
          run: () => { const p = pane(); if (p) { Backend.copyText(p.dir.path); app.toast("Copied folder path") } } },
        { id: "file.terminal", title: "Open terminal here", fp: ["<C-`>"], keys: ["!", "<Space> T"],
          run: () => { const p = pane(); if (p) Backend.openTerminal(p.deep ? p.parentOf(p.currentPath) : p.dir.path) } },
        { id: "file.batchRename", title: "Batch rename…", fp: ["<C-S-r>"], keys: ["R", "<Space> r r"], run: () => app.batchRename.open() },
        { id: "file.copyOther", title: "Copy to other pane", fp: ["<F7>"], keys: ["<F5>", "<Space> w y"], run: () => sendToOther(false) },
        { id: "file.moveOther", title: "Move to other pane", fp: ["<F8>"], keys: ["<F6>", "<Space> w m"], run: () => sendToOther(true) },

        { id: "view.filter", title: "Filter this folder", fp: ["<F3>"], keys: ["/"], run: () => filter() },
        { id: "view.hidden", title: "Toggle hidden files", fp: ["<C-h>"], keys: [".", "<C-h>", "<Space> v h"],
          run: () => { app.showHidden = !app.showHidden; app.toast(app.showHidden ? "Showing hidden files" : "Hiding hidden files") } },
        { id: "view.inspector", title: "Toggle inspector", fp: ["<C-i>"], keys: ["i", "<Space> v i"],
          run: () => { app.inspectorOn = !app.inspectorOn } },
        { id: "view.zoomIn", title: "Zoom in", fp: ["<C-=>", "<C-+>", "<KP-+>"], keys: ["<C-=>", "<C-+>", "<Space> v +", "<KP-+>"], run: () => zoom(0.1) },
        { id: "view.zoomOut", title: "Zoom out", fp: ["<C-->", "<KP-->"], keys: ["<C-->", "<Space> v -", "<KP-->"], run: () => zoom(-0.1) },
        { id: "view.zoomReset", title: "Reset zoom", fp: ["<C-0>", "<KP-0>"], keys: ["<C-0>", "<Space> v z", "<KP-0>"], run: () => { Style.zoomTarget = 1.0 } },
        { id: "view.sortName", title: "Sort by name", keys: ["<Space> o n"], run: () => setSort("name") },
        { id: "view.sortSize", title: "Sort by size", keys: ["<Space> o s"], run: () => setSort("size") },
        { id: "view.sortTime", title: "Sort by modified time", keys: ["<Space> o m"], run: () => setSort("mtime") },
        { id: "view.sortExt", title: "Sort by extension", keys: ["<Space> o e"], run: () => setSort("ext") },
        { id: "view.sortReverse", title: "Reverse sort order", keys: ["<Space> o r"],
          run: () => { app.sortDesc = !app.sortDesc; app.toast("Sort " + app.sortKey + (app.sortDesc ? " ↓" : " ↑")) } },
        { id: "view.list", title: "View: list", fp: ["<A-1>"], keys: ["<Space> v l"], run: () => { const p = pane(); if (p) p.viewMode = "list" } },
        { id: "view.grid", title: "View: icons with thumbnails", keys: ["<Space> v g"], fp: ["<A-4>"],
          run: () => { const p = pane(); if (p) p.viewMode = p.icons ? "list" : "grid" } },
        { id: "view.columns", title: "View: columns", keys: ["<Space> v c"], fp: ["<A-6>"],
          run: () => { const p = pane(); if (p) p.viewMode = p.columns ? "list" : "columns" } },
        { id: "view.tree", title: "View: tree", fp: ["<A-2>"], keys: ["<Space> v t"],
          run: () => { const p = pane(); if (p) p.viewMode = p.tree ? "list" : "tree" } },
        { id: "view.miller", title: "View: Miller columns", fp: ["<A-3>"], keys: ["<Space> v m"],
          run: () => { const p = pane(); if (p) p.viewMode = p.miller ? "list" : "miller" } },
        { id: "view.sidebar", title: "Sidebar: cycle open / icons only / hidden", keys: ["<Space> v s"], fp: ["<C-S-b>"],
          run: () => app.cycleSidebar() },
        { id: "view.group", title: "Group by: cycle none / type / date / size", keys: ["<Space> v b"], fp: ["<A-5>"],
          run: () => {
              const order = ["none", "type", "date", "size"]
              app.groupBy = order[(order.indexOf(app.groupBy) + 1) % order.length]
              app.toast(app.groupBy === "none" ? "Grouping off" : "Grouped by " + app.groupBy)
          } },
        { id: "tab.list", title: "Search open tabs…", keys: ["<Space> t t"], fp: ["<A-t>", "<C-S-t>"], run: () => app.tabsPicker() },
        { id: "media.toggle", title: "Media: play / pause", keys: ["<Space> m p"], fp: ["<C-m>"], run: () => app.inspector.togglePlayback() },
        { id: "media.seekBack", title: "Media: back 10 seconds", keys: ["<Space> m h"], fp: ["<C-Left>"], run: () => app.inspector.seekBy(-10000) },
        { id: "media.seekFwd", title: "Media: forward 10 seconds", keys: ["<Space> m l"], fp: ["<C-Right>"], run: () => app.inspector.seekBy(10000) },
        { id: "media.mute", title: "Media: mute / unmute", keys: ["<Space> m m"], fp: ["<C-S-m>"], run: () => app.inspector.toggleMute() },
        { id: "media.loop", title: "Media: loop on / off", keys: ["<Space> m r"], fp: [], run: () => app.inspector.toggleLoop() },
        { id: "view.animations", title: "Toggle animations", fp: [], keys: ["<Space> v a"],
          run: () => { Style.animations = !Style.animations; app.toast(Style.animations ? "Animations on" : "Animations off"); app.markDirty() } },
        { id: "view.density", title: "Cycle row spacing (compact / normal / roomy)", fp: [], keys: ["<Space> v d"],
          run: () => {
              const steps = [0.8, 1.0, 1.25]
              const next = steps[(steps.findIndex(s => Math.abs(s - Style.density) < 0.01) + 1) % steps.length]
              Style.density = next
              app.toast(next < 1 ? "Compact spacing" : (next > 1 ? "Roomy spacing" : "Normal spacing"))
              app.markDirty()
          } },
        { id: "menu.context", title: "Context menu (actions for the item)", fp: ["<Menu>", "<S-F10>"], keys: ["<Space> c", "<S-F10>", "<Menu>"], run: () => app.contextMenu() },
        { id: "tab.moveLeft", title: "Move tab left", fp: ["<C-S-PageUp>"], keys: ["<Space> t H"], run: () => app.moveTab(app.tabIndex, app.tabIndex - 1) },
        { id: "tab.moveRight", title: "Move tab right", fp: ["<C-S-PageDown>"], keys: ["<Space> t L"], run: () => app.moveTab(app.tabIndex, app.tabIndex + 1) },
        { id: "pane.swap", title: "Swap pane with the next one", fp: ["<C-S-x>"], keys: ["<Space> w x"],
          run: () => { const t = app.tab; if (t && t.paneCount > 1) t.swapPanes(t.activeIndex, (t.activeIndex + 1) % t.paneCount) } },
        { id: "view.layoutCycle", title: "Cycle pane layout", keys: ["<Space> w l"],
          run: () => { if (app.tab) app.tab.cycleLayout() } },

        { id: "search.folder", title: "Search this folder (deep)", fp: ["<C-f>", "<C-e>"], keys: ["<C-f>", "<Space> f f"], run: () => search(false) },
        { id: "search.drive", title: "Search whole drive", fp: ["<C-S-f>"], keys: ["<C-S-f>", "<Space> f d"], run: () => search(true) },

        { id: "pin.folder", title: "Pin / unpin this folder", fp: ["<C-b>"], keys: ["<Space> p f"],
          run: () => {
              const p = pane(); if (!p) return
              const on = Backend.toggleBookmark(p.dir.path)
              app.toast(on ? "Pinned " + p.pretty(p.dir.path) : "Unpinned")
              app.pinsChanged()
          } },
        { id: "pin.item", title: "Pin / unpin the item under the cursor", keys: ["<Space> p i"], fp: ["<C-d>"],
          run: () => {
              const p = pane(); if (!p || p.currentPath === "") return
              const on = Backend.toggleBookmark(p.currentPath)
              app.toast(on ? "Pinned " + p.baseName(p.currentPath) : "Unpinned " + p.baseName(p.currentPath))
              app.pinsChanged()
          } },

        { id: "tab.new", title: "New tab", fp: ["<C-t>"], keys: ["<C-t>", "<Space> t n"],
          run: () => app.newTab(pane() ? pane().dir.path : Backend.home()) },
        { id: "tab.close", title: "Close tab", fp: ["<C-w>"], keys: ["q", "<Space> t c"], run: () => app.closeTab(app.tabIndex) },
        { id: "tab.next", title: "Next tab", fp: ["<C-Tab>", "<C-PageDown>"], keys: ["g t", "<C-PageDown>", "<Space> t l"], run: () => app.selectTab(app.tabIndex + 1) },
        { id: "tab.prev", title: "Previous tab", fp: ["<C-S-Tab>", "<C-PageUp>"], keys: ["g T", "<C-PageUp>", "<Space> t h"], run: () => app.selectTab(app.tabIndex - 1) },
        { id: "tab.1", title: "Tab 1", fp: ["<C-1>"], keys: ["<A-1>"], palette: false, run: () => app.selectTab(0) },
        { id: "tab.2", title: "Tab 2", fp: ["<C-2>"], keys: ["<A-2>"], palette: false, run: () => app.selectTab(1) },
        { id: "tab.3", title: "Tab 3", fp: ["<C-3>"], keys: ["<A-3>"], palette: false, run: () => app.selectTab(2) },
        { id: "tab.4", title: "Tab 4", fp: ["<C-4>"], keys: ["<A-4>"], palette: false, run: () => app.selectTab(3) },
        { id: "tab.5", title: "Tab 5", fp: ["<C-5>"], keys: ["<A-5>"], palette: false, run: () => app.selectTab(4) },
        { id: "tab.6", title: "Tab 6", fp: ["<C-6>"], keys: ["<A-6>"], palette: false, run: () => app.selectTab(5) },
        { id: "tab.7", title: "Tab 7", fp: ["<C-7>"], keys: ["<A-7>"], palette: false, run: () => app.selectTab(6) },
        { id: "tab.8", title: "Tab 8", fp: ["<C-8>"], keys: ["<A-8>"], palette: false, run: () => app.selectTab(7) },
        { id: "tab.9", title: "Tab 9", fp: ["<C-9>"], keys: ["<A-9>"], palette: false, run: () => app.selectTab(8) },

        { id: "pane.next", title: "Focus next pane", fp: ["<Tab>", "<F6>"], keys: ["<Tab>", "<C-w> w", "<C-w> l", "<Space> w w"],
          run: () => { if (app.tab) app.tab.focusPane(app.tab.activeIndex + 1) } },
        { id: "pane.prev", title: "Focus previous pane", fp: ["<S-Tab>"], keys: ["<S-Tab>", "<C-w> h", "<Space> w h"],
          run: () => { if (app.tab) app.tab.focusPane(app.tab.activeIndex - 1) } },
        { id: "pane.splitV", title: "Split: add pane beside", fp: ["<C-\\>"], keys: ["<C-w> v", "<Space> w v"],
          run: () => { const t = app.tab; if (!t) return; t.layoutMode = "columns"; if (!t.addPane(pane().dir.path)) app.toast("At most 4 panes", "warn") } },
        { id: "pane.splitH", title: "Split: add pane below", fp: ["<C-|>"], keys: ["<C-w> s", "<Space> w s"],
          run: () => { const t = app.tab; if (!t) return; t.layoutMode = "rows"; if (!t.addPane(pane().dir.path)) app.toast("At most 4 panes", "warn") } },
        { id: "pane.close", title: "Close pane", fp: ["<C-S-w>"], keys: ["<C-w> c", "<C-w> q", "<Space> w c"],
          run: () => { if (app.tab && !app.tab.closePane(app.tab.activeIndex)) app.toast("Last pane", "warn") } },
        { id: "pane.only", title: "Close other panes", keys: ["<C-w> o", "<Space> w o"], run: () => { if (app.tab) app.tab.onlyPane() } },

        { id: "layout.save", title: "Save layout…", keys: ["<Space> l s"], run: () => app.saveLayoutPrompt() },
        { id: "layout.load", title: "Load layout…", keys: ["<Space> l l"], run: () => app.loadLayoutPicker() },

        { id: "devices.open", title: "Devices and remotes…", keys: ["<Space> g d"], run: () => app.devicesPicker("open") },
        { id: "devices.eject", title: "Unmount / eject device…", keys: ["<Space> g e"], run: () => app.devicesPicker("eject") },
        { id: "remote.connect", title: "Connect to server… (smb, sftp, ftp, dav)", keys: ["<Space> g c"], run: () => app.connectPrompt() },
        { id: "app.palette", title: "Command palette", fp: ["<C-p>", "<C-S-p>", "<C-k>"], keys: ["<Space> <Space>", ":", "<C-p>"], run: () => app.openPalette() },
        { id: "app.settings", title: "Settings…", fp: ["<C-,>"], keys: ["<Space> ,"], run: () => app.openSettings() },
        { id: "app.keys", title: "Show key bindings file", keys: ["<Space> ?"],
          run: () => { Backend.openPath(Backend.home() + "/.config/captainfm/keys.json"); } },
        { id: "app.quit", title: "Quit", fp: ["<C-q>"], keys: ["<C-q>", "Z Z", "<Space> q"], run: () => app.quit() }
    ]
}
